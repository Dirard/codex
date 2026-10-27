//! Fork: the workflow residency pool (`specs/dynamic-workflow-2026-10-25.md`, UC4).
//!
//! Workflow children have their own residency pool, so a run neither consumes nor evicts
//! the agents the model delegates to directly. The pool is a second `V2ResidencyState`
//! inside the first one, under the same lock.

use super::*;

/// The residency pool a V2 child counts against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResidencyPool {
    Agents,
    Workflow,
}

impl ResidencyPool {
    /// Workflow children use the workflow pool.
    pub(crate) fn for_spawn(options: &crate::agent::types::SpawnAgentOptions) -> Self {
        if options.workflow_child.is_some() {
            Self::Workflow
        } else {
            Self::Agents
        }
    }

    /// The pool capacity. The workflow pool holds the agents that every allowed run may have
    /// working at once; `agents_capacity` is the capacity of the agents pool.
    pub(super) fn capacity(self, config: &Config, agents_capacity: usize) -> usize {
        match self {
            Self::Agents => agents_capacity,
            Self::Workflow => config
                .workflows
                .max_concurrent_agents
                .saturating_mul(config.workflows.max_active_runs)
                .max(1),
        }
    }
}

impl V2ResidencyState {
    /// The residents and pending slots of `pool`.
    pub(super) fn pool(&mut self, pool: ResidencyPool) -> &mut Self {
        match pool {
            ResidencyPool::Agents => self,
            ResidencyPool::Workflow => self.workflow.get_or_insert_with(Box::default),
        }
    }

    /// The pool that holds `thread_id`: a workflow child stays in its own pool.
    pub(super) fn pool_of(&mut self, thread_id: ThreadId) -> &mut Self {
        let in_workflow_pool = self
            .workflow
            .as_ref()
            .is_some_and(|workflow| workflow.residents.contains(&thread_id));
        if in_workflow_pool {
            self.pool(ResidencyPool::Workflow)
        } else {
            self
        }
    }

    /// Leave every pool.
    pub(super) fn remove_resident(&mut self, thread_id: ThreadId) {
        self.residents
            .retain(|resident_thread_id| *resident_thread_id != thread_id);
        if let Some(workflow) = self.workflow.as_mut() {
            workflow
                .residents
                .retain(|resident_thread_id| *resident_thread_id != thread_id);
        }
    }
}

impl LocalAgentControl {
    /// A reloaded workflow child returns to the workflow pool.
    pub(super) fn reload_pool(&self, protected_thread_id: Option<ThreadId>) -> ResidencyPool {
        if protected_thread_id
            .is_some_and(|thread_id| self.runtime.workflow_children.is_workflow_thread(thread_id))
        {
            ResidencyPool::Workflow
        } else {
            ResidencyPool::Agents
        }
    }

    /// Reserve a slot in `pool`.
    pub(in crate::agent::control) async fn reserve_v2_residency_slot_in(
        &self,
        state: &Arc<ThreadManagerState>,
        config: &Config,
        membership: &AgentTreeMembership,
        protected_thread_id: Option<ThreadId>,
        pool: ResidencyPool,
    ) -> CodexResult<V2ResidencySlot> {
        let capacity = pool.capacity(
            config,
            config
                .effective_agent_max_threads(MultiAgentVersion::V2)
                .unwrap_or(usize::MAX),
        );
        Arc::clone(&self.runtime.residency)
            .reserve_slot(state, capacity, membership, protected_thread_id, pool)
            .await
    }
}

impl CodexThread {
    /// Fork (agent worktree isolation, P2-1): the loaded agent rests: it started its work
    /// and could be unloaded now, with no turn, background work or pending trigger.
    pub(crate) async fn fork_at_rest(&self) -> bool {
        !matches!(
            self.agent_status().await,
            crate::agent::AgentStatus::PendingInit
        ) && super::is_unloadable(self).await
    }
}

/// Unloading would kill an armed process or skip a turn a notification starts (other queued
/// notifications are recorded by the shutdown, R3.6, R2.5), or a workflow run still waits
/// for this child's answer.
pub(super) async fn blocks_eviction(thread: &CodexThread) -> bool {
    thread.session.background_terminals_block_eviction().await
        || thread
            .session
            .services
            .local_agent_runtime
            .workflow_children
            .has_pending_call(thread.session.thread_id)
}
