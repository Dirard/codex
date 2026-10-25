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
