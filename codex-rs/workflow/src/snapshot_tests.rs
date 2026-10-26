use super::*;
use pretty_assertions::assert_eq;

fn tracker() -> RunTracker {
    RunTracker::new(
        "wf-00000001".to_string(),
        "find-bugs".to_string(),
        "d".to_string(),
        &["Find".to_string(), "Verify".to_string()],
        Some(100),
        1_000,
    )
}

/// R8.1, R2.8: phases attribute agents, counts follow call states, revision grows.
#[test]
fn tracker_counts_agents_by_state_and_phase() {
    let mut tracker = tracker();
    let first_revision = tracker.revision();
    tracker.set_phase(1, "Verify");
    tracker.agent_queued(1, Some("verify a".to_string()), Some("Verify".to_string()));
    tracker.agent_queued(2, None, Some("Synthesize".to_string()));
    tracker.agent_started(1, "/root/wf_1".to_string(), "thread-1".to_string());
    tracker.agent_awaiting_approval(1, true);
    assert_eq!(
        tracker.snapshot(false).agents,
        AgentCounts {
            queued: 1,
            running: 0,
            awaiting_approval: 1,
            completed: 0,
            null_results: 0,
        }
    );
    tracker.agent_awaiting_approval(1, false);
    tracker.agent_finished(
        1,
        true,
        None,
        TokenCharge {
            charged: 7,
            cached: 3,
        },
    );
    tracker.agent_finished(2, false, Some("budget_exhausted"), TokenCharge::default());

    let snapshot = tracker.snapshot(true);
    assert!(snapshot.revision > first_revision);
    assert_eq!(snapshot.current_phase.as_deref(), Some("Verify"));
    assert_eq!(
        snapshot.phases,
        vec![
            PhaseSnapshot {
                title: "Find".to_string(),
                started: 0,
                finished: 0,
            },
            PhaseSnapshot {
                title: "Verify".to_string(),
                started: 1,
                finished: 1,
            },
            PhaseSnapshot {
                title: "Synthesize".to_string(),
                started: 1,
                finished: 1,
            },
        ]
    );
    assert_eq!(snapshot.agents.completed, 1);
    assert_eq!(snapshot.agents.null_results, 1);
    assert_eq!(snapshot.agents.total(), 2);
    assert_eq!(snapshot.null_reasons.get("budget_exhausted"), Some(&1));
    assert_eq!((snapshot.charged_tokens, snapshot.cached_tokens), (7, 3));
    assert_eq!(snapshot.agent_list.len(), 2);
    assert_eq!(snapshot.agent_list[0].label.as_deref(), Some("verify a"));
    assert!(tracker.snapshot(false).agent_list.is_empty());
}

/// R2.8: the later `phase()` call wins even when it arrives first.
#[test]
fn current_phase_follows_event_order() {
    let mut tracker = tracker();
    tracker.set_phase(5, "Verify");
    tracker.set_phase(3, "Find");
    assert_eq!(
        tracker.snapshot(false).current_phase.as_deref(),
        Some("Verify")
    );
}

/// R2.8, R8.1: long logs are cut to 2000 characters and only 20 lines are kept.
#[test]
fn logs_are_truncated_and_tailed() {
    let mut tracker = tracker();
    let long = "x".repeat(MAX_LOG_CHARS + 10);
    assert_eq!(tracker.log(&long).chars().count(), MAX_LOG_CHARS);
    for index in 0..30 {
        tracker.log(&format!("line {index}"));
    }
    let logs = tracker.snapshot(false).logs_tail;
    assert_eq!(logs.len(), LOG_TAIL_LINES);
    assert_eq!(logs.first().map(String::as_str), Some("line 10"));
}

/// R5.1: live charging and the call's total are not counted twice; a cached call adds no
/// tokens and counts as completed.
#[test]
fn live_charges_and_totals_do_not_double_count() {
    let mut tracker = tracker();
    tracker.agent_queued(1, None, None);
    tracker.agent_started(1, "/root/wf_1".to_string(), "t-1".to_string());
    tracker.agent_charged(
        1,
        TokenCharge {
            charged: 40,
            cached: 10,
        },
    );
    tracker.agent_charged(
        1,
        TokenCharge {
            charged: 20,
            cached: 0,
        },
    );
    assert_eq!(tracker.snapshot(false).charged_tokens, 60);
    // The child's total also counts a response the live hook did not see.
    tracker.agent_finished(
        1,
        true,
        None,
        TokenCharge {
            charged: 75,
            cached: 10,
        },
    );
    // A late live charge after the call settled is ignored.
    tracker.agent_charged(
        1,
        TokenCharge {
            charged: 5,
            cached: 5,
        },
    );
    tracker.agent_cached(2, Some("again".to_string()), None);
    let snapshot = tracker.snapshot(true);
    assert_eq!((snapshot.charged_tokens, snapshot.cached_tokens), (75, 10));
    assert_eq!(
        snapshot.agent_list[0].tokens,
        TokenCharge {
            charged: 75,
            cached: 10,
        }
    );
    assert_eq!(snapshot.agents.completed, 2);
    assert!(snapshot.agent_list[1].cached);
    assert!(!snapshot.agent_list[0].cached);
}

/// R6.4, R6.7: a journal rebuilds phases, logs, calls, null reasons, tokens and status; an
/// open call of an interrupted run settles as null `interrupted`; journals without call
/// numbers match calls by key and ordinal.
#[test]
fn replay_rebuilds_a_run_from_its_journal() {
    let records: Vec<JournalRecord> = [
        r#"{"seq":1,"kind":"run_started","run_id":"wf-00000002","name":"resumed","script_sha256":"s","args_sha256":"a","parent_turn_id":"t","resume_from":"wf-00000001","started_at_ms":1000,"budget_tokens":500,"resume_source_results":3}"#,
        r#"{"seq":2,"kind":"phase","title":"Find"}"#,
        r#"{"seq":3,"kind":"agent_finished","call":1,"key":"a","ordinal":0,"ok":true,"value":"x","reason":null,"tokens":{"charged":0,"cached":0},"cached":true}"#,
        r#"{"seq":4,"kind":"agent_started","call":2,"key":"b","ordinal":0,"label":"two","phase":"Find","thread_id":"t-2","agent_path":"/root/wf_2"}"#,
        r#"{"seq":5,"kind":"agent_finished","call":2,"key":"b","ordinal":0,"ok":true,"value":"y","reason":null,"tokens":{"charged":30,"cached":7}}"#,
        r#"{"seq":6,"kind":"agent_finished","call":3,"key":"c","ordinal":0,"ok":false,"value":null,"reason":"timeout","tokens":{"charged":4,"cached":0}}"#,
        r#"{"seq":7,"kind":"agent_started","call":4,"key":"d","ordinal":0,"label":null,"phase":"Find","thread_id":"t-4","agent_path":"/root/wf_4"}"#,
        r#"{"seq":8,"kind":"log","message":"waiting"}"#,
        r#"{"seq":9,"kind":"run_finished","status":"interrupted","result_bytes":0,"tokens":{"charged":34,"cached":7},"error":"the Codex process stopped while the run was active","finished_at_ms":4000}"#,
    ]
    .iter()
    .map(|line| serde_json::from_str(line).expect("record"))
    .collect();
    let tracker = RunTracker::replay(
        "wf-00000002".to_string(),
        "desc".to_string(),
        &["Find".to_string()],
        &records,
        /*fallback_ms*/ 9_999,
        /*result_preview*/ None,
    );
    let snapshot = tracker.snapshot(true);
    assert_eq!(snapshot.name, "resumed");
    assert_eq!(snapshot.status, RunStatus::Interrupted);
    assert_eq!(
        snapshot.error.as_deref(),
        Some("the Codex process stopped while the run was active")
    );
    assert_eq!(
        (snapshot.started_at_ms, snapshot.finished_at_ms),
        (1000, Some(4000))
    );
    assert_eq!(snapshot.budget_tokens, Some(500));
    assert_eq!(snapshot.current_phase.as_deref(), Some("Find"));
    assert_eq!(snapshot.agents.completed, 2);
    assert_eq!(snapshot.agents.null_results, 2);
    assert_eq!(snapshot.null_reasons.get("timeout"), Some(&1));
    assert_eq!(snapshot.null_reasons.get("interrupted"), Some(&1));
    assert_eq!((snapshot.charged_tokens, snapshot.cached_tokens), (34, 7));
    assert_eq!(snapshot.logs_tail, vec!["waiting".to_string()]);
    assert_eq!(
        snapshot.resume,
        Some(ResumeStats {
            from: "wf-00000001".to_string(),
            source_results: 3,
            served: 1,
            unused: 2,
        })
    );
    assert_eq!(snapshot.agent_list[1].label.as_deref(), Some("two"));
    assert_eq!(snapshot.agent_list[1].thread_id.as_deref(), Some("t-2"));

    // A journal written before call numbers existed.
    let old: Vec<JournalRecord> = [
        r#"{"seq":1,"kind":"run_started","run_id":"wf-00000003","name":"old","script_sha256":"s","args_sha256":"a","parent_turn_id":"t","resume_from":null}"#,
        r#"{"seq":2,"kind":"agent_started","key":"a","ordinal":0,"label":null,"phase":null,"thread_id":"t-1","agent_path":"/root/wf_1"}"#,
        r#"{"seq":3,"kind":"agent_started","key":"a","ordinal":1,"label":null,"phase":null,"thread_id":"t-2","agent_path":"/root/wf_2"}"#,
        r#"{"seq":4,"kind":"agent_finished","key":"a","ordinal":1,"ok":true,"value":"2","reason":null,"tokens":{"charged":2,"cached":0}}"#,
        r#"{"seq":5,"kind":"agent_finished","key":"a","ordinal":0,"ok":true,"value":"1","reason":null,"tokens":{"charged":1,"cached":0}}"#,
        r#"{"seq":6,"kind":"run_finished","status":"completed","result_bytes":3,"tokens":{"charged":3,"cached":0},"error":null}"#,
    ]
    .iter()
    .map(|line| serde_json::from_str(line).expect("record"))
    .collect();
    let snapshot = RunTracker::replay(
        "wf-00000003".to_string(),
        String::new(),
        &[],
        &old,
        /*fallback_ms*/ 7,
        Some("[1,2]".to_string()),
    )
    .snapshot(true);
    assert_eq!(snapshot.status, RunStatus::Completed);
    assert_eq!(snapshot.agents.completed, 2);
    assert_eq!(snapshot.charged_tokens, 3);
    assert_eq!(snapshot.result_preview.as_deref(), Some("[1,2]"));
    assert_eq!(
        (snapshot.started_at_ms, snapshot.finished_at_ms),
        (7, Some(7))
    );
    assert_eq!(snapshot.resume, None);
    assert_eq!(
        snapshot
            .agent_list
            .iter()
            .map(|agent| (agent.seq, agent.thread_id.as_deref()))
            .collect::<Vec<_>>(),
        vec![(1, Some("t-1")), (2, Some("t-2"))]
    );
}

#[test]
fn finish_is_terminal_once() {
    let mut tracker = tracker();
    tracker.finish(RunStatus::Cancelled, 5_000, None, None);
    let revision = tracker.revision();
    tracker.finish(RunStatus::Completed, 6_000, Some("x".to_string()), None);
    let snapshot = tracker.snapshot(false);
    assert_eq!(snapshot.status, RunStatus::Cancelled);
    assert_eq!(snapshot.revision, revision);
    assert_eq!(snapshot.duration_ms(/*now_ms*/ 9_000), 4_000);
}
