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

    // A young dead journal stays for a resume, with its temporary file: the sweep does not
    // even lock an owner whose files are young (review of the fixes, P1-1).
    sweep(&dir, JOURNAL_MAX_AGE, SystemTime::now());
    assert_eq!(files(&dir), expected);

    // An old one goes, temporary file included; the live owners stay, whatever their age:
    // a session of this process, never opened, and one of another process, locked.
    let foreign_live = "4242-0badcafe";
    fs::write(dir.join(format!("{other}.{foreign_live}.json")), b"[]").expect("journal");
    let foreign_lock =
        fs::File::create(dir.join(format!("{other}.{foreign_live}.lock"))).expect("lock file");
    foreign_lock.lock().expect("held by another process");
    sweep(
        &dir,
        JOURNAL_MAX_AGE,
        SystemTime::now() + JOURNAL_MAX_AGE + Duration::from_secs(60),
    );
    let mut expected = live_files;
    expected.extend([
        format!("{other}.{foreign_live}.json"),
        format!("{other}.{foreign_live}.lock"),
    ]);
    expected.sort();
    assert_eq!(files(&dir), expected);
}

/// Plants the journal and lock file of a dead owner of `thread`: a process killed while one
/// terminal was armed.
fn plant_dead_owner(dir: &Path, thread: ThreadId, owner: &str, process_id: i32) {
    fs::create_dir_all(dir).expect("journal directory");
    fs::write(
        dir.join(format!("{thread}.{owner}.json")),
        serde_json::to_vec(&[terminal(process_id, "./build.sh")]).expect("json"),
    )
    .expect("journal");
    fs::write(dir.join(format!("{thread}.{owner}.lock")), b"").expect("lock file");
}

/// Review of the fixes P1-1, live recheck 2 R1: the first resume in a new process ran the
/// once-per-process sweep next to itself, the sweep held the dead owner's lock while it checked
/// its age, and the resume took that for a live owner (162 of 200 missed). Each round here is a
/// new process: the sweep has not run yet.
#[tokio::test]
async fn the_first_resume_of_a_new_process_takes_a_dead_journal() {
    for round in 0..200 {
        let home = tempfile::tempdir().expect("tempdir");
        let dir = journal_dir(home.path());
        let thread_id = ThreadId::new();
        plant_dead_owner(&dir, thread_id, "1-deadbeef", 7);
        SWEPT.store(false, Ordering::Release);
        assert_eq!(
            TerminalJournal::new()
                .take_lost(home.path(), thread_id)
                .await,
            vec![terminal(7, "./build.sh")],
            "round {round}"
        );
        assert_eq!(files(&dir), Vec::<String>::new(), "round {round}");
    }
}

/// Review of the fixes P1-1, P2-1: this process never opens the lock file of one of its own
/// sessions, whether that session holds its lock or not: a resume and the sweep of the same
/// process skip it, so its lock is never taken for a dead owner's, and on NFS, where closing
/// a second descriptor would release a per-process lock, never lost. A session is registered
/// from its first armed terminal until it ends.
#[tokio::test]
async fn this_process_never_probes_its_own_sessions() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread_id = ThreadId::new();
    let session = TerminalJournal::new();
    assert!(!is_own_session(session.owner()));
    session
        .write(home.path(), thread_id, vec![terminal(1, "npm run dev")])
        .await;
    assert!(is_own_session(session.owner()));

    // A registered owner whose lock is free, as if this process held it in another way.
    let registered = "4242-00c0ffee";
    plant_dead_owner(&dir, thread_id, registered, 2);
    lock_ignoring_poison(&OWN_SESSIONS).insert(registered.to_string());
    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        Vec::new()
    );
    sweep(
        &dir,
        JOURNAL_MAX_AGE,
        SystemTime::now() + JOURNAL_MAX_AGE + Duration::from_secs(60),
    );
    assert_eq!(files(&dir).len(), 4, "{:?}", files(&dir));
    let probe = open_foreign_lock(&lock_path(&dir, &thread_id.to_string(), session.owner()))
        .expect("lock file");
    assert!(matches!(
        probe.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    lock_ignoring_poison(&OWN_SESSIONS).remove(registered);

    session.clear(home.path(), thread_id).await;
    assert!(!is_own_session(session.owner()));
    let dropped = TerminalJournal::new();
    dropped
        .write(home.path(), thread_id, vec![terminal(3, "make")])
        .await;
    let owner = dropped.owner().to_string();
    drop(dropped);
    assert!(!is_own_session(&owner));
}

/// Review of the fixes P1-1: another process's resume or sweep holds a dead owner's lock for a
/// moment. A resume tries again before it takes the owner for a live one.
#[tokio::test]
async fn a_dead_owner_is_taken_after_a_brief_foreign_lock() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread_id = ThreadId::new();
    plant_dead_owner(&dir, thread_id, "2-deadbeef", 9);
    let lock = open_foreign_lock(&lock_path(&dir, &thread_id.to_string(), "2-deadbeef"))
        .expect("lock file");
    lock.lock().expect("live foreign owner");
    // While the other process holds the lock, its owner is alive.
    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        Vec::new()
    );
    assert_eq!(files(&dir).len(), 2);
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(30));
        drop(lock);
    });
    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        vec![terminal(9, "./build.sh")]
    );
    release.join().expect("release");
}

/// Review of the fixes P3-3: a journal of the current format without its lock file is not a
/// dead owner's. The listing may have caught the journal of a new owner without the lock file
/// it created first; it is left alone until its lock file is there or the sweep finds it old.
#[tokio::test]
async fn a_journal_without_its_lock_file_is_left_alone() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread_id = ThreadId::new();
    plant_dead_owner(&dir, thread_id, "3-deadbeef", 11);
    fs::remove_file(dir.join(format!("{thread_id}.3-deadbeef.lock"))).expect("remove lock");
    assert_eq!(
        TerminalJournal::new()
            .take_lost(home.path(), thread_id)
            .await,
        Vec::new()
    );
    assert_eq!(files(&dir), vec![format!("{thread_id}.3-deadbeef.json")]);
}

/// Review of the fixes P3-1: a journal write whose call is dropped mid-way (the tool call of
/// an interrupted turn is aborted) still leaves the session holding its lock, so no other
/// process takes the live journal for a dead one, and the shutdown still removes the journal
/// and its lock file.
#[tokio::test]
async fn a_cancelled_write_keeps_the_session_lock() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread_id = ThreadId::new();
    let session = TerminalJournal::new();
    // Polled once, up to the blocking task, then dropped.
    let _cancelled = tokio::time::timeout(
        Duration::ZERO,
        session.write(home.path(), thread_id, vec![terminal(1, "npm run dev")]),
    )
    .await;
    let lock_file = lock_path(&dir, &thread_id.to_string(), session.owner());
    let journal_file = journal_path(&dir, &thread_id.to_string(), session.owner());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !journal_file.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the blocking write finished");
    // The journal is complete once it exists (rename); the lock is still held.
    let probe = open_foreign_lock(&lock_file).expect("lock file");
    assert!(
        matches!(probe.try_lock(), Err(fs::TryLockError::WouldBlock)),
        "the live session lost its lock"
    );
    drop(probe);

    session.clear(home.path(), thread_id).await;
    assert_eq!(files(&dir), Vec::<String>::new());
}

/// Review of the fixes P3-2: lists reach the disk in the order they were taken. A write of an
/// older list that arrives after a newer one is dropped, and a list is taken under the
/// session's lock, so a slow snapshot keeps the next one waiting.
#[tokio::test]
async fn an_older_list_never_replaces_a_newer_one() {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    let thread = ThreadId::new().to_string();
    let session = TerminalJournal::new();
    {
        let mut files = lock_ignoring_poison(&session.files);
        files.write(&dir, &thread, 2, &[terminal(2, "./b")]);
        files.write(&dir, &thread, 1, &[terminal(1, "./a"), terminal(2, "./b")]);
    }
    let journal = journal_path(&dir, &thread, session.owner());
    assert_eq!(read_journal(&journal), vec![terminal(2, "./b")]);

    let thread_id = ThreadId::new();
    let session = Arc::new(TerminalJournal::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let first = tokio::spawn({
        let session = Arc::clone(&session);
        let release = Arc::clone(&release);
        let home = home.path().to_path_buf();
        async move {
            session
                .sync(&home, thread_id, async move {
                    release.notified().await;
                    vec![terminal(1, "./a"), terminal(2, "./b")]
                })
                .await;
        }
    });
    // The first snapshot is being taken under the session's lock.
    while session.snapshots.try_lock().is_ok() {
        tokio::task::yield_now().await;
    }
    let second = tokio::spawn({
        let session = Arc::clone(&session);
        let home = home.path().to_path_buf();
        async move {
            session
                .sync(&home, thread_id, async { vec![terminal(2, "./b")] })
                .await;
        }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    release.notify_one();
    first.await.expect("first sync");
    second.await.expect("second sync");
    assert_eq!(
        read_journal(&journal_path(&dir, &thread_id.to_string(), session.owner())),
        vec![terminal(2, "./b")]
    );
    session.clear(home.path(), thread_id).await;
}

/// Review of the fixes P3-8 (`is_file_at`): a resume of another process that found the owner's
/// fresh lock file unlocked removes it, while the owner waits for that lock. The owner then
/// holds a removed file, so it takes the lock again on a new one.
#[cfg(unix)]
#[test]
fn the_owner_takes_its_lock_again_when_its_lock_file_was_removed() {
    use std::os::unix::fs::MetadataExt;
    let home = tempfile::tempdir().expect("tempdir");
    let dir = journal_dir(home.path());
    fs::create_dir_all(&dir).expect("journal directory");
    let path = lock_path(&dir, "thread", "5-0000beef");
    let foreign = fs::File::create(&path).expect("lock file");
    foreign.lock().expect("foreign resume holds the lock");
    let owner = std::thread::spawn(move || acquire_owner_lock(&dir, "thread", "5-0000beef"));
    std::thread::sleep(Duration::from_millis(100));
    fs::remove_file(&path).expect("the foreign resume removes the lock file");
    drop(foreign);
    let lock = owner
        .join()
        .expect("owner thread")
        .expect("the owner takes its lock");
    let held = lock.metadata().expect("held file");
    let current = fs::symlink_metadata(&path).expect("the lock file is there again");
    assert_eq!(
        (held.dev(), held.ino()),
        (current.dev(), current.ino()),
        "the owner holds the file at the lock path"
    );
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
