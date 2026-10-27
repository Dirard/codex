use super::*;
use pretty_assertions::assert_eq;

fn terminal(process_id: i32, command: &str) -> JournaledTerminal {
    JournaledTerminal {
        process_id,
        command: command.to_string(),
        started_at_unix_ms: 1_700_000_000_000,
    }
}

/// Names of the files in the journal directory, sorted.
fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// D6: a session's journal lists its armed terminals, an empty list removes it, and once the
/// session's process is gone a resume takes it once, with every file of that session.
#[tokio::test]
async fn the_journal_is_rewritten_removed_and_taken_once() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread_id = ThreadId::new();
    let journal = TerminalJournal::new();

    journal.write(home.path(), thread_id, Vec::new()).await;
    assert_eq!(
        files(&dir),
        Vec::<String>::new(),
        "nothing armed, nothing written"
    );

    journal
        .write(home.path(), thread_id, vec![terminal(7, "./build.sh")])
        .await;
    journal
        .write(
            home.path(),
            thread_id,
            vec![terminal(7, "./build.sh"), terminal(9, "npm run dev")],
        )
        .await;
    let owner = journal.owner().to_string();
    assert_eq!(
        files(&dir),
        vec![
            format!("{thread_id}.{owner}.json"),
            format!("{thread_id}.{owner}.lock"),
        ]
    );
    journal.write(home.path(), thread_id, Vec::new()).await;
    assert_eq!(
        files(&dir),
        vec![format!("{thread_id}.{owner}.lock")],
        "an empty journal is removed"
    );
    journal
        .write(home.path(), thread_id, vec![terminal(9, "npm run dev")])
        .await;

    // The process dies: its lock goes with it, its files stay.
    drop(journal);
    let resumed = TerminalJournal::new();
    assert_eq!(
        resumed.take_lost(home.path(), thread_id).await,
        vec![terminal(9, "npm run dev")]
    );
    assert_eq!(files(&dir), Vec::<String>::new());
    assert_eq!(resumed.take_lost(home.path(), thread_id).await, Vec::new());
}

/// Tails review 2026-10-27, P3-2: a second process that opens a live thread, such as
/// `codex exec resume` next to the TUI, neither reports the first one's terminals as lost nor
/// touches its journal; each process journals and shuts down its own. Once the first one dies,
/// a resume reports exactly its terminals.
#[tokio::test]
async fn a_second_process_leaves_a_live_journal_alone() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread_id = ThreadId::new();
    let first = TerminalJournal::new();
    first
        .write(home.path(), thread_id, vec![terminal(1000, "npm run dev")])
        .await;
    let first_journal = journal_path(&dir, &thread_id.to_string(), first.owner());
    let written = fs::read(&first_journal).expect("first journal");

    let second = TerminalJournal::new();
    assert_eq!(second.take_lost(home.path(), thread_id).await, Vec::new());
    second
        .write(home.path(), thread_id, vec![terminal(1001, "./watch.sh")])
        .await;
    second.clear(home.path(), thread_id).await;
    assert_eq!(fs::read(&first_journal).expect("first journal"), written);

    drop(first);
    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        vec![terminal(1000, "npm run dev")]
    );
    assert_eq!(files(&dir), Vec::<String>::new());
}

/// P3-3: the journal names full commands, which may carry secrets, and a resume puts what it
/// reads into the model's context; the directory and files are the user's alone, whatever
/// the umask, also when an older version created the directory.
#[cfg(unix)]
#[tokio::test]
async fn the_journal_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    fs::create_dir(&dir).expect("old journal directory");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o775)).expect("old mode");
    let thread_id = ThreadId::new();
    let journal = TerminalJournal::new();
    journal
        .write(
            home.path(),
            thread_id,
            vec![terminal(7, "API_KEY=x npm run dev")],
        )
        .await;

    let mode = |path: &Path| fs::metadata(path).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    let thread = thread_id.to_string();
    assert_eq!(mode(&journal_path(&dir, &thread, journal.owner())), 0o600);
    assert_eq!(mode(&lock_path(&dir, &thread, journal.owner())), 0o600);
    assert!(
        files(&dir).iter().all(|name| !name.ends_with(TMP_SUFFIX)),
        "{:?}",
        files(&dir)
    );
}

/// P3-3: a resume reads only regular files; a symbolic link planted as a journal is not
/// followed.
#[cfg(unix)]
#[tokio::test]
async fn a_planted_link_is_not_followed() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    fs::create_dir(&dir).expect("journal directory");
    let target = home.path().join("elsewhere.json");
    fs::write(
        &target,
        serde_json::to_vec(&[terminal(5, "rm -rf ~")]).expect("json"),
    )
    .expect("target");
    let thread_id = ThreadId::new();
    std::os::unix::fs::symlink(&target, dir.join(format!("{thread_id}.json"))).expect("symlink");

    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        Vec::new()
    );
    assert!(target.exists());
}

/// P3-4: a resume reads at most `MAX_UNIFIED_EXEC_PROCESSES` terminals of a journal and none
/// of a file over `JOURNAL_MAX_BYTES`, and more than `MAX_LOST_NOTICES` lost terminals make
/// one summary instead of a notice each.
#[tokio::test]
async fn a_resume_reads_a_bounded_journal_and_sums_up_many_losses() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    fs::create_dir(&dir).expect("journal directory");
    let thread_id = ThreadId::new();
    let many: Vec<JournaledTerminal> = (0..5000).map(|id| terminal(id, "sleep 1")).collect();
    fs::write(
        dir.join(format!("{thread_id}.json")),
        serde_json::to_vec(&many[..1000]).expect("json"),
    )
    .expect("journal");
    let lost = TerminalJournal::new()
        .take_lost(home.path(), thread_id)
        .await;
    assert_eq!(lost, many[..MAX_UNIFIED_EXEC_PROCESSES].to_vec());

    fs::write(
        dir.join(format!("{thread_id}.json")),
        serde_json::to_vec(&many).expect("json"),
    )
    .expect("journal");
    assert!(
        fs::metadata(dir.join(format!("{thread_id}.json")))
            .expect("size")
            .len()
            > JOURNAL_MAX_BYTES
    );
    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        Vec::new()
    );

    assert_eq!(
        lost_notices(&many[..MAX_LOST_NOTICES]).len(),
        MAX_LOST_NOTICES
    );
    let summary = lost_notices(&lost);
    assert_eq!(summary.len(), 1);
    assert!(
        summary[0].starts_with(&format!(
            "{MAX_UNIFIED_EXEC_PROCESSES} background terminal sessions were lost: "
        )),
        "{}",
        summary[0]
    );
    assert!(summary[0].contains(": 0, 1, 2, "), "{}", summary[0]);
    let summary = lost_notices(&many[..100]);
    assert!(summary[0].contains("63, and 36 more."), "{}", summary[0]);
}

/// P3-5: journals nobody resumes do not pile up. Archiving or deleting a thread removes the
/// journals of its dead processes; the sweep removes old files of dead processes and the
/// temporary files a crash left behind. Files of live processes stay.
#[tokio::test]
async fn journals_of_dead_processes_are_cleaned_up() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let archived = ThreadId::new();
    let other = ThreadId::new();

    let live = TerminalJournal::new();
    live.write(home.path(), archived, vec![terminal(1, "npm run dev")])
        .await;
    let dead = TerminalJournal::new();
    dead.write(home.path(), archived, vec![terminal(2, "./build.sh")])
        .await;
    let dead_other = TerminalJournal::new();
    dead_other
        .write(home.path(), other, vec![terminal(3, "make")])
        .await;
    let dead_owner = dead.owner().to_string();
    let dead_other_owner = dead_other.owner().to_string();
    drop(dead);
    drop(dead_other);
    let crashed_tmp = format!("{other}.{dead_other_owner}.json.a1b2c3.tmp");
    fs::write(dir.join(&crashed_tmp), b"[").expect("tmp");

    discard_journals(home.path(), &[archived]).await;
    let live_files = vec![
        format!("{archived}.{}.json", live.owner()),
        format!("{archived}.{}.lock", live.owner()),
    ];
    let mut expected = live_files.clone();
    expected.extend([
        crashed_tmp.clone(),
        format!("{other}.{dead_other_owner}.json"),
        format!("{other}.{dead_other_owner}.lock"),
    ]);
    expected.sort();
    assert_eq!(files(&dir), expected);
    assert!(!files(&dir).iter().any(|name| name.contains(&dead_owner)));

    // A young dead journal keeps its journal for a resume; its temporary file goes.
    sweep(&dir, JOURNAL_MAX_AGE, SystemTime::now());
    let mut expected = live_files.clone();
    expected.extend([
        format!("{other}.{dead_other_owner}.json"),
        format!("{other}.{dead_other_owner}.lock"),
    ]);
    expected.sort();
    assert_eq!(files(&dir), expected);

    // An old one goes; the live one stays, whatever its age.
    sweep(
        &dir,
        JOURNAL_MAX_AGE,
        SystemTime::now() + JOURNAL_MAX_AGE + Duration::from_secs(60),
    );
    assert_eq!(files(&dir), live_files);
}

#[test]
fn journal_file_names_are_recognized() {
    assert_eq!(
        JournalFile::parse("t.123-abc.lock"),
        Some(JournalFile::Lock {
            thread: "t",
            owner: "123-abc"
        })
    );
    assert_eq!(
        JournalFile::parse("t.123-abc.json"),
        Some(JournalFile::Journal {
            thread: "t",
            owner: "123-abc"
        })
    );
    assert_eq!(
        JournalFile::parse("t.123-abc.json.Xy9.tmp"),
        Some(JournalFile::Tmp {
            thread: "t",
            owner: "123-abc"
        })
    );
    assert_eq!(
        JournalFile::parse("t.json"),
        Some(JournalFile::Legacy { thread: "t" })
    );
    assert_eq!(JournalFile::parse("README"), None);
}

/// P3-6: the process may outlive Codex (a server that ignores SIGTERM, or any child outside
/// Linux), so the notice asks to check for it before starting the command again.
#[test]
fn a_lost_terminal_says_what_happened() {
    assert_eq!(
        terminal(9809, "./build.sh <x>").lost_notice(),
        "Background terminal session 9809 was lost: the Codex process that ran it stopped without shutting down, so its exit status and unread output are unknown and its session ID no longer works. The process most likely stopped with Codex, but it may still run, for example a server that still holds its port; if you need the command again, check for the process or its port first, and stop a leftover process before you start the command again.\nCommand: ./build.sh &lt;x>"
    );
}
