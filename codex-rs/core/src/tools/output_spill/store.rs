//! Fork: use cases 3 and 5 of `specs/tool-output-spill-2026-11-07.md`: where the full text of a
//! truncated tool result lives, who can read it, and how long it stays.
//!
//! Files live in `$CODEX_HOME/tool_outputs/<thread_id>/<name>-<suffix>.txt`, where `<name>` is the
//! first 16 hex digits of SHA-256 of the tool call id and `<suffix>` 8 random hex digits, so a
//! repeated call id never replaces an earlier output (R3.1). The directories are `0700` and the
//! files `0600`, and every operation goes through directory descriptors without following a
//! symlink (R3.2). A sandbox that could write the spill directories gets no file (R3.4). A
//! thread directory is kept under `max_thread_bytes` after every write (R5.1); a sweep of old
//! files and the total size runs at every session start and at most once an hour after a save,
//! with the feature on or off (R5.2); a deleted thread or an ephemeral session removes its
//! directory (R5.3, R5.5).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::Weak;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use codex_protocol::ThreadId;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_utils_output_truncation::approx_token_count;
use tracing::debug;
use tracing::warn;

use crate::config::ToolOutputSpillConfig;
use crate::fork_private_fs::DirEntryInfo;
use crate::fork_private_fs::EntryKind;
use crate::fork_private_fs::PrivateDir;

/// The directory of all saved outputs under `CODEX_HOME`.
pub(super) const SPILL_DIR: &str = "tool_outputs";
const SPILL_SUFFIX: &str = ".txt";
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
/// How often one process sweeps a spill root at most (R5.2).
pub(super) const SWEEP_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// Random names tried before a save gives up; a clash needs the same call id and 32 bits.
const NAME_ATTEMPTS: usize = 8;

/// When this process last started a sweep of each `CODEX_HOME` (R5.2).
static LAST_SWEEPS: LazyLock<Mutex<HashMap<PathBuf, Instant>>> = LazyLock::new(Mutex::default);

/// Writes and evictions of one thread directory, one at a time (R5.1): parallel calls of a
/// thread must not delete each other's files while they count the directory.
static THREAD_LOCKS: LazyLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> =
    LazyLock::new(Mutex::default);

/// A saved full output (`Types` of use case 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SpillRecord {
    /// Canonical absolute path of the file.
    pub(crate) path: PathBuf,
    /// Lines of the file.
    pub(crate) lines: usize,
    /// Bytes of the file.
    pub(crate) bytes: u64,
    /// `approx_token_count` of the file.
    pub(crate) tokens: usize,
    /// Set when `max_file_bytes` cut the middle of the full text (R3.3).
    pub(crate) capped: Option<CappedSpill>,
}

/// How `max_file_bytes` cut a full text: its first and last `kept_half` bytes are in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CappedSpill {
    pub(crate) kept_half: u64,
    pub(crate) original_bytes: u64,
}

/// What the primary environment of a step tells about who can read a spill file (R3.4).
pub(super) struct SpillReader {
    /// The environment runs on another machine.
    pub(super) remote: bool,
    /// Filesystem sandbox of `exec_command` in that environment.
    pub(super) policy: FileSystemSandboxPolicy,
    /// Working directory of that environment, for relative sandbox entries.
    pub(super) cwd: PathBuf,
}

/// `$CODEX_HOME/tool_outputs`, under the canonical `CODEX_HOME` (it may be a symlink), so the
/// path the reader check sees is the path the notice names.
pub(super) fn spill_root(codex_home: &Path) -> PathBuf {
    codex_home
        .canonicalize()
        .unwrap_or_else(|_| codex_home.to_path_buf())
        .join(SPILL_DIR)
}

/// The thread directory a model can read with `exec_command`, or `None` (R3.4, R3.5, R3.6):
/// without a local primary environment, without `exec_command`, when the primary
/// environment's sandbox cannot read the directory, or when it could redirect or rewrite the
/// files ([`sandbox_can_redirect`]), the feature does nothing.
pub(super) fn reader_dir(
    primary: Option<SpillReader>,
    exposes_exec_command: bool,
    codex_home: &Path,
    thread_id: ThreadId,
) -> Option<PathBuf> {
    if cfg!(windows) || !exposes_exec_command {
        return None;
    }
    let primary = primary?;
    if primary.remote {
        return None;
    }
    let dir = spill_root(codex_home).join(thread_id.to_string());
    (primary
        .policy
        .can_read_local_path_with_cwd(&dir, &primary.cwd)
        && !sandbox_can_redirect(&primary.policy, &primary.cwd, &dir))
    .then_some(dir)
}

/// Whether a sandboxed command could change what the spill paths lead to (review 2026-11-07,
/// S-1): write the thread directory, the spill root or `CODEX_HOME`, whose entries Codex opens
/// by name, or replace a directory above `CODEX_HOME` that it can write inside a directory it
/// can write too. A directory the sandbox protects inside a writable one, such as `.codex` at
/// the top of a workspace, stays in place. Without a sandbox (`Disabled`, `External`) commands
/// can do what Codex does, so nothing is refused.
fn sandbox_can_redirect(policy: &FileSystemSandboxPolicy, cwd: &Path, thread_dir: &Path) -> bool {
    if policy.has_full_disk_write_access() {
        return false;
    }
    let writable = thread_dir
        .ancestors()
        .map(|path| policy.can_write_local_path_with_cwd(path, cwd))
        .collect::<Vec<_>>();
    let (opened_by_name, above_codex_home) = writable.split_at(writable.len().min(3));
    opened_by_name.iter().any(|writable| *writable)
        || above_codex_home
            .windows(2)
            .any(|pair| pair.iter().all(|writable| *writable))
}

/// The name prefix of a tool call's files: 16 hex digits of SHA-256 of its id (R3.1).
pub(super) fn file_stem(call_id: &str) -> String {
    let mut stem = codex_workflow::sha256_hex(call_id.as_bytes());
    stem.truncate(16);
    stem
}

/// The text kept in the file: all of `full`, or its first and last halves of `max_file_bytes`
/// around one marker line (R3.3, `capped_file_marker`).
fn file_text(full: &str, max_file_bytes: u64) -> (String, Option<CappedSpill>) {
    let max_file_bytes = usize::try_from(max_file_bytes).unwrap_or(usize::MAX);
    if full.len() <= max_file_bytes {
        return (full.to_string(), None);
    }
    let half = max_file_bytes / 2;
    let head = &full[..full.floor_char_boundary(half)];
    let tail = &full[full.ceil_char_boundary(full.len() - half)..];
    let omitted = full.len() - head.len() - tail.len();
    let mut text = String::with_capacity(head.len() + tail.len() + 80);
    text.push_str(head);
    if !head.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "[... {omitted} bytes not saved: tool_output_spill.max_file_bytes ...]\n"
    ));
    text.push_str(tail);
    (
        text,
        Some(CappedSpill {
            kept_half: half as u64,
            original_bytes: full.len() as u64,
        }),
    )
}

/// Saves `full` for `call_id` in `thread_dir` and keeps the thread under its limit (R3.1–R3.3,
/// R5.1); then starts a sweep unless one ran within [`SWEEP_INTERVAL`] (R5.2).
pub(super) async fn save(
    thread_dir: PathBuf,
    call_id: &str,
    full: String,
    limits: ToolOutputSpillConfig,
) -> std::io::Result<SpillRecord> {
    let stem = file_stem(call_id);
    let started = Instant::now();
    let codex_home = thread_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let record =
        tokio::task::spawn_blocking(move || save_blocking(&thread_dir, &stem, &full, limits))
            .await
            .map_err(std::io::Error::other)??;
    debug!(
        path = %record.path.display(),
        bytes = record.bytes,
        elapsed_ms = started.elapsed().as_millis(),
        "saved the full output of a truncated tool result"
    );
    if let Some(codex_home) = codex_home {
        sweep_in_background(&codex_home, limits);
    }
    Ok(record)
}

pub(super) fn save_blocking(
    thread_dir: &Path,
    stem: &str,
    full: &str,
    limits: ToolOutputSpillConfig,
) -> std::io::Result<SpillRecord> {
    save_blocking_with(thread_dir, stem, full, limits, || {})
}

/// [`save_blocking`] that runs `before_write` once the thread directory is open, so tests can
/// change the paths under it.
pub(super) fn save_blocking_with(
    thread_dir: &Path,
    stem: &str,
    full: &str,
    limits: ToolOutputSpillConfig,
    before_write: impl FnOnce(),
) -> std::io::Result<SpillRecord> {
    let (text, capped) = file_text(full, limits.max_file_bytes);
    let lock = thread_lock(thread_dir);
    let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut before_write = Some(before_write);
    let mut retried = false;
    let (dir, name) = loop {
        let dir = open_thread_dir(thread_dir, /*create*/ true)?;
        if let Some(before_write) = before_write.take() {
            before_write();
        }
        match write_new_file(&dir, stem, &text) {
            // A sweep removed the directory while it was empty; create it once more.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound && !retried => retried = true,
            written => break (dir, written?),
        }
    };
    enforce_thread_limit(&dir, limits.max_thread_bytes, &name);
    Ok(SpillRecord {
        path: thread_dir.join(&name),
        lines: text.lines().count(),
        bytes: text.len() as u64,
        tokens: approx_token_count(&text),
        capped,
    })
}

fn thread_lock(thread_dir: &Path) -> Arc<Mutex<()>> {
    let mut locks = THREAD_LOCKS.lock().unwrap_or_else(PoisonError::into_inner);
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(thread_dir).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(thread_dir.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn name_of(path: &Path) -> std::io::Result<&OsStr> {
    path.file_name()
        .ok_or_else(|| std::io::Error::other(format!("{} has no name", path.display())))
}

/// The spill root `<canonical CODEX_HOME>/tool_outputs`, opened without following a symlink;
/// with `create`, made private when missing (R3.2).
fn open_root(root: &Path, create: bool) -> std::io::Result<PrivateDir> {
    let codex_home = root
        .parent()
        .ok_or_else(|| std::io::Error::other("the spill directory has no parent"))?;
    PrivateDir::open_no_follow(codex_home)?.private_child(name_of(root)?, create)
}

/// The thread directory `<spill root>/<thread_id>`, opened like [`open_root`].
pub(super) fn open_thread_dir(thread_dir: &Path, create: bool) -> std::io::Result<PrivateDir> {
    let root = thread_dir
        .parent()
        .ok_or_else(|| std::io::Error::other("the thread directory has no parent"))?;
    open_root(root, create)?.private_child(name_of(thread_dir)?, create)
}

/// Writes `text` to a new file `<stem>-<8 random hex digits>.txt` of `dir`. The file is created
/// exclusively, so it never replaces another output or opens a symlink (R3.1, R3.2).
fn write_new_file(dir: &PrivateDir, stem: &str, text: &str) -> std::io::Result<OsString> {
    for _ in 0..NAME_ATTEMPTS {
        let name = OsString::from(format!(
            "{stem}-{:08x}{SPILL_SUFFIX}",
            rand::random::<u32>()
        ));
        let mut file = match dir.create_new_file(&name) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        };
        if let Err(err) = file.write_all(text.as_bytes()) {
            remove_file(dir, &name);
            return Err(err);
        }
        return Ok(name);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free name for a saved tool output",
    ))
}

/// The saved outputs of a thread directory; symlinks and directories are not counted.
fn spill_files(dir: &PrivateDir) -> Vec<DirEntryInfo> {
    dir.entries()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| {
            entry.kind == EntryKind::File
                && entry
                    .name
                    .as_encoded_bytes()
                    .ends_with(SPILL_SUFFIX.as_bytes())
        })
        .collect()
}

fn remove_file(dir: &PrivateDir, name: &OsStr) {
    if let Err(err) = dir.remove_file(name)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!(name = %name.display(), %err, "failed to remove a saved tool output");
    }
}

/// Deletes the oldest files of `dir`, never `keep`, until it fits `max_bytes` (R5.1).
pub(super) fn enforce_thread_limit(dir: &PrivateDir, max_bytes: u64, keep: &OsStr) {
    let mut files = spill_files(dir);
    let mut total: u64 = files.iter().map(|file| file.bytes).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|file| file.modified);
    for file in files {
        if total <= max_bytes {
            break;
        }
        if file.name == keep {
            continue;
        }
        remove_file(dir, &file.name);
        total = total.saturating_sub(file.bytes);
    }
}

/// Claims the sweep of `codex_home` at `now` for this process: `false` while the last one is
/// younger than [`SWEEP_INTERVAL`] (R5.2).
pub(super) fn claim_sweep(codex_home: &Path, now: Instant) -> bool {
    let mut sweeps = LAST_SWEEPS.lock().unwrap_or_else(PoisonError::into_inner);
    if sweeps
        .get(codex_home)
        .is_some_and(|last| now.saturating_duration_since(*last) < SWEEP_INTERVAL)
    {
        return false;
    }
    sweeps.insert(codex_home.to_path_buf(), now);
    true
}

/// Use case 5, step "sweep old files and the total size": starts [`sweep`] of the spill root of
/// `codex_home` on a blocking thread with `limits`, unless this process started one within
/// [`SWEEP_INTERVAL`] (R5.2). Session starts call it with the feature on or off, saves call it
/// after writing; the handle lets tests wait for the sweep.
pub(crate) fn sweep_in_background(
    codex_home: &Path,
    limits: ToolOutputSpillConfig,
) -> Option<tokio::task::JoinHandle<()>> {
    if cfg!(windows) {
        return None;
    }
    let runtime = tokio::runtime::Handle::try_current().ok()?;
    if !claim_sweep(codex_home, Instant::now()) {
        return None;
    }
    let codex_home = codex_home.to_path_buf();
    Some(runtime.spawn_blocking(move || {
        sweep(&spill_root(&codex_home), limits, SystemTime::now());
    }))
}

/// Deletes spill files older than `max_age_days` and empty thread directories, then the oldest
/// files of all threads until the root fits `max_total_bytes` (R5.2). Errors are logged;
/// nothing panics.
pub(super) fn sweep(root: &Path, limits: ToolOutputSpillConfig, now: SystemTime) {
    sweep_with(root, limits, now, |_| {});
}

/// [`sweep`] that runs `before_thread` with the path of each thread directory before opening
/// it, so tests can change the paths under the root.
pub(super) fn sweep_with(
    root: &Path,
    limits: ToolOutputSpillConfig,
    now: SystemTime,
    mut before_thread: impl FnMut(&Path),
) {
    let root_dir = match open_root(root, /*create*/ false) {
        Ok(root_dir) => root_dir,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(path = %root.display(), %err, "not sweeping saved tool outputs");
            }
            return;
        }
    };
    let Ok(threads) = root_dir.entries() else {
        return;
    };
    let max_age = Duration::from_secs(limits.max_age_days.saturating_mul(SECONDS_PER_DAY));
    let mut kept = Vec::new();
    for thread in threads {
        if thread.kind != EntryKind::Dir {
            continue;
        }
        before_thread(&root.join(&thread.name));
        let Ok(dir) = root_dir.private_child(&thread.name, /*create*/ false) else {
            continue;
        };
        for file in spill_files(&dir) {
            if now
                .duration_since(file.modified)
                .is_ok_and(|age| age >= max_age)
            {
                remove_file(&dir, &file.name);
            } else {
                kept.push((thread.name.clone(), file));
            }
        }
        // Fails while the directory still holds files; only empty ones go.
        let _ = root_dir.remove_empty_dir(&thread.name);
    }
    let mut total: u64 = kept.iter().map(|(_, file)| file.bytes).sum();
    if total <= limits.max_total_bytes {
        return;
    }
    kept.sort_by_key(|(_, file)| file.modified);
    let mut open: Option<(OsString, PrivateDir)> = None;
    for (thread, file) in kept {
        if total <= limits.max_total_bytes {
            break;
        }
        if open.as_ref().is_none_or(|(name, _)| *name != thread) {
            open = root_dir
                .private_child(&thread, /*create*/ false)
                .ok()
                .map(|dir| (thread, dir));
        }
        if let Some((_, dir)) = &open {
            remove_file(dir, &file.name);
        }
        total = total.saturating_sub(file.bytes);
    }
}

/// Removes the spill directories of `thread_ids` (R5.3, R5.5), through descriptors: a thread
/// entry that is a symlink, or a spill root that is not the user's own directory, is left
/// alone.
pub(crate) fn discard_threads_blocking(codex_home: &Path, thread_ids: &[ThreadId]) {
    let root = spill_root(codex_home);
    let root_dir = match open_root(&root, /*create*/ false) {
        Ok(root_dir) => root_dir,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                warn!(path = %root.display(), %err, "not removing saved tool outputs");
            }
            return;
        }
    };
    for thread_id in thread_ids {
        let name = OsString::from(thread_id.to_string());
        match root_dir.remove_dir_all(&name) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                warn!(path = %root.join(&name).display(), %err, "failed to remove saved tool outputs");
            }
        }
    }
}

/// [`discard_threads_blocking`] on a blocking thread.
pub(crate) async fn discard_threads(codex_home: &Path, thread_ids: &[ThreadId]) {
    let codex_home = codex_home.to_path_buf();
    let thread_ids = thread_ids.to_vec();
    if let Err(err) = tokio::task::spawn_blocking(move || {
        discard_threads_blocking(&codex_home, &thread_ids);
    })
    .await
    {
        warn!(%err, "failed to remove saved tool outputs");
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
