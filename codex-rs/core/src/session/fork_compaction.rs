//! Fork: the session side of guaranteed compaction headroom and prepared compaction windows
//! (docs/fork/02-compaction-context.md F1): the headroom check of a prepared replacement, the
//! initial context of a prepared window, and the compare-and-swap of that window.

use codex_history::ResponseItemEnvelope;
use codex_protocol::config_types::AutoCompactTokenLimitScope;
use codex_protocol::models::BaseInstructions;
use codex_protocol::openai_models::ModelInfo;

use super::session::Session;
use super::step_context::StepContext;
use super::turn_context::TurnContext;
use crate::context::world_state::WorldState;
use crate::context::world_state::WorldStateSnapshot;
use crate::context::world_state::WorldStateUpdate;
use crate::state::AutoCompactWindowIds;
use crate::state::PreparedAutoCompactWindow;

impl Session {
    /// Fork: upstream's entry point, for the current compaction window; the body and its
    /// documentation are in `build_initial_context_with_world_state_inner`.
    pub(crate) async fn build_initial_context_with_world_state(
        &self,
        step_context: &StepContext,
        world_state: &WorldState,
    ) -> (Vec<WorldStateUpdate>, WorldStateSnapshot) {
        self.build_initial_context_with_world_state_inner(
            step_context,
            world_state,
            /*window_ids*/ None,
        )
        .await
    }

    /// Fork: the initial context for a prepared compaction window, which is not installed yet.
    pub(crate) async fn build_initial_context_with_world_state_for_window(
        &self,
        step_context: &StepContext,
        world_state: &WorldState,
        auto_compact_window_ids: AutoCompactWindowIds,
    ) -> (Vec<WorldStateUpdate>, WorldStateSnapshot) {
        self.build_initial_context_with_world_state_inner(
            step_context,
            world_state,
            Some(auto_compact_window_ids),
        )
        .await
    }

    pub(crate) async fn prepare_auto_compact_window(&self) -> PreparedAutoCompactWindow {
        let state = self.state.lock().await;
        state.prepare_auto_compact_window()
    }

    pub(crate) async fn commit_prepared_auto_compact_window(
        &self,
        prepared: PreparedAutoCompactWindow,
    ) -> bool {
        let mut state = self.state.lock().await;
        state.commit_prepared_auto_compact_window(prepared)
    }
}

/// Fork: whether the complete prepared replacement, including the live suffix, overflows the
/// compaction headroom. Checked under the state lock before the compaction window advances or
/// history changes, so it stays synchronous.
pub(super) fn prepared_candidate_overflows(
    items: &[ResponseItemEnvelope],
    base_instructions: &BaseInstructions,
    model_info: &ModelInfo,
    tool_declaration_tokens: i64,
    turn_context: &TurnContext,
) -> bool {
    let candidate_tokens = crate::compact_remote_v2::estimate_candidate_request_tokens(
        items,
        base_instructions,
        model_info,
        tool_declaration_tokens,
    );
    let scope = turn_context.config.model_auto_compact_token_limit_scope;
    let status = crate::session::context_window::context_window_token_status_for_usage(
        turn_context.config.as_ref(),
        model_info,
        candidate_tokens,
        matches!(scope, AutoCompactTokenLimitScope::BodyAfterPrefix).then_some(candidate_tokens),
    );
    crate::compact_remote_v2::enforces_compaction_headroom(&turn_context.session_source)
        && crate::compact_remote_v2::compaction_candidate_overflow(&status, scope) > 0
}
