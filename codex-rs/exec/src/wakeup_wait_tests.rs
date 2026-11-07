use super::*;
use chrono::TimeZone;
use codex_app_server_protocol::ThreadLoop;
use codex_app_server_protocol::ThreadWakeup;
use codex_app_server_protocol::ThreadWakeupChain;
use codex_app_server_protocol::ThreadWakeupOrigin;
use pretty_assertions::assert_eq;

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 30).unwrap()
}

fn wakeup(reason: Option<&str>) -> ThreadWakeup {
    ThreadWakeup {
        id: "wk-1a2b3c4d".to_string(),
        origin: ThreadWakeupOrigin::Model,
        due_at: Utc
            .with_ymd_and_hms(2026, 11, 7, 12, 5, 0)
            .unwrap()
            .timestamp(),
        delay_seconds: 300,
        requested_delay_seconds: 300,
        due: false,
        reason: reason.map(str::to_string),
        prompt: Some("check CI".to_string()),
        scheduled_in_turn_id: None,
    }
}

/// R7.2, R10.4: the waiting line names the due time, the reason without control characters,
/// the count, the time left and Ctrl-C.
#[test]
fn waiting_line_names_what_exec_waits_for() {
    let state = ThreadWakeupState {
        wakeup: Some(wakeup(Some("CI still\u{1b}running"))),
        loop_state: None,
        chain: ThreadWakeupChain {
            fired: 1,
            max_wakeups: 50,
            ends_at: None,
        },
    };
    assert_eq!(
        waiting_message(&state, now(), Duration::from_secs(3_310)),
        Some("codex exec is waiting for a scheduled wakeup at 12:05:00Z (in 4m 30s; wakeup 2 of at most 50): CI still?running; up to 55m 10s more; Ctrl-C to stop".to_string())
    );
    let looping = ThreadWakeupState {
        wakeup: Some(ThreadWakeup {
            origin: ThreadWakeupOrigin::Loop,
            reason: None,
            ..wakeup(None)
        }),
        loop_state: Some(ThreadLoop {
            id: "loop-9f8e7d6c".to_string(),
            prompt: "check".to_string(),
            interval_seconds: Some(300),
            iteration: 3,
            max_iterations: 50,
            started_at: 0,
            ends_at: 0,
        }),
        chain: ThreadWakeupChain::default(),
    };
    assert_eq!(
        waiting_message(&looping, now(), Duration::from_secs(600)),
        Some("codex exec is waiting for /loop iteration 4 of at most 50 at 12:05:00Z (in 4m 30s); up to 10m 0s more; Ctrl-C to stop".to_string())
    );
    assert_eq!(
        waiting_message(&ThreadWakeupState::default(), now(), Duration::ZERO),
        None
    );
}

/// R7.3, R7.4: the timeout and interrupt warnings.
#[test]
fn warnings_name_the_cancel() {
    assert_eq!(
        timeout_warning(
            Duration::from_secs(3_600),
            Some(
                Utc.with_ymd_and_hms(2026, 11, 7, 13, 5, 0)
                    .unwrap()
                    .timestamp()
            )
        ),
        "codex exec stopped waiting for the scheduled wakeup after 60m 0s; it is cancelled now (it was due at 2026-11-07T13:05:00Z)."
    );
    assert_eq!(
        INTERRUPT_WARNING,
        "codex exec was interrupted while it waited for a scheduled wakeup; it is cancelled now."
    );
}
