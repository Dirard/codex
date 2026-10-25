//! Host-side Git operations for managed worktrees that isolate MultiAgentV2 children.
//!
//! Children run sandboxed with read-only Git metadata, so the host builds their base
//! snapshot, records worktree metadata, commits their changes onto an agent branch and
//! removes worktrees that never changed. Every command runs through the crate's isolated
//! Git invocation: hooks, fsmonitor, configured filters and commit signing are disabled.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, use cases 2, 4 and 6.

use crate::git::GitOperation;
use crate::git::git_command;
use crate::git::git_output;
use crate::git::git_path;
use crate::git::run_git_command;
use crate::paths::remove_empty_bucket;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

/// Identity recorded on host-created snapshot and checkpoint commits.
pub const AGENT_COMMIT_AUTHOR_NAME: &str = "Codex Agent";
pub const AGENT_COMMIT_AUTHOR_EMAIL: &str = "codex-agent@localhost.invalid";

/// Changed paths listed in a branch summary before the remainder is only counted.
pub const MAX_LISTED_CHANGED_PATHS: usize = 20;

const THREAD_KEY: &str = "codex.agentThread";
const BASE_KEY: &str = "codex.agentBase";
const BASE_KIND_KEY: &str = "codex.agentBaseKind";
const BRANCH_KEY: &str = "codex.agentBranch";

/// The commit an agent worktree starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentWorktreeBaseKind {
    /// The parent's HEAD plus tracked modifications and untracked, non-ignored files.
    WorkingTree,
    /// The parent's HEAD commit.
    Head,
}

impl AgentWorktreeBaseKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkingTree => "working_tree",
            Self::Head => "head",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "working_tree" => Some(Self::WorkingTree),
            "head" => Some(Self::Head),
            _ => None,
        }
    }

    /// Human-readable base description used in agent-facing text.
    pub fn description(self) -> &'static str {
        match self {
            Self::WorkingTree => "snapshot of the parent working tree",
            Self::Head => "parent HEAD",
        }
    }
}

/// Agent identity and base recorded in the worktree's `config.worktree`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWorktreeMetadata {
    pub thread_id: String,
    pub base_sha: String,
    pub base_kind: AgentWorktreeBaseKind,
    /// Created lazily by the first checkpoint that finds changes.
    pub branch: String,
}

/// Resolves the full SHA of the commit checked out at `source_cwd`.
pub fn resolve_head_commit(source_cwd: &Path) -> Result<String> {
    let root = repository_root(source_cwd)?;
    optional_commit(&root, "HEAD")?.context("the parent repository has no commits yet")
}

/// Commits a snapshot of the parent working tree without touching its index or refs.
///
/// The tree is HEAD plus tracked modifications and untracked, non-ignored files. It is built
/// in a temporary copy of the parent's index, so the parent's index, HEAD, refs and files stay
/// unchanged. Unmerged index entries make the snapshot fail: staging would otherwise record
/// files with conflict markers as resolved.
pub fn snapshot_working_tree(source_cwd: &Path) -> Result<String> {
    let root = repository_root(source_cwd)?;
    let head = optional_commit(&root, "HEAD")?;
    let temporary = tempfile::tempdir().context("cannot create a temporary index directory")?;
    let index = temporary.path().join("index");
    copy_index(&root, &index)?;
    if has_unmerged_entries(&root, &index)? {
        bail!("the parent index has unmerged paths; resolve them or use the `head` base");
    }
    stage_all(&root, &index)?;
    let tree = write_tree(&root, &index)?;
    commit_tree(
        &root,
        &tree,
        head.as_deref(),
        "Codex agent base snapshot of the parent working tree",
    )
}

/// Records agent ownership and base details in the worktree's `config.worktree`.
pub fn write_agent_metadata(root: &Path, metadata: &AgentWorktreeMetadata) -> Result<()> {
    let config = worktree_config_path(root)?;
    for (key, value) in [
        (THREAD_KEY, metadata.thread_id.as_str()),
        (BASE_KEY, metadata.base_sha.as_str()),
        (BASE_KIND_KEY, metadata.base_kind.as_str()),
        (BRANCH_KEY, metadata.branch.as_str()),
    ] {
        git_output(
            root,
            GitOperation::Metadata,
            [
                OsStr::new("config"),
                OsStr::new("--file"),
                config.as_os_str(),
                OsStr::new(key),
                OsStr::new(value),
            ],
        )
        .with_context(|| format!("cannot record {key} for the agent worktree"))?;
    }
    Ok(())
}

/// Reads metadata written by [`write_agent_metadata`]; `None` when the worktree has none.
pub fn read_agent_metadata(root: &Path) -> Result<Option<AgentWorktreeMetadata>> {
    let config = worktree_config_path(root)?;
    if !config.exists() {
        return Ok(None);
    }
    let mut command = git_command(root, GitOperation::Metadata)?;
    command.args([
        OsStr::new("config"),
        OsStr::new("--file"),
        config.as_os_str(),
        OsStr::new("--null"),
        OsStr::new("--get-regexp"),
        OsStr::new("^codex\\.agent"),
    ]);
    let output = run_git_command(&mut command, &[1])?;
    let mut thread_id = None;
    let mut base_sha = None;
    let mut base_kind = None;
    let mut branch = None;
    for entry in output.stdout.split(|byte| *byte == b'\0') {
        let Some(separator) = entry.iter().position(|byte| *byte == b'\n') else {
            continue;
        };
        let key = String::from_utf8_lossy(&entry[..separator]);
        let value = String::from_utf8(entry[separator + 1..].to_vec())
            .context("agent worktree metadata is not valid UTF-8")?;
        // Git reports section and variable names in lower case.
        match key.as_ref() {
            "codex.agentthread" => thread_id = Some(value),
            "codex.agentbase" => base_sha = Some(value),
            "codex.agentbasekind" => base_kind = Some(value),
            "codex.agentbranch" => branch = Some(value),
            _ => {}
        }
    }
    let (Some(thread_id), Some(base_sha), Some(base_kind), Some(branch)) =
        (thread_id, base_sha, base_kind, branch)
    else {
        return Ok(None);
    };
    let base_kind = AgentWorktreeBaseKind::parse(&base_kind)
        .with_context(|| format!("unknown agent worktree base kind `{base_kind}`"))?;
    Ok(Some(AgentWorktreeMetadata {
        thread_id,
        base_sha,
        base_kind,
        branch,
    }))
}

/// Identifies the agent whose changes a checkpoint records.
#[derive(Clone, Copy, Debug)]
pub struct CheckpointRequest<'a> {
    pub root: &'a Path,
    pub metadata: &'a AgentWorktreeMetadata,
    pub agent_path: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointOutcome {
    /// Nothing changed since the base or the last checkpoint; no commit or branch was made.
    Unchanged,
    /// The agent branch now points at `commit` and the worktree HEAD is attached to it.
    Committed {
        commit: String,
        /// Set when the commit succeeded but refreshing the worktree index did not.
        index_refresh_error: Option<String>,
    },
}

/// Commits the worktree's changes onto the agent branch (spec step "checkpoint the worktree").
///
/// Tracked changes and untracked, non-ignored files are committed through a temporary
/// index onto `refs/heads/<branch>`. The update refuses to drop commits: an existing branch
/// must be an ancestor of the new commit, and a branch created or moved concurrently fails
/// the compare-and-swap. The worktree HEAD is then attached to the branch and its index
/// refreshed without touching files. A clean worktree at its last recorded commit produces
/// no commit; a failure leaves the branch, HEAD, index and files unchanged.
pub fn checkpoint(request: CheckpointRequest<'_>) -> Result<CheckpointOutcome> {
    let root = request.root;
    let metadata = request.metadata;
    let branch_ref = branch_ref(&metadata.branch)?;
    let head = optional_commit(root, "HEAD")?.context("the agent worktree has no HEAD commit")?;
    let branch_tip = optional_commit(root, &branch_ref)?;
    let recorded = branch_tip.as_deref().unwrap_or(&metadata.base_sha);
    let dirty = has_changes(root)?;
    if !dirty && head == recorded {
        return Ok(CheckpointOutcome::Unchanged);
    }
    if let Some(tip) = branch_tip.as_deref()
        && !is_ancestor(root, tip, &head)?
    {
        bail!(
            "agent branch {} points at {tip}, which is not in the worktree history; refusing to overwrite it",
            metadata.branch
        );
    }

    let commit = if dirty {
        let temporary = tempfile::tempdir().context("cannot create a temporary index directory")?;
        let index = temporary.path().join("index");
        copy_index(root, &index)?;
        stage_all(root, &index)?;
        let tree = write_tree(root, &index)?;
        let message = format!(
            "codex agent {agent_path} checkpoint\n\nCodex-Agent-Path: {agent_path}\nCodex-Thread-Id: {thread_id}\n",
            agent_path = request.agent_path,
            thread_id = metadata.thread_id,
        );
        commit_tree(root, &tree, Some(&head), &message)?
    } else {
        // The agent committed by itself (for example with full access); keep its commits.
        head.clone()
    };

    git_output(
        root,
        GitOperation::Metadata,
        [
            OsStr::new("update-ref"),
            OsStr::new("-m"),
            OsStr::new("codex agent checkpoint"),
            OsStr::new(&branch_ref),
            OsStr::new(&commit),
            OsStr::new(branch_tip.as_deref().unwrap_or("")),
        ],
    )
    .context("cannot update the agent branch")?;
    git_output(
        root,
        GitOperation::Metadata,
        ["symbolic-ref", "HEAD", branch_ref.as_str()],
    )
    .context("cannot attach the agent worktree to its branch")?;
    let index_refresh_error = git_output(root, GitOperation::WorkingTree, ["reset", "-q"])
        .err()
        .map(|error| error.to_string());
    Ok(CheckpointOutcome::Committed {
        commit,
        index_refresh_error,
    })
}

/// One path changed between the base and the agent branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedPath {
    /// Git name-status letter, for example `M`, `A` or `D`.
    pub status: String,
    pub path: String,
}

/// Changes on an agent branch relative to its base.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchChanges {
    /// Locale-independent `git diff --shortstat` equivalent.
    pub shortstat: String,
    /// At most [`MAX_LISTED_CHANGED_PATHS`] changed paths in Git order.
    pub files: Vec<ChangedPath>,
    pub total_files: usize,
}

/// Summarizes the agent branch against its base; `None` when the branch does not exist.
pub fn branch_changes(root: &Path, metadata: &AgentWorktreeMetadata) -> Result<Option<BranchChanges>> {
    let branch_ref = branch_ref(&metadata.branch)?;
    if optional_commit(root, &branch_ref)?.is_none() {
        return Ok(None);
    }
    let range = [metadata.base_sha.as_str(), branch_ref.as_str()];
    let numstat = git_output(
        root,
        GitOperation::Metadata,
        [
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--numstat",
            "-z",
            range[0],
            range[1],
            "--",
        ],
    )?;
    let mut insertions = 0_u64;
    let mut deletions = 0_u64;
    for record in numstat.stdout.split(|byte| *byte == b'\0') {
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let (Some(added), Some(removed)) = (fields.next(), fields.next()) else {
            continue;
        };
        insertions += parse_count(added);
        deletions += parse_count(removed);
    }
    let name_status = git_output(
        root,
        GitOperation::Metadata,
        [
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--name-status",
            "-z",
            range[0],
            range[1],
            "--",
        ],
    )?;
    let mut fields = name_status
        .stdout
        .split(|byte| *byte == b'\0')
        .filter(|field| !field.is_empty());
    let mut files = Vec::new();
    let mut total_files = 0;
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        total_files += 1;
        if files.len() < MAX_LISTED_CHANGED_PATHS {
            files.push(ChangedPath {
                status: String::from_utf8_lossy(status).into_owned(),
                path: String::from_utf8_lossy(path).into_owned(),
            });
        }
    }
    Ok(Some(BranchChanges {
        shortstat: format_shortstat(total_files, insertions, deletions),
        files,
        total_files,
    }))
}

/// Why [`remove_if_unchanged`] kept a worktree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeptReason {
    /// The agent branch exists.
    Branch,
    /// The worktree has uncommitted changes.
    Changes,
    /// The worktree HEAD moved away from the base.
    Commits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    /// The checkout no longer exists; only its registration was pruned.
    AlreadyMissing,
    Kept(KeptReason),
}

/// Removes an agent worktree that never changed (spec step "remove the worktree").
///
/// Keeps the worktree when its branch exists, when it has tracked or untracked non-ignored
/// changes, or when its HEAD left the base. Ignored files such as build output do not prevent
/// removal.
pub fn remove_if_unchanged(
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<RemoveOutcome> {
    if !root.join(".git").exists() {
        if source_root.is_dir() {
            let _ = git_output(source_root, GitOperation::Metadata, ["worktree", "prune"]);
        }
        remove_empty_bucket(root);
        return Ok(RemoveOutcome::AlreadyMissing);
    }
    let branch_ref = branch_ref(&metadata.branch)?;
    if optional_commit(root, &branch_ref)?.is_some() {
        return Ok(RemoveOutcome::Kept(KeptReason::Branch));
    }
    if has_changes(root)? {
        return Ok(RemoveOutcome::Kept(KeptReason::Changes));
    }
    if optional_commit(root, "HEAD")?.as_deref() != Some(metadata.base_sha.as_str()) {
        return Ok(RemoveOutcome::Kept(KeptReason::Commits));
    }
    // `--force` is needed for ignored files; the checks above exclude every other change.
    let cwd = if source_root.is_dir() {
        source_root
    } else {
        root
    };
    git_output(
        cwd,
        GitOperation::WorkingTree,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            OsStr::new("--force"),
            root.as_os_str(),
        ],
    )
    .context("cannot remove the unchanged agent worktree")?;
    remove_empty_bucket(root);
    Ok(RemoveOutcome::Removed)
}

/// Removes a worktree created for a child that never started; it holds no agent work.
pub fn discard_agent_worktree(source_root: &Path, root: &Path) -> Result<()> {
    let result = git_output(
        source_root,
        GitOperation::WorkingTree,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            OsStr::new("--force"),
            root.as_os_str(),
        ],
    );
    remove_empty_bucket(root);
    result.map(|_| ())
}

/// Validates an agent branch name and returns its full ref.
fn branch_ref(branch: &str) -> Result<String> {
    if !branch.starts_with("codex/agents/")
        || branch.contains("..")
        || branch
            .chars()
            .any(|character| character.is_whitespace() || "~^:?*[\\".contains(character))
    {
        bail!("invalid agent branch name `{branch}`");
    }
    Ok(format!("refs/heads/{branch}"))
}

fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    let mut command = git_command(root, GitOperation::Metadata)?;
    command.args(["merge-base", "--is-ancestor", ancestor, descendant]);
    Ok(run_git_command(&mut command, &[1])?.status.success())
}

fn has_changes(root: &Path) -> Result<bool> {
    let status = git_output(
        root,
        GitOperation::WorkingTree,
        ["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(!status.stdout.is_empty())
}

fn optional_commit(cwd: &Path, revision: &str) -> Result<Option<String>> {
    let mut command = git_command(cwd, GitOperation::Metadata)?;
    command.args([
        "rev-parse",
        "--verify",
        "--quiet",
        "--end-of-options",
        &format!("{revision}^{{commit}}"),
    ]);
    let output = run_git_command(&mut command, &[1])?;
    if !output.status.success() {
        return Ok(None);
    }
    let sha = String::from_utf8(output.stdout).context("git output is not valid UTF-8")?;
    let sha = sha.trim();
    Ok((!sha.is_empty()).then(|| sha.to_owned()))
}

fn copy_index(root: &Path, destination: &Path) -> Result<()> {
    let index = git_path(
        root,
        ["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?;
    match fs::copy(&index, destination) {
        Ok(_) => Ok(()),
        // A repository without an index starts from an empty one.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("cannot copy the Git index {}", index.display()))
        }
    }
}

fn has_unmerged_entries(root: &Path, index: &Path) -> Result<bool> {
    let mut command = git_command(root, GitOperation::Metadata)?;
    command
        .env("GIT_INDEX_FILE", index)
        .args(["ls-files", "--unmerged", "-z"]);
    Ok(!run_git_command(&mut command, &[])?.stdout.is_empty())
}

fn stage_all(root: &Path, index: &Path) -> Result<()> {
    let mut command = git_command(root, GitOperation::WorkingTree)?;
    command
        .env("GIT_INDEX_FILE", index)
        .args(["add", "--all", "--", "."]);
    run_git_command(&mut command, &[])
        .map(|_| ())
        .context("cannot stage the working tree into a temporary index")
}

fn write_tree(root: &Path, index: &Path) -> Result<String> {
    let mut command = git_command(root, GitOperation::Metadata)?;
    command.env("GIT_INDEX_FILE", index).arg("write-tree");
    stdout_string(run_git_command(&mut command, &[])?.stdout)
}

fn commit_tree(root: &Path, tree: &str, parent: Option<&str>, message: &str) -> Result<String> {
    let mut command = git_command(root, GitOperation::Metadata)?;
    set_agent_identity(&mut command);
    command.args(["commit-tree", "--no-gpg-sign", "-m", message]);
    if let Some(parent) = parent {
        command.args(["-p", parent]);
    }
    command.arg(tree);
    stdout_string(run_git_command(&mut command, &[])?.stdout)
}

fn set_agent_identity(command: &mut Command) {
    command
        .env("GIT_AUTHOR_NAME", AGENT_COMMIT_AUTHOR_NAME)
        .env("GIT_AUTHOR_EMAIL", AGENT_COMMIT_AUTHOR_EMAIL)
        .env("GIT_COMMITTER_NAME", AGENT_COMMIT_AUTHOR_NAME)
        .env("GIT_COMMITTER_EMAIL", AGENT_COMMIT_AUTHOR_EMAIL);
}

fn stdout_string(stdout: Vec<u8>) -> Result<String> {
    let value = String::from_utf8(stdout).context("git output is not valid UTF-8")?;
    let value = value.trim();
    if value.is_empty() {
        bail!("git returned no object id");
    }
    Ok(value.to_owned())
}

fn worktree_config_path(root: &Path) -> Result<PathBuf> {
    git_path(
        root,
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    )
}

fn repository_root(cwd: &Path) -> Result<PathBuf> {
    let path = git_path(cwd, ["rev-parse", "--show-toplevel"]).with_context(|| {
        format!("{} is not inside a Git working tree", cwd.display())
    })?;
    dunce::canonicalize(&path)
        .with_context(|| format!("cannot resolve repository root {}", path.display()))
}

fn parse_count(field: &[u8]) -> u64 {
    std::str::from_utf8(field)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn format_shortstat(files: usize, insertions: u64, deletions: u64) -> String {
    let mut parts = vec![format!(
        "{files} file{} changed",
        if files == 1 { "" } else { "s" }
    )];
    if insertions > 0 {
        parts.push(format!(
            "{insertions} insertion{}(+)",
            if insertions == 1 { "" } else { "s" }
        ));
    }
    if deletions > 0 {
        parts.push(format!(
            "{deletions} deletion{}(-)",
            if deletions == 1 { "" } else { "s" }
        ));
    }
    parts.join(", ")
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
