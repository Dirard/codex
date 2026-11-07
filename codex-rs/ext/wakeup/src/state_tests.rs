use super::*;
use crate::scheduler::StartRequest;
use crate::scheduler::fire_if_due;
use crate::test_support::Harness;
use pretty_assertions::assert_eq;

fn args(delay_seconds: u64, prompt: Option<&str>, reason: Option<&str>) -> ScheduleArgs {
    ScheduleArgs {
        delay_seconds,
        prompt: prompt.map(str::to_string),
        reason: reason.map(str::to_string),
    }
}

fn wakeup_id(h: &Harness) -> String {
    h.state.snapshot().wakeup.expect("a wakeup is pending").id
}

fn loop_id(h: &Harness) -> String {
    h.state.snapshot().loop_state.expect("a loop runs").id
}

/// R2.1, R2.2, R2.6: the delay is clamped to the configured range with both values in the
/// output, and a second call replaces the first and names its due time.
#[tokio::test]
async fn schedule_clamps_reports_and_replaces() {
    let h = Harness::new();
    let output = h
        .state
        .schedule_from_model(args(30, Some("check CI"), Some("CI\u{7}running")), "turn-1")
        .expect("scheduled");
    let first = wakeup_id(&h);
    assert_eq!(
        output.replace(&first, "wk-ID"),
        "Wakeup wk-ID scheduled for 2026-11-07T12:01:00Z (in 60 s; asked for 30 s, raised to the 60 s minimum). End your turn now; the wakeup starts a new turn with your prompt. It lasts only while this Codex session runs. Wakeups left in this chain: 49 of 50, until 00:00Z."
    );
    let wakeup = h.state.snapshot().wakeup.expect("pending");
    assert_eq!(wakeup.reason.as_deref(), Some("CI?running"));
    assert_eq!(wakeup.scheduled_in_turn_id.as_deref(), Some("turn-1"));

    let output = h
        .state
        .schedule_from_model(args(7_200, Some("check again"), None), "turn-1")
        .expect("replaced");
    let second = wakeup_id(&h);
    assert_ne!(first, second);
    assert_eq!(
        output.replace(&second, "wk-ID"),
        "Wakeup wk-ID scheduled for 2026-11-07T13:00:00Z (in 3600 s; asked for 7200 s, lowered to the 3600 s maximum). Replaced the wakeup due at 12:01:00Z. End your turn now; the wakeup starts a new turn with your prompt. It lasts only while this Codex session runs. Wakeups left in this chain: 49 of 50, until 00:00Z."
    );
    let changes = h
        .take_updates()
        .into_iter()
        .map(|update| update.change)
        .collect::<Vec<_>>();
    assert_eq!(
        changes,
        vec![WakeupChange::Scheduled, WakeupChange::Replaced]
    );
    let view = h.beacon.get().expect("the beacon names the wakeup");
    assert_eq!(view.due_at.timestamp(), wakeup_id_due(&h));
}

fn wakeup_id_due(h: &Harness) -> i64 {
    h.state.snapshot().wakeup.expect("pending").due_at
}

/// R2.3: a missing prompt outside a loop, a long prompt or a long reason is an error that
/// keeps the state.
#[tokio::test]
async fn invalid_arguments_keep_the_state() {
    let h = Harness::new();
    h.state
        .schedule_from_model(args(60, Some("first"), None), "turn-1")
        .expect("scheduled");
    let before = h.state.snapshot();
    for (call, error) in [
        (
            args(60, None, None),
            "Wakeup not scheduled: prompt is required outside a self-paced /loop; say what to do on wakeup.",
        ),
        (
            args(60, Some(&"x".repeat(MAX_PROMPT_BYTES + 1)), None),
            "Wakeup not scheduled: prompt is 4001 bytes; at most 4000.",
        ),
        (
            args(60, Some("p"), Some(&"r".repeat(MAX_REASON_CHARS + 1))),
            "Wakeup not scheduled: reason is 201 characters; at most 200.",
        ),
        (
            args(0, Some("p"), None),
            "Wakeup not scheduled: delay_seconds must be a positive integer.",
        ),
    ] {
        assert_eq!(
            h.state.schedule_from_model(call, "turn-1"),
            Err(error.to_string())
        );
        assert_eq!(h.state.snapshot(), before);
    }
}

/// R2.4, R10.3: the 50th wakeup of a chain is allowed, the 51st is not; a wakeup due after
/// the chain's deadline is rejected; user input in a turn the extension did not start resets
/// the chain, its own turns do not.
#[tokio::test]
async fn chain_limits_and_reset() {
    let h = Harness::new();
    {
        let mut inner = h.state.inner();
        inner.chain = ChainState {
            fired: 49,
            started_at: Some(h.clock.now()),
        };
    }
    let output = h
        .state
        .schedule_from_model(args(60, Some("last one"), None), "turn-1")
        .expect("the 50th wakeup is allowed");
    assert!(output.ends_with("Wakeups left in this chain: 0 of 50, until 00:00Z."));
    h.state.inner().chain.fired = 50;
    assert_eq!(
        h.state
            .schedule_from_model(args(60, Some("one more"), None), "turn-1"),
        Err("Wakeup not scheduled: this thread already ran 50 wakeups since the user's last message (features.scheduled_wakeups.max_wakeups). Report to the user instead.".to_string())
    );

    // A wakeup's own turn and a loop iteration do not reset the chain.
    h.state.inner().own_turn = Some(OwnTurn {
        turn_id: "wake-turn".to_string(),
        kind: TurnKind::Wakeup {
            wakeup_id: "wk-1".to_string(),
        },
    });
    h.state.on_user_input("wake-turn");
    assert_eq!(h.state.inner().chain.fired, 50);
    h.state.on_user_input("user-turn");
    assert_eq!(h.state.inner().chain.fired, 0);
    assert_eq!(h.state.inner().chain.started_at, None);

    let short = Harness::with_limits(ScheduledWakeupsConfig {
        max_duration_minutes: 1,
        ..Default::default()
    });
    assert_eq!(
        short
            .state
            .schedule_from_model(args(120, Some("late"), None), "turn-1"),
        Err("Wakeup not scheduled: it would be due after this chain's end at 12:01Z (features.scheduled_wakeups.max_duration_minutes). Report to the user instead.".to_string())
    );
}

/// R4.1: the model's cancel names what it stopped and leaves no notice.
#[tokio::test]
async fn model_cancel_outputs() {
    let h = Harness::new();
    assert_eq!(
        crate::fragments::cancel_output(&h.state.cancel(CancelReason::Model)),
        "No wakeup was pending."
    );
    h.state
        .schedule_from_model(args(60, Some("poll"), None), "turn-1")
        .expect("scheduled");
    assert_eq!(
        crate::fragments::cancel_output(&h.state.cancel(CancelReason::Model)),
        "Cancelled the wakeup due at 2026-11-07T12:01:00Z."
    );
    h.state
        .start_loop(LoopStartRequest {
            prompt: "check the deploy".to_string(),
            interval_seconds: Some(300),
            max_iterations: None,
        })
        .expect("loop started");
    let id = loop_id(&h);
    assert_eq!(
        crate::fragments::cancel_output(&h.state.cancel(CancelReason::Model)),
        format!("Stopped the /loop {id} after 0 iterations.")
    );
    assert_eq!(h.state.take_notice(), None);
    assert_eq!(h.beacon.get(), None);
}

/// R4.3, R4.5: an interrupt of the turn that scheduled the wakeup stops it with one notice;
/// an interrupt of another turn does not.
#[tokio::test]
async fn interrupt_of_the_scheduling_turn_cancels_with_a_notice() {
    let h = Harness::new();
    h.state
        .schedule_from_model(args(60, Some("poll"), None), "turn-1")
        .expect("scheduled");
    let id = wakeup_id(&h);
    h.state.on_turn_interrupted("turn-2");
    assert!(h.state.snapshot().wakeup.is_some());
    h.state.on_turn_interrupted("turn-1");
    assert_eq!(h.state.snapshot().wakeup, None);
    let last = h.take_updates().pop().expect("an update");
    assert_eq!(
        (last.change, last.cancel_reason),
        (WakeupChange::Cancelled, Some(CancelReason::Interrupt))
    );
    assert_eq!(
        h.state.take_notice(),
        Some(format!(
            "Your scheduled wakeup {id} (due 2026-11-07 12:01:00 UTC) was cancelled: the user interrupted the turn it belonged to. Nothing is scheduled now."
        ))
    );
    assert_eq!(h.state.take_notice(), None);
}

/// R5.1, R5.3, R5.5, R5.6, R2.5: a fixed loop runs iteration 1 at once, schedules the next
/// one `interval` after the turn ends, rejects model wakeups and ends at `maxIterations`
/// with a notice; a user turn between iterations ends nothing.
#[tokio::test]
async fn fixed_loop_runs_until_its_limit() {
    let h = Harness::new();
    let snapshot = h
        .state
        .start_loop(LoopStartRequest {
            prompt: "check the deploy".to_string(),
            interval_seconds: Some(300),
            max_iterations: Some(2),
        })
        .expect("loop started");
    let id = loop_id(&h);
    assert_eq!(
        snapshot.wakeup.as_ref().map(|wakeup| wakeup.due),
        Some(true)
    );
    fire_if_due(Arc::clone(&h.state)).await;
    let requests = h.host.requests();
    assert!(matches!(
        &requests[0],
        StartRequest::LoopIteration { prompt, kind: TurnKind::LoopIteration { iteration: 1, .. } }
            if prompt == "check the deploy"
    ));
    assert_eq!(
        h.state.snapshot().loop_state.map(|state| state.iteration),
        Some(1)
    );
    assert!(
        h.state
            .loop_context("wake-turn-1")
            .expect("loop context")
            .starts_with(&format!("/loop {id}, iteration 1 of at most 2: the user's prompt in this turn repeats every 5m after each iteration ends"))
    );
    let error = h
        .state
        .schedule_from_model(args(60, Some("poll"), None), "wake-turn-1")
        .expect_err("a fixed loop owns the wakeups");
    assert!(error.contains("a fixed-interval /loop owns this thread's wakeups; its next iteration starts 5m after this one ends"), "{error}");

    h.clock.advance(30);
    h.state.on_turn_finished("wake-turn-1");
    let wakeup = h.state.snapshot().wakeup.expect("next iteration");
    assert_eq!(wakeup.due_at, h.clock.now().timestamp() + 300);
    assert_eq!(
        h.beacon.get().and_then(|view| view.loop_iteration),
        Some((2, 2))
    );

    h.state.on_user_input("user-turn");
    assert!(h.state.snapshot().loop_state.is_some());

    assert!(h.state.mark_due(&wakeup.id));
    fire_if_due(Arc::clone(&h.state)).await;
    h.state.on_turn_finished("wake-turn-2");
    assert_eq!(h.state.snapshot().loop_state, None);
    let last = h.take_updates().pop().expect("an update");
    assert_eq!(
        (last.change, last.cancel_reason),
        (WakeupChange::LoopEnded, Some(CancelReason::Limit))
    );
    assert_eq!(
        h.state.take_notice(),
        Some(format!(
            "The /loop {id} ended after 2 iterations: the /loop reached its limit of 2 iterations. Nothing is scheduled now."
        ))
    );
}

/// R5.4, R2.5: a self-paced loop runs the next iteration only when the model schedules it,
/// with its prompt as the iteration's note, and ends `completed` otherwise; a call after the
/// limit is an error.
#[tokio::test]
async fn self_paced_loop_follows_the_model() {
    let h = Harness::new();
    h.state
        .start_loop(LoopStartRequest {
            prompt: "watch CI".to_string(),
            interval_seconds: None,
            max_iterations: Some(2),
        })
        .expect("loop started");
    let id = loop_id(&h);
    fire_if_due(Arc::clone(&h.state)).await;
    assert_eq!(
        h.state
            .schedule_from_model(args(120, Some("look at job 7"), None), "wake-turn-1")
            .map(|output| output.replace(&wakeup_id(&h), "wk-ID")),
        Ok("Next /loop iteration (2 of at most 2) scheduled for 2026-11-07T12:02:00Z (in 120 s). End your turn now.".to_string())
    );
    h.state.on_turn_finished("wake-turn-1");
    assert!(h.state.snapshot().loop_state.is_some());
    assert!(h.state.mark_due(&wakeup_id(&h)));
    fire_if_due(Arc::clone(&h.state)).await;
    let context = h.state.loop_context("wake-turn-2").expect("loop context");
    assert!(context.contains("self-paced"), "{context}");
    assert!(context.contains("look at job 7"), "{context}");
    assert_eq!(
        h.state
            .schedule_from_model(args(120, None, None), "wake-turn-2"),
        Err("Wakeup not scheduled: this /loop already ran its 2 iterations (maxIterations). Report to the user instead.".to_string())
    );
    h.state.on_turn_finished("wake-turn-2");
    assert_eq!(h.state.snapshot().loop_state, None);

    // Without a call the loop completes, without a notice.
    let h = Harness::new();
    h.state
        .start_loop(LoopStartRequest {
            prompt: "watch CI".to_string(),
            interval_seconds: None,
            max_iterations: None,
        })
        .expect("loop started");
    fire_if_due(Arc::clone(&h.state)).await;
    h.state.on_turn_finished("wake-turn-1");
    let last = h.take_updates().pop().expect("an update");
    assert_eq!(
        (last.change, last.cancel_reason),
        (WakeupChange::LoopEnded, Some(CancelReason::Completed))
    );
    assert_eq!(h.state.take_notice(), None);
    let _ = id;
}

/// R5.1, R4.5: a loop replaces a pending wakeup, and a new loop the old one, each with a
/// notice; R5.7: invalid parameters change nothing.
#[tokio::test]
async fn loop_start_replaces_and_validates() {
    let h = Harness::new();
    h.state
        .schedule_from_model(args(60, Some("poll"), None), "turn-1")
        .expect("scheduled");
    let id = wakeup_id(&h);
    let request = |prompt: &str, interval: Option<u32>, max: Option<u32>| LoopStartRequest {
        prompt: prompt.to_string(),
        interval_seconds: interval,
        max_iterations: max,
    };
    h.state
        .start_loop(request("check", Some(60), None))
        .expect("loop started");
    assert_eq!(
        h.state.take_notice(),
        Some(format!(
            "Your scheduled wakeup {id} (due 2026-11-07 12:01:00 UTC) was cancelled: the user started a new /loop. Nothing is scheduled now."
        ))
    );
    let before = h.state.snapshot();
    for (bad, error) in [
        (request(" ", None, None), "prompt must not be empty"),
        (
            request("check", Some(5), None),
            "intervalSeconds must be from 60 to 3600",
        ),
        (
            request("check", None, Some(0)),
            "maxIterations must be from 1 to 50",
        ),
        (
            request("check", None, Some(51)),
            "maxIterations must be from 1 to 50",
        ),
    ] {
        assert_eq!(h.state.start_loop(bad), Err(error.to_string()));
        assert_eq!(h.state.snapshot(), before);
    }
    let old = loop_id(&h);
    h.state
        .start_loop(request("check again", None, None))
        .expect("second loop");
    assert_eq!(
        h.state.take_notice(),
        Some(format!(
            "The /loop {old} ended after 0 iterations: the user started a new /loop. Nothing is scheduled now."
        ))
    );
}

/// R4.2, R4.5: a user cancel stops the wakeup with one notice; R4.4: a closed state
/// reports `threadClosed` and accepts nothing.
#[tokio::test]
async fn user_cancel_and_close() {
    let h = Harness::new();
    h.state
        .schedule_from_model(args(60, Some("poll"), None), "turn-1")
        .expect("scheduled");
    assert!(h.state.cancel(CancelReason::User).anything());
    assert!(
        h.state
            .take_notice()
            .is_some_and(|notice| notice.contains("the user cancelled it"))
    );
    h.state
        .schedule_from_model(args(60, Some("poll"), None), "turn-1")
        .expect("scheduled");
    h.state.close();
    let last = h.take_updates().pop().expect("an update");
    assert_eq!(last.cancel_reason, Some(CancelReason::ThreadClosed));
    assert_eq!(h.state.take_notice(), None);
    assert_eq!(h.beacon.get(), None);
    assert!(
        h.state
            .schedule_from_model(args(60, Some("poll"), None), "turn-1")
            .is_err()
    );
}
