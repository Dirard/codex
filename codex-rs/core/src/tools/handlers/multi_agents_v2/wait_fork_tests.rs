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
                        first_match_wait: None,
                    },
                    WaitingTerminal {
                        session_id: 1001,
                        running_for: Duration::from_millis(5_000),
                        quiet_for: None,
                        first_match_wait: None,
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

/// Live recheck 2026-10-27, N4: a model waited for a server's "ready" with seven short waits
/// in its turn. When a terminal has only an output watch that has not matched, the timeout
/// says how long its first match is still awaited: after the turn that started the watch, or,
/// for a watch of an earlier turn, what is left (review of the fixes, P3-7).
#[test]
fn timeout_snapshot_says_how_long_a_first_match_is_awaited() {
    let terminal = |session_id, first_match_wait| WaitingTerminal {
        session_id,
        running_for: Duration::from_millis(12_000),
        quiet_for: None,
        first_match_wait,
    };
    assert_eq!(
        describe_background_terminals(&[terminal(
            1000,
            Some(FirstMatchWait::AfterTurn(Duration::from_secs(300)))
        )]),
        "Background terminals that will report their exit: 1 (session 1000: running for 12.0s, no output yet). Their exits are reported even after this turn ends, so you can end the turn instead of waiting again. Session 1000 has only an output watch that has not matched yet: its first matching line is awaited for up to 5 minutes after your turn ends. A match starts a new turn, so end your turn now rather than wait again with a short timeout."
    );
    let text = describe_background_terminals(&[
        terminal(
            1000,
            Some(FirstMatchWait::AfterTurn(Duration::from_secs(90))),
        ),
        terminal(1001, None),
        terminal(1002, Some(FirstMatchWait::Left(Duration::from_secs(52)))),
    ]);
    assert!(
        text.ends_with(
            " Session 1000 has only an output watch that has not matched yet: its first matching line is awaited for up to 90 seconds after your turn ends. Session 1002 has only an output watch that has not matched yet: its first matching line is awaited for 52 seconds more (it started in an earlier turn, so the end of this one does not extend it). A match starts a new turn, so end your turn now rather than wait again with a short timeout."
        ),
        "{text}"
    );
    assert!(
        !describe_background_terminals(&[terminal(1001, None)]).contains("first matching line")
    );
}

/// Review of the fixes P3-7, P3-8: the first-match sentence takes the watch's own window, and
/// only a terminal that waits for nothing else gets it: with `notify_on_exit` it is awaited
/// until it exits, and after a match only while matches keep coming.
#[test]
fn only_a_watch_only_terminal_before_its_first_match_gets_the_sentence() {
    use crate::unified_exec::PendingTerminal;
    use crate::unified_exec::WatchWait;
    let now = Instant::now();
    let pending = |notify_on_exit, watch_wait| PendingTerminal {
        process_id: 1000,
        command: "./dev.sh".to_string(),
        started_at: now,
        last_output_at: None,
        output_watch: Some("ready".to_string()),
        notify_on_exit,
        watch_wait,
    };
    let after_turn = Some(WatchWait::FirstMatchAfterTurn(Duration::from_secs(300)));
    let left = Some(WatchWait::FirstMatch(Duration::from_secs(100)));
    assert_eq!(
        waiting_terminal(pending(false, after_turn), now).first_match_wait,
        Some(FirstMatchWait::AfterTurn(Duration::from_secs(300)))
    );
    assert_eq!(
        waiting_terminal(pending(false, left), now).first_match_wait,
        Some(FirstMatchWait::Left(Duration::from_secs(100)))
    );
    for (notify_on_exit, watch_wait) in [
        (true, after_turn),
        (true, left),
        (false, Some(WatchWait::Quiet(Duration::from_secs(60)))),
        (false, None),
    ] {
        assert_eq!(
            waiting_terminal(pending(notify_on_exit, watch_wait), now).first_match_wait,
            None,
            "{notify_on_exit} {watch_wait:?}"
        );
    }
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
