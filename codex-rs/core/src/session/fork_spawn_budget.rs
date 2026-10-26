//! Fork: the cumulative spawn budget of one outer root input (`agents.max_spawned_threads_per_turn`,
//! docs/fork/01-multi-agent-runtime.md F1): the session's budget, how a turn starts or inherits
//! it, and the submission that hands it to the turn it starts.

use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::SessionSource;

use super::SessionIo;
use super::Submission;
use super::new_submission_id;
use super::session::Session;
use super::turn_context::TurnContext;
use crate::WithTurnExtensionData;
use crate::agent::control::TurnSpawnBudget;

impl SessionIo {
    /// Fork: submits `op` like `submit_with_trace`, and the turn it starts inherits the
    /// caller's cumulative spawn budget.
    pub(crate) async fn submit_with_spawn_budget(
        &self,
        op: impl Into<WithTurnExtensionData<Op>>,
        parent_turn_id: Option<String>,
        root_turn_id: Option<String>,
        residency_guard: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
        turn_spawn_budget: Option<TurnSpawnBudget>,
        message_delivery: Option<crate::agent_communication::MessageDelivery>,
    ) -> CodexResult<String> {
        let id = new_submission_id();
        let WithTurnExtensionData {
            request: op,
            turn_extension_init,
        } = op.into();
        self.submit_with_id(Submission {
            id: id.clone(),
            op,
            turn_extension_init,
            trace: None,
            parent_turn_id,
            root_turn_id,
            turn_spawn_budget,
            residency_guard,
            message_delivery,
        })
        .await?;
        Ok(id)
    }
}

impl Session {
    pub(crate) async fn start_turn_spawn_budget(&self, limit: usize) {
        self.state.lock().await.start_turn_spawn_budget(limit);
    }

    pub(crate) async fn set_turn_spawn_budget(&self, budget: TurnSpawnBudget) {
        self.state.lock().await.set_turn_spawn_budget(budget);
    }

    pub(crate) async fn current_turn_spawn_budget(&self, limit: usize) -> TurnSpawnBudget {
        self.state.lock().await.current_turn_spawn_budget(limit)
    }

    /// Fork: explicit input that starts a turn begins a fresh budget at the root of the agent
    /// tree; a sub-agent's turn keeps the budget its parent handed over, if any.
    pub(crate) async fn fork_begin_turn_spawn_budget(
        &self,
        turn_context: &TurnContext,
        inherited: Option<TurnSpawnBudget>,
    ) {
        if !matches!(&turn_context.session_source, SessionSource::SubAgent(_)) {
            self.start_turn_spawn_budget(turn_context.config.max_spawned_threads_per_turn)
                .await;
        } else if let Some(turn_spawn_budget) = inherited {
            self.set_turn_spawn_budget(turn_spawn_budget).await;
        }
    }

    /// Fork: a user turn started while idle begins a fresh budget at the root of the agent tree.
    pub(crate) async fn fork_begin_user_turn_spawn_budget(
        &self,
        turn_context: &TurnContext,
        user_turn: bool,
    ) {
        if user_turn && !matches!(&turn_context.session_source, SessionSource::SubAgent(_)) {
            self.start_turn_spawn_budget(turn_context.config.max_spawned_threads_per_turn)
                .await;
        }
    }
}
