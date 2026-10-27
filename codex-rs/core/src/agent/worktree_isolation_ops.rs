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
    restore_if_missing(binding).await.map(|_| ())
}

/// R5.3: the restore of [`ensure_worktree_present`]; the caller holds the binding lock.
/// Returns whether the worktree was restored.
async fn restore_if_missing(binding: &Arc<AgentWorktreeBinding>) -> Result<bool, String> {
    if binding.root.join(".git").exists() {
        return Ok(false);
    }
    let blocking = Arc::clone(binding);
    let restored = tokio::task::spawn_blocking(move || {
        let metadata = blocking.metadata();
        let source_root = blocking.source_root.as_path();
        let record = blocking.sweep.as_ref().and_then(|target| {
            codex_worktree::read_agent_record(&target.records, &metadata.thread_id)
                .ok()
                .flatten()
        });
        let start = restore_start(source_root, &metadata, record.as_ref())?;
        let manager = WorktreeManager::new(blocking.settings.clone());
        codex_worktree::restore_agent_worktree(
            &manager,
            source_root,
            blocking.root.as_path(),
            &metadata,
            &start.from,
        )
        .map_err(|err| format!("{err:#}"))?;
        if let RestoreFrom::Branch { name, .. } = &start.from {
            blocking.record_branch(name);
        }
        blocking
            .live
            .replace_hold(hold_worktree(blocking.root.as_path()));
        Ok::<_, String>(start.source)
    })
    .await
    .map_err(|err| format!("worktree restore task failed: {err}"))
    .and_then(|restored| restored);
    match restored {
        Ok(source) => {
            record_worktree_outcome("restored");
            binding.note_restored(&source);
            Ok(true)
        }
        Err(reason) => Err(missing_worktree_error(
            &binding.root,
            &binding.metadata().branch,
            &reason,
        )),
    }
}

/// Where a removed worktree comes back from, and how the notices name it (P3-5).
pub(super) struct RestoreStart {
    pub(super) from: RestoreFrom,
    pub(super) source: String,
}

/// R5.3, R12.7, Q22, P3-5: the start of a restored worktree: the agent's own branch; without
/// it, the last checkpoint or the applied tip the host record names while the commit is
/// still usable; the base only when the record names no commit of the agent. A record whose
/// commits are all gone fails the restore: the base would silently drop the agent's work.
pub(super) fn restore_start(
    source_root: &Path,
    metadata: &AgentWorktreeMetadata,
    record: Option<&codex_worktree::AgentWorktreeRecord>,
) -> Result<RestoreStart, String> {
    let error = |err: anyhow::Error| format!("{err:#}");
    if let Some(branch) =
        codex_worktree::agent_branch_for_thread(source_root, &metadata.branch, &metadata.thread_id)
            .map_err(error)?
    {
        return Ok(RestoreStart {
            source: format!("branch {} at {}", branch.branch, short_sha(&branch.tip)),
            from: RestoreFrom::Branch {
                name: branch.branch,
                tip: branch.tip,
            },
        });
    }
    let recorded = record
        .map(|record| {
            [
                (
                    record.last_checkpoint.as_deref(),
                    "the last recorded commit",
                ),
                (
                    record.applied_tip.as_deref(),
                    "the commit the parent applied",
                ),
            ]
        })
        .into_iter()
        .flatten()
        .filter_map(|(tip, kind)| tip.map(|tip| (tip, kind)))
        .collect::<Vec<_>>();
    for (tip, kind) in &recorded {
        if codex_worktree::usable_commit(source_root, &metadata.base_sha, tip).map_err(error)? {
            return Ok(RestoreStart {
                source: format!(
                    "{kind} {} (branch {} is gone)",
                    short_sha(tip),
                    metadata.branch
                ),
                from: RestoreFrom::Commit {
                    tip: (*tip).to_string(),
                },
            });
        }
    }
    if let Some((tip, _)) = recorded.first() {
        return Err(format!(
            "no agent branch holds its checkpoints, and its last recorded commit {} is no longer in the repository",
            short_sha(tip)
        ));
    }
    Ok(RestoreStart {
        source: format!(
            "the base {} ({}); no changes of the agent were recorded",
            short_sha(&metadata.base_sha),
            metadata.base_kind.description()
        ),
        from: RestoreFrom::Base,
    })
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
    let (
        DiscoveredBinding {
            root,
            cwd,
            source_root,
            metadata,
            hold,
            submodules,
        },
        source,
    ) = restored.map_err(|reason| missing_worktree_error(&error_root, &branch, &reason))?;
    record_worktree_outcome("restored");
    let sweep = Some(SweepTarget {
        settings: settings.clone(),
        source: source_root.to_path_buf(),
        cleanup,
        records,
    });
    let binding = Arc::new(AgentWorktreeBinding {
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
        submodules,
        applied_tip: StdMutex::default(),
        restored: StdMutex::default(),
        settings,
        sweep,
        state: Mutex::new(BindingState {
            stopped: true,
            finalized: false,
        }),
    });
    binding.note_restored(&source);
    Ok(binding)
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
) -> Result<(DiscoveredBinding, String), String> {
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
    let base_kind = record
        .base_kind()
        .ok_or_else(|| "its record has an unknown base kind".to_string())?;
    let mut metadata = AgentWorktreeMetadata {
        thread_id: thread_id.to_string(),
        base_sha: record.base_sha.clone(),
        base_kind,
        branch: record.branch.clone(),
        source_root: source_root.to_str().map(str::to_owned),
    };
    // Q22, P3-5: the branch, else the agent's last recorded commit, else the base of an agent
    // that never recorded a change; the notices say which one.
    let start = restore_start(&source_root, &metadata, Some(&record))?;
    if let RestoreFrom::Branch { name, .. } = &start.from {
        metadata.branch.clone_from(name);
    }
    let manager = WorktreeManager::new(settings);
    codex_worktree::restore_agent_worktree(
        &manager,
        &source_root,
        &missing.root,
        &metadata,
        &start.from,
    )
    .map_err(|err| format!("{err:#}"))?;
    let cwd = if missing.cwd.is_dir() {
        missing.cwd.clone()
    } else {
        missing.root.clone()
    };
    Ok((
        DiscoveredBinding {
            root: absolute(&missing.root)?,
            cwd: absolute(&cwd)?,
            source_root: absolute(&source_root)?,
            metadata,
            hold: hold_worktree(&missing.root),
            submodules: unpopulated_submodules(&missing.root),
        },
        start.source,
    ))
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
/// block's fallback command under the same `fallback:` label (P3-7).
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
        "cannot apply agent {target}: {reason} {paths}. Nothing was written. Resolve it yourself: read the agent's version with `git show {branch}:<path>` and edit your files, or run this fallback command and fix what it rejects.\nfallback: {command}",
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

// ---------------------------------------------------------------------------------------------
// UC12: close the worktree of an agent that is done (amend 2026-10-27, deferred items)
// ---------------------------------------------------------------------------------------------

/// What closing an agent worktree did (R12.5, R12.6, R12.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CloseOutcome {
    /// The checkout is gone; `branch` keeps the agent's changes, `None` when it made none or
    /// when the parent applied it and Codex retired it (`applied` names it then).
    Removed {
        branch: Option<String>,
        applied: Option<AppliedBranch>,
        /// Ignored files that went with the checkout (P3-3).
        ignored: IgnoredEntries,
    },
    /// The checkout stays for this reason; `submodule_files` names the submodule
    /// directories that hold files of the agent (P3-3).
    Kept {
        reason: codex_worktree::KeptReason,
        submodule_files: Vec<String>,
    },
}

/// An applied branch Codex retired with the worktree (R12.7, P3-2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppliedBranch {
    pub(crate) branch: String,
    pub(crate) tip: String,
    /// The ref that keeps its commit until the parent's HEAD contains it.
    pub(crate) reference: String,
}

/// Ignored files and directories of a checkout: some names and how many there were.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IgnoredEntries {
    pub(crate) listed: Vec<String>,
    pub(crate) total: usize,
}

/// Ignored entries a close names before they go with the checkout.
const LISTED_IGNORED: usize = 3;

/// The refusal when an agent was woken between its unload and the close (R12.2).
const WOKEN_AGAIN: &str = "it was woken again; wait for it, then close its worktree";

/// R12.3: records the pending work of an agent at rest (an interrupted turn is not
/// checkpointed when it ends) before anything is applied or removed.
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock serializes checkpoint, close and finalize of one worktree"
)]
pub(crate) async fn checkpoint_for_close(
    binding: &Arc<AgentWorktreeBinding>,
) -> Result<(), String> {
    let state = binding.state.lock().await;
    if !state.stopped {
        return Err(WOKEN_AGAIN.to_string());
    }
    if !binding.root.join(".git").exists() {
        return Ok(());
    }
    let blocking = Arc::clone(binding);
    let checkpoint = tokio::task::spawn_blocking(move || {
        let checkpoint = codex_worktree::checkpoint(CheckpointRequest {
            root: blocking.root.as_path(),
            metadata: &blocking.metadata(),
            agent_path: blocking.agent_path.as_str(),
        })
        .map_err(|err| format!("{err:#}"));
        if let Ok(CheckpointOutcome::Committed { commit, .. }) = &checkpoint {
            record_checkpoint(&blocking, commit);
        }
        checkpoint
    })
    .await
    .map_err(|err| format!("checkpoint task failed: {err}"))??;
    if let CheckpointOutcome::Committed { branch, .. } = checkpoint {
        binding.record_branch(&branch);
    }
    drop(state);
    Ok(())
}

/// R12.5: removes the checkout of an agent at rest, keeping its branch and its binding, so a
/// later task restores the worktree (R5.3). Under the binding lock, which a turn of the agent
/// takes when it starts (P3-9), the agent must still be stopped, and `busy_agent` must find
/// no other agent working in the worktree (P2-1); the worktree stays otherwise.
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock serializes checkpoint, close, finalize and the start of a turn"
)]
pub(crate) async fn close_worktree<Busy>(
    binding: &Arc<AgentWorktreeBinding>,
    discard_submodule_files: bool,
    busy_agent: impl FnOnce() -> Busy,
) -> Result<CloseOutcome, String>
where
    Busy: std::future::Future<Output = Option<String>>,
{
    let state = binding.state.lock().await;
    if !state.stopped {
        return Err(WOKEN_AGAIN.to_string());
    }
    if let Some(agent) = busy_agent().await {
        return Err(working_in_worktree(&agent));
    }
    let blocking = Arc::clone(binding);
    let closed = tokio::task::spawn_blocking(move || {
        let metadata = blocking.metadata();
        let root = blocking.root.as_path();
        // P3-3: what the removal would drop that Codex never records is named first.
        let ignored = if root.join(".git").exists() {
            codex_worktree::ignored_entries(root, LISTED_IGNORED)
                .map(|(listed, total)| IgnoredEntries { listed, total })
                .unwrap_or_default()
        } else {
            IgnoredEntries::default()
        };
        // The binding's own hold would keep the exclusive lock away (R10.3).
        blocking.live.release_hold();
        let outcome = codex_worktree::remove_agent_checkout(
            blocking.source_root.as_path(),
            root,
            &metadata,
            discard_submodule_files,
        )
        .map_err(|err| format!("{err:#}"));
        let removed = matches!(
            outcome,
            Ok(RemoveOutcome::Removed | RemoveOutcome::AlreadyMissing)
        );
        if !removed {
            blocking.live.replace_hold(hold_worktree(root));
        }
        Ok::<_, String>(match outcome? {
            RemoveOutcome::Removed | RemoveOutcome::AlreadyMissing => {
                // R12.7: a branch the parent applied goes with its checkout.
                let applied = retire_applied_branch(&blocking, &metadata);
                let branch = codex_worktree::agent_branch_for_thread(
                    blocking.source_root.as_path(),
                    &metadata.branch,
                    &metadata.thread_id,
                )
                .ok()
                .flatten()
                .map(|branch| branch.branch);
                CloseOutcome::Removed {
                    branch,
                    applied,
                    ignored,
                }
            }
            RemoveOutcome::Kept(reason) => CloseOutcome::Kept {
                reason,
                submodule_files: codex_worktree::unpopulated_submodule_files(root)
                    .unwrap_or_default()
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
            },
        })
    })
    .await
    .map_err(|err| format!("close task failed: {err}"))??;
    drop(state);
    if matches!(closed, CloseOutcome::Removed { .. }) {
        record_worktree_outcome("closed");
    }
    Ok(closed)
}

/// Q22, P3-5: records `commit` as the agent's last checkpoint in its host record (blocking),
/// so a restore without the branch starts from it rather than from the base.
pub(super) fn record_checkpoint(binding: &AgentWorktreeBinding, commit: &str) {
    let Some(target) = binding.sweep.as_ref() else {
        return;
    };
    let metadata = binding.metadata();
    let recorded = codex_worktree::read_agent_record(&target.records, &metadata.thread_id)
        .and_then(|record| {
            let mut record = record.unwrap_or_else(|| {
                codex_worktree::AgentWorktreeRecord::new(
                    binding.root.as_path(),
                    binding.source_root.as_path(),
                    &metadata,
                )
            });
            record.last_checkpoint = Some(commit.to_string());
            codex_worktree::write_agent_record(&target.records, &record)
        });
    if let Err(err) = recorded {
        warn!(
            "cannot record the checkpoint of agent {}: {err:#}",
            binding.agent_path.as_str()
        );
    }
}

/// R12.7: the tip of the agent's own branch when it is the one `apply_agent_worktree` applied
/// in this process (blocking).
pub(super) fn applied_branch_tip(
    binding: &AgentWorktreeBinding,
    metadata: &AgentWorktreeMetadata,
) -> Option<String> {
    let applied = binding.applied_tip()?;
    let branch = codex_worktree::agent_branch_for_thread(
        binding.source_root.as_path(),
        &metadata.branch,
        &metadata.thread_id,
    )
    .ok()
    .flatten()?;
    (branch.tip == applied).then_some(applied)
}

/// R12.7, P3-2: the parent took the agent's branch at `tip` through `apply_agent_worktree`.
/// The host record keeps the tip, so a later restore gets the work back and a later sweep
/// retires the branch once its worktree is gone, also after a restart.
pub(crate) async fn record_applied(binding: &Arc<AgentWorktreeBinding>, tip: &str) {
    binding.remember_applied(tip);
    let blocking = Arc::clone(binding);
    let tip = tip.to_string();
    let recorded = tokio::task::spawn_blocking(move || {
        write_applied_tip(&blocking, &blocking.metadata(), &tip)
    })
    .await;
    if let Ok(Err(err)) | Err(err) = recorded.map_err(anyhow::Error::from) {
        warn!(
            "cannot record the applied tip of agent {}: {err:#}",
            binding.agent_path.as_str()
        );
    }
}

/// Writes `tip` as the applied tip of the binding's host record (blocking).
fn write_applied_tip(
    binding: &AgentWorktreeBinding,
    metadata: &AgentWorktreeMetadata,
    tip: &str,
) -> anyhow::Result<()> {
    let Some(target) = binding.sweep.as_ref() else {
        return Ok(());
    };
    let mut record = codex_worktree::read_agent_record(&target.records, &metadata.thread_id)?
        .unwrap_or_else(|| {
            codex_worktree::AgentWorktreeRecord::new(
                binding.root.as_path(),
                binding.source_root.as_path(),
                metadata,
            )
        });
    if record.applied_tip.as_deref() == Some(tip) {
        return Ok(());
    }
    record.applied_tip = Some(tip.to_string());
    codex_worktree::write_agent_record(&target.records, &record)
}

/// R12.7, P3-2: retires the agent branch the parent applied, after its checkout is gone
/// (blocking): `git branch` stops listing it, and a hidden ref keeps its commit until the
/// parent's HEAD contains the changes. The tip is in the host record first, so a later
/// restore gets the work back.
fn retire_applied_branch(
    binding: &AgentWorktreeBinding,
    metadata: &AgentWorktreeMetadata,
) -> Option<AppliedBranch> {
    let tip = applied_branch_tip(binding, metadata)?;
    if let Err(err) = write_applied_tip(binding, metadata, &tip) {
        warn!(
            "cannot record the applied tip of {}: {err:#}",
            metadata.branch
        );
        return None;
    }
    match codex_worktree::retire_applied_branch(
        binding.source_root.as_path(),
        binding.root.as_path(),
        metadata,
        &tip,
    ) {
        Ok(true) => {
            record_worktree_outcome("branch_retired");
            Some(AppliedBranch {
                branch: metadata.branch.clone(),
                reference: codex_worktree::applied_ref(&metadata.thread_id).ok()?,
                tip,
            })
        }
        Ok(false) => None,
        Err(err) => {
            warn!(
                "cannot retire the applied agent branch {}: {err:#}",
                metadata.branch
            );
            None
        }
    }
}

/// P2-1: why a worktree another agent still works in is not closed.
pub(crate) fn working_in_worktree(agent: &str) -> String {
    format!(
        "agent {agent} still works in its worktree (it descends from the agent or runs in its checkout); wait for it or interrupt it, then close the worktree"
    )
}

impl AgentWorktrees {
    /// The binding whose worktree holds `cwd`, for an agent that works in another agent's
    /// worktree, such as a helper spawned there with isolation `none` (P2-1).
    fn containing(&self, cwd: &Path) -> Option<Arc<AgentWorktreeBinding>> {
        self.all()
            .into_iter()
            .find(|binding| cwd.starts_with(binding.root.as_path()))
    }
}

// ---------------------------------------------------------------------------------------------
// R5.8: a loaded child gets its removed worktree back before its next turn
// ---------------------------------------------------------------------------------------------

impl crate::session::session::Session {
    /// Fork hook at the start of every task: an isolated child whose worktree was removed while
    /// it stayed loaded (Codex Desktop's cleanup, a close, the user) gets it back before the
    /// turn runs (R5.8), and so does an agent that works in another agent's worktree, such as
    /// a helper spawned there with isolation `none` (P2-1). The check and the restore run
    /// under the binding lock, which also marks the child's own worktree in use: a close that
    /// takes the lock later refuses, and one that took it first is undone here (P3-9).
    /// Without a binding of the tree this is a lookup in an empty map.
    pub(crate) async fn fork_restore_agent_worktree(
        &self,
        turn_context: &crate::session::turn_context::TurnContext,
    ) {
        let worktrees = &self.services.local_agent_runtime.worktrees;
        let (binding, own) = match worktrees.get(self.thread_id) {
            Some(binding) => (binding, true),
            None => match turn_context
                .initial_environments
                .turn_environments()
                .filter_map(|environment| environment.cwd().to_abs_path().ok())
                .find_map(|cwd| worktrees.containing(cwd.as_path()))
            {
                Some(binding) => (binding, false),
                None => return,
            },
        };
        let note = turn_note(&binding, own, begin_turn(&binding, own).await);
        if let Some(note) = note {
            // P3-5: the turn learns what happened to its working directory before it runs.
            let item = crate::context::ContextualUserFragment::into(
                crate::context::DeveloperInstructions::new(note),
            );
            self.record_conversation_items(turn_context, turn_context.model_info(), &[item])
                .await;
        }
    }
}

/// P3-5: what a turn is told about the worktree it runs in: a restore of the agent's own
/// worktree since its last turn (by this hook or by a reload), the restore of the worktree a
/// helper works in, or a restore that failed (R5.8), which used to reach only the log.
pub(super) fn turn_note(
    binding: &AgentWorktreeBinding,
    own: bool,
    begun: Result<bool, String>,
) -> Option<String> {
    match begun {
        Ok(_) if own => binding.take_restore_note(),
        Ok(true) => Some(format!(
            "Codex recreated the worktree {} of agent {}, which you work in, because it had been removed. Edits made in it after that agent's last turn ended, and ignored files, are gone. Check the files you rely on before you continue.",
            binding.root.display(),
            binding.agent_path.as_str()
        )),
        Ok(false) => None,
        Err(err) => {
            warn!(
                "cannot restore the worktree of agent {} before a turn: {err}",
                binding.agent_path.as_str()
            );
            Some(format!(
                "Your working directory is in the agent worktree {}, and it cannot be used: {err}. Commands that use it fail. Do not recreate it yourself; stop and report this in your answer.",
                binding.root.display()
            ))
        }
    }
}

/// R5.8, P3-9: under the binding lock, restores the worktree a turn is about to run in and,
/// for the worktree's own agent (`own`), marks it in use, so a close cannot remove it between
/// this check and the turn. Returns whether it restored the worktree.
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock serializes the start of a turn with a close of its worktree"
)]
pub(crate) async fn begin_turn(
    binding: &Arc<AgentWorktreeBinding>,
    own: bool,
) -> Result<bool, String> {
    let mut state = binding.state.lock().await;
    let restored = restore_if_missing(binding).await;
    if own {
        state.stopped = false;
    }
    restored
}

// ---------------------------------------------------------------------------------------------
// R3.8: Guardian learns that the reviewed agent works in an isolated worktree
// ---------------------------------------------------------------------------------------------

/// Fork hook of the Guardian prompt: the planned action JSON of an isolated child gets a
/// host-written `agent_worktree` object (R3.8). Other sessions keep the JSON unchanged.
pub(crate) fn fork_guardian_action_json(
    session: &crate::session::session::Session,
    action_json: String,
) -> String {
    match session
        .services
        .local_agent_runtime
        .worktrees
        .get(session.thread_id)
    {
        Some(binding) => guardian_action_json(&binding, action_json),
        None => action_json,
    }
}

/// The note Guardian reads next to the worktree paths (R3.8). It states only what the sandbox
/// enforces (P3-6, as D8 for the agent's note): Guardian reviews escalations, which leave the
/// sandbox, so the worktree limits nothing an approved action does.
fn guardian_note(parent_checkout_protected: bool) -> &'static str {
    if parent_checkout_protected {
        "Host context, not part of the action: Codex runs this agent in an isolated git worktree. Inside its sandbox the agent can write only that worktree, and the parent checkout is read-only for it. An approved escalation runs outside that sandbox and is not limited to the worktree: writes to the parent checkout or to its .git directory bypass the isolation. The agent's parent reviews the worktree's changes and applies them to the parent checkout; the agent was told not to edit the parent checkout and not to request escalated permissions for it."
    } else {
        "Host context, not part of the action: Codex runs this agent in an isolated git worktree, but its sandbox does not protect the parent checkout: its commands can write the parent checkout and its .git directory directly, which bypasses the isolation, and an approved escalation is not limited to the worktree either. The agent's parent reviews the worktree's changes and applies them to the parent checkout; the agent was told not to edit the parent checkout and not to request escalated permissions for it."
    }
}

pub(crate) fn guardian_action_json(binding: &AgentWorktreeBinding, action_json: String) -> String {
    let Ok(mut action) = serde_json::from_str::<serde_json::Value>(&action_json) else {
        return action_json;
    };
    let Some(fields) = action.as_object_mut() else {
        return action_json;
    };
    fields.insert(
        "agent_worktree".to_string(),
        serde_json::json!({
            "note": guardian_note(binding.parent_checkout_protected()),
            "worktree": binding.root.to_string_lossy(),
            "parent_checkout": binding.source_root.to_string_lossy(),
            "parent_checkout_protected": binding.parent_checkout_protected(),
        }),
    );
    action.sort_all_objects();
    serde_json::to_string_pretty(&action).unwrap_or(action_json)
}

// ---------------------------------------------------------------------------------------------
// R5.9: a failed restore reaches clients of a direct `thread/resume`
// ---------------------------------------------------------------------------------------------

/// The words of every reload error about a worktree that cannot be restored
/// ([`missing_worktree_error`]).
const MISSING_WORKTREE_MARKER: &str = " no longer exists and cannot be restored (";

/// The reload error of an isolated child whose worktree cannot be restored, if `err` is one
/// (R5.9).
pub fn agent_worktree_reload_error(err: &codex_protocol::error::CodexErr) -> Option<String> {
    match err.details() {
        codex_protocol::error::CodexErrorDetails::InvalidRequest(message)
            if message.contains("its worktree ") && message.contains(MISSING_WORKTREE_MARKER) =>
        {
            Some(message.clone())
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// D7, R9.6: isolated agents that are not loaded stay visible in `list_agents`
// ---------------------------------------------------------------------------------------------

/// Fork hook of `list_agents`: appends the isolated agents that `agents` misses because their
/// thread is not loaded (after a cold resume or an eviction).
pub(crate) async fn fork_add_unloaded(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
    path_prefix: &Option<String>,
    agents: &mut Vec<crate::agent::types::LiveAgent>,
) {
    let unloaded = session
        .services
        .local_agent_runtime
        .fork_unloaded_isolated_agents(
            &turn.config,
            &turn.session_source,
            path_prefix.as_deref(),
            agents,
        )
        .await;
    agents.extend(unloaded);
}
