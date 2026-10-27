//! Fork: isolates MultiAgentV2 children in managed git worktrees.
//!
//! The host creates each isolated child's worktree, moves the child's cwd, environment
//! selection and writable roots onto it, records the child's changes on an agent branch
//! after every terminal turn, and removes unchanged worktrees when the agent tree stops.
//! Bindings live in the tree's local runtime and are recovered after a restart from the
//! child's stored cwd, guarded by the worktree's owner record.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`.

use crate::config::Config;
use crate::environment_selection::TurnEnvironmentSnapshot;
use crate::environment_selection::TurnEnvironmentState;
use crate::session::turn_context::TurnEnvironment;
use codex_features::AgentIsolation;
use codex_features::AgentWorktreeBase;
use codex_features::Feature;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::models::ManagedFileSystemPermissions;
use codex_protocol::models::PermissionProfile;
use codex_protocol::models::PermissionProfileSnapshot;
use codex_protocol::models::ProfileWorkspaceRoot;
use codex_protocol::permissions::FileSystemPath;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxKind;
use codex_protocol::protocol::EnvironmentConfigState;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use codex_worktree::AgentWorktreeBaseKind;
use codex_worktree::AgentWorktreeMetadata;
use codex_worktree::BranchChanges;
use codex_worktree::CheckpointOutcome;
use codex_worktree::CheckpointRequest;
use codex_worktree::CreateWorktree;
use codex_worktree::RemoveOutcome;
use codex_worktree::WorktreeManager;
use codex_worktree::WorktreeSettings;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;
use tracing::warn;

const WORKTREE_METRIC: &str = "codex.multi_agent.worktree";

/// A child's managed worktree and the parent checkout it was created from.
#[derive(Debug)]
pub struct AgentWorktreeBinding {
    pub(crate) thread_id: ThreadId,
    pub(crate) agent_path: AgentPath,
    pub(crate) environment_id: String,
    /// Root of the managed worktree.
    pub(crate) root: AbsolutePathBuf,
    /// The child's cwd inside the worktree, matching the parent's cwd in its checkout.
    pub(crate) cwd: AbsolutePathBuf,
    /// Root of the parent's repository checkout.
    pub(crate) source_root: AbsolutePathBuf,
    /// Agent metadata. Its branch moves to a free name when a checkpoint finds the recorded
    /// one taken by another worktree (R4.2).
    metadata: StdMutex<AgentWorktreeMetadata>,
    parent_checkout_protected: AtomicBool,
    /// The child is a read-only explorer, so its note gives no edit instructions
    /// (explorer-and-questions spec, R9.3).
    read_only_explorer: AtomicBool,
    /// Submodules the worktree does not check out, named by the note (R2.11).
    submodules: Vec<String>,
    /// The branch tip that `apply_agent_worktree` applied in this process (R12.7).
    applied_tip: StdMutex<Option<String>>,
    /// Managed worktree settings the worktree was created or found with.
    settings: WorktreeSettings,
    /// Repository and limits of the sweep after the tree stops (UC10); `None` keeps it out.
    sweep: Option<SweepTarget>,
    /// Keeps the sweep and finalize of every process away from the worktree while the binding
    /// lives (R10.3).
    live: LiveRoot,
    /// Serializes checkpoint and finalize operations of this worktree.
    state: Mutex<BindingState>,
}

#[derive(Debug, Default)]
struct BindingState {
    /// No runtime of the child is loaded, so its worktree is quiescent.
    stopped: bool,
    finalized: bool,
}

impl AgentWorktreeBinding {
    /// The agent metadata with the branch that currently holds the child's work.
    pub(crate) fn metadata(&self) -> AgentWorktreeMetadata {
        self.metadata
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn record_branch(&self, branch: &str) {
        branch.clone_into(
            &mut self
                .metadata
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .branch,
        );
    }

    /// R12.7: the parent took the agent's branch at `tip` through `apply_agent_worktree`.
    pub(crate) fn record_applied(&self, tip: &str) {
        *self
            .applied_tip
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(tip.to_owned());
    }

    fn applied_tip(&self) -> Option<String> {
        self.applied_tip
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn parent_checkout_protected(&self) -> bool {
        self.parent_checkout_protected.load(Ordering::Acquire)
    }

    fn set_parent_checkout_protected(&self, protected: bool) {
        self.parent_checkout_protected
            .store(protected, Ordering::Release);
    }

    fn read_only_explorer(&self) -> bool {
        self.read_only_explorer.load(Ordering::Acquire)
    }

    /// Marks the child as a read-only explorer before its note is written (R9.3).
    pub(crate) fn set_read_only_explorer(&self, read_only_explorer: bool) {
        self.read_only_explorer
            .store(read_only_explorer, Ordering::Release);
    }

    /// Worktree details reported in the `spawn_agent` result.
    pub(crate) fn spawned_worktree(&self) -> SpawnedWorktree {
        let metadata = self.metadata();
        SpawnedWorktree {
            path: self.root.to_string_lossy().into_owned(),
            branch: metadata.branch,
            base: metadata.base_sha,
            base_kind: metadata.base_kind.as_str().to_owned(),
            parent_checkout_protected: self.parent_checkout_protected(),
        }
    }

    pub(crate) async fn mark_running(&self) {
        self.state.lock().await.stopped = false;
    }
}

/// The `worktree` object of a successful isolated `spawn_agent` call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct SpawnedWorktree {
    pub(crate) path: String,
    pub(crate) branch: String,
    pub(crate) base: String,
    pub(crate) base_kind: String,
    pub(crate) parent_checkout_protected: bool,
}

/// Isolated children of one local agent tree, loaded or not.
#[derive(Debug, Default)]
pub(crate) struct AgentWorktrees {
    bindings: StdMutex<HashMap<ThreadId, Arc<AgentWorktreeBinding>>>,
    tree_stopping: AtomicBool,
}

impl AgentWorktrees {
    pub(crate) fn insert(&self, binding: Arc<AgentWorktreeBinding>) {
        self.bindings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(binding.thread_id, binding);
    }

    pub(crate) fn remove(&self, thread_id: ThreadId) -> Option<Arc<AgentWorktreeBinding>> {
        self.bindings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&thread_id)
    }

    pub(crate) fn get(&self, thread_id: ThreadId) -> Option<Arc<AgentWorktreeBinding>> {
        self.bindings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&thread_id)
            .cloned()
    }

    fn all(&self) -> Vec<Arc<AgentWorktreeBinding>> {
        self.bindings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .cloned()
            .collect()
    }

    /// Records that a session of this tree stopped and finalizes worktrees once the tree stops.
    ///
    /// Eviction also stops a child, so a child finalizes only while the tree is stopping. The
    /// tree's root marks the tree as stopping; whichever of a child and the root stops last
    /// finalizes that child's worktree (spec UC6 step "finalize each isolated child").
    pub(crate) async fn session_stopped(
        &self,
        thread_id: ThreadId,
        is_tree_root: bool,
        tree_cancelled: bool,
    ) {
        if is_tree_root {
            self.tree_stopping.store(true, Ordering::Release);
        }
        let tree_stopping = tree_cancelled || self.tree_stopping.load(Ordering::Acquire);
        if let Some(binding) = self.get(thread_id) {
            binding.state.lock().await.stopped = true;
            if tree_stopping {
                finalize_tree_worktree(binding).await;
            }
        }
        if is_tree_root || tree_cancelled {
            finalize_tree_worktrees(self.all()).await;
        }
        if is_tree_root {
            ops::sweep_tree_repositories(&self.all()).await;
        }
    }
}

/// Resolves the effective isolation of a native MAv2 child.
///
/// An explicit argument wins; otherwise the prepared child config (including its role
/// config file) chooses. Worktree isolation requires the `agent_worktree_isolation` feature.
pub(crate) fn resolve_isolation(
    requested: Option<AgentIsolation>,
    config: &Config,
) -> Result<AgentIsolation, String> {
    let enabled = config.features.enabled(Feature::AgentWorktreeIsolation);
    match requested {
        Some(AgentIsolation::Worktree) if !enabled => Err(
            "worktree isolation is not enabled; enable the `agent_worktree_isolation` feature or spawn with isolation `none`"
                .to_string(),
        ),
        Some(isolation) => Ok(isolation),
        None if enabled => Ok(config.agent_worktree_isolation.default_isolation),
        None => Ok(AgentIsolation::None),
    }
}

/// The worktree handed to the local backend with a spawn request.
#[derive(Clone, Debug)]
pub struct SpawnWorktree {
    pub(crate) binding: Arc<AgentWorktreeBinding>,
    pub(crate) base_duration: Duration,
    pub(crate) create_duration: Duration,
}

/// A created and registered-to-be worktree with the child's rebased environments.
pub(crate) struct PreparedAgentWorktree {
    pub(crate) binding: Arc<AgentWorktreeBinding>,
    pub(crate) environments: TurnEnvironmentSnapshot,
    pub(crate) base_duration: Duration,
    pub(crate) create_duration: Duration,
}

impl PreparedAgentWorktree {
    pub(crate) fn spawn_worktree(&self) -> SpawnWorktree {
        SpawnWorktree {
            binding: Arc::clone(&self.binding),
            base_duration: self.base_duration,
            create_duration: self.create_duration,
        }
    }

    /// Removes a worktree whose child never registered; it holds no agent work (R2.7).
    pub(crate) async fn discard(self) {
        discard_worktree(&self.binding).await;
    }
}

/// Creates the managed worktree and records its metadata, then moves the child config onto it.
///
/// Spec UC2 step "create the managed worktree and record its metadata" and UC3 step "rebase
/// the environments onto the worktree". On any failure after creation the worktree is removed.
/// `read_only_explorer` selects the explorer's note (explorer-and-questions spec, R9.3).
pub(crate) async fn prepare_agent_worktree(
    thread_id: ThreadId,
    inherited_environments: &TurnEnvironmentSnapshot,
    config: &mut Config,
    agent_path: &AgentPath,
    read_only_explorer: bool,
) -> Result<PreparedAgentWorktree, String> {
    if cfg!(windows) {
        return Err(WINDOWS_UNSUPPORTED.to_string());
    }
    let environment = inherited_environments
        .single_local_environment()
        .ok_or_else(|| isolation_environment_unavailable(inherited_environments))?;
    let environment_id = environment.selection.environment_id.clone();
    let source_cwd = environment.cwd().to_abs_path().map_err(|_| {
        "worktree isolation requires a local absolute working directory".to_string()
    })?;
    let settings = worktree_settings(config)?;
    let base_kind = match config.agent_worktree_isolation.base {
        AgentWorktreeBase::WorkingTree => AgentWorktreeBaseKind::WorkingTree,
        AgentWorktreeBase::Head => AgentWorktreeBaseKind::Head,
    };
    let branch = agent_branch_name(agent_path, thread_id);
    let thread_id_text = thread_id.to_string();
    let blocking_source_cwd = source_cwd.clone();
    let blocking_settings = settings.clone();
    let records = agent_records_dir(config);
    let blocking_records = records.clone();
    let created = tokio::task::spawn_blocking(move || {
        create_worktree_blocking(
            blocking_settings,
            &blocking_records,
            blocking_source_cwd.as_path(),
            base_kind,
            thread_id_text,
            branch,
        )
    })
    .await
    .map_err(|err| format!("worktree isolation failed: {err}"))??;
    let CreatedWorktree {
        root,
        cwd,
        source_root,
        metadata,
        hold,
        submodules,
        base_duration,
        create_duration,
    } = created;
    let sweep = Some(SweepTarget {
        settings: settings.clone(),
        source: source_root.to_path_buf(),
        cleanup: config.agent_worktree_isolation.cleanup,
        records,
    });
    let binding = Arc::new(AgentWorktreeBinding {
        thread_id,
        agent_path: agent_path.clone(),
        environment_id: environment_id.clone(),
        live: LiveRoot::register(root.as_path(), hold),
        root,
        cwd,
        source_root,
        metadata: StdMutex::new(metadata),
        parent_checkout_protected: AtomicBool::new(false),
        read_only_explorer: AtomicBool::new(read_only_explorer),
        submodules,
        applied_tip: StdMutex::default(),
        settings,
        sweep,
        state: Mutex::default(),
    });
    let rebased = inherited_environments
        .rebase_onto_worktree(
            &environment_id,
            &binding.source_root,
            &binding.root,
            &binding.cwd,
        )
        .and_then(|environments| {
            rebase_child_config(config, &binding)?;
            Ok(environments)
        });
    let environments = match rebased {
        Ok(environments) => environments,
        Err(err) => {
            discard_worktree(&binding).await;
            return Err(err);
        }
    };
    binding.set_parent_checkout_protected(parent_checkout_protected(
        &environments,
        &environment_id,
        &binding,
    ));
    apply_isolation_note(config, &binding);
    record_worktree_outcome("created");
    Ok(PreparedAgentWorktree {
        binding,
        environments,
        base_duration,
        create_duration,
    })
}

struct CreatedWorktree {
    root: AbsolutePathBuf,
    cwd: AbsolutePathBuf,
    source_root: AbsolutePathBuf,
    metadata: AgentWorktreeMetadata,
    hold: Option<codex_worktree::AgentWorktreeHold>,
    submodules: Vec<String>,
    base_duration: Duration,
    create_duration: Duration,
}

/// Creates the worktree, its owner record and agent metadata, and the host record of where
/// it came from (R5.6), then holds it for the new binding.
fn create_worktree_blocking(
    settings: WorktreeSettings,
    records: &Path,
    source_cwd: &Path,
    base_kind: AgentWorktreeBaseKind,
    thread_id: String,
    branch: String,
) -> Result<CreatedWorktree, String> {
    let unavailable = |err: anyhow::Error| format!("worktree isolation is unavailable: {err:#}");
    let base_started = Instant::now();
    let base_sha = match base_kind {
        AgentWorktreeBaseKind::WorkingTree => codex_worktree::snapshot_working_tree(source_cwd),
        AgentWorktreeBaseKind::Head => codex_worktree::resolve_head_commit(source_cwd),
    }
    .map_err(unavailable)?;
    let base_duration = base_started.elapsed();
    let create_started = Instant::now();
    let manager = WorktreeManager::new(settings);
    let checkout = manager
        .create(&CreateWorktree {
            source_cwd: source_cwd.to_path_buf(),
            base: Some(base_sha.clone()),
        })
        .map_err(unavailable)?;
    let metadata = AgentWorktreeMetadata {
        thread_id: thread_id.clone(),
        base_sha,
        base_kind,
        branch,
        source_root: checkout.source_root.to_str().map(str::to_owned),
    };
    let recorded = manager
        .bind_thread(&checkout.root, &thread_id)
        .and_then(|()| codex_worktree::write_agent_metadata(&checkout.root, &metadata))
        .and_then(|()| {
            codex_worktree::write_agent_record(
                records,
                &codex_worktree::AgentWorktreeRecord::new(
                    &checkout.root,
                    &checkout.source_root,
                    &metadata,
                ),
            )
        })
        .map_err(unavailable)
        .and_then(|()| {
            Ok((
                absolute(&checkout.root)?,
                absolute(&checkout.cwd)?,
                absolute(&checkout.source_root)?,
            ))
        });
    match recorded {
        Ok((root, cwd, source_root)) => Ok(CreatedWorktree {
            hold: ops::hold_worktree(root.as_path()),
            submodules: unpopulated_submodules(root.as_path()),
            root,
            cwd,
            source_root,
            metadata,
            base_duration,
            create_duration: create_started.elapsed(),
        }),
        Err(err) => {
            if let Err(cleanup) =
                codex_worktree::discard_agent_worktree(&checkout.source_root, &checkout.root)
            {
                warn!("failed to remove incomplete agent worktree: {cleanup:#}");
            }
            Err(err)
        }
    }
}

fn absolute(path: &Path) -> Result<AbsolutePathBuf, String> {
    AbsolutePathBuf::from_absolute_path(path)
        .map_err(|err| format!("worktree path {} is not absolute: {err}", path.display()))
}

async fn discard_worktree(binding: &AgentWorktreeBinding) {
    let source_root = binding.source_root.to_path_buf();
    let root = binding.root.to_path_buf();
    match tokio::task::spawn_blocking(move || {
        codex_worktree::discard_agent_worktree(&source_root, &root)
    })
    .await
    {
        Ok(Ok(())) => record_worktree_outcome("discarded"),
        Ok(Err(err)) => warn!(
            "failed to remove agent worktree {}: {err:#}",
            binding.root.display()
        ),
        Err(err) => warn!("agent worktree cleanup task failed: {err}"),
    }
}

/// Hex digits of the thread ID that end an agent branch name.
const BRANCH_SUFFIX_DIGITS: usize = 16;

/// `codex/agents/<last agent path segment>-<last 16 hex digits of the thread ID>`.
///
/// Thread IDs are UUIDv7: their leading digits are a millisecond timestamp shared by agents
/// spawned within about a minute. The last 16 digits hold the low bits of the generator's
/// per-millisecond counter and 32 random bits, so siblings with the same task name get
/// different branches; a remaining collision moves the checkpoint to a free name (R4.2).
pub(crate) fn agent_branch_name(agent_path: &AgentPath, thread_id: ThreadId) -> String {
    let digits = thread_id
        .to_string()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<Vec<_>>();
    let suffix = digits[digits.len().saturating_sub(BRANCH_SUFFIX_DIGITS)..]
        .iter()
        .collect::<String>();
    format!("codex/agents/{}-{suffix}", agent_path.name())
}

/// Host records of where each agent worktree came from (R5.6), outside the managed root so
/// that removing worktrees by hand keeps them.
fn agent_records_dir(config: &Config) -> std::path::PathBuf {
    config.codex_home.as_path().join("agent-worktrees")
}

/// Shares Desktop's managed root (`$CODEX_HOME/worktrees` or `desktop.git-worktree-root`).
fn worktree_settings(config: &Config) -> Result<WorktreeSettings, String> {
    let desktop = config
        .config_layer_stack
        .effective_config()
        .get("desktop")
        .and_then(|desktop| serde_json::to_value(desktop).ok())
        .and_then(|desktop| match desktop {
            serde_json::Value::Object(settings) => Some(settings.into_iter().collect()),
            _ => None,
        });
    WorktreeSettings::for_cli(config.codex_home.as_path(), desktop.as_ref())
        .map_err(|err| format!("worktree isolation is unavailable: {err:#}"))
}

/// Where a path lies relative to the parent's repository.
enum RebasedPath {
    Inside(AbsolutePathBuf),
    Outside,
    ContainsRepository,
}

fn rebase_path(
    path: &AbsolutePathBuf,
    source_root: &AbsolutePathBuf,
    worktree_root: &AbsolutePathBuf,
) -> RebasedPath {
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    for candidate in [path.as_path(), canonical.as_path()] {
        if let Ok(relative) = candidate.strip_prefix(source_root) {
            return RebasedPath::Inside(worktree_root.join(relative));
        }
    }
    if source_root.as_path().starts_with(path.as_path())
        || source_root.as_path().starts_with(&canonical)
    {
        RebasedPath::ContainsRepository
    } else {
        RebasedPath::Outside
    }
}

/// Maps workspace roots onto the worktree (R3.2).
///
/// Roots inside the repository move to the same relative path in the worktree, roots outside
/// stay and roots containing the repository are dropped. When no root lies in the worktree
/// afterwards, the worktree root is added so the child can still write its own checkout.
pub(crate) fn rebase_workspace_roots(
    roots: &[AbsolutePathBuf],
    source_root: &AbsolutePathBuf,
    worktree_root: &AbsolutePathBuf,
) -> Vec<AbsolutePathBuf> {
    let mut rebased = Vec::with_capacity(roots.len().max(1));
    for root in roots {
        let next = match rebase_path(root, source_root, worktree_root) {
            RebasedPath::Inside(path) => path,
            RebasedPath::Outside => root.clone(),
            RebasedPath::ContainsRepository => continue,
        };
        if !rebased.contains(&next) {
            rebased.push(next);
        }
    }
    if !rebased
        .iter()
        .any(|root| root.as_path().starts_with(worktree_root.as_path()))
    {
        rebased.push(worktree_root.clone());
    }
    rebased
}

impl TurnEnvironmentSnapshot {
    /// Moves one ready environment onto an agent worktree: its cwd becomes `cwd`, and its
    /// workspace roots and permissions inside the parent's repository move to the worktree.
    /// Other environments stay unchanged.
    ///
    /// Spec `agent-worktree-isolation` UC3 step "rebase the environments onto the worktree".
    pub(crate) fn rebase_onto_worktree(
        &self,
        environment_id: &str,
        source_root: &AbsolutePathBuf,
        worktree_root: &AbsolutePathBuf,
        cwd: &AbsolutePathBuf,
    ) -> Result<Self, String> {
        let mut found = false;
        let environments = self
            .environments
            .iter()
            .map(|environment| match environment {
                TurnEnvironmentState::Ready(ready)
                    if ready.selection.environment_id == environment_id =>
                {
                    found = true;
                    let mut ready = ready.clone();
                    let selection = &mut ready.selection;
                    selection.cwd = PathUri::from_abs_path(cwd);
                    selection.workspace_roots = rebase_workspace_root_uris(
                        &selection.workspace_roots,
                        source_root,
                        worktree_root,
                    )?;
                    if let EnvironmentConfigState::Ready(config) = &mut selection.config {
                        config.workspace_roots = rebase_workspace_root_uris(
                            &config.workspace_roots,
                            source_root,
                            worktree_root,
                        )?;
                        config.permission_profile = rebase_permission_snapshot(
                            &config.permission_profile,
                            source_root,
                            worktree_root,
                        )?;
                    }
                    Ok(TurnEnvironmentState::Ready(ready))
                }
                TurnEnvironmentState::Starting(starting)
                    if starting.selection.environment_id == environment_id =>
                {
                    Err(format!(
                        "environment {environment_id} is still starting; retry once it is ready"
                    ))
                }
                other => Ok(other.clone()),
            })
            .collect::<Result<Vec<_>, String>>()?;
        if !found {
            return Err(format!(
                "environment {environment_id} is not available for the agent worktree"
            ));
        }
        Ok(Self { environments })
    }
}

pub(crate) fn rebase_workspace_root_uris(
    roots: &[PathUri],
    source_root: &AbsolutePathBuf,
    worktree_root: &AbsolutePathBuf,
) -> Result<Vec<PathUri>, String> {
    let roots = roots
        .iter()
        .map(PathUri::to_abs_path)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "worktree isolation requires local workspace roots".to_string())?;
    Ok(rebase_workspace_roots(&roots, source_root, worktree_root)
        .iter()
        .map(PathUri::from_abs_path)
        .collect())
}

/// Moves absolute permission entries inside the repository onto the worktree.
pub(crate) fn rebase_permission_profile(
    profile: &PermissionProfile,
    source_root: &AbsolutePathBuf,
    worktree_root: &AbsolutePathBuf,
) -> Result<PermissionProfile, String> {
    let PermissionProfile::Managed {
        file_system:
            ManagedFileSystemPermissions::Restricted {
                entries,
                glob_scan_max_depth,
            },
        network,
    } = profile
    else {
        return Ok(profile.clone());
    };
    let source_prefix = format!("{}/", source_root.display());
    let entries = entries
        .iter()
        .map(|entry| {
            let path = match &entry.path {
                FileSystemPath::Path { path } => match path.to_abs_path() {
                    Ok(absolute) => match rebase_path(&absolute, source_root, worktree_root) {
                        RebasedPath::Inside(rebased) => FileSystemPath::Path {
                            path: PathUri::from_abs_path(&rebased),
                        },
                        RebasedPath::Outside | RebasedPath::ContainsRepository => {
                            entry.path.clone()
                        }
                    },
                    Err(_) => entry.path.clone(),
                },
                FileSystemPath::GlobPattern { pattern } => {
                    match pattern.strip_prefix(&source_prefix) {
                        Some(relative) => FileSystemPath::GlobPattern {
                            pattern: format!("{}/{relative}", worktree_root.display()),
                        },
                        None if pattern == &source_root.display().to_string() => {
                            return Err(format!(
                                "permission pattern `{pattern}` covers the whole repository and cannot be moved to the worktree"
                            ));
                        }
                        None => entry.path.clone(),
                    }
                }
                FileSystemPath::Special { .. } => entry.path.clone(),
            };
            Ok(FileSystemSandboxEntry {
                path,
                access: entry.access,
                missing_path_behavior: entry.missing_path_behavior,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PermissionProfile::Managed {
        file_system: ManagedFileSystemPermissions::Restricted {
            entries,
            glob_scan_max_depth: *glob_scan_max_depth,
        },
        network: *network,
    })
}

pub(crate) fn rebase_permission_snapshot(
    snapshot: &PermissionProfileSnapshot,
    source_root: &AbsolutePathBuf,
    worktree_root: &AbsolutePathBuf,
) -> Result<PermissionProfileSnapshot, String> {
    let profile =
        rebase_permission_profile(snapshot.permission_profile(), source_root, worktree_root)?;
    Ok(match snapshot.active_permission_profile() {
        Some(active) => {
            let roots = snapshot
                .profile_workspace_roots()
                .iter()
                .filter_map(|root| match root.as_uri().to_abs_path() {
                    Ok(path) => match rebase_path(&path, source_root, worktree_root) {
                        RebasedPath::Inside(rebased) => Some(ProfileWorkspaceRoot::from(rebased)),
                        RebasedPath::Outside => Some(root.clone()),
                        RebasedPath::ContainsRepository => None,
                    },
                    Err(_) => Some(root.clone()),
                })
                .collect();
            PermissionProfileSnapshot::active_with_profile_workspace_roots(profile, active, roots)
        }
        None => PermissionProfileSnapshot::legacy(profile),
    })
}

/// Moves the child config's cwd, workspace roots and permissions onto the worktree (R3.1–R3.3).
pub(crate) fn rebase_child_config(
    config: &mut Config,
    binding: &AgentWorktreeBinding,
) -> Result<(), String> {
    config.cwd = binding.cwd.clone();
    let roots =
        rebase_workspace_roots(&config.workspace_roots, &binding.source_root, &binding.root);
    config.workspace_roots = roots;
    let snapshot = config.permissions.permission_profile_state().snapshot();
    let rebased = rebase_permission_snapshot(&snapshot, &binding.source_root, &binding.root)?;
    if rebased != snapshot {
        config
            .permissions
            .set_permission_profile_from_session_snapshot(rebased)
            .map_err(|err| format!("worktree permissions are invalid: {err}"))?;
    }
    Ok(())
}

/// Whether the child's sandbox keeps the parent's checkout read-only (R3.4).
fn parent_checkout_protected(
    environments: &TurnEnvironmentSnapshot,
    environment_id: &str,
    binding: &AgentWorktreeBinding,
) -> bool {
    environments
        .turn_environments()
        .find(|environment| environment.selection.environment_id == environment_id)
        .is_some_and(|environment| environment_protects_source(environment, binding))
}

fn environment_protects_source(
    environment: &TurnEnvironment,
    binding: &AgentWorktreeBinding,
) -> bool {
    let policy = environment
        .permission_profile_with_workspace_roots()
        .file_system_sandbox_policy();
    match policy.kind {
        FileSystemSandboxKind::Restricted => !policy
            .can_write_local_path_with_cwd(binding.source_root.as_path(), binding.cwd.as_path()),
        FileSystemSandboxKind::Unrestricted | FileSystemSandboxKind::ExternalSandbox => false,
    }
}

/// Developer instructions appended for an isolated child (R3.5); a read-only explorer gets its
/// own note without edit instructions (explorer-and-questions spec, R9.3).
pub(crate) fn isolation_developer_note(binding: &AgentWorktreeBinding) -> String {
    let metadata = binding.metadata();
    if binding.read_only_explorer() {
        return crate::agent::explorer_profile::worktree_developer_note(
            binding.cwd.as_path(),
            metadata.base_kind.description(),
            short_sha(&metadata.base_sha),
            binding.source_root.as_path(),
        );
    }
    // D8: without a protecting sandbox Git writes would succeed in the shared metadata.
    let (protection, git_writes) = if binding.parent_checkout_protected() {
        ("read-only", "they fail in the sandbox")
    } else {
        (
            "still writable (the sandbox does not protect it)",
            "they would change the repository metadata shared with the parent checkout",
        )
    };
    format!(
        "You are working in an isolated git worktree at {cwd}. It was created from the {base_description} ({base_short}) of the parent checkout {source_root}, which is {protection} for you. Keep all edits inside your working directory and prefer relative paths. Do not run git commands that write repository metadata (add, commit, switch, stash, merge): {git_writes}. Do not request escalated permissions to edit the parent checkout. When you finish, Codex records your changes on branch {branch} and appends an <agent_worktree> block with the merge command to your result; do not write that block yourself.",
        cwd = binding.cwd.display(),
        base_description = metadata.base_kind.description(),
        base_short = short_sha(&metadata.base_sha),
        source_root = binding.source_root.display(),
        branch = metadata.branch,
    ) + &submodule_note(&binding.submodules)
}

/// Submodules listed by name in the isolation note before the rest are only counted.
const LISTED_SUBMODULES: usize = 5;

/// R2.11: the sentence of the isolation note about submodules the worktree does not check
/// out; empty without such submodules.
fn submodule_note(submodules: &[String]) -> String {
    if submodules.is_empty() {
        return String::new();
    }
    let mut names = submodules
        .iter()
        .take(LISTED_SUBMODULES)
        .map(|path| quote_block_path(path).into_owned())
        .collect::<Vec<_>>()
        .join(", ");
    if submodules.len() > LISTED_SUBMODULES {
        names.push_str(&format!(
            " and {} more",
            submodules.len() - LISTED_SUBMODULES
        ));
    }
    format!(
        " Git submodules are not checked out in this worktree ({names}): their directories stay empty, and Codex does not record changes inside them."
    )
}

/// Submodules of a fresh, discovered or restored worktree that it does not check out
/// (blocking); a failure only loses the note's sentence.
pub(crate) fn unpopulated_submodules(root: &Path) -> Vec<String> {
    match codex_worktree::unpopulated_gitlinks(root) {
        Ok(paths) => paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        Err(err) => {
            warn!(
                "cannot list the submodules of agent worktree {}: {err:#}",
                root.display()
            );
            Vec::new()
        }
    }
}

/// R2.6: worktree isolation is refused on Windows until it is verified there.
pub(crate) const WINDOWS_UNSUPPORTED: &str =
    "worktree isolation is not supported on Windows yet; spawn the agent with isolation `none`";

/// R2.5: the spawn refusal for environments that cannot host an agent worktree.
pub(crate) fn isolation_environment_unavailable(environments: &TurnEnvironmentSnapshot) -> String {
    format!(
        "{}; spawn the agent with isolation `none`",
        isolation_environment_reason(environments)
    )
}

/// R2.5: why the environments of a step cannot host an agent worktree. Every Git operation of
/// the isolation runs on this host, so a remote executor (exec-server) is named as such.
pub(crate) fn isolation_environment_reason(environments: &TurnEnvironmentSnapshot) -> String {
    let remote = environments
        .turn_environments()
        .filter(|environment| environment.environment.is_remote())
        .map(|environment| format!("`{}`", environment.selection.environment_id))
        .collect::<Vec<_>>();
    let ready = environments.turn_environments().count();
    if !remote.is_empty() {
        format!(
            "worktree isolation runs Git on this host and does not support the remote executor {} yet",
            remote.join(", ")
        )
    } else if environments.starting().next().is_some() {
        "worktree isolation needs a ready local environment, and an environment is still starting"
            .to_string()
    } else if ready > 1 {
        format!("worktree isolation supports one local environment, and this turn has {ready}")
    } else {
        "worktree isolation requires a single local environment, and this turn has none".to_string()
    }
}

pub(crate) fn apply_isolation_note(config: &mut Config, binding: &AgentWorktreeBinding) {
    let note = isolation_developer_note(binding);
    config.developer_instructions = Some(match config.developer_instructions.take() {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{note}"),
        Some(_) | None => note,
    });
}

/// Prepares a reloaded isolated child: its worktree must exist, and its config moves onto it.
///
/// Spec UC5 step "rebase the environments onto the worktree" for cold reload and resume.
pub(crate) fn prepare_reload(
    binding: &AgentWorktreeBinding,
    config: &mut Config,
    environments: Option<TurnEnvironmentSnapshot>,
) -> Result<Option<TurnEnvironmentSnapshot>, String> {
    if !binding.root.join(".git").exists() {
        return Err(format!(
            "its worktree {} no longer exists; changes recorded before it was removed are on branch {}",
            binding.root.display(),
            binding.metadata().branch
        ));
    }
    rebase_child_config(config, binding)?;
    let environments = environments
        .map(|environments| {
            environments.rebase_onto_worktree(
                &binding.environment_id,
                &binding.source_root,
                &binding.root,
                &binding.cwd,
            )
        })
        .transpose()?;
    if let Some(environments) = &environments {
        binding.set_parent_checkout_protected(parent_checkout_protected(
            environments,
            &binding.environment_id,
            binding,
        ));
    }
    apply_isolation_note(config, binding);
    Ok(environments)
}

/// What a child's stored cwd says about its isolation after a restart.
pub(crate) enum Discovery {
    NotIsolated,
    Bound(Arc<AgentWorktreeBinding>),
    /// The cwd lies in a managed worktree that no longer exists (R5.6).
    Missing(MissingWorktree),
}

/// Recovers a binding from a child's stored cwd after a restart (R5.2).
///
/// Paths outside the managed root, and worktrees whose agent metadata names another thread,
/// are not this child's isolation. An agent worktree for this thread is accepted only when
/// its owner record names the same thread. A removed managed worktree is reported as
/// `Missing`, so a reload can restore it (R5.6).
pub(crate) async fn discover_binding_or_missing(
    config: &Config,
    thread_id: ThreadId,
    agent_path: AgentPath,
    stored_cwd: &Path,
) -> Result<Discovery, String> {
    let settings = worktree_settings(config)?;
    let records = agent_records_dir(config);
    let stored_cwd = stored_cwd.to_path_buf();
    let blocking_settings = settings.clone();
    let blocking_records = records.clone();
    let discovered = tokio::task::spawn_blocking(move || {
        discover_binding_blocking(blocking_settings, &blocking_records, thread_id, &stored_cwd)
    })
    .await
    .map_err(|err| format!("worktree discovery failed: {err}"))??;
    Ok(match discovered {
        DiscoveryResult::NotIsolated => Discovery::NotIsolated,
        DiscoveryResult::Missing(missing) => Discovery::Missing(missing),
        DiscoveryResult::Bound(DiscoveredBinding {
            root,
            cwd,
            source_root,
            metadata,
            hold,
            submodules,
        }) => {
            let sweep = Some(SweepTarget {
                settings: settings.clone(),
                source: source_root.to_path_buf(),
                cleanup: config.agent_worktree_isolation.cleanup,
                records,
            });
            Discovery::Bound(Arc::new(AgentWorktreeBinding {
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
                settings,
                sweep,
                state: Mutex::new(BindingState {
                    stopped: true,
                    finalized: false,
                }),
            }))
        }
    })
}

/// A binding found or restored on disk, held for this process.
struct DiscoveredBinding {
    root: AbsolutePathBuf,
    cwd: AbsolutePathBuf,
    source_root: AbsolutePathBuf,
    metadata: AgentWorktreeMetadata,
    hold: Option<codex_worktree::AgentWorktreeHold>,
    submodules: Vec<String>,
}

enum DiscoveryResult {
    NotIsolated,
    Bound(DiscoveredBinding),
    Missing(MissingWorktree),
}

fn discover_binding_blocking(
    settings: WorktreeSettings,
    records: &Path,
    thread_id: ThreadId,
    stored_cwd: &Path,
) -> Result<DiscoveryResult, String> {
    let manager = WorktreeManager::new(settings);
    let Some(root) = manager.managed_checkout_root(stored_cwd) else {
        return Ok(DiscoveryResult::NotIsolated);
    };
    if !root.join(".git").exists() {
        // The cwd no longer resolves, so it is matched against the root as stored.
        let cwd = stored_cwd
            .strip_prefix(&root)
            .map(|relative| root.join(relative))
            .unwrap_or_else(|_| root.clone());
        return Ok(DiscoveryResult::Missing(MissingWorktree { root, cwd }));
    }
    let metadata = codex_worktree::read_agent_metadata(&root)
        .map_err(|err| format!("cannot read agent worktree metadata: {err:#}"))?;
    let Some(metadata) = metadata.filter(|metadata| metadata.thread_id == thread_id.to_string())
    else {
        return Ok(DiscoveryResult::NotIsolated);
    };
    let owner = manager.owner(&root).map_err(|err| {
        format!(
            "cannot read the owner of worktree {}: {err:#}",
            root.display()
        )
    })?;
    if owner.as_deref() != Some(metadata.thread_id.as_str()) {
        return Err(format!(
            "worktree {} is owned by {}, not by this agent",
            root.display(),
            owner.as_deref().unwrap_or("no thread")
        ));
    }
    // The recorded parent checkout may itself be a linked worktree (a `--worktree` root);
    // the main checkout is only a fallback for metadata without it.
    let recorded_source = metadata
        .source_root
        .as_deref()
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir());
    let source_root = recorded_source
        .map(Ok)
        .unwrap_or_else(|| codex_worktree::main_worktree_root(&root))
        .map_err(|err| {
            format!(
                "cannot locate the repository of worktree {}: {err:#}",
                root.display()
            )
        })?;
    let relative = stored_cwd
        .strip_prefix(&root)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    // Worktrees created before the host records existed get one now, from metadata the host
    // wrote, so they can be restored after a removal too (R5.6).
    let thread = thread_id.to_string();
    if matches!(
        codex_worktree::read_agent_record(records, &thread),
        Ok(None)
    ) && let Err(err) = codex_worktree::write_agent_record(
        records,
        &codex_worktree::AgentWorktreeRecord::new(&root, &source_root, &metadata),
    ) {
        warn!("cannot record agent worktree {}: {err:#}", root.display());
    }
    Ok(DiscoveryResult::Bound(DiscoveredBinding {
        hold: ops::hold_worktree(&root),
        submodules: unpopulated_submodules(&root),
        root: absolute(&root)?,
        cwd: absolute(&root.join(relative))?,
        source_root: absolute(&source_root)?,
        metadata,
    }))
}

/// Checkpoints the worktree and renders the completion-mail block (spec UC4).
pub(crate) async fn checkpoint_report(binding: &Arc<AgentWorktreeBinding>) -> String {
    let WorktreeCheckpoint {
        checkpoint_error,
        changes,
    } = checkpoint_changes(binding).await;
    render_worktree_block(binding, checkpoint_error.as_ref(), changes)
}

/// What one checkpoint of an agent worktree found: the failure of the checkpoint itself, if
/// any, and the agent branch's changes against its base (`None` without a branch).
pub(crate) struct WorktreeCheckpoint {
    pub(crate) checkpoint_error: Option<String>,
    pub(crate) changes: Result<Option<BranchChanges>, String>,
}

/// Records the worktree's changes on the agent branch and summarizes that branch (spec UC4);
/// [`checkpoint_report`] renders the result as the completion-mail block.
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock serializes checkpoint and finalize of one worktree (R4.6)"
)]
pub(crate) async fn checkpoint_changes(binding: &Arc<AgentWorktreeBinding>) -> WorktreeCheckpoint {
    let _state = binding.state.lock().await;
    let blocking = Arc::clone(binding);
    let result = tokio::task::spawn_blocking(move || {
        let mut metadata = blocking.metadata();
        let checkpoint = codex_worktree::checkpoint(CheckpointRequest {
            root: blocking.root.as_path(),
            metadata: &metadata,
            agent_path: blocking.agent_path.as_str(),
        })
        .map_err(|err| format!("{err:#}"));
        if let Ok(CheckpointOutcome::Committed { branch, .. }) = &checkpoint {
            metadata.branch.clone_from(branch);
        }
        let changes = codex_worktree::branch_changes(blocking.root.as_path(), &metadata)
            .map_err(|err| format!("{err:#}"));
        (checkpoint, changes)
    })
    .await;
    let (checkpoint, changes) = match result {
        Ok(result) => result,
        Err(err) => (Err(format!("checkpoint task failed: {err}")), Ok(None)),
    };
    if let Ok(CheckpointOutcome::Committed {
        branch,
        index_refresh_error,
        ..
    }) = &checkpoint
    {
        binding.record_branch(branch);
        if let Some(error) = index_refresh_error {
            warn!("agent worktree index refresh failed after checkpoint: {error}");
        }
    }
    WorktreeCheckpoint {
        checkpoint_error: checkpoint.err(),
        changes,
    }
}

/// The default command that applies an agent branch to the parent's checkout (R7.2).
///
/// `git diff-tree` is plumbing, so the user's `diff.external`, textconv, color, prefix and
/// relative-path settings do not reach the patch. `git apply` runs from the top of the
/// parent's checkout, because from a subdirectory it silently skips paths outside it, and
/// `--whitespace=nowarn` keeps `apply.whitespace` from rejecting or rewriting the patch.
pub(crate) fn apply_command(base_sha: &str, branch: &str) -> String {
    format!(
        "git diff-tree -p --binary {base_sha} {branch} | git -C \"$(git rev-parse --show-toplevel)\" apply --whitespace=nowarn"
    )
}

/// The tool that applies an agent branch, named first by the block (R4.10).
const APPLY_TOOL: &str = "apply_agent_worktree";

/// Renders the `<agent_worktree>` block appended to the completion message (R4.4, R4.5,
/// R4.7, R4.10, R7.2): the step "append the worktree block to the completion message".
///
/// The child chooses its file names, so paths, the checkout and failure reasons are escaped:
/// no value can add a line, close the block or split an entry of the `files` list.
pub(crate) fn render_worktree_block(
    binding: &AgentWorktreeBinding,
    checkpoint_error: Option<&String>,
    changes: Result<Option<BranchChanges>, String>,
) -> String {
    let metadata = binding.metadata();
    let base_short = short_sha(&metadata.base_sha);
    let mut lines = vec!["<agent_worktree>".to_string()];
    let changes = match changes {
        Ok(changes) => changes,
        Err(err) => {
            lines.push(format!("summary_failed: {}", block_text(&err)));
            None
        }
    };
    // R4.10: the tool comes first; it checks, approves and hooks the write like apply_patch.
    if changes.is_some() {
        lines.push(format!(
            "apply: {APPLY_TOOL} {} (the fallback command is only for binary files, executables and symlinks that it refuses)",
            serde_json::json!({ "target": binding.agent_path.as_str() })
        ));
    }
    match &changes {
        Some(_) => lines.push(format!("branch: {}", metadata.branch)),
        None => lines.push("branch: none".to_string()),
    }
    lines.push(format!(
        "base: {base_short} ({})",
        metadata.base_kind.description()
    ));
    if let Some(err) = checkpoint_error {
        lines.push(format!("checkpoint_failed: {}", block_text(err)));
    }
    match &changes {
        Some(changes) => {
            lines.push(format!("changes: {}", block_text(&changes.shortstat)));
            let mut files = changes
                .files
                .iter()
                .map(|file| {
                    format!(
                        "{} {}",
                        block_text(&file.status),
                        quote_block_path(&file.path)
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            if changes.total_files > changes.files.len() {
                files.push_str(&format!(
                    "; and {} more",
                    changes.total_files - changes.files.len()
                ));
            }
            lines.push(format!("files: {files}"));
        }
        None => lines.push("changes: none".to_string()),
    }
    lines.push(format!(
        "checkout: {}",
        quote_block_path(&binding.root.to_string_lossy())
    ));
    if changes.is_some() {
        lines.push(format!(
            "fallback: {}",
            apply_command(&metadata.base_sha, &metadata.branch)
        ));
    }
    lines.push("</agent_worktree>".to_string());
    lines.join("\n")
}

/// Whether `character` may appear verbatim in a block value: it cannot end a line, open or
/// close a tag, or hide text the way control, format and separator characters can.
fn is_plain_block_char(character: char) -> bool {
    match character {
        '<' | '>' => false,
        ' '..='~' => true,
        _ => !character.is_ascii() && character.is_alphanumeric(),
    }
}

/// Appends `character` as a Git-style C escape: a named escape for common control
/// characters, otherwise its UTF-8 bytes in octal.
fn push_escaped(out: &mut String, character: char) {
    let named = match character {
        '\u{7}' => "\\a",
        '\u{8}' => "\\b",
        '\t' => "\\t",
        '\n' => "\\n",
        '\u{b}' => "\\v",
        '\u{c}' => "\\f",
        '\r' => "\\r",
        _ => {
            let mut buffer = [0; 4];
            for byte in character.encode_utf8(&mut buffer).bytes() {
                let _ = write!(out, "\\{byte:03o}");
            }
            return;
        }
    };
    out.push_str(named);
}

/// Renders a path for the block (R4.7).
///
/// A path of plain characters other than `"`, `\` and `;` stays as is. Any other path is
/// quoted like Git's `core.quotePath` output, with `<` and `>` escaped too, so `; ` always
/// separates entries and a file name never starts a line or closes the block.
fn quote_block_path(path: &str) -> Cow<'_, str> {
    if path
        .chars()
        .all(|character| is_plain_block_char(character) && !matches!(character, '"' | '\\' | ';'))
    {
        return Cow::Borrowed(path);
    }
    let mut quoted = String::with_capacity(path.len() + 2);
    quoted.push('"');
    for character in path.chars() {
        match character {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(character);
            }
            character if is_plain_block_char(character) => quoted.push(character),
            character => push_escaped(&mut quoted, character),
        }
    }
    quoted.push('"');
    Cow::Owned(quoted)
}

/// Renders free text, such as a failure reason, on one block line (R4.7): whitespace runs,
/// including Unicode line separators, become one space and other non-plain characters are
/// escaped.
fn block_text(text: &str) -> String {
    let mut line = String::with_capacity(text.len());
    for (index, word) in text.split_whitespace().enumerate() {
        if index > 0 {
            line.push(' ');
        }
        for character in word.chars() {
            if is_plain_block_char(character) {
                line.push(character);
            } else {
                push_escaped(&mut line, character);
            }
        }
    }
    line
}

async fn finalize_tree_worktrees(bindings: Vec<Arc<AgentWorktreeBinding>>) {
    for binding in bindings {
        if binding.state.lock().await.stopped {
            finalize_tree_worktree(binding).await;
        }
    }
}

/// Checkpoints a stopped child's worktree and removes it when it never changed (R6.1, R6.4).
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the binding lock serializes checkpoint and finalize of one worktree (R4.6)"
)]
async fn finalize_tree_worktree(binding: Arc<AgentWorktreeBinding>) {
    let mut state = binding.state.lock().await;
    if state.finalized || !state.stopped {
        return;
    }
    state.finalized = true;
    let blocking = Arc::clone(&binding);
    let result = tokio::task::spawn_blocking(move || {
        let mut metadata = blocking.metadata();
        if blocking.root.join(".git").exists()
            && let CheckpointOutcome::Committed { branch, .. } =
                codex_worktree::checkpoint(CheckpointRequest {
                    root: blocking.root.as_path(),
                    metadata: &metadata,
                    agent_path: blocking.agent_path.as_str(),
                })?
        {
            blocking.record_branch(&branch);
            metadata.branch = branch;
        }
        // The stopped child no longer uses its worktree; a live binding of another process
        // still keeps it (R10.3).
        blocking.live.release_hold();
        let outcome = codex_worktree::remove_if_unchanged(
            blocking.source_root.as_path(),
            blocking.root.as_path(),
            &metadata,
        )?;
        // R12.7: a branch the parent applied goes with its worktree.
        if outcome == RemoveOutcome::Kept(codex_worktree::KeptReason::Branch)
            && ops::applied_branch_tip(&blocking, &metadata).is_some()
        {
            let closed = codex_worktree::remove_agent_checkout(
                blocking.source_root.as_path(),
                blocking.root.as_path(),
                &metadata,
            )?;
            if closed == RemoveOutcome::Removed {
                ops::delete_applied_branch(&blocking, &metadata);
            }
            return Ok(closed);
        }
        Ok::<_, anyhow::Error>(outcome)
    })
    .await;
    match result {
        Ok(Ok(RemoveOutcome::Removed | RemoveOutcome::AlreadyMissing)) => {
            record_worktree_outcome("removed");
        }
        Ok(Ok(RemoveOutcome::Kept(reason))) => {
            tracing::info!(
                worktree = %binding.root.display(),
                branch = %binding.metadata().branch,
                ?reason,
                "kept agent worktree with changes"
            );
            record_worktree_outcome("kept");
        }
        Ok(Err(err)) => {
            warn!(
                "failed to finalize agent worktree {}: {err:#}",
                binding.root.display()
            );
            record_worktree_outcome("kept");
        }
        Err(err) => warn!("agent worktree finalize task failed: {err}"),
    }
}

pub(crate) fn record_worktree_outcome(outcome: &'static str) {
    if let Some(metrics) = codex_otel::global() {
        let _ = metrics.counter(WORKTREE_METRIC, /*inc*/ 1, &[("outcome", outcome)]);
    }
}

fn short_sha(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

#[path = "worktree_isolation_ops.rs"]
mod ops;

pub use ops::AgentWorktreeView;
pub(crate) use ops::CloseOutcome;
pub(crate) use ops::ListedWorktree;
pub(crate) use ops::LiveRoot;
pub(crate) use ops::MissingWorktree;
pub(crate) use ops::SweepTarget;
pub use ops::agent_worktree_reload_error;
pub(crate) use ops::apply_refusal;
pub(crate) use ops::checkpoint_for_close;
pub(crate) use ops::close_worktree;
pub use ops::describe_agent_worktree;
pub(crate) use ops::ensure_worktree_present;
pub(crate) use ops::fork_add_unloaded;
pub(crate) use ops::fork_guardian_action_json;
pub(crate) use ops::listed_worktrees;
pub(crate) use ops::merge_for_parent;
pub(crate) use ops::restore_missing_binding;
pub(crate) use ops::sweep_on_session_start;

#[cfg(test)]
#[path = "worktree_isolation_tests.rs"]
mod tests;
