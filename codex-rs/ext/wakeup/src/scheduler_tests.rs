use super::*;
use crate::state::CancelReason;
use crate::state::ScheduleArgs;
use crate::state::WakeupChange;
use crate::test_support::Harness;
use pretty_assertions::assert_eq;
use std::time::Duration;

fn schedule(h: &Harness, delay_seconds: u64, turn_id: &str) -> String {
    h.state
        .schedule_from_model(
            ScheduleArgs {
                delay_seconds,
                prompt: Some("check CI".to_string()),
                reason: Some("CI still running".to_string()),
            },
            turn_id,
        )
        .expect("scheduled");
    h.state.snapshot().wakeup.expect("pending").id
}

/// Lets spawned timer and fire tasks run.
async fn settle() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

/// R3.1, R3.5: a due wakeup of an idle thread starts exactly one turn with the wake input and
/// the turn's marker; clients hear `fired` with the turn id, and the chain counts it.
#[tokio::test(start_paused = true)]
async fn due_wakeup_starts_one_turn() {
    let h = Harness::new();
    let id = schedule(&h, 60, "turn-1");
    tokio::time::sleep(Duration::from_secs(59)).await;
    settle().await;
    assert!(h.host.requests().is_empty());
    tokio::time::sleep(Duration::from_secs(2)).await;
    settle().await;
    let requests = h.host.requests();
    assert_eq!(requests.len(), 1);
    let StartRequest::Wakeup { kind, body } = &requests[0] else {
        panic!("expected a wakeup start: {requests:?}");
    };
    assert_eq!(
        kind,
        &TurnKind::Wakeup {
            wakeup_id: id.clone()
        }
    );
    assert!(
        body.starts_with(&format!("Scheduled wakeup {id} is due.")),
        "{body}"
    );
    assert!(body.contains("check CI"), "{body}");
    assert!(
        !body.contains("CI still running"),
        "the reason stays out: {body}"
    );
    let fired = h
        .take_updates()
        .into_iter()
        .find(|update| update.change == WakeupChange::Fired)
        .expect("fired");
    assert_eq!(fired.turn_id.as_deref(), Some("wake-turn-1"));
    assert_eq!(fired.snapshot.wakeup, None);
    assert_eq!(fired.snapshot.chain.fired, 1);
    assert_eq!(h.beacon.get(), None);
    // A second attempt finds nothing due.
    fire_if_due(Arc::clone(&h.state)).await;
    assert_eq!(h.host.requests().len(), 1);
}

/// R3.2, R3.3: a busy, Plan-mode or interrupted thread holds a due wakeup, which the next
/// idle starts.
#[tokio::test(start_paused = true)]
async fn held_wakeup_fires_at_the_next_idle() {
    let h = Harness::new();
    h.host.push(StartOutcome::Held);
    schedule(&h, 60, "turn-1");
    tokio::time::sleep(Duration::from_secs(61)).await;
    settle().await;
    assert_eq!(h.host.requests().len(), 1);
    let wakeup = h.state.snapshot().wakeup.expect("still pending");
    assert!(wakeup.due);
    assert_eq!(h.state.inner().own_turn, None);
    fire_if_due(Arc::clone(&h.state)).await;
    assert_eq!(h.host.requests().len(), 2);
    assert_eq!(h.state.snapshot().wakeup, None);
}

/// R3.4: a draining host or a stopped runtime cancels with `threadClosed` without a turn;
/// R4.4: a timer of a dropped runtime starts nothing.
#[tokio::test(start_paused = true)]
async fn closed_host_or_runtime_starts_nothing() {
    let h = Harness::new();
    h.host.push(StartOutcome::Closed);
    schedule(&h, 60, "turn-1");
    tokio::time::sleep(Duration::from_secs(61)).await;
    settle().await;
    let last = h.take_updates().pop().expect("an update");
    assert_eq!(last.cancel_reason, Some(CancelReason::ThreadClosed));
    assert_eq!(h.state.snapshot().wakeup, None);

    let h = Harness::new();
    let id = schedule(&h, 60, "turn-1");
    let host = Arc::clone(&h.host);
    let weak = Arc::downgrade(&h.state);
    drop(h);
    tokio::time::sleep(Duration::from_secs(120)).await;
    settle().await;
    assert!(weak.upgrade().is_none());
    assert!(host.requests().is_empty());
    let _ = id;

    let h = Harness::new();
    let id = schedule(&h, 60, "turn-1");
    h.state.close();
    assert!(!h.state.mark_due(&id));
    tokio::time::sleep(Duration::from_secs(120)).await;
    settle().await;
    assert!(h.host.requests().is_empty());
}

/// R3.5: the timer and `on_thread_idle` at once start one turn.
#[tokio::test(start_paused = true)]
async fn concurrent_fires_start_one_turn() {
    let h = Harness::new();
    let id = schedule(&h, 60, "turn-1");
    assert!(h.state.mark_due(&id));
    tokio::join!(
        fire_if_due(Arc::clone(&h.state)),
        fire_if_due(Arc::clone(&h.state))
    );
    assert_eq!(h.host.requests().len(), 1);
}

/// R3.6: when the wall clock passes the due time before the monotonic deadline (the host
/// slept), the wakeup fires within 30 seconds.
#[tokio::test(start_paused = true)]
async fn wall_clock_due_time_fires_after_sleep() {
    let h = Harness::new();
    schedule(&h, 3_600, "turn-1");
    settle().await;
    h.clock.advance(3_600);
    tokio::time::sleep(Duration::from_secs(29)).await;
    settle().await;
    assert!(h.host.requests().is_empty());
    tokio::time::sleep(Duration::from_secs(2)).await;
    settle().await;
    assert_eq!(h.host.requests().len(), 1);
}

/// R3.8: three failed wakeup turns in a row cancel with `failures`; a completed one resets
/// the count.
#[tokio::test(start_paused = true)]
async fn three_failed_wakeup_turns_stop_the_wakeups() {
    let h = Harness::new();
    let mut turn = "turn-1".to_string();
    for (round, failed) in [true, true, false, true, true, true]
        .into_iter()
        .enumerate()
    {
        let id = schedule(&h, 60, &turn);
        assert!(h.state.mark_due(&id));
        fire_if_due(Arc::clone(&h.state)).await;
        turn = format!("wake-turn-{}", round + 1);
        // The wakeup turn schedules the next one, then ends.
        schedule(&h, 60, &turn);
        h.state.on_turn_finished(&turn);
        h.state.on_idle(failed);
        if round < 5 {
            assert!(h.state.snapshot().wakeup.is_some(), "round {round}");
            h.state.inner().chain.fired = 0;
        }
    }
    assert_eq!(h.state.snapshot().wakeup, None);
    let last = h.take_updates().pop().expect("an update");
    assert_eq!(last.cancel_reason, Some(CancelReason::Failures));
    assert!(
        h.state
            .take_notice()
            .is_some_and(|notice| notice.contains("3 wakeup turns in a row failed"))
    );
}
