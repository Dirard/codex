use super::*;
use crate::snapshot::RunStatus;
use crate::snapshot::RunTracker;
use pretty_assertions::assert_eq;

fn finished_run(run_id: &str, status: RunStatus) -> RunSnapshot {
    let mut tracker = RunTracker::new(
        run_id.to_string(),
        "find-bugs".to_string(),
        "Find bugs".to_string(),
        &["Find".to_string()],
        Some(1_000),
        /*started_at_ms*/ 10,
    );
    tracker.agent_queued(1, Some("find".to_string()), Some("Find".to_string()));
    tracker.log("working");
    tracker.finish(status, 20, Some("\"result\"".to_string()), None);
    tracker.snapshot(/*include_agents*/ true)
}

/// R6.8: an index line drops the per-call list, the log tail and the result preview and
/// keeps the rest of the summary.
#[test]
fn index_entries_are_summaries() {
    let snapshot = finished_run("wf-00000001", RunStatus::Completed);
    assert!(!snapshot.agent_list.is_empty());
    let entry = index_entry(&snapshot);
    assert_eq!(
        entry,
        RunSnapshot {
            agent_list: Vec::new(),
            logs_tail: Vec::new(),
            result_preview: None,
            ..snapshot
        }
    );
}

/// R6.8: the last line of each run wins in first-appearance order; a cut line, a foreign
/// line and a line with an invalid run id are skipped; a missing index is empty.
#[test]
fn index_keeps_the_last_entry_of_each_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(read_index(dir.path()).expect("missing index"), Vec::new());

    let first = finished_run("wf-00000001", RunStatus::Interrupted);
    let second = finished_run("wf-00000002", RunStatus::Completed);
    let first_again = finished_run("wf-00000001", RunStatus::Completed);
    append_index(dir.path(), &first).expect("append");
    append_index(dir.path(), &second).expect("append");
    let mut text = std::fs::read_to_string(dir.path().join(INDEX_FILE)).expect("index");
    text.push_str("{\"not\": \"a run\"}\n");
    let mut invalid = index_entry(&second);
    invalid.run_id = "../escape".to_string();
    text.push_str(&serde_json::to_string(&invalid).expect("json"));
    text.push('\n');
    std::fs::write(dir.path().join(INDEX_FILE), text).expect("write");
    append_index(dir.path(), &first_again).expect("append");
    let mut text = std::fs::read_to_string(dir.path().join(INDEX_FILE)).expect("index");
    let cut = serde_json::to_string(&index_entry(&second)).expect("json");
    text.push_str(&cut[..cut.len() / 2]);
    std::fs::write(dir.path().join(INDEX_FILE), text).expect("write");

    assert_eq!(
        read_index(dir.path()).expect("index"),
        vec![index_entry(&first_again), index_entry(&second)]
    );
}

/// P3-11: a line is taken only as a journal could have produced it: a run that has not
/// finished is not listed, and lists, tail, preview and free text keep the limits of a run.
#[test]
fn index_lines_are_checked_when_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut running = finished_run("wf-00000001", RunStatus::Completed);
    running.status = RunStatus::Running;
    let mut oversized = finished_run("wf-00000002", RunStatus::Failed);
    oversized.name = "n".repeat(500);
    oversized.description = "d".repeat(5_000);
    oversized.error = Some("e".repeat(5_000));
    oversized.logs_tail = vec!["x".repeat(10); 50];
    oversized.result_preview = Some("p".repeat(5_000));
    oversized.null_reasons = (0..1_000)
        .map(|index| (format!("{index}{}", "r".repeat(100)), 1))
        .collect();
    let text = [&running, &oversized]
        .iter()
        .map(|line| format!("{}\n", serde_json::to_string(line).expect("json")))
        .collect::<String>();
    std::fs::write(dir.path().join(INDEX_FILE), text).expect("write");

    let entries = read_index(dir.path()).expect("index");
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.run_id.as_str())
            .collect::<Vec<_>>(),
        vec!["wf-00000002"]
    );
    let entry = &entries[0];
    assert_eq!(entry.run_id, "wf-00000002");
    assert_eq!(entry.name.chars().count(), 64);
    assert_eq!(entry.description.chars().count(), 2_000);
    assert_eq!(
        entry.error.as_deref().map(|error| error.chars().count()),
        Some(2_000)
    );
    assert!(entry.logs_tail.is_empty() && entry.agent_list.is_empty());
    assert_eq!(entry.result_preview, None);
    assert_eq!(entry.null_reasons.len(), 256);
    assert!(
        entry
            .null_reasons
            .keys()
            .all(|reason| reason.chars().count() <= 64)
    );
}
