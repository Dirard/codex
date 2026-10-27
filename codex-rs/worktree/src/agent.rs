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
    let status = worktree_status(root)?;
    let dirty = status.changed;
    if !dirty && head == recorded {
        return Ok(CheckpointOutcome::Unchanged);
    }

    let commit = if dirty {
        let temporary = tempfile::tempdir().context("cannot create a temporary index directory")?;
        let index = temporary.path().join("index");
        copy_index(root, &index)?;
        // A removed submodule directory is not the agent's change (R2.11); files inside an
        // unpopulated one are not staged by `git add` either.
        stage_all(root, &index, &status.removed_gitlinks)?;
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
    /// The agent put files into the directory of a submodule the worktree does not check
    /// out; Codex never records them ([`unpopulated_submodule_files`], P3-3).
    SubmoduleFiles,
    /// A repository nested in the worktree holds work the repository does not have
    /// ([`nested_repositories`], F3).
    NestedRepository,
    /// A live process works in the worktree ([`processes_in`], F1).
    Process,
}

/// File in a worktree's administration directory that live bindings lock shared.
const HOLD_FILE: &str = "codex-agent.lock";

/// Hold files this process holds shared for a live binding, by canonical path, with how many
/// bindings hold each (deferred-fixes-3 coordinator note). An advisory lock on a network
/// filesystem does not reliably conflict with another lock of the *same* process, so this
/// process's own holds are tracked here instead of relied on through the lock: a removal never
/// tries to lock a file this process already holds, and reads a foreign hold through the lock.
fn own_holds() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, usize>> {
    static OWN_HOLDS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<PathBuf, usize>>,
    > = std::sync::OnceLock::new();
    OWN_HOLDS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn register_own_hold(path: &Path) {
    *own_holds()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry(path.to_path_buf())
        .or_insert(0) += 1;
}

fn release_own_hold(path: &Path) {
    let mut holds = own_holds()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(count) = holds.get_mut(path) {
        *count -= 1;
        if *count == 0 {
            holds.remove(path);
        }
    }
}

fn this_process_holds(path: &Path) -> bool {
    own_holds()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(path)
}

/// A shared hold of an agent worktree by a live binding (R10.3).
///
/// While any process keeps one, [`remove_if_unchanged`] and the sweep of every process leave
/// the worktree alone: removal needs the exclusive lock. The kernel drops the lock when the
/// process exits, so a crashed process never blocks the cleanup of its worktrees. A hold of
/// this process is also recorded in [`own_holds`], so a removal from the same process does not
/// depend on the lock conflicting with it (which it may not on a network filesystem).
#[derive(Debug)]
pub struct AgentWorktreeHold {
    _file: fs::File,
    path: PathBuf,
}

impl Drop for AgentWorktreeHold {
    fn drop(&mut self) {
        release_own_hold(&self.path);
    }
}

/// Holds an agent worktree for a live binding; waits while a removal holds it exclusively.
pub fn hold_agent_worktree(root: &Path) -> Result<AgentWorktreeHold> {
    let path = hold_file_path(root)?;
    let file = open_hold_file_at(&path)?;
    file.lock_shared()
        .with_context(|| format!("cannot hold the agent worktree {}", root.display()))?;
    register_own_hold(&path);
    Ok(AgentWorktreeHold { _file: file, path })
}

/// Attempts at the exclusive lock before a worktree counts as held.
const EXCLUSIVE_ATTEMPTS: u32 = 5;

/// The exclusive lock of a worktree that no live binding holds; `None` while one does.
///
/// A child process between `fork` and `exec` briefly shares every open file of its parent,
/// lock included, so a hold dropped a moment ago can still look taken; a few short retries
/// let such a copy go away. A hold of this very process is read from [`own_holds`] rather than
/// through the lock, since an advisory lock may not conflict with another of the same process.
fn exclusive_hold(root: &Path) -> Result<Option<fs::File>> {
    let path = hold_file_path(root)?;
    if this_process_holds(&path) {
        return Ok(None);
    }
    let file = open_hold_file_at(&path)?;
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

/// The canonical hold-file path of a worktree, the key both the lock and [`own_holds`] use. The
/// administration directory exists, so it canonicalizes; the hold file itself need not.
fn hold_file_path(root: &Path) -> Result<PathBuf> {
    let git_dir = worktree_git_dir(root)?;
    let git_dir = dunce::canonicalize(&git_dir).unwrap_or(git_dir);
    Ok(git_dir.join(HOLD_FILE))
}

fn open_hold_file_at(path: &Path) -> Result<fs::File> {
    // The hold file is opened for writing even to read a foreign hold: an advisory lock on a
    // read-only descriptor is unreliable on a network filesystem.
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
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
/// exists, when it has tracked or untracked non-ignored changes, when its HEAD left the
/// base, when the agent put files into a submodule directory it does not check out or work
/// into a nested repository, or while a process runs in it. A branch of the same name
/// outside the worktree history is not its own. Ignored files such as build output do not
/// prevent removal. A missing checkout loses only its own registration.
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
    if let Some(reason) = unrecorded_work(root, /*discard_unrecorded*/ false)? {
        return Ok(RemoveOutcome::Kept(reason));
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
    Ok(worktree_status(root)?.changed)
}

/// What `git status` reports about an agent worktree.
struct WorktreeStatus {
    /// Something changed that a checkpoint records.
    changed: bool,
    /// Gitlinks whose directory is gone. The host never checks submodules out, so the
    /// directory of a submodule is empty, and its removal is not the agent's change (R2.11);
    /// a checkpoint leaves these paths out.
    removed_gitlinks: Vec<PathBuf>,
}

/// Reads the state of a worktree with one `git status` (R2.11, P3-8, F2).
///
/// Without optional locks `status` does not rewrite the index, so inspecting a worktree does
/// not count as activity for the sweep (R10.4). Only whether anything changed matters:
/// `normal` reports an untracked directory once instead of walking all of it, and still
/// hides directories without untracked, non-ignored files. The `porcelain=v2` entries carry
/// the index and worktree modes, so the removed directory of a gitlink (index mode `160000`,
/// worktree mode `000000`) is recognized without `.gitmodules` or a listing of the index;
/// files inside an unpopulated gitlink's directory never show in `git status`.
fn worktree_status(root: &Path) -> Result<WorktreeStatus> {
    let status = git_output(
        root,
        GitOperation::WorkingTree,
        [
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "-z",
            "--untracked-files=normal",
            // K1: never descend into a submodule's or nested repository's work tree, so its
            // configured clean filter and fsmonitor cannot run through this status. `dirty`
            // still reports a gitlink whose recorded commit moved and a removed gitlink
            // directory (`160000`/`000000`), which R2.11 and F3 rely on.
            "--ignore-submodules=dirty",
        ],
    )?;
    let mut changed = false;
    let mut removed_gitlinks = Vec::new();
    let mut fields = status.stdout.split(|byte| *byte == b'\0');
    while let Some(entry) = fields.next() {
        match entry.first() {
            None | Some(b'#') => {}
            // `1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>`
            Some(b'1') => {
                let parts = entry.splitn(9, |byte| *byte == b' ').collect::<Vec<_>>();
                match parts.as_slice() {
                    [_, _, _, _, b"160000", b"000000", _, _, path] => {
                        removed_gitlinks.push(crate::git::git_path_from_bytes(path)?);
                    }
                    _ => changed = true,
                }
            }
            // A rename or copy: its original path follows as a field of its own.
            Some(b'2') => {
                fields.next();
                changed = true;
            }
            Some(_) => changed = true,
        }
    }
    Ok(WorktreeStatus {
        changed,
        removed_gitlinks,
    })
}

/// Gitlinks of the worktree index with whether their directory holds a checkout (a `.git`).
/// It lists the whole index, so only creation and removal call it, never a status or a
/// checkpoint (P3-8).
fn index_gitlinks(root: &Path) -> Result<Vec<(PathBuf, bool)>> {
    let output = git_output(root, GitOperation::Metadata, ["ls-files", "--stage", "-z"])?;
    let mut gitlinks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for record in output.stdout.split(|byte| *byte == b'\0') {
        // `<mode> <object> <stage>\t<path>`
        let Some(rest) = record.strip_prefix(b"160000 ") else {
            continue;
        };
        let Some(tab) = rest.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let path = crate::git::git_path_from_bytes(&rest[tab + 1..])?;
        if seen.insert(path.clone()) {
            let populated = fs::symlink_metadata(root.join(&path).join(".git")).is_ok();
            gitlinks.push((path, populated));
        }
    }
    Ok(gitlinks)
}

/// Gitlinks (submodules) of the worktree index whose directory holds no checkout (R2.11).
///
/// The host never initializes submodules in an agent worktree, so their directories stay
/// empty. Every gitlink of the index counts, also one that `.gitmodules` does not register
/// (an embedded repository, or a submodule moved by hand), so files the agent puts into such
/// a directory, which `git status` never shows, keep the worktree from a removal (F2).
pub fn unpopulated_gitlinks(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(index_gitlinks(root)?
        .into_iter()
        .filter_map(|(path, populated)| (!populated).then_some(path))
        .collect())
}

/// Files the agent put into directories of submodules the worktree does not check out
/// (R2.11, P3-3, F2): Codex never records them, so a removal must not drop them silently.
pub fn unpopulated_submodule_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for path in unpopulated_gitlinks(root)? {
        let Ok(entries) = fs::read_dir(root.join(&path)) else {
            continue;
        };
        if entries.flatten().next().is_some() {
            found.push(path);
        }
    }
    Ok(found)
}

/// Repositories nested in the worktree (a gitlink of the index whose directory holds a
/// `.git`, such as one the agent created with `git init` and a checkpoint recorded) that hold
/// work the repository of the worktree does not: uncommitted changes, or a commit of their own
/// that exists nowhere else (F3). A checkpoint records only the gitlink, so removing the
/// checkout would lose that work. Each entry names the path and what it holds.
///
/// K2 (deferred-fixes-3 review): a submodule the agent only initialized (`git submodule update
/// --init`) is not work. Its HEAD and every commit it has come from its remote-tracking refs,
/// so work is a commit reachable from the nested repository's HEAD or its local branches and
/// tags but not from its `refs/remotes/*`, and not already in the worktree repository's object
/// store either (`all_commits_present`). An initialized, unchanged submodule has no such commit
/// and does not keep the worktree.
pub fn nested_repositories(root: &Path) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for (path, populated) in index_gitlinks(root)? {
        if !populated {
            continue;
        }
        let nested = root.join(&path);
        let mut held = Vec::new();
        let status = git_output(
            &nested,
            GitOperation::WorkingTree,
            [
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=normal",
                // K1: this status runs inside a repository the agent can write to. The command
                // is isolated (its own clean filter and fsmonitor are cleared), and
                // `--ignore-submodules=all` stops it from descending into a further nested
                // repository and running that one's configured programs.
                "--ignore-submodules=all",
            ],
        )?;
        if !status.stdout.is_empty() {
            held.push("uncommitted files");
        }
        if !local_only_commits(&nested, root)?.is_empty() {
            held.push("commits that the repository does not have");
        }
        if !held.is_empty() {
            found.push(format!("{} ({})", path.display(), held.join(" and ")));
        }
    }
    Ok(found)
}

/// Commits of the nested repository at `nested` that exist nowhere but there (K2): reachable
/// from its HEAD, local branches and tags, but not from its `refs/remotes/*`, and absent from
/// the object store of the worktree repository at `root`. A submodule that was only initialized
/// has none: its HEAD is a remote-tracking commit. At most one is returned, since the caller
/// only asks whether any exists.
fn local_only_commits(nested: &Path, root: &Path) -> Result<Vec<String>> {
    if optional_commit(nested, "HEAD")?.is_none() {
        return Ok(Vec::new());
    }
    // Commits reachable from HEAD and local refs but not from any remote-tracking ref.
    let mut command = git_command(nested, GitOperation::Metadata)?;
    command.args([
        "rev-list",
        "--max-count=50",
        "HEAD",
        "--branches",
        "--tags",
        "--not",
        "--remotes",
    ]);
    let local = run_git_command(&mut command, &[])?;
    let candidates = String::from_utf8_lossy(&local.stdout)
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    // Those the worktree repository does not already have are the agent's own work.
    let mut own = Vec::new();
    for candidate in candidates {
        if !all_commits_present(root, std::slice::from_ref(&candidate))? {
            own.push(candidate);
            break;
        }
    }
    Ok(own)
}

/// Whether the object store of the repository at `cwd` holds every one of `objects`.
fn all_commits_present(cwd: &Path, objects: &[String]) -> Result<bool> {
    if objects.is_empty() {
        return Ok(true);
    }
    let mut command = git_command(cwd, GitOperation::Metadata)?;
    command.args(["cat-file", "--batch-check=%(objecttype)"]);
    let input = objects
        .iter()
        .map(|object| format!("{object}\n"))
        .collect::<String>();
    let output = run_git_with_input(&mut command, input.into_bytes())?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .all(|line| line.trim() != "missing" && !line.ends_with(" missing")))
}

/// Live processes whose working directory lies in the worktree, as `pid N: <command>`;
/// removing the checkout under one would let it recreate the path as a plain directory and
/// lose what it writes (F1). Read from `/proc`, so empty on other systems. The Git children
/// this process spawned are skipped by pid (K5), so a concurrent status of the crate's own
/// does not count while a foreign Git does.
pub fn processes_in(root: &Path) -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        linux_processes_in(root)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn linux_processes_in(root: &Path) -> Vec<String> {
    let canonical = dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let own = std::process::id();
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == own {
            continue;
        }
        // K5: skip exactly the Git children this process spawned (by pid, not by name or
        // parent), so a long-lived own Git child does not block a removal and a foreign Git,
        // whatever its name or parent, still does.
        if crate::git::fork::is_own_git_child(pid) {
            continue;
        }
        let proc_dir = entry.path();
        let Ok(cwd) = fs::read_link(proc_dir.join("cwd")) else {
            continue;
        };
        if !(cwd.starts_with(root) || cwd.starts_with(&canonical)) {
            continue;
        }
        // `<pid> (<comm>) <state> <ppid> ...`
        let stat = fs::read_to_string(proc_dir.join("stat")).unwrap_or_default();
        let comm = stat
            .split_once(" (")
            .and_then(|(_, rest)| rest.rsplit_once(") "))
            .map(|(comm, _)| comm.to_string());
        let command = fs::read(proc_dir.join("cmdline"))
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .split('\0')
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        let command = if command.is_empty() {
            comm.unwrap_or_default()
        } else {
            command
        };
        found.push(format!("pid {pid}: {}", truncate_command(&command)));
    }
    found.sort();
    found
}

/// A command line cut to a readable length.
fn truncate_command(command: &str) -> String {
    const MAX_CHARS: usize = 120;
    match command.char_indices().nth(MAX_CHARS) {
        Some((end, _)) => format!("{}…", &command[..end]),
        None => command.to_string(),
    }
}

/// Why a removal must keep a worktree whose changes are all recorded: files of the agent
/// that Codex never records (files in unpopulated submodule directories, nested repositories
/// with their own work), which `discard_unrecorded` confirms may go, or a live process in
/// the worktree, which nothing overrides (P3-3, F1, F2, F3).
fn unrecorded_work(root: &Path, discard_unrecorded: bool) -> Result<Option<KeptReason>> {
    if !discard_unrecorded {
        if !unpopulated_submodule_files(root)?.is_empty() {
            return Ok(Some(KeptReason::SubmoduleFiles));
        }
        if !nested_repositories(root)?.is_empty() {
            return Ok(Some(KeptReason::NestedRepository));
        }
    }
    if !processes_in(root).is_empty() {
        return Ok(Some(KeptReason::Process));
    }
    Ok(None)
}

/// Ignored files and directories of the worktree, as `git status --ignored` lists them (a
/// directory with only ignored content once), with the number of entries; at most `limit`
/// are returned. They are not changes Codex records, so a removal drops them (P3-3).
pub fn ignored_entries(root: &Path, limit: usize) -> Result<(Vec<String>, usize)> {
    let status = git_output(
        root,
        GitOperation::WorkingTree,
        [
            "--no-optional-locks",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
            "--ignored=traditional",
            // K1: the ignored listing never needs a submodule's state, so it never descends
            // into one and never runs its configured programs.
            "--ignore-submodules=all",
        ],
    )?;
    let mut listed = Vec::new();
    let mut total = 0;
    for entry in status.stdout.split(|byte| *byte == b'\0') {
        let Some(path) = entry.strip_prefix(b"!! ") else {
            continue;
        };
        total += 1;
        if listed.len() < limit {
            listed.push(String::from_utf8_lossy(path).into_owned());
        }
    }
    Ok((listed, total))
}

/// Pathspecs, NUL-terminated for `--pathspec-from-file=- --pathspec-file-nul`, of the whole
/// worktree without exactly `paths`. Read from standard input, they never reach the command
/// line, so thousands of submodules cannot exceed its limit (P3-8).
fn pathspecs_without(paths: &[PathBuf]) -> Vec<u8> {
    let mut input = b".\0".to_vec();
    for path in paths {
        input.extend_from_slice(b":(exclude,literal)");
        input.extend_from_slice(path.as_os_str().as_encoded_bytes());
        input.push(b'\0');
    }
    input
}

/// Runs a prepared Git command with `input` on its standard input.
fn run_git_with_input(command: &mut Command, input: Vec<u8>) -> Result<std::process::Output> {
    use std::io::Write as _;
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to start git")?;
    // K5: record the child in the fork registry while it runs, so the `/proc` scan does not
    // mistake this process's own Git for one that holds the worktree.
    let _tracked = crate::git::fork::GitChild::new(child.id());
    let mut stdin = child.stdin.take().context("git has no standard input")?;
    // Written from another thread, so a full output pipe cannot block the input.
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().context("failed to wait for git")?;
    let written = writer
        .join()
        .map_err(|_| anyhow::anyhow!("the git input writer panicked"))?;
    if !output.status.success() {
        bail!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    written.context("cannot write the git input")?;
    Ok(output)
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
    command.env("GIT_INDEX_FILE", index).args(["add", "--all"]);
    let staged = if excluded.is_empty() {
        run_git_command(command.args(["--", "."]), &[])
    } else {
        command.args(["--pathspec-from-file=-", "--pathspec-file-nul"]);
        run_git_with_input(&mut command, pathspecs_without(excluded))
    };
    staged
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
pub use maintenance::applied_ref;
pub use maintenance::checkout_root;
pub use maintenance::last_activity;
pub use maintenance::remove_agent_checkout;
pub use maintenance::restore_agent_worktree;
pub use maintenance::retire_applied_branch;
pub use maintenance::sweep_agent_worktrees;
pub use maintenance::usable_commit;
pub use maintenance::worktree_list_fingerprint;
pub use merge::CleanMerge;
pub use merge::MergeOutcome;
pub use merge::MergedChange;
pub use merge::merge_agent_branch;
pub use merge::shell_quote;
pub use merge::submodule_command;
pub use record::AgentWorktreeRecord;
pub use record::prune_agent_records;
pub use record::prune_orphan_applied_refs;
pub use record::read_agent_record;
pub use record::write_agent_record;

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
