//! State reports, restoration and cleanup of agent worktrees.
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC5 (restore a removed worktree, R5.3,
//! R5.6), UC9 (worktree state) and UC10 (sweep forgotten worktrees).

use super::AGENT_COMMIT_AUTHOR_EMAIL;
use super::AgentWorktreeMetadata;
use super::KeptReason;
use super::LAST_FREE_BRANCH_SUFFIX;
use super::RemoveOutcome;
use super::THREAD_TRAILER;
use super::branch_ref;
use super::exclusive_hold;
use super::has_changes;
use super::is_ancestor;
use super::is_held;
use super::optional_commit;
use super::own_branch_tip;
use super::read_agent_metadata;
use super::remove_if_unchanged;
use super::write_agent_metadata;
use crate::WorktreeManager;
use crate::git::GitOperation;
use crate::git::git_output;
use crate::git::git_path;
use crate::paths::remove_empty_bucket;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

/// Commits inspected on an agent branch when looking for a thread's checkpoints.
const MAX_BRANCH_WALK: usize = 1000;

/// What a parent or client is told about an agent worktree (spec UC9, R9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentWorktreeState {
    /// The own agent branch exists, the worktree has tracked or untracked non-ignored
    /// changes, or its HEAD left the base.
    Changed,
    /// Nothing to apply or keep: no own branch, no changes, HEAD at the base.
    Clean,
    /// The checkout no longer exists.
    Removed,
}

impl AgentWorktreeState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Changed => "changed",
            Self::Clean => "clean",
            Self::Removed => "removed",
        }
    }
}

/// The state of an agent worktree and its own branch, when that branch exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWorktreeStatus {
    pub state: AgentWorktreeState,
    pub branch: Option<String>,
}

/// Reads the state of an agent worktree (R9.1). Ignored files such as build output do not
/// count as changes. The own branch is checked first: with it the worktree is changed and
/// no `git status` runs.
pub fn agent_worktree_status(
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<AgentWorktreeStatus> {
    if !root.join(".git").exists() {
        return Ok(AgentWorktreeStatus {
            state: AgentWorktreeState::Removed,
            branch: None,
        });
    }
    let branch = own_branch_tip(root, &metadata.branch)?.map(|_| metadata.branch.clone());
    let changed = branch.is_some()
        || has_changes(root)?
        || optional_commit(root, "HEAD")?.as_deref() != Some(metadata.base_sha.as_str());
    Ok(AgentWorktreeStatus {
        state: if changed {
            AgentWorktreeState::Changed
        } else {
            AgentWorktreeState::Clean
        },
        branch,
    })
}

/// An agent branch that holds a thread's checkpoints, found without its worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveredBranch {
    pub branch: String,
    pub tip: String,
}

/// Finds the thread's own agent branch in the repository of `source_root` (R5.6).
///
/// The recorded name and its free names `-2` through `-20` are tried in order. A branch is the
/// thread's own when its first-parent history holds a checkpoint commit that the host wrote
/// for the thread: author `Codex Agent`, subject `codex agent <path> checkpoint` and a
/// `Codex-Thread-Id` trailer naming the thread. The base is not read from the branch, whose
/// commits can come from the agent itself; the R4.9 trailers are informational.
pub fn agent_branch_for_thread(
    source_root: &Path,
    recorded_branch: &str,
    thread_id: &str,
) -> Result<Option<RecoveredBranch>> {
    let candidates = std::iter::once(recorded_branch.to_owned())
        .chain((2..=LAST_FREE_BRANCH_SUFFIX).map(|suffix| format!("{recorded_branch}-{suffix}")));
    for name in candidates {
        let reference = branch_ref(&name)?;
        let Some(tip) = optional_commit(source_root, &reference)? else {
            continue;
        };
        if holds_thread_checkpoint(source_root, &tip, thread_id)? {
            return Ok(Some(RecoveredBranch { branch: name, tip }));
        }
    }
    Ok(None)
}

/// Whether the first-parent history of `tip` holds a host checkpoint of the thread.
fn holds_thread_checkpoint(source_root: &Path, tip: &str, thread_id: &str) -> Result<bool> {
    let format = format!(
        "--format=%ae%x00%s%x00%(trailers:key={THREAD_TRAILER},valueonly,separator=%x2C)%x1e"
    );
    let output = git_output(
        source_root,
        GitOperation::Metadata,
        [
            "log".to_string(),
            "--first-parent".to_string(),
            format!("--max-count={MAX_BRANCH_WALK}"),
            format,
            tip.to_string(),
            "--".to_string(),
        ],
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.split('\u{1e}').any(|record| {
        let fields = record
            .trim_start_matches('\n')
            .split('\0')
            .collect::<Vec<_>>();
        let [author, subject, threads] = fields.as_slice() else {
            return false;
        };
        *author == AGENT_COMMIT_AUTHOR_EMAIL
            && subject.starts_with("codex agent ")
            && subject.ends_with(" checkpoint")
            && threads.split(',').any(|value| value.trim() == thread_id)
    }))
}

/// Another checkout than `except` that has `branch` checked out, if any. A registered
/// checkout that is missing right now (an unmounted drive) counts: it comes back with the
/// branch checked out.
fn branch_checkout(source_root: &Path, branch: &str, except: &Path) -> Result<Option<PathBuf>> {
    let reference = branch_ref(branch)?;
    let output = git_output(
        source_root,
        GitOperation::Metadata,
        ["worktree", "list", "--porcelain", "-z"],
    )?;
    let fields = output
        .stdout
        .split(|byte| *byte == b'\0')
        .collect::<Vec<_>>();
    for entry in fields.split(|field| field.is_empty()) {
        let mut root = None;
        let mut checked_out = false;
        for field in entry {
            if let Some(path) = field.strip_prefix(b"worktree ") {
                root = Some(crate::git::git_path_from_bytes(path)?);
            } else if field.strip_prefix(b"branch ") == Some(reference.as_bytes()) {
                checked_out = true;
            }
        }
        if checked_out
            && let Some(root) = root
            && root != except
        {
            return Ok(Some(root));
        }
    }
    Ok(None)
}

/// A hash of the repository's worktree list. It changes when a worktree is added or removed
/// or moves its HEAD, so a caller can skip sweeping a repository where nothing happened.
pub fn worktree_list_fingerprint(cwd: &Path) -> Result<u64> {
    use std::hash::Hash as _;
    use std::hash::Hasher as _;
    let output = git_output(
        cwd,
        GitOperation::Metadata,
        ["worktree", "list", "--porcelain", "-z"],
    )?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    output.stdout.hash(&mut hasher);
    Ok(hasher.finish())
}

/// Where a restored worktree starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreFrom {
    /// The tip of the agent's own branch; the worktree HEAD is attached to it.
    Branch { name: String, tip: String },
    /// The tip of a branch the parent applied and Codex then deleted (R12.7), detached; the
    /// recorded base when that commit is gone or does not contain the base.
    Applied { tip: String },
    /// The recorded base, detached.
    Base,
}

/// Recreates a removed agent worktree at its old path (R5.3).
///
/// The path must be free. A branch must still contain the recorded base and must not be
/// checked out in another checkout: the next checkpoint would move it under that checkout.
/// Only a stale registration of this very path is replaced; other missing worktrees of the
/// repository keep theirs. The worktree is populated like a new managed worktree, and its
/// owner record and agent metadata are written again. On failure nothing is left behind.
pub fn restore_agent_worktree(
    manager: &WorktreeManager,
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
    from: &RestoreFrom,
) -> Result<()> {
    if fs::symlink_metadata(root).is_ok() {
        bail!("the path {} is occupied", root.display());
    }
    let commit = match from {
        RestoreFrom::Branch { name, tip } => {
            if !is_ancestor(source_root, &metadata.base_sha, tip)? {
                bail!(
                    "the recorded base {} is not an ancestor of branch {name}",
                    metadata.base_sha
                );
            }
            // The stale registration of `root` itself is replaced below.
            if let Some(checkout) = branch_checkout(source_root, name, root)? {
                bail!(
                    "branch {name} is checked out in {}; switch that checkout to another branch first",
                    checkout.display()
                );
            }
            tip.clone()
        }
        RestoreFrom::Applied { tip } => match optional_commit(source_root, tip)? {
            Some(commit) if is_ancestor(source_root, &metadata.base_sha, &commit)? => commit,
            _ => recorded_base(source_root, metadata)?,
        },
        RestoreFrom::Base => recorded_base(source_root, metadata)?,
    };
    let bucket = root
        .parent()
        .context("the worktree path has no parent directory")?;
    fs::create_dir_all(bucket)
        .with_context(|| format!("cannot recreate the bucket {}", bucket.display()))?;
    // `--force` replaces only a stale registration of this path (not a locked one); unlike
    // `git worktree prune` it leaves other missing worktrees registered.
    if let Err(error) = git_output(
        source_root,
        GitOperation::WorkingTree,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("--force"),
            OsStr::new("--detach"),
            OsStr::new("--no-checkout"),
            root.as_os_str(),
            OsStr::new(&commit),
        ],
    ) {
        remove_empty_bucket(root);
        return Err(error).context("cannot register the restored worktree");
    }
    let populated = populate_restored(manager, root, metadata, from, &commit);
    if let Err(error) = populated {
        return Err(match super::discard_agent_worktree(source_root, root) {
            Ok(()) => error,
            Err(cleanup) => error.context(format!(
                "and the incomplete worktree was not removed: {cleanup:#}"
            )),
        });
    }
    Ok(())
}

fn recorded_base(source_root: &Path, metadata: &AgentWorktreeMetadata) -> Result<String> {
    optional_commit(source_root, &metadata.base_sha)?.with_context(|| {
        format!(
            "its base {} is no longer in the repository",
            metadata.base_sha
        )
    })
}

/// Deletes the agent branch that the parent applied at `tip`, once the worktree is gone
/// (R12.7). The branch must still be the thread's own checkpoint branch at `tip`, checked out
/// nowhere, and a merge of it into the parent's working tree must change nothing; the ref is
/// deleted with a compare-and-swap on `tip`. Returns whether it was deleted.
pub fn delete_applied_branch(
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
    tip: &str,
) -> Result<bool> {
    // While the worktree lives, its HEAD is attached to the branch.
    if root.join(".git").exists() {
        return Ok(false);
    }
    let Some(branch) = agent_branch_for_thread(source_root, &metadata.branch, &metadata.thread_id)?
    else {
        return Ok(false);
    };
    if branch.tip != tip || branch_checkout(source_root, &branch.branch, Path::new(""))?.is_some() {
        return Ok(false);
    }
    match super::merge_agent_branch(source_root, root, metadata)? {
        super::MergeOutcome::AlreadyApplied { tip: merged } if merged == tip => {}
        _ => return Ok(false),
    }
    git_output(
        source_root,
        GitOperation::Metadata,
        [
            "update-ref",
            "-m",
            "codex agent branch applied",
            "-d",
            branch_ref(&branch.branch)?.as_str(),
            tip,
        ],
    )
    .context("cannot delete the applied agent branch")?;
    Ok(true)
}

fn populate_restored(
    manager: &WorktreeManager,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
    from: &RestoreFrom,
    commit: &str,
) -> Result<()> {
    let config = git_path(
        root,
        [
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "config.worktree",
        ],
    )?;
    git_output(
        root,
        GitOperation::Metadata,
        [
            OsStr::new("config"),
            OsStr::new("--file"),
            config.as_os_str(),
            OsStr::new("core.worktree"),
            root.as_os_str(),
        ],
    )?;
    git_output(
        root,
        GitOperation::WorkingTree,
        [
            "--work-tree=.",
            "reset",
            "--hard",
            "--no-recurse-submodules",
            commit,
        ],
    )
    .context("cannot populate the restored worktree")?;
    manager.bind_thread(root, &metadata.thread_id)?;
    let mut metadata = metadata.clone();
    if let RestoreFrom::Branch { name, .. } = from {
        metadata.branch.clone_from(name);
    }
    write_agent_metadata(root, &metadata)?;
    if let RestoreFrom::Branch { name, .. } = from {
        git_output(
            root,
            GitOperation::Metadata,
            ["symbolic-ref", "HEAD", branch_ref(name)?.as_str()],
        )
        .context("cannot attach the restored worktree to its branch")?;
    }
    Ok(())
}

/// Limits of a sweep of forgotten agent worktrees (R10.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SweepPolicy {
    /// Unchanged worktrees kept among the most recently active ones.
    pub keep_count: usize,
    /// Unchanged worktrees idle at least this long are removed, and so are worktrees whose
    /// work is all on their own branch when they can be restored from it.
    pub max_idle: Duration,
    /// Unchanged worktrees idle less than this are never removed.
    pub min_idle: Duration,
}

/// What a sweep did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub removed: Vec<PathBuf>,
    /// Agent worktrees that stay: changed, held, recent or within the keep count.
    pub kept: usize,
    /// Agent worktrees whose inspection or removal failed, with the reason; they stay.
    pub failed: Vec<(PathBuf, String)>,
}

/// Removes forgotten agent worktrees of the repository at `source_cwd` (UC10).
///
/// Only managed worktrees with agent metadata confirmed by their owner record are
/// considered. A worktree is never removed while `in_use` reports it bound in this process,
/// while a live binding of any process holds it ([`super::hold_agent_worktree`]), or while
/// `source_cwd` lies in it. An unchanged worktree is removed when it has been idle for
/// `max_idle`, or when it is not among the `keep_count` most recently active unchanged ones
/// and has been idle for `min_idle`. A worktree whose HEAD is the tip of its own branch and
/// that has no uncommitted changes holds nothing the branch lacks; it is removed after
/// `max_idle` when `restorable` says a reload can bring it back from the branch (R5.3).
/// Other changed worktrees and all agent branches are never touched. Failures are counted
/// and leave the worktree in place.
pub fn sweep_agent_worktrees(
    manager: &WorktreeManager,
    source_cwd: &Path,
    policy: &SweepPolicy,
    in_use: &dyn Fn(&Path) -> bool,
    restorable: &dyn Fn(&AgentWorktreeMetadata) -> bool,
    now: SystemTime,
) -> Result<SweepReport> {
    let source_root = super::repository_root(source_cwd)?;
    let session_cwd = dunce::canonicalize(source_cwd).unwrap_or_else(|_| source_cwd.to_path_buf());
    let mut report = SweepReport::default();
    let mut unchanged = Vec::new();
    let mut parked = Vec::new();
    for worktree in manager.list(source_cwd)? {
        let root = worktree.root;
        let canonical_root = dunce::canonicalize(&root).unwrap_or_else(|_| root.clone());
        if in_use(&root) || session_cwd.starts_with(&canonical_root) {
            report.kept += 1;
            continue;
        }
        let metadata = match read_agent_metadata(&root) {
            Ok(Some(metadata)) => metadata,
            Ok(None) => continue,
            Err(error) => {
                report.failed.push((root, format!("{error:#}")));
                continue;
            }
        };
        match manager.owner(&root) {
            Ok(Some(owner)) if owner == metadata.thread_id => {}
            Ok(_) => continue,
            Err(error) => {
                report.failed.push((root, format!("{error:#}")));
                continue;
            }
        }
        // A live agent of another process: no need to read its state.
        match is_held(&root) {
            Ok(false) => {}
            Ok(true) => {
                report.kept += 1;
                continue;
            }
            Err(error) => {
                report.failed.push((root, format!("{error:#}")));
                continue;
            }
        }
        // Read the activity before inspecting the worktree.
        let activity = last_activity(&root).unwrap_or(now);
        let idle = now.duration_since(activity).unwrap_or_default();
        match agent_worktree_status(&root, &metadata) {
            Ok(status) if status.state == AgentWorktreeState::Clean => {
                unchanged.push((activity, root, metadata));
            }
            Ok(status)
                if status.branch.is_some() && idle >= policy.max_idle && restorable(&metadata) =>
            {
                parked.push((root, metadata));
            }
            Ok(_) => report.kept += 1,
            Err(error) => report.failed.push((root, format!("{error:#}"))),
        }
    }
    unchanged.sort_by_key(|(activity, _, _)| std::cmp::Reverse(*activity));
    for (index, (activity, root, metadata)) in unchanged.into_iter().enumerate() {
        let idle = now.duration_since(activity).unwrap_or_default();
        let expired =
            idle >= policy.max_idle || (index >= policy.keep_count && idle >= policy.min_idle);
        if !expired {
            report.kept += 1;
            continue;
        }
        record_removal(
            &mut report,
            root.clone(),
            remove_if_unchanged(&source_root, &root, &metadata),
        );
    }
    for (root, metadata) in parked {
        record_removal(
            &mut report,
            root.clone(),
            remove_if_parked(&source_root, &root, &metadata),
        );
    }
    Ok(report)
}

fn record_removal(report: &mut SweepReport, root: PathBuf, outcome: Result<RemoveOutcome>) {
    match outcome {
        Ok(RemoveOutcome::Removed | RemoveOutcome::AlreadyMissing) => report.removed.push(root),
        Ok(RemoveOutcome::Kept(_)) => report.kept += 1,
        Err(error) => report.failed.push((root, format!("{error:#}"))),
    }
}

/// Removes a worktree whose HEAD is the tip of its own branch and that has no uncommitted
/// changes: the branch holds all its work and restores it (R5.3). Ignored files are lost,
/// as for an unchanged worktree.
fn remove_if_parked(
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<RemoveOutcome> {
    let Some(_exclusive) = exclusive_hold(root)? else {
        return Ok(RemoveOutcome::Kept(KeptReason::InUse));
    };
    let Some(tip) = own_branch_tip(root, &metadata.branch)? else {
        return Ok(RemoveOutcome::Kept(KeptReason::Changes));
    };
    if optional_commit(root, "HEAD")?.as_deref() != Some(tip.as_str()) {
        return Ok(RemoveOutcome::Kept(KeptReason::Commits));
    }
    if has_changes(root)? {
        return Ok(RemoveOutcome::Kept(KeptReason::Changes));
    }
    git_output(
        source_root,
        GitOperation::WorkingTree,
        [
            OsStr::new("worktree"),
            OsStr::new("remove"),
            OsStr::new("--force"),
            root.as_os_str(),
        ],
    )
    .context("cannot remove the agent worktree kept on its branch")?;
    remove_empty_bucket(root);
    Ok(RemoveOutcome::Removed)
}

/// Removes the checkout of an agent that is done (UC12, R12.5): one at the tip of its own
/// branch, or at its base without a branch, and without uncommitted changes. The branch is
/// never touched, so a reload restores the worktree from it (R5.3). Like
/// [`remove_if_unchanged`], it waits for no live binding: one that holds the worktree keeps it
/// (`InUse`), and the checks run under the exclusive lock that the removal keeps.
pub fn remove_agent_checkout(
    source_root: &Path,
    root: &Path,
    metadata: &AgentWorktreeMetadata,
) -> Result<RemoveOutcome> {
    if !root.join(".git").exists() {
        super::drop_missing_registration(source_root, root);
        remove_empty_bucket(root);
        return Ok(RemoveOutcome::AlreadyMissing);
    }
    let Some(_exclusive) = exclusive_hold(root)? else {
        return Ok(RemoveOutcome::Kept(KeptReason::InUse));
    };
    if has_changes(root)? {
        return Ok(RemoveOutcome::Kept(KeptReason::Changes));
    }
    let recorded = own_branch_tip(root, &metadata.branch)?.unwrap_or(metadata.base_sha.clone());
    if optional_commit(root, "HEAD")?.as_deref() != Some(recorded.as_str()) {
        return Ok(RemoveOutcome::Kept(KeptReason::Commits));
    }
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
    .context("cannot remove the closed agent worktree")?;
    remove_empty_bucket(root);
    Ok(RemoveOutcome::Removed)
}

/// The canonical root of the Git checkout that contains `cwd`.
pub fn checkout_root(cwd: &Path) -> Result<PathBuf> {
    super::repository_root(cwd)
}

/// The newest modification time of the checkout root and of the worktree's own Git files.
pub fn last_activity(root: &Path) -> Result<SystemTime> {
    let git_dir = git_path(root, ["rev-parse", "--path-format=absolute", "--git-dir"])?;
    let mut newest = fs::metadata(root)?.modified()?;
    for file in [
        "HEAD",
        "index",
        "config.worktree",
        "codex-thread.json",
        "logs/HEAD",
    ] {
        if let Ok(modified) =
            fs::metadata(git_dir.join(file)).and_then(|metadata| metadata.modified())
            && modified > newest
        {
            newest = modified;
        }
    }
    Ok(newest)
}
