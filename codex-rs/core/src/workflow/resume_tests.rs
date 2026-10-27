use super::*;
use codex_workflow::RESULT_FILE;
use pretty_assertions::assert_eq;

const STARTED: &str = r#"{"seq":1,"kind":"run_started","run_id":"wf-0000abcd","name":"stopped","script_sha256":"s","args_sha256":"a","parent_turn_id":"t","resume_from":null}"#;
const FINISHED: &str = r#"{"seq":2,"kind":"run_finished","status":"completed","result_bytes":9,"tokens":{"charged":0,"cached":0},"error":null}"#;

fn run_dir(journal: &[&str]) -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = home.path().join("wf-0000abcd");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join(JOURNAL_FILE),
        journal
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .expect("journal");
    (home, dir)
}

fn run_finished_count(dir: &Path) -> usize {
    read_records(dir)
        .expect("records")
        .iter()
        .filter(|record| matches!(record.event, JournalEvent::RunFinished { .. }))
        .count()
}

/// R6.4, R6.7: two lookups of the same stopped run in one process, such as `workflow_status`
/// and `workflow {resume_from}` in one model response, mark it interrupted once.
#[test]
fn a_stopped_run_is_marked_interrupted_once() {
    let (_home, dir) = run_dir(&[STARTED]);
    mark_interrupted(&dir).expect("mark");
    mark_interrupted(&dir).expect("mark again");
    assert_eq!(run_finished_count(&dir), 1);

    let (_home, dir) = run_dir(&[STARTED]);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| mark_interrupted(&dir).expect("mark"));
        }
    });
    assert_eq!(run_finished_count(&dir), 1);
}

/// R6.1, R6.7: a `result.json` that is not JSON, as a crash while writing it would leave
/// one, is not served as the run's result.
#[test]
fn a_cut_result_file_is_not_served() {
    let (_home, dir) = run_dir(&[STARTED, FINISHED]);
    std::fs::write(dir.join(RESULT_FILE), "{\"items\":[").expect("result");
    let run = load_run("wf-0000abcd", &dir).expect("load");
    assert_eq!(run.result_json, None);
    assert_eq!(run.tracker.snapshot(false).result_preview, None);

    std::fs::write(dir.join(RESULT_FILE), "{\"items\":[]}").expect("result");
    let run = load_run("wf-0000abcd", &dir).expect("load");
    assert_eq!(run.result_json.as_deref(), Some("{\"items\":[]}"));
}

fn write_run(thread_dir: &Path, run_id: &str, journal: &[String]) -> PathBuf {
    let dir = thread_dir.join(run_id);
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(
        dir.join(JOURNAL_FILE),
        journal
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .expect("journal");
    dir
}

fn started(run_id: &str, resume_from: Option<&str>) -> String {
    serde_json::json!({
        "seq": 1, "kind": "run_started", "run_id": run_id, "name": run_id,
        "script_sha256": "s", "args_sha256": "a", "parent_turn_id": "t",
        "resume_from": resume_from, "started_at_ms": 5,
    })
    .to_string()
}

fn finished_entry(run_id: &str) -> RunSnapshot {
    let mut tracker = RunTracker::new(
        run_id.to_string(),
        run_id.to_string(),
        String::new(),
        &[],
        /*budget_tokens*/ None,
        /*started_at_ms*/ 1,
    );
    tracker.finish(RunStatus::Completed, 2, None, None);
    tracker.snapshot(/*include_agents*/ false)
}

/// R6.4, R6.8: the load takes indexed runs from the index without reading their journals,
/// indexes a finished journal of a version before the index and the stopped runs it marks,
/// and registers interrupted runs that no other run resumed. An index entry without a run
/// directory is not listed. The next load finds every run in the index.
#[test]
fn past_runs_load_from_the_index() {
    let home = tempfile::tempdir().expect("tempdir");
    let thread_dir = home.path();
    // An indexed run whose journal is unreadable: reading it would mark it interrupted.
    let indexed_dir = write_run(thread_dir, "wf-0000000a", &["not json".to_string()]);
    append_index(thread_dir, &finished_entry("wf-0000000a")).expect("index");
    append_index(thread_dir, &finished_entry("wf-0000000f")).expect("index");
    // A finished run of a version before the index.
    write_run(
        thread_dir,
        "wf-0000000b",
        &[started("wf-0000000b", None), FINISHED.to_string()],
    );
    // A stopped run, and a stopped run that a later run resumed.
    write_run(thread_dir, "wf-0000000c", &[started("wf-0000000c", None)]);
    write_run(thread_dir, "wf-0000000d", &[started("wf-0000000d", None)]);
    write_run(
        thread_dir,
        "wf-0000000e",
        &[
            started("wf-0000000e", Some("wf-0000000d")),
            FINISHED.to_string(),
        ],
    );

    let restored = restore_runs(thread_dir);

    assert_eq!(
        std::fs::read_to_string(indexed_dir.join(JOURNAL_FILE)).expect("journal"),
        "not json\n",
        "an indexed run's journal is neither read nor marked"
    );
    let statuses = |past: &[RunSnapshot]| {
        let mut statuses: Vec<(String, RunStatus)> = past
            .iter()
            .map(|entry| (entry.run_id.clone(), entry.status))
            .collect();
        statuses.sort_by(|left, right| left.0.cmp(&right.0));
        statuses
    };
    let expected = vec![
        ("wf-0000000a".to_string(), RunStatus::Completed),
        ("wf-0000000b".to_string(), RunStatus::Completed),
        ("wf-0000000c".to_string(), RunStatus::Interrupted),
        ("wf-0000000d".to_string(), RunStatus::Interrupted),
        ("wf-0000000e".to_string(), RunStatus::Completed),
    ];
    assert_eq!(statuses(&restored.past), expected);
    let registered =
        |runs: &[RestoredRun]| -> Vec<String> { runs.iter().map(|run| run.id.clone()).collect() };
    assert_eq!(registered(&restored.interrupted), vec!["wf-0000000c"]);

    let mut indexed: Vec<String> = read_index(thread_dir)
        .expect("index")
        .into_iter()
        .map(|entry| entry.run_id)
        .filter(|run_id| run_id != "wf-0000000f")
        .collect();
    indexed.sort();
    assert_eq!(
        indexed,
        expected
            .iter()
            .map(|(run_id, _)| run_id.clone())
            .collect::<Vec<_>>()
    );
    let again = restore_runs(thread_dir);
    assert_eq!(statuses(&again.past), expected);
    assert_eq!(registered(&again.interrupted), vec!["wf-0000000c"]);
}
