//! Accounts for resident MAv2 children and unloads exact idle runtimes while preserving their state.

use super::LocalAgentControl;
use super::LocalAgentRuntime;
use super::runtime::AgentTreeMembership;
use crate::agent::AgentStatus;
use crate::agent::registry::EvictedEnvironments;
use crate::codex_thread::CodexThread;
use crate::config::Config;
use crate::thread_manager::ThreadManagerState;
use codex_protocol::ThreadId;
use codex_protocol::error::AgentErrorContext;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrKind;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::warn;

/// The result of trying to unload one exact MAv2 child runtime.
#[derive(Debug, PartialEq, Eq)]
pub enum ThreadEvictionOutcome {
    /// Work or another teardown owns the runtime; retry later.
    Busy,
    /// This operation stopped and removed the runtime.
    Evicted,
    /// The runtime was removed or replaced; leave its replacement alone.
    NotCurrent,
}

impl ThreadManagerState {
    pub(crate) async fn try_evict_v2_thread(
        self: &Arc<Self>,
        thread: Arc<CodexThread>,
        membership: &AgentTreeMembership,
    ) -> CodexResult<ThreadEvictionOutcome> {
        let Ok(residency_guard) = Arc::clone(&thread.residency_gate).try_write_owned() else {
            return Ok(ThreadEvictionOutcome::Busy);
        };
        let thread_id = thread.session.thread_id;
        if !self
            .get_thread(thread_id)
            .await
            .is_ok_and(|registered| Arc::ptr_eq(&registered, &thread))
        {
            return Ok(ThreadEvictionOutcome::NotCurrent);
        }
        if !is_resident_candidate(&thread) || !is_unloadable(&thread).await {
            return Ok(ThreadEvictionOutcome::Busy);
        }
        let manager = Arc::clone(self);
        let teardown = membership
            .clone()
            .into_teardown_guard("resident_eviction", Some(thread_id));
        // Once shutdown is submitted, cancellation cannot release delivery or capacity early.
        tokio::spawn(async move {
            let _residency_guard = residency_guard;
            thread.ensure_rollout_materialized().await;
            if let Err(err) = thread.shutdown_and_wait().await {
                teardown.record_shutdown_failure("stop_resident", CodexErrKind::from(&err).into());
                teardown.complete();
                return Err(err);
            }
            // Dispatch has stopped and the residency guard excludes senders. A tree
            // shutdown deliberately discards its mailbox instead of retaining it.
            let mail = thread
                .session
                .input_queue
                .drain_mailbox()
                .await
                .into_iter()
                .map(|mail| mail.communication)
                .collect();
            let runtime = &thread.session.services.local_agent_runtime;
            let _ = runtime.mailboxes.enqueue(thread_id, /*id*/ None, mail);
            // Preserve configured bindings across eviction without retaining turn-only attachments.
            // Raw bindings keep the original pending owner configuration alive.
            let environments = EvictedEnvironments {
                selections: thread.environment_selections().await,
                bindings: thread
                    .session
                    .services
                    .turn_environments
                    .configured_snapshot_now(),
            };
            let mut threads = manager.threads.write().await;
            if threads
                .get(&thread_id)
                .is_some_and(|registered| !Arc::ptr_eq(registered, &thread))
            {
                teardown.complete();
                return Ok(ThreadEvictionOutcome::NotCurrent);
            }
            runtime
                .registry
                .save_evicted_environments(thread_id, environments);
            let removed = threads.remove(&thread_id).is_some();
            runtime.residency.remove(thread_id);
            teardown.complete();
            Ok(if removed {
                ThreadEvictionOutcome::Evicted
            } else {
                ThreadEvictionOutcome::NotCurrent
            })
        })
        .await?
    }
}

#[derive(Default)]
pub(super) struct V2Residency {
    state: Mutex<V2ResidencyState>,
}

#[derive(Default)]
struct V2ResidencyState {
    residents: VecDeque<ThreadId>,
    pending_slots: usize,
    /// Fork: residents and pending slots of the workflow pool.
    workflow_residents: VecDeque<ThreadId>,
    workflow_pending_slots: usize,
}

/// Fork: workflow children have their own residency pool (dynamic workflow, UC4), so a
/// run neither consumes nor evicts the agents the model delegates to directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResidencyPool {
    Agents,
    Workflow,
}

impl V2ResidencyState {
    fn pool(&mut self, pool: ResidencyPool) -> (&mut VecDeque<ThreadId>, &mut usize) {
        match pool {
            ResidencyPool::Agents => (&mut self.residents, &mut self.pending_slots),
            ResidencyPool::Workflow => (
                &mut self.workflow_residents,
                &mut self.workflow_pending_slots,
            ),
        }
    }
}

pub(super) struct V2ResidencySlot {
    residency: Arc<V2Residency>,
    active: bool,
    pool: ResidencyPool,
}

impl V2ResidencySlot {
    pub(super) fn commit(mut self, thread_id: ThreadId) {
        self.residency.commit_slot(thread_id, self.pool);
        self.active = false;
    }
}

impl Drop for V2ResidencySlot {
    fn drop(&mut self) {
        if self.active {
            self.residency.release_pending_slot(self.pool);
        }
    }
}

impl LocalAgentControl {
    pub(super) async fn reserve_v2_residency_slot(
        &self,
        state: &Arc<ThreadManagerState>,
        config: &Config,
        membership: &AgentTreeMembership,
        protected_thread_id: Option<ThreadId>,
    ) -> CodexResult<V2ResidencySlot> {
        // Fork: a reloaded workflow child returns to the workflow pool.
        let pool = if protected_thread_id
            .is_some_and(|thread_id| self.runtime.workflow_children.is_workflow_thread(thread_id))
        {
            ResidencyPool::Workflow
        } else {
            ResidencyPool::Agents
        };
        self.reserve_v2_residency_slot_in(state, config, membership, protected_thread_id, pool)
            .await
    }

    /// Fork: reserve a slot in `pool`. The workflow pool holds the agents that every
    /// allowed run may have working at once (dynamic workflow, UC4).
    pub(super) async fn reserve_v2_residency_slot_in(
        &self,
        state: &Arc<ThreadManagerState>,
        config: &Config,
        membership: &AgentTreeMembership,
        protected_thread_id: Option<ThreadId>,
        pool: ResidencyPool,
    ) -> CodexResult<V2ResidencySlot> {
        let capacity = match pool {
            ResidencyPool::Agents => config
                .effective_agent_max_threads(MultiAgentVersion::V2)
                .unwrap_or(usize::MAX),
            ResidencyPool::Workflow => config
                .workflows
                .max_concurrent_agents
                .saturating_mul(config.workflows.max_active_runs)
                .max(1),
        };
        Arc::clone(&self.runtime.residency)
            .reserve_slot(state, capacity, membership, protected_thread_id, pool)
            .await
    }

    pub(super) fn forget_v2_residency(&self, thread_id: ThreadId) {
        self.runtime.residency.remove(thread_id);
    }
}

impl LocalAgentRuntime {
    /// Pins and touches the registered runtime without waiting for unrelated eviction.
    pub(crate) async fn pin_v2_residency(
        &self,
        state: &ThreadManagerState,
        thread: &Arc<CodexThread>,
    ) -> CodexResult<Option<tokio::sync::OwnedRwLockReadGuard<()>>> {
        if !is_resident_candidate(thread) {
            return Ok(None);
        }
        let guard = Arc::clone(&thread.residency_gate).read_owned().await;
        let thread_id = thread.session.thread_id;
        if !Arc::ptr_eq(thread, &state.get_thread(thread_id).await?) {
            return Err(CodexErr::ThreadNotFound(thread_id));
        }
        self.residency.touch(thread_id);
        Ok(Some(guard))
    }
}

impl V2Residency {
    async fn reserve_slot(
        self: Arc<Self>,
        manager: &Arc<ThreadManagerState>,
        capacity: usize,
        membership: &AgentTreeMembership,
        protected_thread_id: Option<ThreadId>,
        pool: ResidencyPool,
    ) -> CodexResult<V2ResidencySlot> {
        loop {
            if self.try_reserve_pending_slot(capacity, pool) {
                return Ok(V2ResidencySlot {
                    residency: self,
                    active: true,
                    pool,
                });
            }
            if !self
                .try_unload_one_resident(manager, membership, protected_thread_id, pool)
                .await
            {
                return Err(CodexErr::new(CodexErrorDetails::AgentLimitReached {
                    max_threads: capacity,
                })
                .with_agent_context(AgentErrorContext::ResidencyCapacity));
            }
        }
    }

    fn try_reserve_pending_slot(&self, capacity: usize, pool: ResidencyPool) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (residents, pending_slots) = state.pool(pool);
        if residents.len().saturating_add(*pending_slots) >= capacity {
            return false;
        }
        *pending_slots += 1;
        true
    }

    async fn try_unload_one_resident(
        self: &Arc<Self>,
        manager: &Arc<ThreadManagerState>,
        membership: &AgentTreeMembership,
        protected_thread_id: Option<ThreadId>,
        pool: ResidencyPool,
    ) -> bool {
        // Keep shutting-down workers counted until removal. Each runtime's write guard
        // excludes delivery and competing evictions without blocking unrelated workers.
        let candidates = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pool(pool)
            .0
            .clone();
        for candidate_thread_id in candidates {
            if Some(candidate_thread_id) == protected_thread_id {
                continue;
            }
            let candidate_thread = {
                let threads = manager.threads.read().await;
                match threads.get(&candidate_thread_id) {
                    Some(thread) if is_resident_candidate(thread) => Arc::clone(thread),
                    Some(_) | None => {
                        // A reload cannot publish between the lookup and stale-entry removal.
                        self.remove(candidate_thread_id);
                        return true;
                    }
                }
            };
            // Capacity eviction keeps its existing policy of retaining unstarted workers.
            if matches!(
                candidate_thread.agent_status().await,
                AgentStatus::PendingInit
            ) {
                continue;
            }
            match manager
                .try_evict_v2_thread(candidate_thread, membership)
                .await
            {
                Ok(ThreadEvictionOutcome::Evicted | ThreadEvictionOutcome::NotCurrent) => {
                    return true;
                }
                Ok(ThreadEvictionOutcome::Busy) => {}
                Err(err) => warn!("v2 resident eviction failed: {err}"),
            }
        }
        false
    }

    fn touch(&self, thread_id: ThreadId) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Fork: a workflow child stays in its own pool.
        if state.workflow_residents.contains(&thread_id) {
            touch_resident(&mut state.workflow_residents, thread_id);
            return;
        }
        touch_resident(&mut state.residents, thread_id);
    }

    fn remove(&self, thread_id: ThreadId) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .residents
            .retain(|resident_thread_id| *resident_thread_id != thread_id);
        state
            .workflow_residents
            .retain(|resident_thread_id| *resident_thread_id != thread_id);
    }

    fn commit_slot(&self, thread_id: ThreadId, pool: ResidencyPool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (residents, pending_slots) = state.pool(pool);
        *pending_slots = pending_slots.saturating_sub(1);
        touch_resident(residents, thread_id);
    }

    fn release_pending_slot(&self, pool: ResidencyPool) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (_, pending_slots) = state.pool(pool);
        *pending_slots = pending_slots.saturating_sub(1);
    }
}

fn touch_resident(residents: &mut VecDeque<ThreadId>, thread_id: ThreadId) {
    residents.retain(|resident_thread_id| *resident_thread_id != thread_id);
    residents.push_back(thread_id);
}

fn is_resident_candidate(thread: &CodexThread) -> bool {
    thread.multi_agent_version() == Some(MultiAgentVersion::V2)
        && is_v2_resident_session_source(&thread.session_source)
}

pub(super) fn is_v2_resident_session_source(session_source: &SessionSource) -> bool {
    matches!(session_source, SessionSource::SubAgent(_))
}

async fn is_unloadable(thread: &CodexThread) -> bool {
    matches!(
        thread.agent_status().await,
        AgentStatus::PendingInit
            | AgentStatus::Completed(_)
            | AgentStatus::Errored(_)
            | AgentStatus::Interrupted
    ) && thread.session.active_turn.lock().await.is_none()
        && !thread.session.has_outstanding_durable_sleep()
        && !thread
            .session
            .input_queue
            .has_trigger_turn_mailbox_items()
            .await
        // Fork: unloading would kill an armed process or skip a turn a notification starts;
        // other queued notifications are recorded by the shutdown (R3.6, R2.5).
        && !thread.session.background_terminals_block_eviction().await
        // Fork: a workflow run still waits for this child's answer.
        && !thread
            .session
            .services
            .local_agent_runtime
            .workflow_children
            .has_pending_call(thread.session.thread_id)
}

#[cfg(test)]
#[path = "residency_tests.rs"]
mod tests;
