//! Three-way merge of an agent branch into the parent's current working tree.
//!
//! The merge runs on objects only: the parent's working tree is committed as a snapshot
//! through a temporary copy of its index, and `git merge-tree --write-tree` merges that
//! snapshot with the agent branch over the recorded base. The parent's index, HEAD, refs and
//! files stay unchanged; the caller writes the result through its own approval path.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC11.

use super::AgentWorktreeMetadata;
use super::agent_branch_for_thread;
use super::has_changes;
use super::own_branch_tip;
use super::snapshot_working_tree;
use crate::git::GitOperation;
use crate::git::git_command;
use crate::git::git_output;
use anyhow::Context;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// One file the merge changes in the parent's checkout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedChange {
    /// Path relative to the parent's checkout root.
    pub path: String,
    /// Current content; `None` when the merge adds the file.
    pub old: Option<String>,
    /// Merged content; `None` when the merge deletes the file.
    pub new: Option<String>,
}

/// A clean merge of the agent branch into the parent's working tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CleanMerge {
    /// The branch tip that was merged.
    pub tip: String,
    pub changes: Vec<MergedChange>,
    /// The agent's worktree has changes its branch does not record yet.
    pub pending_worktree_changes: bool,
}

/// Result of [`merge_agent_branch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// The agent has no branch of its own, so it recorded no changes.
    NoBranch {
        pending_worktree_changes: bool,
    },
    /// Git cannot merge without a working tree (older than 2.40).
    Unsupported(String),
    /// Paths that conflict with the parent's edits or with files outside its snapshot.
    Conflicts(Vec<String>),
    /// A change that a text patch cannot reproduce exactly.
    Unrepresentable {
        path: String,
        reason: &'static str,
    },
    /// The parent's working tree already contains the branch.
    AlreadyApplied {
        tip: String,
    },
    Clean(CleanMerge),
}

/// Merges the agent's own branch into the parent's working tree at `source_root` (R11.3).
///
/// Nothing in the parent's checkout changes: the result lists the files to write. Conflicts,
/// added files that already exist outside the snapshot (ignored files), and changes a text
/// patch cannot express (binary or non-UTF-8 content, symlinks, submodules, executable modes)
/// are reported instead of a result (R11.4, R11.6).
pub fn merge_agent_branch(
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<MergeOutcome> {
    let worktree_exists = root.join(".git").exists();
    let pending_worktree_changes = worktree_exists && has_changes(root)?;
    let tip = if worktree_exists {
        own_branch_tip(root, &metadata.branch)?
    } else {
        agent_branch_for_thread(source_root, &metadata.branch, &metadata.thread_id)?
            .map(|branch| branch.tip)
    };
    let Some(tip) = tip else {
        return Ok(MergeOutcome::NoBranch {
            pending_worktree_changes,
        });
    };
    let ours =
        snapshot_working_tree(source_root).context("cannot snapshot the parent working tree")?;
    let ours_tree = tree_of(source_root, &ours)?;

    let mut command = git_command(source_root, GitOperation::Metadata)?;
    command.args([
        "merge-tree".to_string(),
        "--write-tree".to_string(),
        "--name-only".to_string(),
        "-z".to_string(),
        "--no-messages".to_string(),
        format!("--merge-base={}", metadata.base_sha),
        ours,
        tip.clone(),
    ]);
    let output = command.output().context("failed to start git")?;
    let mut fields = output
        .stdout
        .split(|byte| *byte == b'\0')
        .map(|field| String::from_utf8_lossy(field).trim().to_string());
    let merged_tree = fields.next().unwrap_or_default();
    match output.status.code() {
        Some(0) => {}
        Some(1) => {
            let mut paths = fields.filter(|path| !path.is_empty()).collect::<Vec<_>>();
            paths.dedup();
            return Ok(MergeOutcome::Conflicts(paths));
        }
        _ => {
            return Ok(MergeOutcome::Unsupported(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
    }

    let diff = git_output(
        source_root,
        GitOperation::Metadata,
        [
            "diff-tree",
            "-r",
            "-z",
            "--no-renames",
            "--raw",
            ours_tree.as_str(),
            merged_tree.as_str(),
        ],
    )?;
    let mut records = diff.stdout.split(|byte| *byte == b'\0');
    let mut changes = Vec::new();
    let mut conflicts = Vec::new();
    while let (Some(header), Some(path)) = (records.next(), records.next()) {
        if header.is_empty() {
            break;
        }
        let header = String::from_utf8_lossy(header);
        let header = header.trim_start_matches(':');
        let parts = header.split_whitespace().collect::<Vec<_>>();
        let [old_mode, new_mode, old_oid, new_oid, status] = parts.as_slice() else {
            continue;
        };
        let Ok(path) = String::from_utf8(path.to_vec()) else {
            return Ok(unrepresentable(
                String::from_utf8_lossy(path).into_owned(),
                "a file name that is not valid UTF-8",
            ));
        };
        if path.contains(['\n', '\r']) {
            return Ok(unrepresentable(path, "a file name with a line break"));
        }
        let change = match *status {
            "A" => {
                if *new_mode != "100644" {
                    return Ok(unrepresentable(path, mode_reason(new_mode)));
                }
                if fs::symlink_metadata(source_root.join(&path)).is_ok() {
                    conflicts.push(path);
                    continue;
                }
                MergedChange {
                    new: Some(match blob_text(source_root, new_oid)? {
                        Some(text) => text,
                        None => return Ok(binary(path)),
                    }),
                    old: None,
                    path,
                }
            }
            "D" => {
                if !matches!(*old_mode, "100644" | "100755") {
                    return Ok(unrepresentable(path, mode_reason(old_mode)));
                }
                MergedChange {
                    old: Some(match blob_text(source_root, old_oid)? {
                        Some(text) => text,
                        None => return Ok(binary(path)),
                    }),
                    new: None,
                    path,
                }
            }
            "M" if old_mode == new_mode && matches!(*new_mode, "100644" | "100755") => {
                let (Some(old), Some(new)) = (
                    blob_text(source_root, old_oid)?,
                    blob_text(source_root, new_oid)?,
                ) else {
                    return Ok(binary(path));
                };
                MergedChange {
                    path,
                    old: Some(old),
                    new: Some(new),
                }
            }
            "M" if old_mode != new_mode => {
                return Ok(unrepresentable(path, "a file mode change"));
            }
            _ => return Ok(unrepresentable(path, mode_reason(new_mode))),
        };
        changes.push(change);
    }
    if !conflicts.is_empty() {
        return Ok(MergeOutcome::Conflicts(conflicts));
    }
    if changes.is_empty() {
        return Ok(MergeOutcome::AlreadyApplied { tip });
    }
    Ok(MergeOutcome::Clean(CleanMerge {
        tip,
        changes,
        pending_worktree_changes,
    }))
}

fn unrepresentable(path: String, reason: &'static str) -> MergeOutcome {
    MergeOutcome::Unrepresentable { path, reason }
}

fn binary(path: String) -> MergeOutcome {
    unrepresentable(path, "binary or non-UTF-8 content")
}

fn mode_reason(mode: &str) -> &'static str {
    match mode {
        "100755" => "an executable file",
        "120000" => "a symbolic link",
        "160000" => "a submodule",
        _ => "an unsupported file type",
    }
}

fn tree_of(cwd: &Path, commit: &str) -> Result<String> {
    let output = git_output(
        cwd,
        GitOperation::Metadata,
        ["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Blob content when it is UTF-8 text without NUL bytes.
fn blob_text(cwd: &Path, oid: &str) -> Result<Option<String>> {
    let output = git_output(cwd, GitOperation::Metadata, ["cat-file", "blob", oid])?;
    if output.stdout.contains(&0) {
        return Ok(None);
    }
    Ok(String::from_utf8(output.stdout).ok())
}
