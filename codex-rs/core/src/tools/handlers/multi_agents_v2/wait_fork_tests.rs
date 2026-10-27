use super::*;
use pretty_assertions::assert_eq;

/// Fork (R4.5): only a wait without `timeout_ms` that only terminals hold is capped.
#[test]
fn caps_only_unbounded_waits_for_terminals() {
    let default = 1_800_000;
    assert_eq!(
        effective_timeout_ms(None, default, /*terminals_only*/ true),
        BACKGROUND_TERMINAL_WAIT_CAP_MS
    );
    assert_eq!(
        effective_timeout_ms(None, default, /*terminals_only*/ false),
        default
    );
    assert_eq!(
        effective_timeout_ms(Some(default), default, /*terminals_only*/ true),
        default
    );
    assert_eq!(
        effective_timeout_ms(None, 60_000, /*terminals_only*/ true),
        60_000
    );
}

/// Fork (R4.5): a timeout names each terminal, how long it ran and when it last spoke.
#[test]
fn timeout_snapshot_describes_each_terminal() {
    let result = WaitAgentResult::from_outcome(
        WaitOutcome::TimedOut(
            ActiveAgentSnapshot::default(),
            PendingWork {
                terminals: vec![
                    WaitingTerminal {
                        session_id: 1000,
                        running_for: Duration::from_millis(300_000),
                        quiet_for: Some(Duration::from_millis(298_000)),
                    },
                    WaitingTerminal {
                        session_id: 1001,
                        running_for: Duration::from_millis(5_000),
                        quiet_for: None,
                    },
                ],
                workflow_runs: Vec::new(),
            },
        ),
        /*requested_timeout_ms*/ None,
        /*timeout_ms*/ BACKGROUND_TERMINAL_WAIT_CAP_MS,
    );
    assert_eq!(
        result,
        WaitAgentResult {
            message: "Wait timed out. Active agents: pending_init=0, running=0, interrupted=0. Background terminals that will report their exit: 2 (session 1000: running for 300.0s, last output 298.0s ago; session 1001: running for 5.0s, no output yet). Their exits are reported even after this turn ends, so you can end the turn instead of waiting again.".to_string(),
            timed_out: true,
        }
    );
    assert_eq!(
        WaitAgentResult::from_outcome(
            WaitOutcome::TimedOut(ActiveAgentSnapshot::default(), PendingWork::default()),
            /*requested_timeout_ms*/ None,
            /*timeout_ms*/ 1_000,
        )
        .message,
        "Wait timed out. Active agents: pending_init=0, running=0, interrupted=0."
    );
}

/// Dynamic workflow, D9 of the combined live run: a timeout names the thread's active
/// workflow runs, whose agents the snapshot does not count.
#[test]
fn timeout_snapshot_names_active_workflow_runs() {
    let result = WaitAgentResult::from_outcome(
        WaitOutcome::TimedOut(
            ActiveAgentSnapshot::default(),
            PendingWork {
                terminals: Vec::new(),
                workflow_runs: vec!["wf-0000abcd".to_string(), "wf-0000beef".to_string()],
            },
        ),
        /*requested_timeout_ms*/ None,
        /*timeout_ms*/ 1_000,
    );
    assert_eq!(
        result,
        WaitAgentResult {
            message: "Wait timed out. Active agents: pending_init=0, running=0, interrupted=0. Workflow runs still active: wf-0000abcd, wf-0000beef. Their agents are not counted above; each run's completion is notified automatically, and workflow_status shows its progress.".to_string(),
            timed_out: true,
        }
    );
}
