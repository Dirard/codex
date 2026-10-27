//! Fork: a journal of the armed background terminals of a thread, so that a thread resumed
//! after its Codex process died tells the model which terminals it lost (combined live run
//! 2026-10-27, D6; `specs/background-exec-notifications-2026-10-25.md`, R2.11, R2.14).
//!
//! A shutdown records a notice for every armed terminal it stops (R2.5). A process killed
//! with `kill -9` or a crash records nothing, and the model used to learn about the loss only
//! by polling a session ID that no longer existed, or never, if it waited for the exit
//! notification. The journal lists the armed terminals that have not reported yet; it is
//! rewritten when one is armed or reports and removed when the list is empty or a shutdown
//! recorded its notices. A resumed thread that finds the journal of a dead process records
//! notices for its terminals and removes the journal.
//!
//! Every session of a thread journals into files of its own (tails review 2026-10-27, P3-2),
//! in `$CODEX_HOME/background_terminals/`, a directory only the user can read (`0700`, files
//! `0600`, P3-3):
//!
//! - `<thread>.<owner>.lock`: the session holds an exclusive lock on it while it journals;
//!   the kernel drops the lock when the process dies, as for agent worktrees;
//! - `<thread>.<owner>.json`: its armed terminals, written to a unique temporary file
//!   `<thread>.<owner>.json.<random>.tmp` and renamed over it.
//!
//! A resume takes only the journals whose lock is free, so a second process that opens a
//! live thread neither reports the first one's terminals as lost nor touches its files.
//! Journals of dead processes are also removed when their thread is archived or deleted and,
//! when no resume took them, after `JOURNAL_MAX_AGE` (P3-5).
//!
//! A lock this process holds is no sign of a live owner in another process (review of the
//! fixes, P1-1, P2-1). The process keeps a registry of its own live sessions, whose lock files
//! its resume, sweep and cleanup never open (on NFS, where `flock` is a per-process `fcntl`
//! lock, opening and closing one would even release it), and it runs that work on other
//! owners' files one at a time (`FOREIGN_WORK`), so the sweep never holds a lock that a resume
//! of the same process is probing.

use std::collections::BTreeSet;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::SystemTime;

use codex_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;
use tokio::time::Instant;
use tracing::warn;

use super::ExitNotifier;
use super::MAX_UNIFIED_EXEC_PROCESSES;
use super::command_line;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::session::session::Session;

const JOURNAL_DIR: &str = "background_terminals";
/// Largest journal file a resume reads (P3-4).
const JOURNAL_MAX_BYTES: u64 = 256 * 1024;
/// Lost terminals a resume reports one notice each; more go into one summary (P3-4).
const MAX_LOST_NOTICES: usize = 8;
/// Session IDs one summary lists.
const MAX_SUMMARY_SESSION_IDS: usize = 64;
/// Age after which the files of a dead process that no resume took are removed (P3-5).
const JOURNAL_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The old version kept one journal per thread, `<thread>.json`, without an owner.
const LEGACY_SUFFIX: &str = "json";
const LOCK_SUFFIX: &str = ".lock";
const JOURNAL_SUFFIX: &str = ".json";
const TMP_SUFFIX: &str = ".tmp";

/// The sweep of old journals runs once per process.
static SWEPT: AtomicBool = AtomicBool::new(false);
/// Owners of this process's sessions that hold, or are taking, their journal lock (P1-1,
/// P2-1). An owner is registered before its lock file exists and unregistered after its lock
/// is released.
static OWN_SESSIONS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// This process's work on other owners' files, one at a time: a resume, the sweep, the
/// cleanup of an archived or deleted thread (P1-1).
static FOREIGN_WORK: Mutex<()> = Mutex::new(());
/// Attempts at a free lock before an owner counts as alive: another process's resume or sweep
/// holds a dead owner's lock for a moment, a live owner for its whole session.
const FOREIGN_LOCK_ATTEMPTS: u32 = 5;
const FOREIGN_LOCK_RETRY: Duration = Duration::from_millis(20);

fn lock_ignoring_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn is_own_session(owner: &str) -> bool {
    lock_ignoring_poison(&OWN_SESSIONS).contains(owner)
}

/// An armed terminal that has not reported yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournaledTerminal {
    pub(crate) process_id: i32,
    pub(crate) command: String,
    /// Milliseconds since the Unix epoch when the process started.
    pub(crate) started_at_unix_ms: i64,
}

impl JournaledTerminal {
    fn from_notifier(notifier: &ExitNotifier, now: Instant, now_unix_ms: i64) -> Self {
        let running_for = now.saturating_duration_since(notifier.started_at());
        Self {
            process_id: notifier.process_id(),
            command: notifier.command().to_string(),
            started_at_unix_ms: now_unix_ms
                .saturating_sub(i64::try_from(running_for.as_millis()).unwrap_or(i64::MAX)),
        }
    }

    /// What the model learns about a terminal its previous Codex process lost. The process
    /// may outlive Codex: only Linux stops children with their parent, and a process that
    /// ignores SIGTERM survives even there (P3-6).
    pub(crate) fn lost_notice(&self) -> String {
        format!(
            "Background terminal session {} was lost: the Codex process that ran it stopped without shutting down, so its exit status and unread output are unknown and its session ID no longer works. The process most likely stopped with Codex, but it may still run, for example a server that still holds its port; if you need the command again, check for the process or its port first, and stop a leftover process before you start the command again.\n{}",
            self.process_id,
            command_line(&self.command)
        )
    }
}

/// Notices of terminals lost with a dead Codex process: one each, or one summary when there
/// are more than `MAX_LOST_NOTICES` (P3-4).
pub(crate) fn lost_notices(lost: &[JournaledTerminal]) -> Vec<String> {
    if lost.len() <= MAX_LOST_NOTICES {
        return lost.iter().map(JournaledTerminal::lost_notice).collect();
    }
    let mut ids: Vec<String> = lost
        .iter()
        .take(MAX_SUMMARY_SESSION_IDS)
        .map(|terminal| terminal.process_id.to_string())
        .collect();
    if lost.len() > MAX_SUMMARY_SESSION_IDS {
        ids.push(format!("and {} more", lost.len() - MAX_SUMMARY_SESSION_IDS));
    }
    vec![format!(
        "{} background terminal sessions were lost: the Codex process that ran them stopped without shutting down, so their exit status and unread output are unknown and their session IDs no longer work: {}. The processes most likely stopped with Codex, but some may still run, for example a server that still holds its port; check for a process or its port before you start its command again.",
        lost.len(),
        ids.join(", ")
    )]
}

pub(crate) fn journal_dir(codex_home: &Path) -> PathBuf {
    codex_home.join(JOURNAL_DIR)
}

/// A random owner name for one session: the process ID, for people who look at the files,
/// and a random part, so that no two sessions share files.
fn new_owner() -> String {
    format!("{}-{:08x}", std::process::id(), rand::random::<u32>())
}

/// One file of the journal directory, by its name.
#[derive(Debug, PartialEq, Eq)]
enum JournalFile<'a> {
    Lock { thread: &'a str, owner: &'a str },
    Journal { thread: &'a str, owner: &'a str },
    Tmp { thread: &'a str, owner: &'a str },
    Legacy { thread: &'a str },
}

impl<'a> JournalFile<'a> {
    fn parse(name: &'a str) -> Option<Self> {
        let (thread, rest) = name.split_once('.')?;
        if rest == LEGACY_SUFFIX {
            return Some(Self::Legacy { thread });
        }
        if let Some(owner) = rest.strip_suffix(LOCK_SUFFIX) {
            return Some(Self::Lock { thread, owner });
        }
        if let Some(owner) = rest.strip_suffix(JOURNAL_SUFFIX) {
            return Some(Self::Journal { thread, owner });
        }
        if rest.ends_with(TMP_SUFFIX) {
            let (owner, _) = rest.split_once('.')?;
            return Some(Self::Tmp { thread, owner });
        }
        None
    }

    fn thread(&self) -> &'a str {
        match self {
            Self::Lock { thread, .. }
            | Self::Journal { thread, .. }
            | Self::Tmp { thread, .. }
            | Self::Legacy { thread } => thread,
        }
    }
}

fn lock_path(dir: &Path, thread: &str, owner: &str) -> PathBuf {
    dir.join(format!("{thread}.{owner}{LOCK_SUFFIX}"))
}

fn journal_path(dir: &Path, thread: &str, owner: &str) -> PathBuf {
    dir.join(format!("{thread}.{owner}{JOURNAL_SUFFIX}"))
}

fn remove_if_present(path: &Path) {
    match fs::remove_file(path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            warn!(path = %path.display(), %err, "failed to remove a background terminal journal file");
        }
        _ => {}
    }
}

/// Creates the journal directory readable by the user only, and narrows one that an older
/// version created by the umask (P3-3).
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::PermissionsExt;
        builder.mode(0o700);
        builder.create(dir)?;
        let metadata = fs::metadata(dir)?;
        if metadata.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        builder.create(dir)
    }
}

fn open_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
}

/// Whether `file` is still the file at `path`: a resume removes the lock file of an owner it
/// found dead, and an owner that opened it a moment before must not lock the removed file.
#[cfg(unix)]
fn is_file_at(file: &File, path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let opened = file.metadata()?;
    match fs::symlink_metadata(path) {
        Ok(current) => Ok(opened.dev() == current.dev() && opened.ino() == current.ino()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

#[cfg(not(unix))]
fn is_file_at(_file: &File, path: &Path) -> std::io::Result<bool> {
    Ok(path.exists())
}

/// The exclusive lock of this owner's journal, held while the session journals (P3-2).
fn acquire_owner_lock(dir: &Path, thread: &str, owner: &str) -> std::io::Result<File> {
    create_private_dir(dir)?;
    let path = lock_path(dir, thread, owner);
    for _ in 0..5 {
        let file = open_options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.lock()?;
        if is_file_at(&file, &path)? {
            return Ok(file);
        }
    }
    Err(std::io::Error::other(
        "the background terminal journal lock kept being removed",
    ))
}

/// Replaces this owner's journal with `terminals`, or removes it when there are none. The
/// new list goes to a unique temporary file first, so a crash or another writer never leaves
/// a mixed file (P3-3).
fn write_owner_journal(
    dir: &Path,
    thread: &str,
    owner: &str,
    terminals: &[JournaledTerminal],
) -> std::io::Result<()> {
    let path = journal_path(dir, thread, owner);
    if terminals.is_empty() {
        remove_if_present(&path);
        return Ok(());
    }
    let json = serde_json::to_vec(terminals).map_err(std::io::Error::other)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!("{thread}.{owner}{JOURNAL_SUFFIX}."))
        .suffix(TMP_SUFFIX)
        .tempfile_in(dir)?;
    std::io::Write::write_all(&mut tmp, &json)?;
    tmp.persist(&path).map_err(|err| err.error)?;
    Ok(())
}

/// Reads one journal file of this user, at most `JOURNAL_MAX_BYTES` and
/// `MAX_UNIFIED_EXEC_PROCESSES` terminals; anything else lists nothing (P3-3, P3-4).
fn read_journal(path: &Path) -> Vec<JournaledTerminal> {
    let read = || -> std::io::Result<Option<Vec<u8>>> {
        let file = match open_options().read(true).open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let metadata = file.metadata()?;
        #[cfg(unix)]
        let foreign = {
            use std::os::unix::fs::MetadataExt;
            // SAFETY: `geteuid` has no preconditions and cannot fail.
            metadata.uid() != unsafe { libc::geteuid() }
        };
        #[cfg(not(unix))]
        let foreign = false;
        if !metadata.is_file() || foreign || metadata.len() > JOURNAL_MAX_BYTES {
            return Err(std::io::Error::other(
                "not a journal of this user within the size limit",
            ));
        }
        let mut bytes = Vec::new();
        file.take(JOURNAL_MAX_BYTES).read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    };
    match read() {
        Ok(Some(bytes)) => {
            let mut terminals: Vec<JournaledTerminal> = serde_json::from_slice(&bytes)
                .unwrap_or_else(|err| {
                    warn!(path = %path.display(), %err, "ignoring an unreadable background terminal journal");
                    Vec::new()
                });
            terminals.truncate(MAX_UNIFIED_EXEC_PROCESSES);
            terminals
        }
        Ok(None) => Vec::new(),
        Err(err) => {
            warn!(path = %path.display(), %err, "ignoring a background terminal journal");
            Vec::new()
        }
    }
}

/// Opens another owner's lock file to probe it. Read and write: on NFS `flock` is emulated with
/// `fcntl` locks, and an exclusive one needs a file open for writing (P2-1). Never for an owner
/// of this process (`is_own_session`): there, closing the file would release its lock.
fn open_foreign_lock(path: &Path) -> std::io::Result<File> {
    open_options().read(true).write(true).open(path)
}

/// The lock of a dead owner, or `None` while a live one holds it. Another process's resume or
/// sweep holds a dead owner's lock only for a moment, so a busy lock is tried a few times
/// before its owner counts as alive (P1-1).
fn lock_dead_owner(path: &Path, attempts: u32) -> Option<File> {
    let lock = match open_foreign_lock(path) {
        Ok(lock) => lock,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(err) => {
            warn!(path = %path.display(), %err, "failed to open a background terminal journal lock");
            return None;
        }
    };
    for attempt in 1..=attempts {
        match lock.try_lock() {
            Ok(()) => return Some(lock),
            Err(fs::TryLockError::WouldBlock) if attempt < attempts => {
                std::thread::sleep(FOREIGN_LOCK_RETRY);
            }
            Err(fs::TryLockError::WouldBlock) => return None,
            Err(fs::TryLockError::Error(err)) => {
                warn!(path = %path.display(), %err, "failed to lock a background terminal journal");
                return None;
            }
        }
    }
    None
}

/// The names of the journal directory, or of the files of `thread` in it.
fn journal_names(dir: &Path, thread: Option<&str>) -> Vec<String> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            warn!(dir = %dir.display(), %err, "failed to list background terminal journals");
            return Vec::new();
        }
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            thread.is_none_or(|thread| {
                JournalFile::parse(name).is_some_and(|file| file.thread() == thread)
            })
        })
        .collect()
}

/// The terminals in the journals of the dead owners of `thread`, whose files are then removed.
/// A dead owner is one whose lock nobody holds. The owners of this process's sessions are not
/// looked at (P1-1), and the owners of other live processes keep their files (P3-2). A journal
/// of the old format, `<thread>.json`, has no owner and counts as dead. A journal whose lock
/// file is not in the directory listing is not: its lock file may only have been created
/// after the listing started; it is probed only when its lock file is there now (review of
/// the fixes, P3-3).
fn take_dead_journals(dir: &Path, thread: &str) -> Vec<JournaledTerminal> {
    let _work = lock_ignoring_poison(&FOREIGN_WORK);
    let names = journal_names(dir, Some(thread));
    let mut lost = Vec::new();
    let mut owners: Vec<&str> = Vec::new();
    for name in &names {
        match JournalFile::parse(name) {
            Some(JournalFile::Lock { owner, .. } | JournalFile::Journal { owner, .. })
                if !owners.contains(&owner) =>
            {
                owners.push(owner);
            }
            Some(JournalFile::Legacy { .. }) => {
                let path = dir.join(name);
                lost.extend(read_journal(&path));
                remove_if_present(&path);
            }
            _ => {}
        }
    }
    for owner in owners {
        if is_own_session(owner) {
            continue;
        }
        let path = lock_path(dir, thread, owner);
        let Some(lock) = lock_dead_owner(&path, FOREIGN_LOCK_ATTEMPTS) else {
            continue;
        };
        lost.extend(read_journal(&journal_path(dir, thread, owner)));
        remove_owner_files(dir, &names, thread, owner);
        drop(lock);
    }
    lost
}

/// Removes the journal, temporary files and lock file of a dead owner whose lock the caller
/// holds; the lock file goes last, so a journal never lacks the lock that guards it.
fn remove_owner_files(dir: &Path, names: &[String], thread: &str, owner: &str) {
    remove_if_present(&journal_path(dir, thread, owner));
    for name in names {
        if let Some(JournalFile::Tmp {
            thread: tmp_thread,
            owner: tmp_owner,
        }) = JournalFile::parse(name)
            && tmp_thread == thread
            && tmp_owner == owner
        {
            remove_if_present(&dir.join(name));
        }
    }
    remove_if_present(&lock_path(dir, thread, owner));
}

/// Removes the files of dead owners that are older than `max_age`: journals that no resume
/// took, for example of a crashed thread that was never resumed or of the sub-agents of a
/// crashed parent (P3-5). The age is checked first, so the sweep locks only owners whose files
/// it removes, and never one of this process's sessions (P1-1).
fn sweep(dir: &Path, max_age: Duration, now: SystemTime) {
    let _work = lock_ignoring_poison(&FOREIGN_WORK);
    let names = journal_names(dir, /*thread*/ None);
    let is_old = |name: &str| {
        fs::symlink_metadata(dir.join(name))
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| now.duration_since(modified).is_ok_and(|age| age >= max_age))
    };
    let mut swept: Vec<(&str, &str)> = Vec::new();
    for name in &names {
        match JournalFile::parse(name) {
            Some(
                JournalFile::Lock { thread, owner }
                | JournalFile::Journal { thread, owner }
                | JournalFile::Tmp { thread, owner },
            ) if !swept.contains(&(thread, owner)) && !is_own_session(owner) => {
                swept.push((thread, owner));
                let journal = format!("{thread}.{owner}{JOURNAL_SUFFIX}");
                let lock = format!("{thread}.{owner}{LOCK_SUFFIX}");
                let newest = if names.contains(&journal) {
                    journal
                } else if names.contains(&lock) {
                    lock
                } else {
                    name.clone()
                };
                if !is_old(&newest) {
                    continue;
                }
                let lock = lock_path(dir, thread, owner);
                let held = match fs::symlink_metadata(&lock) {
                    Ok(_) => match lock_dead_owner(&lock, /*attempts*/ 1) {
                        Some(held) => Some(held),
                        None => continue,
                    },
                    Err(_) => None,
                };
                remove_owner_files(dir, &names, thread, owner);
                drop(held);
            }
            Some(JournalFile::Legacy { .. }) if is_old(name) => {
                remove_if_present(&dir.join(name));
            }
            _ => {}
        }
    }
}

/// The sweep of old journals, once per process, after the work of the blocking task that
/// calls it: never alongside a resume of the same process (P1-1).
fn sweep_once(dir: &Path) {
    if SWEPT.swap(true, Ordering::AcqRel) {
        return;
    }
    sweep(dir, JOURNAL_MAX_AGE, SystemTime::now());
}

/// Removes the journals of dead owners of `thread_ids`, whose threads were archived or
/// deleted: nothing will resume them to report their lost terminals (P3-5). A live owner
/// keeps its journal; its shutdown removes it.
pub(crate) async fn discard_journals(codex_home: &Path, thread_ids: &[ThreadId]) {
    let dir = journal_dir(codex_home);
    let threads: Vec<String> = thread_ids.iter().map(ToString::to_string).collect();
    let result = tokio::task::spawn_blocking(move || {
        for thread in threads {
            take_dead_journals(&dir, &thread);
        }
    })
    .await;
    if let Err(err) = result {
        warn!(%err, "failed to discard background terminal journals");
    }
}

/// The journal files of one session (P3-2). Each session orders its own writes, so sessions
/// do not wait for each other's journal (P3-8).
pub(crate) struct TerminalJournal {
    /// The owner name, also in `files`, for tests.
    #[cfg(test)]
    owner: String,
    /// Numbers the snapshots of the terminal list in the order they are taken (review of the
    /// fixes, P3-2): a snapshot is taken under this lock, and a write of an older snapshot never
    /// replaces a newer one.
    snapshots: tokio::sync::Mutex<u64>,
    /// The session's lock and what is on disk. A blocking task stores the lock here as soon as
    /// it takes it, so a cancelled call cannot drop it (review of the fixes, P3-1).
    files: Arc<Mutex<OwnerFiles>>,
}

/// What a session holds of its journal.
struct OwnerFiles {
    owner: String,
    /// The exclusive lock of the session's journal, from its first armed terminal to its end.
    lock: Option<File>,
    /// The owner is in `OWN_SESSIONS`.
    registered: bool,
    /// The snapshot whose list is on disk.
    written: u64,
    /// The session ended: nothing is written any more.
    cleared: bool,
}

impl OwnerFiles {
    fn register(&mut self) {
        if !self.registered {
            lock_ignoring_poison(&OWN_SESSIONS).insert(self.owner.clone());
            self.registered = true;
        }
    }

    /// Releases the lock, then the registration: from then on the owner is a foreign one.
    fn release(&mut self) {
        self.lock = None;
        if self.registered {
            lock_ignoring_poison(&OWN_SESSIONS).remove(&self.owner);
            self.registered = false;
        }
    }

    /// Writes snapshot `snapshot` of the terminal list, unless a newer one is on disk or the
    /// session ended. The first non-empty list takes the session's lock (P3-2): the owner is
    /// registered before its lock file exists, and keeps the lock until the session ends.
    fn write(&mut self, dir: &Path, thread: &str, snapshot: u64, terminals: &[JournaledTerminal]) {
        if self.cleared || snapshot <= self.written {
            return;
        }
        if self.lock.is_none() {
            if terminals.is_empty() {
                self.written = snapshot;
                return;
            }
            self.register();
            match acquire_owner_lock(dir, thread, &self.owner) {
                Ok(lock) => self.lock = Some(lock),
                Err(err) => {
                    warn!(%err, "failed to lock the background terminal journal");
                    self.release();
                    return;
                }
            }
        }
        if let Err(err) = write_owner_journal(dir, thread, &self.owner, terminals) {
            warn!(%err, "failed to update the background terminal journal");
            return;
        }
        self.written = snapshot;
    }

    /// The session ended: its journal and lock file go, whether or not it holds the lock now.
    fn clear(&mut self, dir: &Path, thread: &str) {
        self.cleared = true;
        remove_if_present(&journal_path(dir, thread, &self.owner));
        remove_if_present(&lock_path(dir, thread, &self.owner));
        self.release();
    }
}

impl Drop for OwnerFiles {
    fn drop(&mut self) {
        self.release();
    }
}

impl TerminalJournal {
    pub(crate) fn new() -> Self {
        let owner = new_owner();
        Self {
            #[cfg(test)]
            owner: owner.clone(),
            files: Arc::new(Mutex::new(OwnerFiles {
                owner,
                lock: None,
                registered: false,
                written: 0,
                cleared: false,
            })),
            snapshots: tokio::sync::Mutex::new(0),
        }
    }

    #[cfg(test)]
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    /// `sync` with a list taken already.
    #[cfg(test)]
    pub(crate) async fn write(
        &self,
        codex_home: &Path,
        thread_id: ThreadId,
        terminals: Vec<JournaledTerminal>,
    ) {
        self.sync(codex_home, thread_id, async move { terminals })
            .await;
    }

    /// Writes the terminal list that `snapshot` takes as this session's journal of
    /// `thread_id`. The snapshot is taken under the session's lock, so lists reach the disk in
    /// the order they were taken (review of the fixes, P3-2).
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "snapshots are numbered in the order they are taken: an older list must not overwrite a newer one"
    )]
    pub(crate) async fn sync(
        &self,
        codex_home: &Path,
        thread_id: ThreadId,
        snapshot: impl std::future::Future<Output = Vec<JournaledTerminal>>,
    ) {
        let (number, terminals) = {
            let mut snapshots = self.snapshots.lock().await;
            *snapshots += 1;
            (*snapshots, snapshot.await)
        };
        let dir = journal_dir(codex_home);
        let files = Arc::clone(&self.files);
        let thread = thread_id.to_string();
        let result = tokio::task::spawn_blocking(move || {
            lock_ignoring_poison(&files).write(&dir, &thread, number, &terminals);
            sweep_once(&dir);
        })
        .await;
        if let Err(err) = result {
            warn!(%err, "failed to update the background terminal journal");
        }
    }

    /// Removes this session's journal and lock: the session ended, and a shutdown recorded
    /// what the journal had to tell (R2.5).
    pub(crate) async fn clear(&self, codex_home: &Path, thread_id: ThreadId) {
        let dir = journal_dir(codex_home);
        let files = Arc::clone(&self.files);
        let thread = thread_id.to_string();
        let result =
            tokio::task::spawn_blocking(move || lock_ignoring_poison(&files).clear(&dir, &thread))
                .await;
        if let Err(err) = result {
            warn!(%err, "failed to remove the background terminal journal");
        }
    }

    /// Takes the journals of the dead owners of `thread_id`: the terminals that Codex
    /// processes which stopped without shutting down left behind. The once-per-process sweep
    /// runs after it in the same task (P1-1).
    pub(crate) async fn take_lost(
        &self,
        codex_home: &Path,
        thread_id: ThreadId,
    ) -> Vec<JournaledTerminal> {
        let dir = journal_dir(codex_home);
        let thread = thread_id.to_string();
        tokio::task::spawn_blocking(move || {
            let lost = take_dead_journals(&dir, &thread);
            sweep_once(&dir);
            lost
        })
        .await
        .unwrap_or_else(|err| {
            warn!(%err, "failed to read background terminal journals");
            Vec::new()
        })
    }
}

impl crate::unified_exec::UnifiedExecProcessManager {
    /// Armed terminals of the thread that have not reported yet.
    pub(crate) async fn journaled_terminals(&self) -> Vec<JournaledTerminal> {
        let notifiers: Vec<Arc<ExitNotifier>> = self
            .process_store
            .lock()
            .await
            .processes
            .values()
            .filter_map(|entry| entry.exit_notifier.clone())
            .filter(|notifier| notifier.is_pending())
            .collect();
        let now = Instant::now();
        let now_unix_ms = chrono::Utc::now().timestamp_millis();
        let mut terminals: Vec<JournaledTerminal> = notifiers
            .iter()
            .map(|notifier| JournaledTerminal::from_notifier(notifier, now, now_unix_ms))
            .collect();
        terminals.sort_by_key(|terminal| terminal.process_id);
        terminals
    }
}

impl Session {
    fn terminal_journal(&self) -> Arc<TerminalJournal> {
        self.services
            .session_extension_data
            .get_or_init(TerminalJournal::new)
    }

    /// `$CODEX_HOME` of a session that journals: exit notifications on, not ephemeral.
    async fn terminal_journal_home(&self) -> Option<PathBuf> {
        if !self
            .features()
            .enabled(codex_features::Feature::ExecExitNotifications)
        {
            return None;
        }
        let config = self.get_config().await;
        (!config.ephemeral).then(|| config.codex_home.to_path_buf())
    }

    /// Rewrites the journal after a terminal was armed or reported.
    pub(crate) async fn fork_sync_terminal_journal(&self) {
        let Some(codex_home) = self.terminal_journal_home().await else {
            return;
        };
        self.terminal_journal()
            .sync(
                &codex_home,
                self.thread_id,
                self.services.unified_exec_manager.journaled_terminals(),
            )
            .await;
    }

    /// The session ends, and its shutdown recorded what it owes the model: the journal has
    /// nothing left to tell. Its journal and lock go even if the feature was turned off since,
    /// so the next resume reports no false loss (review of the fixes, P3-1).
    pub(crate) async fn fork_clear_terminal_journal(&self) {
        let Some(journal) = self
            .services
            .session_extension_data
            .get::<TerminalJournal>()
        else {
            return;
        };
        let codex_home = self.get_config().await.codex_home.to_path_buf();
        journal.clear(&codex_home, self.thread_id).await;
    }

    /// Fork hook of a resumed thread, after its history is rebuilt: terminals that a Codex
    /// process which died armed and never reported are gone with it; notices tell the model,
    /// and nothing waits for them (D6, R2.11). Journals of a process that still runs the
    /// thread are not its to take (P3-2).
    pub(crate) async fn fork_record_lost_terminals(&self) {
        let config = self.get_config().await;
        if config.ephemeral {
            return;
        }
        let lost = self
            .terminal_journal()
            .take_lost(config.codex_home.as_path(), self.thread_id)
            .await;
        if lost.is_empty() {
            return;
        }
        let items: Vec<_> = lost_notices(&lost)
            .into_iter()
            .map(|notice| ContextualUserFragment::into(BackgroundTerminalNotification::new(notice)))
            .collect();
        let turn_context = self.new_inject_items_context().await;
        self.record_conversation_items(&turn_context, turn_context.model_info(), &items)
            .await;
    }
}

#[cfg(test)]
#[path = "terminal_journal_tests.rs"]
mod tests;
