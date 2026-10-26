//! Fork: guaranteed headroom after remote compaction v2 (docs/fork/02-compaction-context.md
//! F1). A candidate must leave `min(64k, limit / 4)` below every limit it counts against,
//! with a soft quarter of that for summary-only installs, and an automatic compaction whose
//! instructions and tool declarations alone fill the window is never sent.
//!
//! Child of `compact_remote_v2`: `pub(crate) use headroom::*` there keeps the paths
//! `crate::compact_remote_v2::*` of the session and the installation module.

use std::future::Future;

use super::RETAINED_MESSAGE_TOKEN_BUDGET;
use crate::context_manager::ContextManager;
use crate::hook_runtime::PreCompactHookOutcome;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::session::turn_context::TurnContext;
use codex_analytics::CompactionTrigger;
use codex_history::ResponseItemEnvelope;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_utils_output_truncation::approx_token_count;

// The installation module reaches these through `use super::*`.
pub(super) use crate::session::context_window::context_window_token_status_for_usage;
pub(super) use codex_protocol::config_types::AutoCompactTokenLimitScope;

fn required_headroom(limit: i64) -> i64 {
    RETAINED_MESSAGE_TOKEN_BUDGET
        .min(usize::try_from(limit.saturating_div(4).max(0)).unwrap_or(usize::MAX))
        .try_into()
        .unwrap_or(i64::MAX)
}

/// The part of the headroom target that a soft install must still leave below each limit,
/// so the next step does not compact again at once.
fn soft_headroom(limit: i64) -> i64 {
    required_headroom(limit) / 4
}

fn overflow(used: i64, limit: Option<i64>, headroom: fn(i64) -> i64) -> i64 {
    limit.map_or(0, |limit| {
        used.saturating_sub(limit.saturating_sub(headroom(limit)))
            .max(0)
    })
}

/// Guardian reviewers budget each complete request themselves and compact at most once
/// per step, so the fork headroom target must not reject their summaries. They keep the
/// upstream token-limit check, and Guardian fails closed with its own input-budget reason.
pub(crate) fn enforces_compaction_headroom(session_source: &SessionSource) -> bool {
    !crate::guardian::is_basic_session_source(session_source)
}

/// Approximate tokens of the tool declarations the next request sends outside history.
/// Guardian reviewers keep the upstream estimate without them.
pub(crate) fn tool_declaration_tokens(step_context: &StepContext) -> i64 {
    if !enforces_compaction_headroom(&step_context.turn.session_source) {
        return 0;
    }
    let tokens = step_context
        .tool_router
        .model_visible_specs()
        .iter()
        .map(|spec| serde_json::to_string(spec).map_or(0, |json| approx_token_count(&json)))
        .fold(0usize, usize::saturating_add);
    i64::try_from(tokens).unwrap_or(i64::MAX)
}

/// Estimates a replacement history as the next request sends it: history items, base
/// instructions, and tool declarations unless a Responses Lite window already declares them
/// in history.
pub(crate) fn estimate_candidate_request_tokens(
    items: &[ResponseItemEnvelope],
    base_instructions: &BaseInstructions,
    model_info: &ModelInfo,
    tool_declaration_tokens: i64,
) -> i64 {
    let history_tokens = ContextManager::estimate_history_token_count(items, base_instructions);
    let tools_in_history = model_info.use_responses_lite
        && items
            .iter()
            .any(|envelope| matches!(envelope.item, ResponseItem::AdditionalTools { .. }));
    if tools_in_history {
        history_tokens
    } else {
        history_tokens.saturating_add(tool_declaration_tokens)
    }
}

/// Soft headroom: a summary-only candidate that misses the headroom target is still better
/// than failing the turn when it shrinks the context and leaves a quarter of the target below
/// every limit its own size counts against: the unbuffered auto-compaction limit for `Total`,
/// and the full context window for both scopes (`BodyAfterPrefix` restarts its body count
/// from the candidate). Closer to a limit the next step would compact again at once, so such
/// a candidate is rejected as before.
pub(crate) fn soft_headroom_accepts(
    status: &crate::session::context_window::ContextWindowTokenStatus,
    scope: AutoCompactTokenLimitScope,
    current_context_tokens: i64,
) -> bool {
    !status.token_limit_reached
        && candidate_overflow(status, scope, soft_headroom) == 0
        && status.active_context_tokens < current_context_tokens
}

/// The request part every compaction candidate carries, when it alone fills the window.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MandatoryRequestOverflow {
    /// Base instructions and tool declarations.
    pub(crate) mandatory_tokens: i64,
    pub(crate) context_window_tokens: Option<i64>,
}

impl MandatoryRequestOverflow {
    /// Tells the user why automatic compaction cannot help; starting a new thread would not
    /// either, because the same instructions and tools open every window.
    pub(crate) fn warning_message(&self) -> String {
        let window = self
            .context_window_tokens
            .map(|tokens| format!(" of the {tokens}-token context window"))
            .unwrap_or_default();
        format!(
            "Automatic context compaction was skipped: the instructions and tool declarations \
             alone use about {} tokens{window}, so no summary can fit. Disable some MCP servers \
             or tools, or switch to a model with a larger context window.",
            self.mandatory_tokens
        )
    }
}

/// Fail-fast lower bound before paying for a foreground compaction: every candidate carries
/// the base instructions and the tool declarations, so when they alone reach the full context
/// window no summary can be installed. Returns that lower bound when it does.
/// `base_instructions` must be the copy the request sends, so the bound never overestimates.
pub(crate) fn mandatory_request_overflows_window(
    step_context: &StepContext,
    base_instructions: &BaseInstructions,
) -> Option<MandatoryRequestOverflow> {
    if !enforces_compaction_headroom(&step_context.turn.session_source) {
        return None;
    }
    let mandatory_tokens = i64::try_from(approx_token_count(&base_instructions.text))
        .unwrap_or(i64::MAX)
        .saturating_add(tool_declaration_tokens(step_context));
    let turn = &step_context.turn;
    let status = context_window_token_status_for_usage(
        turn.config.as_ref(),
        turn.model_info().as_ref(),
        mandatory_tokens,
        /*auto_compact_window_prefill_tokens*/ None,
    );
    status
        .full_context_window_limit_reached
        .then_some(MandatoryRequestOverflow {
            mandatory_tokens,
            context_window_tokens: status.full_context_window_limit,
        })
}

pub(crate) fn compaction_candidate_overflow(
    status: &crate::session::context_window::ContextWindowTokenStatus,
    scope: AutoCompactTokenLimitScope,
) -> i64 {
    candidate_overflow(status, scope, required_headroom)
}

fn candidate_overflow(
    status: &crate::session::context_window::ContextWindowTokenStatus,
    scope: AutoCompactTokenLimitScope,
    headroom: fn(i64) -> i64,
) -> i64 {
    let full_window_overflow = overflow(
        status.active_context_tokens,
        status.full_context_window_limit,
        headroom,
    );
    match scope {
        AutoCompactTokenLimitScope::Total => overflow(
            status.auto_compact_scope_tokens,
            status.auto_compact_scope_limit,
            headroom,
        )
        .max(full_window_overflow),
        AutoCompactTokenLimitScope::BodyAfterPrefix => full_window_overflow,
    }
}

/// The fail-fast check of an automatic compaction without a prepared candidate. A hopeless
/// compaction skips its hooks, its started item and its request, but its error still takes
/// the caller's result path, so the attempt is tracked and reported as any other failure.
pub(super) async fn fork_hopeless_auto_compaction(
    sess: &Session,
    turn_context: &TurnContext,
    replacement_step_context: &StepContext,
    trigger: CompactionTrigger,
    candidate_missing: bool,
) -> Option<CodexErr> {
    if !candidate_missing {
        return None;
    }
    super::installation::reject_hopeless_auto_compaction(
        sess,
        turn_context,
        replacement_step_context,
        trigger,
    )
    .await
    .err()
}

/// Awaits the pre-compact hooks, or drops them unpolled for a hopeless compaction.
pub(super) async fn fork_hooks(
    hopeless: &Option<CodexErr>,
    hooks: impl Future<Output = PreCompactHookOutcome>,
) -> PreCompactHookOutcome {
    if hopeless.is_some() {
        PreCompactHookOutcome::Continue
    } else {
        hooks.await
    }
}

/// Awaits the compaction request, or drops it unpolled with the hopeless error.
pub(super) async fn fork_unless_hopeless(
    hopeless: Option<CodexErr>,
    compaction: impl Future<Output = CodexResult<bool>>,
) -> CodexResult<bool> {
    match hopeless {
        Some(error) => Err(error),
        None => compaction.await,
    }
}
