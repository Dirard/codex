//! Fork: use cases 3 and 5 of `specs/tool-output-spill-2026-11-07.md`: where the full text of a
//! truncated tool result lives, who can read it, and how long it stays.
//!
//! Files live in `$CODEX_HOME/tool_outputs/<thread_id>/<name>.txt`, where `<name>` is the first
//! 16 hex digits of SHA-256 of the tool call id. The directories are `0700` and the files `0600`
//! (R3.2). A thread directory is kept under `max_thread_bytes` after every write (R5.1); once per
//! process the first spill sweeps old files and the total size in the background (R5.2); a
//! deleted thread or an ephemeral session removes its directory (R5.3, R5.5).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::Weak;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use codex_protocol::ThreadId;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_utils_output_truncation::approx_token_count;
use tracing::debug;
use tracing::warn;

use crate::config::ToolOutputSpillConfig;
use crate::fork_private_fs::check_owned_dir;
use crate::fork_private_fs::create_owned_private_dir;
use crate::fork_private_fs::private_file_options;

/// The directory of all saved outputs under `CODEX_HOME`.
pub(super) const SPILL_DIR: &str = "tool_outputs";
const SPILL_SUFFIX: &str = ".txt";
const TMP_SUFFIX: &str = ".tmp";
/// A temporary file older than this was left by an interrupted write (R5.2).
const TMP_MAX_AGE: Duration = Duration::from_secs(60 * 60);
const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// The first spill of this process sweeps old files and the total size (R5.2).
static SWEPT: AtomicBool = AtomicBool::new(false);

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
/// without a local primary environment, without `exec_command`, or when the primary
/// environment's sandbox cannot read the directory, the feature does nothing.
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
    primary
        .policy
        .can_read_local_path_with_cwd(&dir, &primary.cwd)
        .then_some(dir)
}

/// The file name of a tool call: 16 hex digits of SHA-256 of its id (R3.1).
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
/// R5.1); the first spill of the process also starts the sweep (R5.2).
pub(super) async fn save(
    thread_dir: PathBuf,
    call_id: &str,
    full: String,
    limits: ToolOutputSpillConfig,
) -> std::io::Result<SpillRecord> {
    let stem = file_stem(call_id);
    let started = Instant::now();
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
    if let Some(root) = record.path.parent().and_then(Path::parent) {
        sweep_once(root.to_path_buf(), limits);
    }
    Ok(record)
}

pub(super) fn save_blocking(
    thread_dir: &Path,
    stem: &str,
    full: &str,
    limits: ToolOutputSpillConfig,
) -> std::io::Result<SpillRecord> {
    let root = thread_dir
        .parent()
        .ok_or_else(|| std::io::Error::other("the spill directory has no parent"))?;
    create_owned_private_dir(root)?;
    create_owned_private_dir(thread_dir)?;
    let (text, capped) = file_text(full, limits.max_file_bytes);
    let path = thread_dir.join(format!("{stem}{SPILL_SUFFIX}"));

    let lock = thread_lock(thread_dir);
    let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!("{stem}."))
        .suffix(TMP_SUFFIX)
        .make_in(thread_dir, |path| {
            private_file_options()
                .write(true)
                .create_new(true)
                .open(path)
        })?;
    tmp.write_all(text.as_bytes())?;
    tmp.persist(&path).map_err(|err| err.error)?;
    enforce_thread_limit(thread_dir, limits.max_thread_bytes, &path);
    Ok(SpillRecord {
        path,
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

/// A saved output in a thread directory.
struct SpillFile {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

/// The regular files of `dir` whose name ends with `suffix`; symlinks and directories are not
/// counted.
fn files_in(dir: &Path, suffix: &str) -> Vec<SpillFile> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(suffix))
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            metadata.file_type().is_file().then(|| SpillFile {
                path: entry.path(),
                bytes: metadata.len(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            })
        })
        .collect()
}

fn remove_file(path: &Path) {
    if let Err(err) = fs::remove_file(path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!(path = %path.display(), %err, "failed to remove a saved tool output");
    }
}

/// Deletes the oldest files of `thread_dir`, never `keep`, until it fits `max_bytes` (R5.1).
pub(super) fn enforce_thread_limit(thread_dir: &Path, max_bytes: u64, keep: &Path) {
    let mut files = files_in(thread_dir, SPILL_SUFFIX);
    let mut total: u64 = files.iter().map(|file| file.bytes).sum();
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|file| file.modified);
    for file in files {
        if total <= max_bytes {
            break;
        }
        if file.path == keep {
            continue;
        }
        remove_file(&file.path);
        total = total.saturating_sub(file.bytes);
    }
}

/// Starts [`sweep`] in the background at the first spill of the process (R5.2).
fn sweep_once(root: PathBuf, limits: ToolOutputSpillConfig) {
    if SWEPT.swap(true, Ordering::AcqRel) {
        return;
    }
    tokio::task::spawn_blocking(move || sweep(&root, limits, SystemTime::now()));
}

/// Deletes spill files older than `max_age_days`, temporary files older than an hour and empty
/// thread directories, then the oldest files of all threads until the root fits
/// `max_total_bytes` (R5.2). Errors are logged; nothing panics.
pub(super) fn sweep(root: &Path, limits: ToolOutputSpillConfig, now: SystemTime) {
    if check_owned_dir(root).is_err() {
        return;
    }
    let max_age = Duration::from_secs(limits.max_age_days.saturating_mul(SECONDS_PER_DAY));
    let older_than = |file: &SpillFile, age: Duration| {
        now.duration_since(file.modified)
            .is_ok_and(|file_age| file_age >= age)
    };
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut kept = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        if !entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            continue;
        }
        for file in files_in(&dir, TMP_SUFFIX) {
            if older_than(&file, TMP_MAX_AGE) {
                remove_file(&file.path);
            }
        }
        for file in files_in(&dir, SPILL_SUFFIX) {
            if older_than(&file, max_age) {
                remove_file(&file.path);
            } else {
                kept.push(file);
            }
        }
        // Fails while the directory still holds files; only empty ones go.
        let _ = fs::remove_dir(&dir);
    }
    let mut total: u64 = kept.iter().map(|file| file.bytes).sum();
    if total <= limits.max_total_bytes {
        return;
    }
    kept.sort_by_key(|file| file.modified);
    for file in kept {
        if total <= limits.max_total_bytes {
            break;
        }
        remove_file(&file.path);
        total = total.saturating_sub(file.bytes);
    }
}

#[cfg(test)]
pub(super) fn reset_sweep_for_test() {
    SWEPT.store(false, Ordering::Release);
}

#[cfg(test)]
pub(super) fn sweep_once_for_test(root: &Path, limits: ToolOutputSpillConfig) -> bool {
    if SWEPT.swap(true, Ordering::AcqRel) {
        return false;
    }
    sweep(root, limits, SystemTime::now());
    true
}

/// Removes the spill directories of `thread_ids` (R5.3, R5.5). A directory that is a symlink,
/// or a spill root that is not the user's own directory, is left alone.
pub(crate) fn discard_threads_blocking(codex_home: &Path, thread_ids: &[ThreadId]) {
    let root = codex_home.join(SPILL_DIR);
    match fs::symlink_metadata(&root) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        _ => {}
    }
    if let Err(err) = check_owned_dir(&root) {
        warn!(path = %root.display(), %err, "not removing saved tool outputs");
        return;
    }
    for thread_id in thread_ids {
        let dir = root.join(thread_id.to_string());
        let Ok(metadata) = fs::symlink_metadata(&dir) else {
            continue;
        };
        if !metadata.file_type().is_dir() {
            warn!(path = %dir.display(), "not removing saved tool outputs: not a directory");
            continue;
        }
        if let Err(err) = fs::remove_dir_all(&dir) {
            warn!(path = %dir.display(), %err, "failed to remove saved tool outputs");
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
