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

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
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

/// The files of the dead owners of `thread`, other than `own`: the ones whose lock is free,
/// and journals that have no lock file at all (the old format). Each dead owner's journal is
/// read, then all its files are removed. Live owners' files stay as they are (P3-2).
fn take_dead_journals(dir: &Path, thread: &str, own: Option<&str>) -> Vec<JournaledTerminal> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            warn!(dir = %dir.display(), %err, "failed to list background terminal journals");
            return Vec::new();
        }
    };
    let names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| JournalFile::parse(name).is_some_and(|file| file.thread() == thread))
        .collect();
    let mut lost = Vec::new();
    let mut owners_with_lock = Vec::new();
    for name in &names {
        match JournalFile::parse(name) {
            Some(JournalFile::Lock { owner, .. }) if Some(owner) != own => {
                owners_with_lock.push(owner);
            }
            Some(JournalFile::Legacy { .. }) => {
                let path = dir.join(name);
                lost.extend(read_journal(&path));
                remove_if_present(&path);
            }
            _ => {}
        }
    }
    for owner in owners_with_lock {
        let path = lock_path(dir, thread, owner);
        let lock = match open_options().read(true).open(&path) {
            Ok(lock) => lock,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                warn!(path = %path.display(), %err, "failed to open a background terminal journal lock");
                continue;
            }
        };
        match lock.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => continue,
            Err(fs::TryLockError::Error(err)) => {
                warn!(path = %path.display(), %err, "failed to lock a background terminal journal");
                continue;
            }
        }
        let journal = journal_path(dir, thread, owner);
        lost.extend(read_journal(&journal));
        remove_owner_files(dir, &names, thread, owner);
        drop(lock);
    }
    for name in &names {
        if let Some(JournalFile::Journal { owner, .. }) = JournalFile::parse(name)
            && Some(owner) != own
            && !names.contains(&format!("{thread}.{owner}{LOCK_SUFFIX}"))
        {
            let path = dir.join(name);
            lost.extend(read_journal(&path));
            remove_if_present(&path);
        }
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

/// Removes the files of dead owners that are older than `max_age`, and temporary files of
/// dead owners of any age: journals that no resume took, for example of a crashed thread that
/// was never resumed or of the sub-agents of a crashed parent (P3-5).
fn sweep(dir: &Path, max_age: Duration, now: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    let is_old = |name: &str| {
        fs::symlink_metadata(dir.join(name))
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| now.duration_since(modified).is_ok_and(|age| age >= max_age))
    };
    for name in &names {
        match JournalFile::parse(name) {
            Some(JournalFile::Lock { thread, owner }) => {
                let path = lock_path(dir, thread, owner);
                let Ok(lock) = open_options().read(true).open(&path) else {
                    continue;
                };
                if lock.try_lock().is_err() {
                    continue;
                }
                let journal = format!("{thread}.{owner}{JOURNAL_SUFFIX}");
                let old = if names.contains(&journal) {
                    is_old(&journal)
                } else {
                    is_old(name)
                };
                if old {
                    remove_owner_files(dir, &names, thread, owner);
                } else {
                    for tmp in &names {
                        if let Some(JournalFile::Tmp {
                            thread: tmp_thread,
                            owner: tmp_owner,
                        }) = JournalFile::parse(tmp)
                            && tmp_thread == thread
                            && tmp_owner == owner
                        {
                            remove_if_present(&dir.join(tmp));
                        }
                    }
                }
            }
            Some(JournalFile::Journal { thread, owner } | JournalFile::Tmp { thread, owner })
                if !names.contains(&format!("{thread}.{owner}{LOCK_SUFFIX}")) && is_old(name) =>
            {
                remove_if_present(&dir.join(name));
            }
            Some(JournalFile::Legacy { .. }) if is_old(name) => {
                remove_if_present(&dir.join(name));
            }
            _ => {}
        }
    }
}

/// Starts the sweep of old journals once per process, in the background.
fn sweep_once(dir: &Path) {
    if SWEPT.swap(true, Ordering::AcqRel) {
        return;
    }
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || sweep(&dir, JOURNAL_MAX_AGE, SystemTime::now()));
}

/// Removes the journals of dead owners of `thread_ids`, whose threads were archived or
/// deleted: nothing will resume them to report their lost terminals (P3-5). A live owner
/// keeps its journal; its shutdown removes it.
pub(crate) async fn discard_journals(codex_home: &Path, thread_ids: &[ThreadId]) {
    let dir = journal_dir(codex_home);
    let threads: Vec<String> = thread_ids.iter().map(ToString::to_string).collect();
    let result = tokio::task::spawn_blocking(move || {
        for thread in threads {
            take_dead_journals(&dir, &thread, /*own*/ None);
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
    owner: String,
    /// The exclusive lock of this owner while it has a journal; its mutex orders the writes.
    lock: tokio::sync::Mutex<Option<File>>,
}

impl TerminalJournal {
    pub(crate) fn new() -> Self {
        Self {
            owner: new_owner(),
            lock: tokio::sync::Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn owner(&self) -> &str {
        &self.owner
    }

    /// Writes `terminals` as this session's journal of `thread_id`. A session that never
    /// journaled a terminal has nothing to remove.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "one session's journal writes are ordered: an older list must not overwrite a newer one"
    )]
    pub(crate) async fn write(
        &self,
        codex_home: &Path,
        thread_id: ThreadId,
        terminals: Vec<JournaledTerminal>,
    ) {
        let mut held = self.lock.lock().await;
        if terminals.is_empty() && held.is_none() {
            return;
        }
        let dir = journal_dir(codex_home);
        sweep_once(&dir);
        let owner = self.owner.clone();
        let lock = held.take();
        let thread = thread_id.to_string();
        let result = tokio::task::spawn_blocking(move || {
            let lock = match lock {
                Some(lock) => lock,
                None => match acquire_owner_lock(&dir, &thread, &owner) {
                    Ok(lock) => lock,
                    Err(err) => return (None, Err(err)),
                },
            };
            let result = write_owner_journal(&dir, &thread, &owner, &terminals);
            (Some(lock), result)
        })
        .await;
        match result {
            Ok((lock, result)) => {
                *held = lock;
                if let Err(err) = result {
                    warn!(%err, "failed to update the background terminal journal");
                }
            }
            Err(err) => warn!(%err, "failed to update the background terminal journal"),
        }
    }

    /// Removes this session's journal and lock: a shutdown recorded what the journal had to
    /// tell (R2.5).
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "one session's journal writes are ordered: an older list must not overwrite a newer one"
    )]
    pub(crate) async fn clear(&self, codex_home: &Path, thread_id: ThreadId) {
        let mut held = self.lock.lock().await;
        let Some(lock) = held.take() else {
            return;
        };
        let dir = journal_dir(codex_home);
        let owner = self.owner.clone();
        let thread = thread_id.to_string();
        let result = tokio::task::spawn_blocking(move || {
            remove_if_present(&journal_path(&dir, &thread, &owner));
            remove_if_present(&lock_path(&dir, &thread, &owner));
            drop(lock);
        })
        .await;
        if let Err(err) = result {
            warn!(%err, "failed to remove the background terminal journal");
        }
    }

    /// Takes the journals of the dead owners of `thread_id`: the terminals that Codex
    /// processes which stopped without shutting down left behind.
    pub(crate) async fn take_lost(
        &self,
        codex_home: &Path,
        thread_id: ThreadId,
    ) -> Vec<JournaledTerminal> {
        let dir = journal_dir(codex_home);
        sweep_once(&dir);
        let owner = self.owner.clone();
        let thread = thread_id.to_string();
        tokio::task::spawn_blocking(move || take_dead_journals(&dir, &thread, Some(&owner)))
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
        let journal = self.terminal_journal();
        let terminals = self
            .services
            .unified_exec_manager
            .journaled_terminals()
            .await;
        journal.write(&codex_home, self.thread_id, terminals).await;
    }

    /// A shutdown recorded what it owes the model: the journal has nothing left to tell.
    pub(crate) async fn fork_clear_terminal_journal(&self) {
        let Some(codex_home) = self.terminal_journal_home().await else {
            return;
        };
        self.terminal_journal()
            .clear(&codex_home, self.thread_id)
            .await;
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
