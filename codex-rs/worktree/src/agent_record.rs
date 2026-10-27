//! Host records of where agent worktrees came from, kept outside the worktrees.
//!
//! A worktree's own `config.worktree` disappears with it, so the record is what names the
//! parent checkout, the base and the branch after the worktree was removed and the process
//! restarted. The host writes one JSON file per isolated thread under the Codex home when it
//! creates the worktree; nothing the agent can write is consulted.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC5 (R5.6) and UC9 (R9.3).

use super::AgentWorktreeBaseKind;
use super::AgentWorktreeMetadata;
use super::branch_ref;
use crate::git::GitOperation;
use crate::git::git_output;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

/// The provenance of one agent worktree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentWorktreeRecord {
    pub thread_id: String,
    /// Root of the agent worktree.
    pub root: PathBuf,
    /// The parent's checkout the worktree was created from.
    pub source_root: PathBuf,
    pub base_sha: String,
    pub base_kind: String,
    /// The branch name recorded at creation; checkpoints may use its free names.
    pub branch: String,
    /// The tip of the agent branch the parent applied before Codex deleted the branch (R12.7);
    /// a restore starts from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_tip: Option<String>,
}

impl AgentWorktreeRecord {
    pub fn new(root: &Path, source_root: &Path, metadata: &AgentWorktreeMetadata) -> Self {
        Self {
            thread_id: metadata.thread_id.clone(),
            root: root.to_path_buf(),
            source_root: source_root.to_path_buf(),
            base_sha: metadata.base_sha.clone(),
            base_kind: metadata.base_kind.as_str().to_owned(),
            branch: metadata.branch.clone(),
            applied_tip: None,
        }
    }

    pub fn base_kind(&self) -> Option<AgentWorktreeBaseKind> {
        AgentWorktreeBaseKind::parse(&self.base_kind)
    }

    /// The agent metadata the record describes, with the recorded branch name.
    pub fn metadata(&self) -> Option<AgentWorktreeMetadata> {
        Some(AgentWorktreeMetadata {
            thread_id: self.thread_id.clone(),
            base_sha: self.base_sha.clone(),
            base_kind: self.base_kind()?,
            branch: self.branch.clone(),
            source_root: self.source_root.to_str().map(str::to_owned),
        })
    }
}

/// Only thread IDs name record files, so no value can leave the records directory.
fn record_path(directory: &Path, thread_id: &str) -> Result<PathBuf> {
    if thread_id.is_empty()
        || !thread_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        bail!("`{thread_id}` is not a thread ID");
    }
    Ok(directory.join(format!("{thread_id}.json")))
}

/// Writes the record of `record.thread_id`, replacing an older one atomically.
pub fn write_agent_record(directory: &Path, record: &AgentWorktreeRecord) -> Result<()> {
    let path = record_path(directory, &record.thread_id)?;
    fs::create_dir_all(directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(directory)
        .with_context(|| format!("cannot write a record in {}", directory.display()))?;
    serde_json::to_writer_pretty(&mut file, record)?;
    file.persist(&path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// Reads the record of a thread; `None` when there is none. A record whose base or branch
/// the host could not have written is an error.
pub fn read_agent_record(directory: &Path, thread_id: &str) -> Result<Option<AgentWorktreeRecord>> {
    let path = record_path(directory, thread_id)?;
    let content = match fs::read(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", path.display()));
        }
    };
    let record: AgentWorktreeRecord = serde_json::from_slice(&content)
        .with_context(|| format!("{} is not an agent worktree record", path.display()))?;
    if record.thread_id != thread_id {
        bail!("{} names thread {}", path.display(), record.thread_id);
    }
    let object_id = |value: &str| {
        matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    };
    if !object_id(&record.base_sha) {
        bail!("{} has an invalid base", path.display());
    }
    if record
        .applied_tip
        .as_deref()
        .is_some_and(|tip| !object_id(tip))
    {
        bail!("{} has an invalid applied tip", path.display());
    }
    if record.base_kind().is_none() {
        bail!("{} has an unknown base kind", path.display());
    }
    branch_ref(&record.branch)?;
    Ok(Some(record))
}

/// Tidies the records older than `min_age` whose worktree is gone (P3-2), and returns how
/// many it deleted; unreadable records are left alone.
///
/// - An applied branch (`applied_tip`) that is still the thread's own branch at that tip is
///   retired when the parent's working tree holds its changes
///   ([`super::retire_applied_branch`]): a worktree finalize kept on its branch and a later
///   sweep removed, or a branch applied by an earlier process.
/// - A record whose parent checkout still has one of the recorded branch names stays.
/// - A record whose retired commit ([`super::applied_ref`]) is the only copy of the work
///   stays; once the parent's HEAD contains that work, the ref and the record go.
/// - Any other record can no longer restore anything and is deleted, as is a record whose
///   parent checkout is gone.
pub fn prune_agent_records(directory: &Path, now: SystemTime, min_age: Duration) -> Result<usize> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot list {}", directory.display()));
        }
    };
    let mut pruned = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(thread_id) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".json"))
        else {
            continue;
        };
        let old_enough = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| now.duration_since(modified).unwrap_or_default() >= min_age);
        if !old_enough {
            continue;
        }
        let Ok(Some(record)) = read_agent_record(directory, thread_id) else {
            continue;
        };
        if record.root.exists() || !useless_record(&record) {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            pruned += 1;
        }
    }
    Ok(pruned)
}

/// Whether a record whose worktree is gone can restore nothing any more; retires its applied
/// branch and drops its retired commit on the way when that is safe (P3-2). Errors keep it.
fn useless_record(record: &AgentWorktreeRecord) -> bool {
    if !record.source_root.is_dir() {
        return true;
    }
    let Some(metadata) = record.metadata() else {
        return false;
    };
    // A branch that cannot be retired now stays, and so does the record.
    if let Some(tip) = record.applied_tip.as_deref() {
        let _ = super::retire_applied_branch(&record.source_root, &record.root, &metadata, tip);
    }
    if has_recorded_branch(record) {
        return false;
    }
    // A repository that cannot be read keeps the record.
    drop_retired_commit(record).unwrap_or(false)
}

/// Deletes the retired commit ref of the record's thread once the parent's HEAD contains its
/// changes; whether the record holds nothing else then (no retired commit, or dropped).
fn drop_retired_commit(record: &AgentWorktreeRecord) -> Result<bool> {
    let reference = super::applied_ref(&record.thread_id)?;
    let mut command = super::git_command(&record.source_root, GitOperation::Metadata)?;
    command.args([
        "rev-parse",
        "--verify",
        "--quiet",
        "--end-of-options",
        &format!("{reference}^{{commit}}"),
    ]);
    let output = super::run_git_command(&mut command, &[1])?;
    if !output.status.success() {
        return Ok(true);
    }
    let tip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !super::maintenance::head_contains(&record.source_root, &record.base_sha, &tip)? {
        return Ok(false);
    }
    git_output(
        &record.source_root,
        GitOperation::Metadata,
        [
            "update-ref",
            "-m",
            "codex agent work committed",
            "-d",
            reference.as_str(),
            tip.as_str(),
        ],
    )?;
    Ok(true)
}

/// Whether the parent checkout still has the recorded branch or one of its free names.
fn has_recorded_branch(record: &AgentWorktreeRecord) -> bool {
    if !record.source_root.is_dir() {
        return false;
    }
    let exact = format!("refs/heads/{}", record.branch);
    let free = format!("refs/heads/{}-*", record.branch);
    match git_output(
        &record.source_root,
        GitOperation::Metadata,
        [
            "for-each-ref",
            "--count=1",
            "--format=%(refname)",
            exact.as_str(),
            free.as_str(),
        ],
    ) {
        Ok(output) => !output.stdout.is_empty(),
        // Keep the record when the repository cannot be read.
        Err(_) => true,
    }
}
