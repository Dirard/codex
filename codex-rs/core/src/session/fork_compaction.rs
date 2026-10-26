//! Fork: the session side of guaranteed compaction headroom and prepared compaction windows
//! (docs/fork/02-compaction-context.md F1): the headroom check of a prepared replacement, the
//! initial context of a prepared window, and the compare-and-swap of that window.

use codex_history::ResponseItemEnvelope;
use codex_protocol::config_types::AutoCompactTokenLimitScope;
use codex_protocol::models::BaseInstructions;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::protocol::TokenUsageInfo;

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

    /// Fork R7 (docs/fork/02-compaction-context.md): the sampling step's hook after a response
    /// records its usage. A response without usable usage still grows the context, but the
    /// token check would keep the last reported usage and add only the items after the latest
    /// model output, leaving auto-compaction blind to a context over the limit. So the context
    /// is estimated as the next request sends it. Like server usage, the recorded usage covers
    /// the history up to the latest model output; the token check adds the items after it,
    /// such as tool outputs recorded later, exactly once.
    ///
    /// Unlike `recompute_token_usage` this emits no `TokenCount`, which the sampling step sends
    /// after its tools resolve, and keeps a window prefill that is already set, because the
    /// prefill marks where the auto-compact scope of the window starts.
    pub(crate) async fn fork_estimate_usage(
        &self,
        step_context: &StepContext,
        token_usage: &Option<TokenUsage>,
    ) {
        if token_usage.is_some() {
            return;
        }
        let turn_context = step_context.turn.as_ref();
        let base_instructions = self.get_base_instructions().await;
        let tool_declaration_tokens =
            crate::compact_remote_v2::tool_declaration_tokens(step_context);
        let mut state = self.state.lock().await;
        let estimated_tokens = crate::compact_remote_v2::estimate_candidate_request_tokens(
            state.history.annotated_items(),
            &base_instructions,
            &step_context.settings.model_info,
            tool_declaration_tokens,
        )
        .max(0);
        let tail_tokens = state
            .history
            .estimated_tokens_after_last_model_generated_item();
        let mut info = state.token_info().unwrap_or(TokenUsageInfo {
            total_token_usage: TokenUsage::default(),
            last_token_usage: TokenUsage::default(),
            model_context_window: None,
        });
        info.last_token_usage = TokenUsage {
            total_tokens: estimated_tokens.saturating_sub(tail_tokens).max(0),
            ..TokenUsage::default()
        };
        if let Some(model_context_window) = turn_context.model_context_window() {
            info.model_context_window = Some(model_context_window);
        }
        state.set_token_info(Some(info));
        // The local estimate already includes every retained reasoning item.
        state.set_server_reasoning_included(/*included*/ true);
        if matches!(
            turn_context.config.model_auto_compact_token_limit_scope,
            AutoCompactTokenLimitScope::BodyAfterPrefix
        ) && state
            .auto_compact_window_snapshot()
            .prefill_input_tokens
            .is_none()
        {
            state.set_auto_compact_window_estimated_prefill(estimated_tokens);
        }
    }
}

#[cfg(test)]
#[path = "fork_compaction_tests.rs"]
mod tests;

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
