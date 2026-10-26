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
