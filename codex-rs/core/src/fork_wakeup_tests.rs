use super::*;
use chrono::TimeZone;
use pretty_assertions::assert_eq;

fn due() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 7, 12, 5, 0).unwrap()
}

/// R6.1: the completion note names the due time, or the loop's next iteration.
#[test]
fn completion_note_names_the_wakeup_or_the_loop_iteration() {
    assert_eq!(
        completion_note(&PendingWakeupView {
            due_at: due(),
            loop_iteration: None,
        }),
        "It scheduled a wakeup for 2026-11-07T12:05:00Z: it resumes by itself and reports again; wait_agent waits for it, close_agent stops it."
    );
    assert_eq!(
        completion_note(&PendingWakeupView {
            due_at: due(),
            loop_iteration: Some((4, 50)),
        }),
        "Its /loop runs iteration 4 of at most 50 at 12:05:00Z; it resumes by itself and reports again; wait_agent waits for it, close_agent stops it."
    );
}

/// R6.2: a `wait_agent` timeout lists the descendants with wakeups and their next due time.
#[test]
fn wait_timeout_note_lists_agents_with_wakeups() {
    assert_eq!(wait_timeout_note(&[]), "");
    assert_eq!(
        wait_timeout_note(&[(
            "/root/ci_watch".to_string(),
            PendingWakeupView {
                due_at: due(),
                loop_iteration: None,
            },
        )]),
        " Agents with scheduled wakeups: /root/ci_watch (next at 12:05:00Z). Each reports again after its wakeup; close_agent stops it."
    );
}

/// UC6: the beacon in a thread store is what core reads; no beacon means no wakeup.
#[test]
fn pending_wakeup_reads_the_beacon_of_the_store() {
    let store = ExtensionData::new("thread");
    assert_eq!(pending_wakeup(&store), None);
    let beacon = store.get_or_init::<WakeupBeacon>(WakeupBeacon::default);
    let view = PendingWakeupView {
        due_at: due(),
        loop_iteration: None,
    };
    beacon.set(Some(view));
    assert_eq!(pending_wakeup(&store), Some(view));
    beacon.set(None);
    assert_eq!(pending_wakeup(&store), None);
}
