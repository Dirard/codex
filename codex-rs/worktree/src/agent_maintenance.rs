//! State reports, restoration and cleanup of agent worktrees.
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC5 (restore a removed worktree, R5.3,
//! R5.6), UC9 (worktree state) and UC10 (sweep forgotten worktrees).

use super::AgentWorktreeBaseKind;
use super::AgentWorktreeMetadata;
use super::BASE_KIND_TRAILER;
use super::BASE_TRAILER;
use super::LAST_FREE_BRANCH_SUFFIX;
use super::RemoveOutcome;
use super::SNAPSHOT_MESSAGE;
use super::THREAD_TRAILER;
use super::branch_ref;
use super::has_changes;
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
/// count as changes.
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
    pub base_sha: String,
    pub base_kind: AgentWorktreeBaseKind,
}

/// Finds the thread's own agent branch in the repository of `source_root` (R5.6).
///
/// The recorded name and its free names `-2` through `-20` are tried in order. A branch is the
/// thread's own when its first-parent history holds a checkpoint commit whose
/// `Codex-Thread-Id` trailer names the thread. The base comes from the newest such commit's
/// `Codex-Agent-Base` trailers, or for older checkpoints from the parent of the oldest one.
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
        if let Some((base_sha, base_kind)) = thread_base_on_branch(source_root, &tip, thread_id)? {
            return Ok(Some(RecoveredBranch {
                branch: name,
                tip,
                base_sha,
                base_kind,
            }));
        }
    }
    Ok(None)
}

/// The base recorded by the thread's checkpoints in the first-parent history of `tip`.
fn thread_base_on_branch(
    source_root: &Path,
    tip: &str,
    thread_id: &str,
) -> Result<Option<(String, AgentWorktreeBaseKind)>> {
    let format = format!(
        "--format=%H%x00%P%x00%(trailers:key={THREAD_TRAILER},valueonly,separator=%x2C)%x00%(trailers:key={BASE_TRAILER},valueonly,separator=%x2C)%x00%(trailers:key={BASE_KIND_TRAILER},valueonly,separator=%x2C)%x1e"
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
    let mut newest_base = None;
    let mut oldest_parent = None;
    for record in stdout.split('\u{1e}') {
        let fields = record
            .trim_start_matches('\n')
            .split('\0')
            .collect::<Vec<_>>();
        let [_, parents, threads, base, base_kind] = fields.as_slice() else {
            continue;
        };
        if !threads.split(',').any(|value| value.trim() == thread_id) {
            continue;
        }
        if newest_base.is_none() {
            let base = base.trim();
            if is_full_object_id(base)
                && let Some(kind) = AgentWorktreeBaseKind::parse(base_kind.trim())
            {
                newest_base = Some((base.to_string(), kind));
            }
        }
        oldest_parent = parents.split_whitespace().next().map(str::to_owned);
    }
    if let Some(base) = newest_base {
        return Ok(Some(base));
    }
    let Some(parent) = oldest_parent else {
        return Ok(None);
    };
    // Checkpoints older than R4.9 carry no base trailers: the base is the parent of the
    // thread's oldest checkpoint, and a snapshot commit marks a `working_tree` base.
    let subject = git_output(
        source_root,
        GitOperation::Metadata,
        ["log", "-1", "--format=%s", parent.as_str(), "--"],
    )?;
    let kind = if String::from_utf8_lossy(&subject.stdout).trim() == SNAPSHOT_MESSAGE {
        AgentWorktreeBaseKind::WorkingTree
    } else {
        AgentWorktreeBaseKind::Head
    };
    Ok(Some((parent, kind)))
}

fn is_full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Where a restored worktree starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreFrom {
    /// The tip of the agent's own branch; the worktree HEAD is attached to it.
    Branch { name: String, tip: String },
    /// The recorded base, detached.
    Base,
}

/// Recreates a removed agent worktree at its old path (R5.3).
///
/// The path must be free. A stale registration of the old checkout is pruned first, the
/// worktree is populated like a new managed worktree, and its owner record and agent metadata
/// are written again. On failure nothing is left behind.
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
        RestoreFrom::Branch { tip, .. } => tip.clone(),
        RestoreFrom::Base => {
            optional_commit(source_root, &metadata.base_sha)?.with_context(|| {
                format!(
                    "its base {} is no longer in the repository",
                    metadata.base_sha
                )
            })?
        }
    };
    let bucket = root
        .parent()
        .context("the worktree path has no parent directory")?;
    fs::create_dir_all(bucket)
        .with_context(|| format!("cannot recreate the bucket {}", bucket.display()))?;
    let _ = git_output(source_root, GitOperation::Metadata, ["worktree", "prune"]);
    if let Err(error) = git_output(
        source_root,
        GitOperation::WorkingTree,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
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
    /// Unchanged worktrees idle at least this long are removed.
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

/// Removes forgotten unchanged agent worktrees of the repository at `source_cwd` (UC10).
///
/// Only managed worktrees with agent metadata confirmed by their owner record are
/// considered; `in_use` holds the ones bound in this process. Changed worktrees and agent
/// branches are never touched. An unchanged worktree is removed when it has been idle for
/// `max_idle`, or when it is not among the `keep_count` most recently active unchanged ones
/// and has been idle for `min_idle`. Failures are counted and leave the worktree in place.
pub fn sweep_agent_worktrees(
    manager: &WorktreeManager,
    source_cwd: &Path,
    policy: &SweepPolicy,
    in_use: &dyn Fn(&Path) -> bool,
    now: SystemTime,
) -> Result<SweepReport> {
    let source_root = super::repository_root(source_cwd)?;
    let mut report = SweepReport::default();
    let mut unchanged = Vec::new();
    for worktree in manager.list(source_cwd)? {
        let root = worktree.root;
        if in_use(&root) {
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
        // Read the activity before inspecting the worktree.
        let activity = last_activity(&root).unwrap_or(now);
        match agent_worktree_status(&root, &metadata) {
            Ok(status) if status.state == AgentWorktreeState::Clean => {
                unchanged.push((activity, root, metadata));
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
        match remove_if_unchanged(&source_root, &root, &metadata) {
            Ok(RemoveOutcome::Removed | RemoveOutcome::AlreadyMissing) => report.removed.push(root),
            Ok(RemoveOutcome::Kept(_)) => report.kept += 1,
            Err(error) => report.failed.push((root, format!("{error:#}"))),
        }
    }
    Ok(report)
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
