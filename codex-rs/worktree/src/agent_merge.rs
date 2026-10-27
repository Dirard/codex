//! Three-way merge of an agent branch into the parent's current working tree.
//!
//! The merge runs on objects only: the parent's working tree is committed as a snapshot
//! through a temporary copy of its index, and `git merge-tree --write-tree` merges that
//! snapshot with the agent branch over the recorded base. The parent's index, HEAD, refs and
//! files stay unchanged; the caller writes the result through its own approval path.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC11.

use super::AgentWorktreeMetadata;
use super::agent_branch_for_thread;
use super::format_shortstat;
use super::has_changes;
use super::is_ancestor;
use super::own_branch_tip;
use super::parse_count;
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
    /// Locale-independent `git diff --shortstat` equivalent of `changes` (R11.5).
    pub shortstat: String,
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
    /// The recorded base is not an ancestor of the branch tip, so a three-way merge over it
    /// would not apply the agent's changes.
    BaseNotAncestor {
        tip: String,
    },
    /// Paths that conflict with the parent's edits or with files outside its snapshot.
    Conflicts(Vec<String>),
    /// A change that a text patch cannot reproduce exactly.
    Unrepresentable {
        path: String,
        reason: &'static str,
    },
    /// The merge moves, adds or removes a submodule (a gitlink), which only the parent can
    /// update in its own submodule checkout (R11.12).
    Submodule {
        path: String,
        /// The commit the branch puts the submodule at; `None` when it removes it.
        commit: Option<String>,
    },
    /// The parent's working tree already contains the branch.
    AlreadyApplied {
        tip: String,
    },
    Clean(CleanMerge),
}

/// Merges the agent's own branch into the parent's working tree at `source_root` (R11.3).
///
/// Nothing in the parent's checkout changes: the result lists the files to write. Reported
/// instead of a result (R11.4, R11.6, R11.9): conflicts; an added file that already exists
/// outside the snapshot (an ignored file) or whose directory is an ignored file or symlink
/// in the parent's checkout; paths Codex never writes (`..`, `.git`, `.codex`, `.agents` or
/// `.aws` components); changes a text patch cannot express (binary or non-UTF-8 content,
/// symlinks, executable modes); submodule changes, which the parent makes itself (R11.12);
/// and a recorded base that is not an ancestor of the branch tip.
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
    if !is_ancestor(source_root, &metadata.base_sha, &tip)? {
        return Ok(MergeOutcome::BaseNotAncestor { tip });
    }
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
        if let Some(reason) = forbidden_path_reason(&path) {
            return Ok(unrepresentable(path, reason));
        }
        // A submodule's directory is not a file the write could trip over: the change is to
        // the submodule itself, which the parent updates (R11.12).
        if *old_mode == GITLINK_MODE || *new_mode == GITLINK_MODE {
            return Ok(MergeOutcome::Submodule {
                path,
                commit: (*new_mode == GITLINK_MODE).then(|| (*new_oid).to_string()),
            });
        }
        // What the parent's checkout has on the path, ignored files and symlinks included:
        // the snapshot does not show them, the write would trip over them.
        if blocked_in_checkout(source_root, &path, *status == "A") {
            conflicts.push(path);
            continue;
        }
        let change = match *status {
            "A" => {
                if *new_mode != "100644" {
                    return Ok(unrepresentable(path, mode_reason(new_mode)));
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
    let shortstat = shortstat(source_root, &ours_tree, &merged_tree)?;
    Ok(MergeOutcome::Clean(CleanMerge {
        tip,
        changes,
        shortstat,
        pending_worktree_changes,
    }))
}

/// The tree mode of a gitlink, the entry of a submodule.
const GITLINK_MODE: &str = "160000";

fn unrepresentable(path: String, reason: &'static str) -> MergeOutcome {
    MergeOutcome::Unrepresentable { path, reason }
}

/// Directory names Codex never writes through a merge, at any depth: Git metadata, and the
/// configuration and credential directories the sandbox keeps read-only in writable roots.
const PROTECTED_NAMES: [&str; 4] = [".git", ".codex", ".agents", ".aws"];

/// Why a merged path must not be written, if it must not (P3-1). Git's own `verify_path`
/// rules are mirrored: names are compared case-insensitively, without trailing dots and
/// spaces (NTFS) and without the code points HFS+ ignores, and `git~1` is the 8.3 name of
/// `.git`.
fn forbidden_path_reason(path: &str) -> Option<&'static str> {
    if path.starts_with('/') || path.contains('\\') {
        return Some("a path outside the checkout");
    }
    for component in path.split('/') {
        if matches!(component, "" | "." | "..") {
            return Some("a path outside the checkout");
        }
        let normalized = component
            .chars()
            .filter(|character| !is_hfs_ignorable(*character))
            .collect::<String>()
            .trim_end_matches(['.', ' '])
            .to_lowercase();
        if PROTECTED_NAMES.contains(&normalized.as_str()) || normalized == "git~1" {
            return Some("a protected Git or Codex path");
        }
    }
    None
}

/// Code points HFS+ drops when comparing names (Git's `is_hfs_dotgit` list).
fn is_hfs_ignorable(character: char) -> bool {
    matches!(
        character,
        '\u{200c}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{206a}'..='\u{206f}' | '\u{feff}'
    )
}

/// Whether the parent's checkout blocks writing `path`: a directory on the way is a file or
/// a symlink (ignored ones too), an added file already exists, or a changed file is not a
/// regular file any more.
fn blocked_in_checkout(source_root: &Path, path: &str, added: bool) -> bool {
    let components = path.split('/').collect::<Vec<_>>();
    let mut current = source_root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component);
        let last = index + 1 == components.len();
        match fs::symlink_metadata(&current) {
            Ok(metadata) if !last => {
                if !metadata.file_type().is_dir() {
                    return true;
                }
            }
            Ok(metadata) => return added || !metadata.file_type().is_file(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return !added,
            Err(_) => return true,
        }
    }
    false
}

/// The size of the change from the parent snapshot to the merge result.
fn shortstat(cwd: &Path, from_tree: &str, to_tree: &str) -> Result<String> {
    let numstat = git_output(
        cwd,
        GitOperation::Metadata,
        [
            "diff-tree",
            "-r",
            "-z",
            "--no-renames",
            "--numstat",
            from_tree,
            to_tree,
        ],
    )?;
    let mut files = 0;
    let mut insertions = 0;
    let mut deletions = 0;
    for record in numstat.stdout.split(|byte| *byte == b'\0') {
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let (Some(added), Some(removed), Some(_)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        files += 1;
        insertions += parse_count(added);
        deletions += parse_count(removed);
    }
    Ok(format_shortstat(files, insertions, deletions))
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
