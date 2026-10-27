//! Fork: worktree isolation hooks of the local agent backend (`spawn.rs`).
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md` (UC2 spawn, UC4 forked history,
//! UC5 reload and restore). The worktree logic itself lives in `agent/worktree_isolation.rs`.

use super::*;
use crate::agent::worktree_isolation::AgentWorktreeBinding;
use crate::agent::worktree_isolation::Discovery;
use crate::agent::worktree_isolation::discover_binding_or_missing;
use crate::agent::worktree_isolation::ensure_worktree_present;
use crate::agent::worktree_isolation::prepare_reload;
use crate::agent::worktree_isolation::restore_missing_binding;
use crate::codex_thread::CodexThread;
use crate::context::DeveloperInstructions;
use crate::thread_manager::StartThreadOptions;
use std::path::Path;
use std::path::PathBuf;

/// Keeps an isolated child's worktree binding registered only if its spawn commits.
pub(super) struct WorktreeRegistration {
    registered: Option<(LocalAgentRuntime, ThreadId)>,
}

impl WorktreeRegistration {
    /// An isolated child starts under the thread ID its worktree is bound to, and its binding
    /// is visible before its first turn can finish (spec UC2, UC4).
    pub(super) fn for_spawn(
        runtime: &LocalAgentRuntime,
        options: &crate::agent::types::SpawnAgentOptions,
        start_options: &mut StartThreadOptions,
    ) -> Self {
        let registered = options.worktree.as_ref().map(|worktree| {
            let binding = Arc::clone(&worktree.binding);
            let thread_id = binding.thread_id;
            start_options.reserved_thread_id = Some(thread_id);
            runtime.worktrees.insert(binding);
            (runtime.clone(), thread_id)
        });
        Self { registered }
    }

    pub(super) fn commit(mut self) {
        self.registered = None;
    }
}

impl Drop for WorktreeRegistration {
    fn drop(&mut self) {
        if let Some((runtime, thread_id)) = self.registered.take() {
            runtime.worktrees.remove(thread_id);
        }
    }
}

/// The worktree binding of a child being reloaded, if it is isolated.
pub(super) struct ReloadWorktree(Option<Arc<AgentWorktreeBinding>>);

impl ReloadWorktree {
    /// Without saved bindings an isolated child inherits the parent's environments moved
    /// onto its worktree; `inherits_parent` says that `inherited` came from the parent.
    pub(super) fn rebase_inherited(
        &self,
        inherited: Option<TurnEnvironmentSnapshot>,
        inherits_parent: bool,
        thread_id: ThreadId,
    ) -> CodexResult<Option<TurnEnvironmentSnapshot>> {
        match (self.0.as_ref(), inherited) {
            (Some(binding), Some(environments)) if inherits_parent => Ok(Some(
                environments
                    .rebase_onto_worktree(
                        &binding.environment_id,
                        &binding.source_root,
                        &binding.root,
                        &binding.cwd,
                    )
                    .map_err(|err| {
                        CodexErr::InvalidRequest(format!(
                            "cannot resume multi-agent v2 child {thread_id}: {err}"
                        ))
                    })?,
            )),
            (Some(_), None) if inherits_parent => Err(CodexErr::InvalidRequest(format!(
                "cannot resume multi-agent v2 child {thread_id}: its parent environments are unavailable"
            ))),
            (_, environments) => Ok(environments),
        }
    }

    /// A loaded runtime owns the worktree again until it stops.
    pub(super) async fn finish_reload<T>(self, reloaded: CodexResult<T>) -> CodexResult<T> {
        if reloaded.is_ok()
            && let Some(binding) = self.0
        {
            binding.mark_running().await;
        }
        reloaded
    }
}

/// A reloaded parent and the environments it hands to the child.
type ReloadParent = Option<(Arc<CodexThread>, TurnEnvironmentSnapshot)>;

impl LocalAgentControl {
    /// Restores an isolated child's worktree binding with its identity, without loading its
    /// runtime (R5.4). A rejected binding fails that child's reload.
    pub(super) async fn restore_worktree_binding(
        &self,
        config: &Config,
        thread_id: ThreadId,
        restored: Option<(Option<AgentPath>, PathBuf)>,
    ) {
        let Some((Some(agent_path), cwd)) = restored else {
            return;
        };
        match discover_binding_or_missing(config, thread_id, agent_path, &cwd).await {
            Ok(Discovery::Bound(binding)) => self.runtime.worktrees.insert(binding),
            // A removed worktree is restored by the reload, which knows the caller's cwd (R5.6).
            Ok(Discovery::NotIsolated | Discovery::Missing(_)) => {}
            Err(err) => {
                warn!("failed to restore the worktree of V2 agent {thread_id}: {err}");
            }
        }
    }

    /// An isolated child reloads into its worktree, never into the parent's checkout, bounded
    /// by the parent's current authority moved onto the worktree (spec UC5).
    pub(super) async fn prepare_worktree_reload(
        &self,
        config: &mut Config,
        thread_id: ThreadId,
        stored_cwd: &Path,
        session_source: &SessionSource,
        parent: ReloadParent,
    ) -> CodexResult<(ReloadWorktree, ReloadParent)> {
        let worktree = self
            .worktree_binding_for_reload(config, thread_id, stored_cwd, session_source)
            .await?;
        // R5.3: a removed worktree comes back from the agent branch or the base first.
        if let Some(binding) = worktree.as_ref() {
            ensure_worktree_present(binding).await.map_err(|err| {
                CodexErr::InvalidRequest(format!(
                    "cannot resume multi-agent v2 child {thread_id}: {err}"
                ))
            })?;
        }
        // A reloaded explorer gets the read-only worktree note, also after a restart or
        // without its parent (explorer-and-questions spec, R9.3).
        let explorer = crate::agent::explorer_profile::applies_to_source(config, session_source);
        if let Some(binding) = worktree.as_ref() {
            binding.set_read_only_explorer(explorer);
        }
        let parent = match (worktree.as_ref(), parent) {
            (Some(binding), Some((parent, environments))) => {
                // A reloaded explorer's worktree note judges the parent checkout by its
                // read-only access (explorer-and-questions spec, R9.2).
                let environments = if explorer {
                    crate::agent::explorer_profile::clamp_environment_snapshot(&environments)
                } else {
                    environments
                };
                let environments = prepare_reload(binding, config, Some(environments))
                    .map_err(|err| {
                        CodexErr::InvalidRequest(format!(
                            "cannot resume multi-agent v2 child {thread_id}: {err}"
                        ))
                    })?
                    .ok_or_else(|| {
                        CodexErr::Fatal("rebased parent environments are missing".to_string())
                    })?;
                Some((parent, environments))
            }
            (Some(binding), None) => {
                prepare_reload(binding, config, None).map_err(|err| {
                    CodexErr::InvalidRequest(format!(
                        "cannot resume multi-agent v2 child {thread_id}: {err}"
                    ))
                })?;
                None
            }
            (None, parent) => parent,
        };
        Ok((ReloadWorktree(worktree), parent))
    }

    /// The worktree binding of a child being reloaded, recovered from its stored cwd after a
    /// restart (spec UC5 step "find the worktree binding").
    async fn worktree_binding_for_reload(
        &self,
        config: &Config,
        thread_id: ThreadId,
        stored_cwd: &Path,
        session_source: &SessionSource,
    ) -> CodexResult<Option<Arc<AgentWorktreeBinding>>> {
        if let Some(binding) = self.runtime.worktrees.get(thread_id) {
            return Ok(Some(binding));
        }
        let Some(agent_path) = session_source.get_agent_path() else {
            return Ok(None);
        };
        let resume_error = |err: String| {
            CodexErr::InvalidRequest(format!(
                "cannot resume multi-agent v2 child {thread_id}: {err}"
            ))
        };
        let binding =
            match discover_binding_or_missing(config, thread_id, agent_path.clone(), stored_cwd)
                .await
                .map_err(resume_error)?
            {
                Discovery::NotIsolated => return Ok(None),
                Discovery::Bound(binding) => binding,
                // R5.6: the host record of the worktree names the parent checkout, verified
                // against the parent thread; whoever wakes the child does not choose it.
                Discovery::Missing(missing) => restore_missing_binding(
                    config,
                    thread_id,
                    agent_path,
                    missing,
                    session_source.parent_thread_id(),
                )
                .await
                .map_err(resume_error)?,
            };
        self.runtime.worktrees.insert(Arc::clone(&binding));
        Ok(Some(binding))
    }
}

impl LocalAgentControl {
    /// Records an isolated child's changes on its branch before the parent hears about the
    /// turn, and tells the parent how to apply them (spec UC4).
    pub(super) async fn with_worktree_checkpoint(
        &self,
        thread_id: ThreadId,
        mut message: String,
    ) -> String {
        if let Some(binding) = self.runtime.worktrees.get(thread_id) {
            let block = crate::agent::worktree_isolation::checkpoint_report(&binding).await;
            message.push_str("\n\n");
            message.push_str(&block);
        }
        message
    }
}

/// A reused parent context does not carry the child's developer instructions, so an
/// isolated child would never learn about its worktree. Appends the note once.
pub(super) fn append_isolation_note(
    options: &crate::agent::types::SpawnAgentOptions,
    forked_rollout_items: &mut Vec<RolloutItem>,
) {
    let Some(worktree) = options.worktree.as_ref() else {
        return;
    };
    let note = crate::agent::worktree_isolation::isolation_developer_note(&worktree.binding);
    let already_present = forked_rollout_items.iter().any(|item| {
        matches!(item, RolloutItem::ResponseItem(envelope)
            if serde_json::to_string(&envelope.item)
                .is_ok_and(|text| text.contains(&note)))
    });
    if !already_present {
        let note_message = ContextualUserFragment::into(DeveloperInstructions::new(note));
        forked_rollout_items.push(RolloutItem::ResponseItem(note_message.into()));
    }
}

impl LocalAgentRuntime {
    /// Unloads an isolated agent at rest before its worktree is closed (UC12, R12.2), as
    /// residency eviction does: its mail and environments stay for a later reload. An agent
    /// that is not loaded is already at rest; a running one is refused.
    pub(crate) async fn fork_unload_for_close(&self, thread_id: ThreadId) -> Result<(), String> {
        let state = self.upgrade().map_err(|err| err.to_string())?;
        let Ok(thread) = state.get_thread(thread_id).await else {
            return Ok(());
        };
        let membership = self.admit_start().map_err(|err| err.to_string())?;
        match state.try_evict_v2_thread(thread, &membership).await {
            Ok(super::ThreadEvictionOutcome::Evicted | super::ThreadEvictionOutcome::NotCurrent) => {
                Ok(())
            }
            Ok(super::ThreadEvictionOutcome::Busy) => Err(
                "it is still running a turn or has background work; wait for it or interrupt it, then close its worktree"
                    .to_string(),
            ),
            Err(err) => Err(format!("it could not be unloaded: {err}")),
        }
    }
}

impl LocalAgentRuntime {
    /// D7, R9.6: isolated agents of this tree whose thread is not loaded (after a cold resume
    /// or an eviction) and that `listed` therefore misses, as `shutdown` agents: a follow-up
    /// task reloads them. Without the feature, or without a worktree binding, nothing is added.
    pub(crate) async fn fork_unloaded_isolated_agents(
        &self,
        config: &Config,
        source: &SessionSource,
        path_prefix: Option<&str>,
        listed: &[crate::agent::types::LiveAgent],
    ) -> Vec<crate::agent::types::LiveAgent> {
        if !config
            .features
            .enabled(codex_features::Feature::AgentWorktreeIsolation)
        {
            return Vec::new();
        }
        let Ok(state) = self.upgrade() else {
            return Vec::new();
        };
        let prefix = match path_prefix {
            Some(prefix) => match source
                .get_agent_path()
                .unwrap_or_else(AgentPath::root)
                .resolve(prefix)
            {
                Ok(prefix) => Some(prefix),
                Err(_) => return Vec::new(),
            },
            None => None,
        };
        let mut unloaded = Vec::new();
        for metadata in self.registry.live_agents() {
            let Some(thread_id) = metadata.agent_id else {
                continue;
            };
            if listed.iter().any(|agent| agent.thread_id == thread_id)
                || self.worktrees.get(thread_id).is_none()
                || prefix.as_ref().is_some_and(|prefix| {
                    !super::agent_matches_prefix(metadata.agent_path.as_ref(), prefix)
                })
                || state.get_thread(thread_id).await.is_ok()
            {
                continue;
            }
            unloaded.push(crate::agent::types::LiveAgent {
                thread_id,
                metadata,
                status: AgentStatus::Shutdown,
            });
        }
        unloaded.sort_by(|left, right| {
            left.metadata
                .agent_path
                .as_deref()
                .unwrap_or_default()
                .cmp(right.metadata.agent_path.as_deref().unwrap_or_default())
        });
        unloaded
    }
}
