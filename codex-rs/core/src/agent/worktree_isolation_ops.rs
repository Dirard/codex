//! Fork: visibility, cleanup, restoration and merging of agent worktrees (amend 2026-10-27).
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC9 (`list_agents` and app-server
//! fields), UC10 (sweep forgotten worktrees), UC5 R5.3/R5.6 (restore a removed worktree) and
//! UC11 (merge for `apply_agent_worktree`).

use super::*;
use crate::config::AgentWorktreeCleanup;
use codex_protocol::protocol::SessionSource;
use codex_worktree::AgentWorktreeHold;
use codex_worktree::AgentWorktreeState;
use codex_worktree::MergeOutcome;
use codex_worktree::RestoreFrom;
use codex_worktree::SweepPolicy;
use futures::StreamExt as _;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::SystemTime;

/// A sweep never removes a worktree that was active within this period (R10.4).
const MIN_SWEEP_IDLE: Duration = Duration::from_secs(3600);

/// A repository whose worktree list did not change is swept at most this often per process.
const SWEEP_INTERVAL: Duration = Duration::from_secs(600);

/// Worktree states read at once for one `list_agents` or `thread/list` answer.
const STATE_CONCURRENCY: usize = 8;

// ---------------------------------------------------------------------------------------------
// Roots held by bindings of this process
// ---------------------------------------------------------------------------------------------

static LIVE_ROOTS: LazyLock<StdMutex<HashMap<PathBuf, usize>>> = LazyLock::new(Default::default);

fn live_key(root: &Path) -> PathBuf {
    dunce::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

/// Marks a worktree root as held by a binding until dropped (R10.3): the in-process registry
/// keeps this process's sweep away, and the shared hold keeps every process's sweep and
/// finalize away while this process lives.
#[derive(Debug)]
pub(crate) struct LiveRoot {
    key: PathBuf,
    hold: StdMutex<Option<AgentWorktreeHold>>,
}

impl LiveRoot {
    pub(crate) fn register(root: &Path, hold: Option<AgentWorktreeHold>) -> Self {
        let key = live_key(root);
        *LIVE_ROOTS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(key.clone())
            .or_default() += 1;
        Self {
            key,
            hold: StdMutex::new(hold),
        }
    }

    /// Takes a new hold after the worktree was recreated: the old one locks a removed file.
    pub(crate) fn replace_hold(&self, hold: Option<AgentWorktreeHold>) {
        *self.hold.lock().unwrap_or_else(PoisonError::into_inner) = hold;
    }

    /// Lets the finalize of a stopped tree remove the worktree (R6.1).
    pub(crate) fn release_hold(&self) {
        self.replace_hold(None);
    }
}

impl Drop for LiveRoot {
    fn drop(&mut self) {
        let mut roots = LIVE_ROOTS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(count) = roots.get_mut(&self.key) {
            *count -= 1;
            if *count == 0 {
                roots.remove(&self.key);
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

/// A shared hold of a worktree for a binding (blocking); a failure only loses the protection
/// against other processes and is logged.
pub(crate) fn hold_worktree(root: &Path) -> Option<AgentWorktreeHold> {
    codex_worktree::hold_agent_worktree(root)
        .inspect_err(|err| warn!("cannot hold agent worktree {}: {err:#}", root.display()))
        .ok()
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
/// (R9.1, R9.2). At most [`STATE_CONCURRENCY`] states are read at once.
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
    futures::stream::iter(bindings)
        .map(|binding| async move {
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
        })
        .buffer_unordered(STATE_CONCURRENCY)
        .filter_map(std::future::ready)
        .collect()
        .await
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
/// metadata and owner record name another thread. A missing checkout is `Removed` only when
/// the host recorded an agent worktree of this thread there (P3-6): a non-isolated thread
/// that merely ran in a removed managed checkout has no worktree of its own.
pub async fn describe_agent_worktree(
    config: &Config,
    thread_id: ThreadId,
    cwd: &Path,
) -> Option<AgentWorktreeView> {
    if !config.features.enabled(Feature::AgentWorktreeIsolation) {
        return None;
    }
    let settings = worktree_settings(config).ok()?;
    let records = agent_records_dir(config);
    let cwd = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || describe_blocking(settings, &records, thread_id, &cwd))
        .await
        .ok()
        .flatten()
}

fn describe_blocking(
    settings: WorktreeSettings,
    records: &Path,
    thread_id: ThreadId,
    cwd: &Path,
) -> Option<AgentWorktreeView> {
    let manager = WorktreeManager::new(settings);
    let root = manager.managed_checkout_root(cwd)?;
    if !root.join(".git").exists() {
        let record = codex_worktree::read_agent_record(records, &thread_id.to_string())
            .ok()
            .flatten()?;
        return (live_key(&record.root) == live_key(&root)).then_some(AgentWorktreeView {
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
    /// Host records of agent worktrees (R5.6): a worktree kept on its branch is removed only
    /// when its record can restore it.
    pub(crate) records: PathBuf,
}

impl SweepTarget {
    fn key(&self) -> SweepKey {
        (self.settings.root.clone(), live_key(&self.source))
    }
}

/// A swept repository: managed root and repository checkout.
type SweepKey = (PathBuf, PathBuf);

/// When each repository was swept by this process, with the fingerprint of its worktree list
/// then (P3-5).
static RECENT_SWEEPS: LazyLock<StdMutex<HashMap<SweepKey, (Instant, u64)>>> =
    LazyLock::new(Default::default);

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
        records: agent_records_dir(config),
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
        let result = tokio::task::spawn_blocking(move || sweep_blocking(&target)).await;
        match result {
            Ok(Ok(Some(report))) => {
                for root in &report.removed {
                    tracing::info!(worktree = %root.display(), "removed a forgotten agent worktree");
                    record_worktree_outcome("swept");
                }
                for (root, err) in &report.failed {
                    warn!("cannot sweep agent worktree {}: {err}", root.display());
                }
            }
            Ok(Ok(None)) => {}
            // Outside a Git work tree there is nothing to sweep.
            Ok(Err(err)) => tracing::debug!(
                "agent worktree sweep skipped for {}: {err:#}",
                source.display()
            ),
            Err(err) => warn!("agent worktree sweep task failed: {err}"),
        }
    }
}

/// Sweeps one repository unless this process swept it recently and its worktree list has not
/// changed since (P3-5); then drops host records that can no longer restore anything.
fn sweep_blocking(target: &SweepTarget) -> anyhow::Result<Option<codex_worktree::SweepReport>> {
    let key = target.key();
    let fingerprint = codex_worktree::worktree_list_fingerprint(&target.source)?;
    let recent = RECENT_SWEEPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
        .is_some_and(|(at, seen)| *seen == fingerprint && at.elapsed() < SWEEP_INTERVAL);
    if recent {
        return Ok(None);
    }
    let max_idle = Duration::from_secs(u64::from(target.cleanup.max_idle_hours) * 3600);
    let policy = SweepPolicy {
        keep_count: target.cleanup.keep_count as usize,
        max_idle,
        min_idle: MIN_SWEEP_IDLE,
    };
    let restorable = |metadata: &AgentWorktreeMetadata| {
        codex_worktree::read_agent_record(&target.records, &metadata.thread_id)
            .ok()
            .flatten()
            .is_some()
    };
    let now = SystemTime::now();
    let report = codex_worktree::sweep_agent_worktrees(
        &WorktreeManager::new(target.settings.clone()),
        &target.source,
        &policy,
        &is_live,
        &restorable,
        now,
    )?;
    if let Err(err) = codex_worktree::prune_agent_records(&target.records, now, max_idle) {
        warn!("cannot prune agent worktree records: {err:#}");
    }
    let fingerprint = codex_worktree::worktree_list_fingerprint(&target.source)?;
    RECENT_SWEEPS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, (Instant::now(), fingerprint));
    Ok(Some(report))
}

// ---------------------------------------------------------------------------------------------
// UC5: restore a removed worktree (Q13)
// ---------------------------------------------------------------------------------------------

/// R5.3: recreates a bound child's removed worktree from its own branch or its base before a
/// reload. A present worktree is left alone; the check is repeated under the binding lock, so
/// concurrent reloads restore it once (P3-8).
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
    if binding.root.join(".git").exists() {
        return Ok(());
    }
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
        blocking
            .live
            .replace_hold(hold_worktree(blocking.root.as_path()));
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
/// the host record written when the worktree was created: it names the parent checkout, the
/// base and the branch. Who wakes the child does not matter (P2-2): the caller's cwd is
/// never used to pick the parent checkout. The record is verified before anything is
/// restored; a missing or inconsistent record fails the reload.
pub(crate) async fn restore_missing_binding(
    config: &Config,
    thread_id: ThreadId,
    agent_path: AgentPath,
    missing: MissingWorktree,
    parent_thread_id: Option<ThreadId>,
) -> Result<Arc<AgentWorktreeBinding>, String> {
    let settings = worktree_settings(config)?;
    let records = agent_records_dir(config);
    let branch = agent_branch_name(&agent_path, thread_id);
    let cleanup = config.agent_worktree_isolation.cleanup;
    let error_root = missing.root.clone();
    let blocking_settings = settings.clone();
    let blocking_records = records.clone();
    let restored = tokio::task::spawn_blocking(move || {
        restore_missing_blocking(
            blocking_settings,
            &blocking_records,
            thread_id,
            parent_thread_id,
            &missing,
        )
    })
    .await
    .map_err(|err| format!("worktree restore task failed: {err}"))
    .and_then(|restored| restored);
    let DiscoveredBinding {
        root,
        cwd,
        source_root,
        metadata,
        hold,
    } = restored.map_err(|reason| missing_worktree_error(&error_root, &branch, &reason))?;
    record_worktree_outcome("restored");
    let sweep = Some(SweepTarget {
        settings: settings.clone(),
        source: source_root.to_path_buf(),
        cleanup,
        records,
    });
    Ok(Arc::new(AgentWorktreeBinding {
        thread_id,
        agent_path,
        environment_id: codex_exec_server::LOCAL_ENVIRONMENT_ID.to_string(),
        live: LiveRoot::register(root.as_path(), hold),
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
    records: &Path,
    thread_id: ThreadId,
    parent_thread_id: Option<ThreadId>,
    missing: &MissingWorktree,
) -> Result<DiscoveredBinding, String> {
    let record = codex_worktree::read_agent_record(records, &thread_id.to_string())
        .map_err(|err| format!("{err:#}"))?
        .ok_or_else(|| "Codex has no record of the checkout it was created from".to_string())?;
    if live_key(&record.root) != live_key(&missing.root) {
        return Err(format!(
            "the record of its worktree names {} instead",
            record.root.display()
        ));
    }
    let source_root = verified_parent_checkout(&settings, &record.source_root, parent_thread_id)?;
    let recovered = codex_worktree::agent_branch_for_thread(
        &source_root,
        &record.branch,
        &thread_id.to_string(),
    )
    .map_err(|err| format!("{err:#}"))?
    .ok_or_else(|| "no agent branch holds its checkpoints".to_string())?;
    let metadata = AgentWorktreeMetadata {
        thread_id: thread_id.to_string(),
        base_sha: record.base_sha.clone(),
        base_kind: record
            .base_kind()
            .ok_or_else(|| "its record has an unknown base kind".to_string())?,
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
    Ok(DiscoveredBinding {
        root: absolute(&missing.root)?,
        cwd: absolute(&cwd)?,
        source_root: absolute(&source_root)?,
        metadata,
        hold: hold_worktree(&missing.root),
    })
}

/// The recorded parent checkout, accepted only when it is still the top of a Git checkout
/// and, when it is itself a managed checkout (the worktree of an isolated parent, or a
/// `--worktree` root), when its owner record names the child's parent thread.
fn verified_parent_checkout(
    settings: &WorktreeSettings,
    recorded: &Path,
    parent_thread_id: Option<ThreadId>,
) -> Result<PathBuf, String> {
    let checkout = codex_worktree::checkout_root(recorded).map_err(|err| {
        format!(
            "its parent checkout {} is gone ({err:#})",
            recorded.display()
        )
    })?;
    if checkout != live_key(recorded) {
        return Err(format!(
            "its recorded parent checkout {} is not the top of a Git checkout",
            recorded.display()
        ));
    }
    let manager = WorktreeManager::new(settings.clone());
    if let Some(managed) = manager.managed_checkout_root(&checkout) {
        let owner = manager.owner(&managed).ok().flatten();
        let parent = parent_thread_id.map(|parent| parent.to_string());
        if owner.is_none() || owner != parent {
            return Err(format!(
                "its recorded parent checkout {} belongs to {}, not to its parent",
                checkout.display(),
                owner.as_deref().unwrap_or("no thread")
            ));
        }
    }
    Ok(checkout)
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
