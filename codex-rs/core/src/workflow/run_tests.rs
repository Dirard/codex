use super::*;
use codex_workflow::Journal;
use codex_workflow::JournalRecord;
use pretty_assertions::assert_eq;

fn charge(charged: u64) -> TokenCharge {
    TokenCharge { charged, cached: 0 }
}

fn limits(hard_budget_threshold: Option<u64>) -> RunLimits {
    RunLimits {
        max_concurrent_agents: 4,
        max_agents_per_run: 10,
        capacity_wait_ms: 0,
        budget_tokens: Some(1_000),
        notification_max_tokens: 2_000,
        max_heap_bytes: 0,
        budget_mode: if hard_budget_threshold.is_some() {
            WorkflowBudgetMode::Hard
        } else {
            WorkflowBudgetMode::Soft
        },
        hard_budget_threshold,
        structured_output: WorkflowStructuredOutput::Auto,
    }
}

/// A running run whose calls 1..=`agents` started agents.
fn test_run(
    agents: u64,
    hard_budget_threshold: Option<u64>,
    journal: Journal,
    ephemeral_files: Option<(String, String)>,
    resume: Option<JournalCache>,
) -> WorkflowRun {
    let mut tracker = RunTracker::new(
        "wf-0000abcd".to_string(),
        "budget".to_string(),
        String::new(),
        &[],
        Some(1_000),
        1,
    );
    for seq in 1..=agents {
        tracker.agent_queued(seq, None, None);
        tracker.agent_started(seq, format!("/root/wf_{seq}"), format!("t-{seq}"));
    }
    WorkflowRun::from_parts(RunParts {
        id: "wf-0000abcd".to_string(),
        meta: WorkflowMeta {
            name: "budget".to_string(),
            description: String::new(),
            phases: Vec::new(),
        },
        limits: limits(hard_budget_threshold),
        parent_turn_id: "turn".to_string(),
        tracker,
        updates: broadcast::channel(16).0,
        journal,
        ephemeral_files,
        resume,
        result_json: None,
        restored: false,
    })
}

fn budget_used(run: &WorkflowRun) -> u64 {
    run.budget_json()["used"].as_u64().expect("used")
}

/// R5.1, R5.4 (review finding 2): a response the live hook reports after the call settled
/// from the agent's total, as on a timeout, cancellation or hard budget while the hook was
/// running, is not counted again: the budget the script and the hard threshold see and the
/// snapshot agree.
#[test]
fn a_late_live_charge_is_not_counted_twice() {
    let run = test_run(1, None, Journal::in_memory(), None, None);
    run.charge_live(1, charge(100));
    // The call settles from a total that already holds the next response of 50 ...
    let total = run.settle_call_tokens(1, charge(150));
    assert_eq!(total, charge(150));
    // ... and the hook, which took the call's sink earlier, reports that response now.
    run.charge_live(1, charge(50));
    run.update(|tracker| tracker.agent_finished(1, true, None, total));
    run.charge_live(1, charge(50));
    let snapshot = run.snapshot(/*include_agents*/ true);
    assert_eq!(snapshot.charged_tokens, 150);
    assert_eq!(snapshot.agent_list[0].tokens, charge(150));
    assert_eq!(budget_used(&run), 150);
}

/// R5.4 (review finding 7): agents that cross the hard threshold at the same time cancel
/// the run's hard budget once and log it once.
#[test]
fn the_hard_budget_is_logged_once() {
    let run = test_run(8, Some(100), Journal::in_memory(), None, None);
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        for seq in 1..=8 {
            let run = &run;
            let barrier = &barrier;
            scope.spawn(move || {
                barrier.wait();
                run.charge_live(seq, charge(60));
            });
        }
    });
    assert!(run.hard_budget().is_cancelled());
    let logged = run
        .snapshot(/*include_agents*/ false)
        .logs_tail
        .iter()
        .filter(|line| line.contains("above its hard budget of 100"))
        .count();
    assert_eq!(logged, 1);
    assert_eq!(budget_used(&run), 480);
}

/// R6.3, R6.6 (review finding 6): a finished run drops the answers of the run it resumed,
/// and only an ephemeral run keeps its script and args in memory; a persistent run reads
/// them from its directory when `resume_from` needs them.
#[tokio::test]
async fn finished_runs_keep_no_resume_answers_and_persistent_files_stay_on_disk() {
    let records: Vec<JournalRecord> = [
        r#"{"seq":1,"kind":"agent_finished","call":1,"key":"a","ordinal":0,"ok":true,"value":"x","reason":null,"tokens":{"charged":0,"cached":0}}"#,
        r#"{"seq":2,"kind":"agent_finished","call":2,"key":"b","ordinal":0,"ok":true,"value":"y","reason":null,"tokens":{"charged":0,"cached":0}}"#,
    ]
    .iter()
    .map(|line| serde_json::from_str(line).expect("record"))
    .collect();
    let cache = JournalCache::from_records("wf-00000001", &records);
    let run = test_run(
        0,
        None,
        Journal::in_memory(),
        Some(("script".to_string(), "{\"a\":1}".to_string())),
        Some(cache),
    );
    let (answer, from) = run.take_cached("a", 0).expect("cached answer");
    assert_eq!((answer.value, from.as_str()), (json!("x"), "wf-00000001"));
    run.release_finished_state();
    assert!(run.take_cached("b", 0).is_none());
    assert_eq!(
        run.script_and_args().await,
        Ok(("script".to_string(), "{\"a\":1}".to_string()))
    );

    let home = tempfile::tempdir().expect("tempdir");
    let dir = home.path().join("wf-0000abcd");
    let journal = Journal::create(&dir, "on disk", "[1]").expect("journal");
    let run = test_run(0, None, journal, None, None);
    assert_eq!(
        run.script_and_args().await,
        Ok(("on disk".to_string(), "[1]".to_string()))
    );
}
