use codex_workflow::RunStatus;
use codex_workflow::RunTracker;
use pretty_assertions::assert_eq;

use super::*;

fn tracker(run_id: &str) -> RunTracker {
    RunTracker::new(
        run_id.to_string(),
        "name".to_string(),
        "description".to_string(),
        &[],
        /*budget_tokens*/ None,
        /*started_at_ms*/ 0,
    )
}

fn revision(snapshot: Option<RunSnapshot>) -> Option<u64> {
    snapshot.map(|snapshot| snapshot.revision)
}

/// R8.2: the first change goes out at once, later ones at most every 250 ms, and an
/// unchanged revision is never sent twice.
#[test]
fn active_runs_are_sent_at_most_once_per_interval() {
    let start = Instant::now();
    let mut pacer = UpdatePacer::default();
    let mut run = tracker("wf-1");

    assert_eq!(revision(pacer.offer(run.snapshot(false), start)), Some(1));
    assert_eq!(
        pacer.offer(run.snapshot(false), start),
        None,
        "same revision"
    );
    assert_eq!(pacer.next_deadline(), None);

    run.log("one");
    let early = start + Duration::from_millis(100);
    assert_eq!(pacer.offer(run.snapshot(false), early), None);
    assert_eq!(pacer.next_deadline(), Some(start + MIN_UPDATE_INTERVAL));
    assert!(pacer.take_due_runs(early).is_empty());

    run.log("two");
    let due = start + MIN_UPDATE_INTERVAL;
    assert_eq!(pacer.take_due_runs(due), vec!["wf-1".to_string()]);
    assert_eq!(
        pacer.next_deadline(),
        None,
        "a taken run waits until it is offered again"
    );
    assert_eq!(revision(pacer.offer(run.snapshot(false), due)), Some(3));
    assert_eq!(pacer.next_deadline(), None);
}

/// R8.2: the terminal snapshot skips the interval and is sent exactly once.
#[test]
fn terminal_snapshot_is_sent_at_once_and_once() {
    let start = Instant::now();
    let mut pacer = UpdatePacer::default();
    let mut run = tracker("wf-1");
    let mut other = tracker("wf-2");

    assert!(pacer.offer(run.snapshot(false), start).is_some());
    assert!(pacer.offer(other.snapshot(false), start).is_some());
    run.log("pending");
    other.log("pending");
    assert_eq!(pacer.offer(run.snapshot(false), start), None);
    assert_eq!(pacer.offer(other.snapshot(false), start), None);

    run.finish(RunStatus::Cancelled, 10, None, None);
    let terminal = pacer
        .offer(run.snapshot(false), start + Duration::from_millis(1))
        .expect("terminal snapshot");
    assert_eq!(terminal.status, RunStatus::Cancelled);
    let later = start + Duration::from_secs(1);
    assert_eq!(pacer.offer(run.snapshot(false), later), None);
    assert_eq!(
        pacer.take_due_runs(later),
        vec!["wf-2".to_string()],
        "only the other run still waits"
    );
}
