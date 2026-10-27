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
use std::ffi::OsString;
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

/// Highest suffix of a free agent branch name: `<branch>-2` through `<branch>-20`.
const LAST_FREE_BRANCH_SUFFIX: usize = 20;

const AGENT_PATH_TRAILER: &str = "Codex-Agent-Path";
const THREAD_TRAILER: &str = "Codex-Thread-Id";
const BASE_TRAILER: &str = "Codex-Agent-Base";
const BASE_KIND_TRAILER: &str = "Codex-Agent-Base-Kind";
/// Message of a `working_tree` base snapshot commit.
const SNAPSHOT_MESSAGE: &str = "Codex agent base snapshot of the parent working tree";

const THREAD_KEY: &str = "codex.agentThread";
const BASE_KEY: &str = "codex.agentBase";
const BASE_KIND_KEY: &str = "codex.agentBaseKind";
const BRANCH_KEY: &str = "codex.agentBranch";
const SOURCE_KEY: &str = "codex.agentSource";

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
    /// The parent checkout the worktree was created from. It can be another linked worktree,
    /// so the repository's main checkout is not a substitute. `None` when not UTF-8.
    pub source_root: Option<String>,
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
    stage_all(&root, &index, &[])?;
    let tree = write_tree(&root, &index)?;
    commit_tree(&root, &tree, head.as_deref(), SNAPSHOT_MESSAGE)
}

/// Records agent ownership and base details in the worktree's `config.worktree`.
pub fn write_agent_metadata(root: &Path, metadata: &AgentWorktreeMetadata) -> Result<()> {
    let config = worktree_config_path(root)?;
    for (key, value) in [
        (THREAD_KEY, metadata.thread_id.as_str()),
        (BASE_KEY, metadata.base_sha.as_str()),
        (BASE_KIND_KEY, metadata.base_kind.as_str()),
        (BRANCH_KEY, metadata.branch.as_str()),
        (
            SOURCE_KEY,
            metadata.source_root.as_deref().unwrap_or_default(),
        ),
    ] {
        if value.is_empty() {
            continue;
        }
        set_metadata_value(root, &config, key, value)?;
    }
    Ok(())
}

fn set_metadata_value(root: &Path, config: &Path, key: &str, value: &str) -> Result<()> {
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
    .map(|_| ())
    .with_context(|| format!("cannot record {key} for the agent worktree"))
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
    let mut source_root = None;
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
            "codex.agentsource" => source_root = Some(value),
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
    // The base and branch reach the parent's merge command and the completion block, so
    // only values the host itself could have written are accepted.
    if !matches!(base_sha.len(), 40 | 64) || !base_sha.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("agent worktree base `{base_sha}` is not a full object ID");
    }
    branch_ref(&branch)?;
    Ok(Some(AgentWorktreeMetadata {
        thread_id,
        base_sha,
        base_kind,
        branch,
        source_root,
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
    /// `branch` now points at `commit` and the worktree HEAD is attached to it.
    Committed {
        commit: String,
        /// The agent branch that holds the work. It differs from the recorded branch when
        /// that name belonged to another worktree; the new name is already persisted.
        branch: String,
        /// Set when the commit succeeded but refreshing the worktree index did not.
        index_refresh_error: Option<String>,
    },
}

/// Commits the worktree's changes onto the agent branch (spec step "checkpoint the worktree").
///
/// Tracked changes and untracked, non-ignored files are committed through a temporary
/// index onto the worktree's own agent branch. A branch is the worktree's own while it does
/// not exist or its tip lies in the worktree history, so a checkpoint never drops commits or
/// moves another worktree's branch. When the recorded name belongs to someone else, the
/// first own `<branch>-<n>` is persisted as the agent branch and used instead. A branch
/// created or moved concurrently fails the compare-and-swap. The worktree HEAD is then
/// attached to the branch and its index refreshed without touching files. A clean worktree
/// at its last recorded commit produces no commit; a failure leaves branches, HEAD, index
/// and files unchanged.
pub fn checkpoint(request: CheckpointRequest<'_>) -> Result<CheckpointOutcome> {
    let root = request.root;
    let metadata = request.metadata;
    let head = optional_commit(root, "HEAD")?.context("the agent worktree has no HEAD commit")?;
    let branch = owned_branch(root, &metadata.branch, &head)?;
    let recorded = branch.tip.as_deref().unwrap_or(&metadata.base_sha);
    let dirty = has_changes(root)?;
    if !dirty && head == recorded {
        return Ok(CheckpointOutcome::Unchanged);
    }

    let commit = if dirty {
        let temporary = tempfile::tempdir().context("cannot create a temporary index directory")?;
        let index = temporary.path().join("index");
        copy_index(root, &index)?;
        // An emptied or removed submodule directory is not the agent's change (R2.11).
        stage_all(root, &index, &unpopulated_gitlinks(root)?)?;
        let tree = write_tree(root, &index)?;
        // The base trailers let a removed worktree be restored from its branch (R4.9).
        let message = format!(
            "codex agent {agent_path} checkpoint\n\n{AGENT_PATH_TRAILER}: {agent_path}\n{THREAD_TRAILER}: {thread_id}\n{BASE_TRAILER}: {base}\n{BASE_KIND_TRAILER}: {base_kind}\n",
            agent_path = request.agent_path,
            thread_id = metadata.thread_id,
            base = metadata.base_sha,
            base_kind = metadata.base_kind.as_str(),
        );
        commit_tree(root, &tree, Some(&head), &message)?
    } else {
        // The agent committed by itself (for example with full access); keep its commits.
        head
    };

    if branch.name != metadata.branch {
        // Persist the free name before any ref moves, so a failure leaves refs as they were
        // and a restart finds the branch that holds the work.
        set_metadata_value(root, &worktree_config_path(root)?, BRANCH_KEY, &branch.name)?;
    }
    git_output(
        root,
        GitOperation::Metadata,
        [
            OsStr::new("update-ref"),
            OsStr::new("-m"),
            OsStr::new("codex agent checkpoint"),
            OsStr::new(&branch.reference),
            OsStr::new(&commit),
            OsStr::new(branch.tip.as_deref().unwrap_or("")),
        ],
    )
    .context("cannot update the agent branch")?;
    git_output(
        root,
        GitOperation::Metadata,
        ["symbolic-ref", "HEAD", branch.reference.as_str()],
    )
    .context("cannot attach the agent worktree to its branch")?;
    let index_refresh_error = git_output(root, GitOperation::WorkingTree, ["reset", "-q"])
        .err()
        .map(|error| error.to_string());
    Ok(CheckpointOutcome::Committed {
        commit,
        branch: branch.name,
        index_refresh_error,
    })
}

/// The agent branch a checkpoint records onto.
struct OwnedBranch {
    name: String,
    reference: String,
    /// Current tip; `None` while the branch does not exist yet.
    tip: Option<String>,
}

/// Picks the worktree's own agent branch: the recorded one, or the first own free name.
///
/// A branch is the worktree's own while it does not exist or its tip is an ancestor of
/// `head`. A colliding sibling's branch or one moved outside the worktree is someone else's
/// and is never moved; `<branch>-2` through `<branch>-20` are tried instead. A candidate
/// that already holds this worktree's history is reused, which also covers a free name
/// whose persistence failed earlier.
fn owned_branch(root: &Path, recorded: &str, head: &str) -> Result<OwnedBranch> {
    let candidates = std::iter::once(recorded.to_owned())
        .chain((2..=LAST_FREE_BRANCH_SUFFIX).map(|suffix| format!("{recorded}-{suffix}")));
    for name in candidates {
        let reference = branch_ref(&name)?;
        let tip = optional_commit(root, &reference)?;
        if let Some(tip) = tip.as_deref()
            && !is_ancestor(root, tip, head)?
        {
            continue;
        }
        return Ok(OwnedBranch {
            name,
            reference,
            tip,
        });
    }
    bail!(
        "agent branch {recorded} and its free names up to {recorded}-{LAST_FREE_BRANCH_SUFFIX} hold other work; refusing to overwrite them"
    )
}

/// Tip of `branch` when it exists and lies in the worktree history.
fn own_branch_tip(root: &Path, branch: &str) -> Result<Option<String>> {
    let reference = branch_ref(branch)?;
    let (Some(tip), Some(head)) = (
        optional_commit(root, &reference)?,
        optional_commit(root, "HEAD")?,
    ) else {
        return Ok(None);
    };
    Ok(is_ancestor(root, &tip, &head)?.then_some(tip))
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

/// Summarizes the agent branch against its base.
///
/// `None` when the branch does not exist or is not the worktree's own: a branch of the same
/// name outside the worktree history holds someone else's work.
pub fn branch_changes(
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<Option<BranchChanges>> {
    let branch_ref = branch_ref(&metadata.branch)?;
    if own_branch_tip(root, &metadata.branch)?.is_none() {
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
    /// A live binding of some process holds the worktree ([`hold_agent_worktree`]).
    InUse,
}

/// File in a worktree's administration directory that live bindings lock shared.
const HOLD_FILE: &str = "codex-agent.lock";

/// A shared hold of an agent worktree by a live binding (R10.3).
///
/// While any process keeps one, [`remove_if_unchanged`] and the sweep of every process leave
/// the worktree alone: removal needs the exclusive lock. The kernel drops the lock when the
/// process exits, so a crashed process never blocks the cleanup of its worktrees.
#[derive(Debug)]
pub struct AgentWorktreeHold {
    _file: fs::File,
}

/// Holds an agent worktree for a live binding; waits while a removal holds it exclusively.
pub fn hold_agent_worktree(root: &Path) -> Result<AgentWorktreeHold> {
    let file = open_hold_file(root)?;
    file.lock_shared()
        .with_context(|| format!("cannot hold the agent worktree {}", root.display()))?;
    Ok(AgentWorktreeHold { _file: file })
}

/// Attempts at the exclusive lock before a worktree counts as held.
const EXCLUSIVE_ATTEMPTS: u32 = 5;

/// The exclusive lock of a worktree that no live binding holds; `None` while one does.
///
/// A child process between `fork` and `exec` briefly shares every open file of its parent,
/// lock included, so a hold dropped a moment ago can still look taken; a few short retries
/// let such a copy go away.
fn exclusive_hold(root: &Path) -> Result<Option<fs::File>> {
    let file = open_hold_file(root)?;
    for attempt in 1..=EXCLUSIVE_ATTEMPTS {
        match file.try_lock() {
            Ok(()) => return Ok(Some(file)),
            Err(fs::TryLockError::WouldBlock) if attempt < EXCLUSIVE_ATTEMPTS => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(fs::TryLockError::WouldBlock) => return Ok(None),
            Err(fs::TryLockError::Error(error)) => {
                return Err(error)
                    .with_context(|| format!("cannot lock the agent worktree {}", root.display()));
            }
        }
    }
    Ok(None)
}

/// Whether a live binding of any process holds the worktree right now.
fn is_held(root: &Path) -> Result<bool> {
    Ok(exclusive_hold(root)?.is_none())
}

fn open_hold_file(root: &Path) -> Result<fs::File> {
    let path = worktree_git_dir(root)?.join(HOLD_FILE);
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("cannot open {}", path.display()))
}

/// The administration directory of a linked worktree, read from its `.git` file without a
/// Git process when possible.
fn worktree_git_dir(root: &Path) -> Result<PathBuf> {
    if let Ok(pointer) = fs::read_to_string(root.join(".git"))
        && let Some(path) = pointer.strip_prefix("gitdir: ")
    {
        let path = Path::new(path.trim_end_matches(['\n', '\r']));
        return Ok(if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        });
    }
    git_path(root, ["rev-parse", "--path-format=absolute", "--git-dir"])
}

/// Drops the registration of a worktree whose checkout is gone, and only that one: unlike
/// `git worktree prune`, other missing worktrees (for example on an unmounted drive) keep
/// theirs. Failures are ignored; a stale registration is harmless.
fn drop_missing_registration(source_root: &Path, root: &Path) {
    if source_root.is_dir() {
        let _ = git_output(
            source_root,
            GitOperation::Metadata,
            [
                OsStr::new("worktree"),
                OsStr::new("remove"),
                OsStr::new("--force"),
                root.as_os_str(),
            ],
        );
    }
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
/// Keeps the worktree when a live binding of any process holds it, when its own branch
/// exists, when it has tracked or untracked non-ignored changes, or when its HEAD left the
/// base. A branch of the same name outside the worktree history is not its own. Ignored
/// files such as build output do not prevent removal. A missing checkout loses only its own
/// registration.
pub fn remove_if_unchanged(
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<RemoveOutcome> {
    if !root.join(".git").exists() {
        drop_missing_registration(source_root, root);
        remove_empty_bucket(root);
        return Ok(RemoveOutcome::AlreadyMissing);
    }
    // The exclusive lock is kept until the worktree is gone, so no binding can take it over
    // between the checks and the removal (R10.3).
    let Some(_exclusive) = exclusive_hold(root)? else {
        return Ok(RemoveOutcome::Kept(KeptReason::InUse));
    };
    if own_branch_tip(root, &metadata.branch)?.is_some() {
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

/// Returns the main checkout of the repository that the linked worktree `root` belongs to.
pub fn main_worktree_root(root: &Path) -> Result<PathBuf> {
    let output = git_output(
        root,
        GitOperation::Metadata,
        ["worktree", "list", "--porcelain", "-z"],
    )?;
    let first = output
        .stdout
        .split(|byte| *byte == b'\0')
        .next()
        .and_then(|field| field.strip_prefix(b"worktree "))
        .context("git did not report a main worktree")?;
    crate::git::git_path_from_bytes(first)
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
///
/// Only ASCII letters, digits, `_`, `-`, `.` and `/` are accepted: the name appears verbatim
/// in the parent's merge command and in the completion block.
fn branch_ref(branch: &str) -> Result<String> {
    if !branch.starts_with("codex/agents/")
        || branch.contains("..")
        || !branch
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-./".contains(character))
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
    // Without optional locks `status` does not rewrite the index, so inspecting a worktree
    // does not count as activity for the sweep (R10.4). Only whether anything changed
    // matters: `normal` reports an untracked directory once instead of walking all of it,
    // and still hides directories without untracked, non-ignored files.
    let mut args = [
        "--no-optional-locks",
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=normal",
    ]
    .map(OsString::from)
    .to_vec();
    // A submodule the worktree never checked out shows as deleted once its empty directory
    // is gone; that is not the agent's change (R2.11).
    let unpopulated = unpopulated_gitlinks(root)?;
    if !unpopulated.is_empty() {
        args.push(OsString::from("--"));
        args.push(OsString::from("."));
        args.extend(exclude_pathspecs(&unpopulated));
    }
    let status = git_output(root, GitOperation::WorkingTree, args)?;
    Ok(!status.stdout.is_empty())
}

/// Gitlinks (submodules) of the worktree index whose directory holds no checkout (R2.11).
///
/// The host never initializes submodules in an agent worktree, so their directories stay
/// empty; an empty or removed directory, and files placed into it, are not changes Codex
/// records.
pub fn unpopulated_gitlinks(root: &Path) -> Result<Vec<PathBuf>> {
    let output = git_output(root, GitOperation::Metadata, ["ls-files", "--stage", "-z"])?;
    let mut paths = Vec::new();
    for record in output.stdout.split(|byte| *byte == b'\0') {
        // `<mode> <object> <stage>\t<path>`
        let Some(rest) = record.strip_prefix(b"160000 ") else {
            continue;
        };
        let Some(tab) = rest.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let path = crate::git::git_path_from_bytes(&rest[tab + 1..])?;
        if fs::symlink_metadata(root.join(&path).join(".git")).is_err() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

/// Pathspecs that leave exactly `paths` out of a command run at the worktree root.
fn exclude_pathspecs(paths: &[PathBuf]) -> Vec<OsString> {
    paths
        .iter()
        .map(|path| {
            let mut pathspec = OsString::from(":(exclude,literal)");
            pathspec.push(path.as_os_str());
            pathspec
        })
        .collect()
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

fn stage_all(root: &Path, index: &Path, excluded: &[PathBuf]) -> Result<()> {
    let mut command = git_command(root, GitOperation::WorkingTree)?;
    command
        .env("GIT_INDEX_FILE", index)
        .args(["add", "--all", "--", "."])
        .args(exclude_pathspecs(excluded));
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
    let path = git_path(cwd, ["rev-parse", "--show-toplevel"])
        .with_context(|| format!("{} is not inside a Git working tree", cwd.display()))?;
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

#[path = "agent_maintenance.rs"]
mod maintenance;
#[path = "agent_merge.rs"]
mod merge;
#[path = "agent_record.rs"]
mod record;

pub use maintenance::AgentWorktreeState;
pub use maintenance::AgentWorktreeStatus;
pub use maintenance::RecoveredBranch;
pub use maintenance::RestoreFrom;
pub use maintenance::SweepPolicy;
pub use maintenance::SweepReport;
pub use maintenance::agent_branch_for_thread;
pub use maintenance::agent_worktree_status;
pub use maintenance::checkout_root;
pub use maintenance::delete_applied_branch;
pub use maintenance::last_activity;
pub use maintenance::remove_agent_checkout;
pub use maintenance::restore_agent_worktree;
pub use maintenance::sweep_agent_worktrees;
pub use maintenance::worktree_list_fingerprint;
pub use merge::CleanMerge;
pub use merge::MergeOutcome;
pub use merge::MergedChange;
pub use merge::merge_agent_branch;
pub use record::AgentWorktreeRecord;
pub use record::prune_agent_records;
pub use record::read_agent_record;
pub use record::write_agent_record;

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
