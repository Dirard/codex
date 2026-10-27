//! Fork: visibility, cleanup, restoration and merging of agent worktrees (amend 2026-10-27).
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC9 (`list_agents` and app-server
//! fields), UC10 (sweep forgotten worktrees), UC5 R5.3/R5.6 (restore a removed worktree) and
//! UC11 (merge for `apply_agent_worktree`).

use super::*;
use crate::config::AgentWorktreeCleanup;
use codex_protocol::protocol::SessionSource;
use codex_worktree::AgentWorktreeState;
use codex_worktree::MergeOutcome;
use codex_worktree::RestoreFrom;
use codex_worktree::SweepPolicy;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::SystemTime;

/// A sweep never removes a worktree that was active within this period (R10.4).
const MIN_SWEEP_IDLE: Duration = Duration::from_secs(3600);

// ---------------------------------------------------------------------------------------------
// Roots held by bindings of this process
// ---------------------------------------------------------------------------------------------

static LIVE_ROOTS: LazyLock<StdMutex<HashMap<PathBuf, usize>>> = LazyLock::new(Default::default);

fn live_key(root: &Path) -> PathBuf {
    dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Marks a worktree root as held by a binding of this process until dropped, so a sweep
/// skips it (R10.3).
#[derive(Debug)]
pub(crate) struct LiveRoot(PathBuf);

impl LiveRoot {
    pub(crate) fn register(root: &Path) -> Self {
        let key = live_key(root);
        *LIVE_ROOTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key.clone())
            .or_default() += 1;
        Self(key)
    }
}

impl Drop for LiveRoot {
    fn drop(&mut self) {
        let mut roots = LIVE_ROOTS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = roots.get_mut(&self.0) {
            *count -= 1;
            if *count == 0 {
                roots.remove(&self.0);
            }
        }
    }
}

fn is_live(root: &Path) -> bool {
    LIVE_ROOTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains_key(&live_key(root))
}

// ---------------------------------------------------------------------------------------------
// UC9: worktree state in `list_agents` and app-server
// ---------------------------------------------------------------------------------------------

/// The `worktree` object of an agent in `list_agents` (R9.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ListedWorktree {
    pub(crate) path: String,
    pub(crate) branch: Option<String>,
    pub(crate) status: &'static str,
}

/// Worktree fields of the listed agents that have a binding; empty while the feature is off
/// (R9.1, R9.2).
pub(crate) async fn listed_worktrees(
    config: &Config,
    worktrees: &AgentWorktrees,
    thread_ids: impl IntoIterator<Item = ThreadId>,
) -> HashMap<ThreadId, ListedWorktree> {
    if !config.features.enabled(Feature::AgentWorktreeIsolation) {
        return HashMap::new();
    }
    let bindings = thread_ids
        .into_iter()
        .filter_map(|thread_id| worktrees.get(thread_id))
        .collect::<Vec<_>>();
    let listed = futures::future::join_all(bindings.into_iter().map(|binding| async move {
        let blocking = Arc::clone(&binding);
        let status = tokio::task::spawn_blocking(move || {
            codex_worktree::agent_worktree_status(blocking.root.as_path(), &blocking.metadata())
        })
        .await;
        let status = match status {
            Ok(Ok(status)) => status,
            Ok(Err(err)) => {
                warn!(
                    "cannot read the state of agent worktree {}: {err:#}",
                    binding.root.display()
                );
                return None;
            }
            Err(err) => {
                warn!("agent worktree state task failed: {err}");
                return None;
            }
        };
        Some((
            binding.thread_id,
            ListedWorktree {
                path: binding.root.to_string_lossy().into_owned(),
                branch: status.branch,
                status: status.state.as_str(),
            },
        ))
    }))
    .await;
    listed.into_iter().flatten().collect()
}

/// What `thread/list` and `thread/read` report about a thread's agent worktree (R9.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentWorktreeView {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub state: AgentWorktreeState,
}

/// Describes the agent worktree that a thread's working directory lies in (R9.3).
///
/// `None` while the feature is off, outside the managed root, or for a worktree whose agent
/// metadata and owner record name another thread. A missing checkout is `Removed`.
pub async fn describe_agent_worktree(
    config: &Config,
    thread_id: ThreadId,
    cwd: &Path,
) -> Option<AgentWorktreeView> {
    if !config.features.enabled(Feature::AgentWorktreeIsolation) {
        return None;
    }
    let settings = worktree_settings(config).ok()?;
    let cwd = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || describe_blocking(settings, thread_id, &cwd))
        .await
        .ok()
        .flatten()
}

fn describe_blocking(
    settings: WorktreeSettings,
    thread_id: ThreadId,
    cwd: &Path,
) -> Option<AgentWorktreeView> {
    let manager = WorktreeManager::new(settings);
    let root = manager.managed_checkout_root(cwd)?;
    if !root.join(".git").exists() {
        return Some(AgentWorktreeView {
            path: root,
            branch: None,
            state: AgentWorktreeState::Removed,
        });
    }
    let metadata = codex_worktree::read_agent_metadata(&root)
        .ok()
        .flatten()
        .filter(|metadata| metadata.thread_id == thread_id.to_string())?;
    if manager.owner(&root).ok().flatten().as_deref() != Some(metadata.thread_id.as_str()) {
        return None;
    }
    let status = codex_worktree::agent_worktree_status(&root, &metadata).ok()?;
    Some(AgentWorktreeView {
        path: root,
        branch: status.branch,
        state: status.state,
    })
}

// ---------------------------------------------------------------------------------------------
// UC10: sweep forgotten agent worktrees
// ---------------------------------------------------------------------------------------------

/// Where and how a sweep runs.
#[derive(Clone, Debug)]
pub(crate) struct SweepTarget {
    pub(crate) settings: WorktreeSettings,
    pub(crate) source: PathBuf,
    pub(crate) cleanup: AgentWorktreeCleanup,
}

impl SweepTarget {
    fn key(&self) -> (PathBuf, PathBuf) {
        (self.settings.root.clone(), live_key(&self.source))
    }
}

/// R10.1: a root session with the feature and cleanup enabled sweeps the repository of its
/// cwd in the background.
pub(crate) fn sweep_on_session_start(config: &Config, session_source: &SessionSource) {
    if matches!(session_source, SessionSource::SubAgent(_))
        || !config.features.enabled(Feature::AgentWorktreeIsolation)
        || !config.agent_worktree_isolation.cleanup.enabled
    {
        return;
    }
    let Ok(settings) = worktree_settings(config) else {
        return;
    };
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let target = SweepTarget {
        settings,
        source: config.cwd.to_path_buf(),
        cleanup: config.agent_worktree_isolation.cleanup,
    };
    runtime.spawn(sweep(vec![target]));
}

/// R10.2: after the tree's worktrees were finalized, sweeps the repositories its isolated
/// children came from.
pub(super) async fn sweep_tree_repositories(bindings: &[Arc<AgentWorktreeBinding>]) {
    let mut seen = HashSet::new();
    let targets = bindings
        .iter()
        .filter_map(|binding| binding.sweep.clone())
        .filter(|target| target.cleanup.enabled && seen.insert(target.key()))
        .collect();
    sweep(targets).await;
}

async fn sweep(targets: Vec<SweepTarget>) {
    for target in targets {
        let source = target.source.clone();
        let result = tokio::task::spawn_blocking(move || {
            let policy = SweepPolicy {
                keep_count: target.cleanup.keep_count as usize,
                max_idle: Duration::from_secs(u64::from(target.cleanup.max_idle_hours) * 3600),
                min_idle: MIN_SWEEP_IDLE,
            };
            codex_worktree::sweep_agent_worktrees(
                &WorktreeManager::new(target.settings),
                &target.source,
                &policy,
                &is_live,
                SystemTime::now(),
            )
        })
        .await;
        match result {
            Ok(Ok(report)) => {
                for root in &report.removed {
                    tracing::info!(worktree = %root.display(), "removed a forgotten agent worktree");
                    record_worktree_outcome("swept");
                }
                for (root, err) in &report.failed {
                    warn!("cannot sweep agent worktree {}: {err}", root.display());
                }
            }
            // Outside a Git work tree there is nothing to sweep.
            Ok(Err(err)) => tracing::debug!(
                "agent worktree sweep skipped for {}: {err:#}",
                source.display()
            ),
            Err(err) => warn!("agent worktree sweep task failed: {err}"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// UC5: restore a removed worktree (Q13)
// ---------------------------------------------------------------------------------------------

/// R5.3: recreates a bound child's removed worktree from its own branch or its base before a
/// reload. A present worktree is left alone.
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock serializes restore, checkpoint and finalize of one worktree"
)]
pub(crate) async fn ensure_worktree_present(
    binding: &Arc<AgentWorktreeBinding>,
) -> Result<(), String> {
    if binding.root.join(".git").exists() {
        return Ok(());
    }
    let _state = binding.state.lock().await;
    let blocking = Arc::clone(binding);
    let restored = tokio::task::spawn_blocking(move || {
        let metadata = blocking.metadata();
        let source_root = blocking.source_root.as_path();
        let from = match codex_worktree::agent_branch_for_thread(
            source_root,
            &metadata.branch,
            &metadata.thread_id,
        )
        .map_err(|err| format!("{err:#}"))?
        {
            Some(branch) => RestoreFrom::Branch {
                name: branch.branch,
                tip: branch.tip,
            },
            None => RestoreFrom::Base,
        };
        let manager = WorktreeManager::new(blocking.settings.clone());
        codex_worktree::restore_agent_worktree(
            &manager,
            source_root,
            blocking.root.as_path(),
            &metadata,
            &from,
        )
        .map_err(|err| format!("{err:#}"))?;
        if let RestoreFrom::Branch { name, .. } = &from {
            blocking.record_branch(name);
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|err| format!("worktree restore task failed: {err}"))
    .and_then(|restored| restored);
    match restored {
        Ok(()) => {
            record_worktree_outcome("restored");
            Ok(())
        }
        Err(reason) => Err(missing_worktree_error(
            &binding.root,
            &binding.metadata().branch,
            &reason,
        )),
    }
}

/// R5.6: after a restart the binding of a child whose worktree was removed comes back from
/// its agent branch in the repository of the reloading caller's cwd or, failing that, of the
/// parent's stored cwd.
pub(crate) async fn restore_missing_binding(
    config: &Config,
    thread_id: ThreadId,
    agent_path: AgentPath,
    missing: MissingWorktree,
    parent_cwd: Option<PathBuf>,
) -> Result<Arc<AgentWorktreeBinding>, String> {
    let settings = worktree_settings(config)?;
    let branch = agent_branch_name(&agent_path, thread_id);
    let sources = std::iter::once(config.cwd.to_path_buf())
        .chain(parent_cwd)
        .collect::<Vec<_>>();
    let cleanup = config.agent_worktree_isolation.cleanup;
    let error_branch = branch.clone();
    let error_root = missing.root.clone();
    let blocking_settings = settings.clone();
    let restored = tokio::task::spawn_blocking(move || {
        restore_missing_blocking(blocking_settings, thread_id, &branch, &sources, &missing)
    })
    .await
    .map_err(|err| format!("worktree restore task failed: {err}"))
    .and_then(|restored| restored);
    let (root, cwd, source_root, metadata) =
        restored.map_err(|reason| missing_worktree_error(&error_root, &error_branch, &reason))?;
    record_worktree_outcome("restored");
    let sweep = Some(SweepTarget {
        settings: settings.clone(),
        source: source_root.to_path_buf(),
        cleanup,
    });
    Ok(Arc::new(AgentWorktreeBinding {
        thread_id,
        agent_path,
        environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
        _live: LiveRoot::register(root.as_path()),
        root,
        cwd,
        source_root,
        metadata: StdMutex::new(metadata),
        parent_checkout_protected: AtomicBool::new(false),
        read_only_explorer: AtomicBool::new(false),
        settings,
        sweep,
        state: Mutex::new(BindingState {
            stopped: true,
            finalized: false,
        }),
    }))
}

/// A child's stored cwd lies in a managed worktree that no longer exists.
#[derive(Clone, Debug)]
pub(crate) struct MissingWorktree {
    pub(crate) root: PathBuf,
    pub(crate) cwd: PathBuf,
}

fn restore_missing_blocking(
    settings: WorktreeSettings,
    thread_id: ThreadId,
    branch: &str,
    sources: &[PathBuf],
    missing: &MissingWorktree,
) -> Result<DiscoveredBinding, String> {
    // The checkout a worktree came from has the worktree directory's name.
    let candidates = sources
        .iter()
        .filter_map(|source| codex_worktree::checkout_root(source).ok())
        .filter(|root| root.file_name() == missing.root.file_name())
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(format!(
            "neither {} is in the repository the worktree came from",
            sources
                .iter()
                .map(|source| source.display().to_string())
                .collect::<Vec<_>>()
                .join(" nor ")
        ));
    }
    let mut found = None;
    for source_root in candidates {
        if let Some(recovered) =
            codex_worktree::agent_branch_for_thread(&source_root, branch, &thread_id.to_string())
                .map_err(|err| format!("{err:#}"))?
        {
            found = Some((source_root, recovered));
            break;
        }
    }
    let (source_root, recovered) =
        found.ok_or_else(|| "no agent branch holds its checkpoints".to_string())?;
    let metadata = AgentWorktreeMetadata {
        thread_id: thread_id.to_string(),
        base_sha: recovered.base_sha,
        base_kind: recovered.base_kind,
        branch: recovered.branch.clone(),
        source_root: source_root.to_str().map(str::to_owned),
    };
    let manager = WorktreeManager::new(settings);
    codex_worktree::restore_agent_worktree(
        &manager,
        &source_root,
        &missing.root,
        &metadata,
        &RestoreFrom::Branch {
            name: recovered.branch,
            tip: recovered.tip,
        },
    )
    .map_err(|err| format!("{err:#}"))?;
    let cwd = if missing.cwd.is_dir() {
        missing.cwd.clone()
    } else {
        missing.root.clone()
    };
    Ok((
        absolute(&missing.root)?,
        absolute(&cwd)?,
        absolute(&source_root)?,
        metadata,
    ))
}

/// R5.3: the reload error names the missing path, the agent branch and the reason.
pub(crate) fn missing_worktree_error(root: &Path, branch: &str, reason: &str) -> String {
    format!(
        "its worktree {} no longer exists and cannot be restored ({reason}); changes recorded before it was removed are on branch {branch}",
        root.display()
    )
}

// ---------------------------------------------------------------------------------------------
// UC11: merge for `apply_agent_worktree`
// ---------------------------------------------------------------------------------------------

/// Merges the child's branch into its parent checkout under the binding lock (R11.3).
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock keeps a checkpoint from moving the branch during the merge"
)]
pub(crate) async fn merge_for_parent(
    binding: &Arc<AgentWorktreeBinding>,
) -> Result<MergeOutcome, String> {
    let _state = binding.state.lock().await;
    let blocking = Arc::clone(binding);
    tokio::task::spawn_blocking(move || {
        codex_worktree::merge_agent_branch(
            blocking.source_root.as_path(),
            blocking.root.as_path(),
            &blocking.metadata(),
        )
        .map_err(|err| format!("{err:#}"))
    })
    .await
    .map_err(|err| format!("merge task failed: {err}"))?
}

/// R11.4, R11.6: a refusal names the paths, escaped like the completion block, and gives the
/// block's apply command.
pub(crate) fn apply_refusal(
    binding: &AgentWorktreeBinding,
    target: &str,
    reason: &str,
    paths: &[String],
) -> String {
    let metadata = binding.metadata();
    let paths = paths
        .iter()
        .map(|path| quote_block_path(path).into_owned())
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "cannot apply agent {target}: {reason} {paths}. Nothing was written. Resolve it yourself: read the agent's version with `git show {branch}:<path>` and edit your files, or run the command from its <agent_worktree> block and fix what it rejects.\napply: {command}",
        target = block_text(target),
        reason = block_text(reason),
        paths = if paths.is_empty() {
            "this branch".to_string()
        } else {
            paths
        },
        branch = metadata.branch,
        command = apply_command(&metadata.base_sha, &metadata.branch),
    )
}
