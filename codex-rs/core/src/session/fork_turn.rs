//! Fork: blocked-batch rules of the turn loop and its compaction progress guard
//! (specs/blocked-batch-turn-input-2026-10-25.md, specs/blocked-batch-review-fixes-2026-10-25.md,
//! specs/stop-hooks-blocked-batch-2026-10-25.md, docs/fork/02-compaction-context.md).

use std::sync::Arc;

use codex_analytics::CompactionPhase;
use codex_protocol::config_types::AutoCompactTokenLimitScope;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_thread_store::PersistContext;
use tokio_util::sync::CancellationToken;

use super::TurnInput;
use super::context_window::ContextWindowTokenStatus;
use super::session::Session;
use super::step_context::StepContext;
use super::turn::run_hooks_and_record_inputs;
use super::turn_context::TurnContext;
use crate::config::Config;
use crate::context::world_state::WorldState;

/// Optional compactions in a row without progress after which no new background compaction
/// starts until the model makes progress (review R9-1).
const OPTIONAL_COMPACTIONS_WITHOUT_PROGRESS: usize = 2;

/// Context tokens the compactions of one turn may replace by default
/// (`model_auto_compact_turn_token_limit`; docs/fork/02-compaction-context.md §4 item 11,
/// review 2026-10-27 P2-1). The progress guards above stop loops without progress; this
/// backstop stops a turn whose every step refills the context. It counts tokens, not
/// compactions: a small window, a low `model_auto_compact_token_limit`, early optional
/// compactions (`AsyncCompaction`) or `TokenBudget` window resets compact a productive turn
/// many times before it replaced this much context.
pub(super) const DEFAULT_TURN_COMPACTION_TOKEN_LIMIT: u64 = 8_000_000;
/// Context windows the compactions of one turn may replace by default when they hold more
/// than `DEFAULT_TURN_COMPACTION_TOKEN_LIMIT`: a turn always gets this many full windows.
pub(super) const DEFAULT_TURN_COMPACTION_WINDOWS: u64 = 32;

/// How much context the compactions of one turn may replace before the turn stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TurnCompactionLimit {
    /// `None` when the limit is off.
    tokens: Option<u64>,
}

impl TurnCompactionLimit {
    pub(super) const OFF: Self = Self { tokens: None };

    pub(super) fn tokens(tokens: u64) -> Self {
        Self {
            tokens: Some(tokens),
        }
    }

    /// `configured` is `model_auto_compact_turn_token_limit`, where 0 turns the limit off;
    /// without it, the larger of `DEFAULT_TURN_COMPACTION_TOKEN_LIMIT` and
    /// `DEFAULT_TURN_COMPACTION_WINDOWS` windows of `context_window`.
    pub(super) fn resolve(configured: Option<u64>, context_window: Option<i64>) -> Self {
        match configured {
            Some(0) => Self::OFF,
            Some(tokens) => Self::tokens(tokens),
            None => Self::tokens(
                context_window
                    .and_then(|window| u64::try_from(window).ok())
                    .unwrap_or(0)
                    .saturating_mul(DEFAULT_TURN_COMPACTION_WINDOWS)
                    .max(DEFAULT_TURN_COMPACTION_TOKEN_LIMIT),
            ),
        }
    }

    /// The limit of a turn of `config` whose context window `token_status` reports.
    fn of_turn(config: &Config, token_status: &ContextWindowTokenStatus) -> Self {
        Self::resolve(
            config.model_auto_compact_turn_token_limit,
            token_status.full_context_window_limit,
        )
    }

    fn is_reached(self, compacted_tokens: u64) -> bool {
        self.tokens.is_some_and(|limit| compacted_tokens >= limit)
    }
}

/// Fork: what one sampled response did for the turn loop.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SamplingOutcome {
    /// The response produced an answer or a successful tool result.
    pub(super) made_progress: bool,
    /// The response ended with its terminal answer and owes no tool output, continuation or
    /// mail delivery. A user-input preemption after that answer leaves the model nothing to
    /// redo.
    pub(super) answered: bool,
}

/// Fork: while a response streams, whether its latest output item is the terminal answer and
/// whether the model still owes a step of its own (tool output, continuation, mail delivery).
#[derive(Default)]
pub(super) struct SamplingProgress {
    pub(super) final_answer: bool,
    pub(super) model_owes_step: bool,
}

impl SamplingProgress {
    pub(super) fn on_output_item(&mut self, terminal_answer: bool, needs_follow_up: bool) {
        self.final_answer = terminal_answer;
        self.model_owes_step |= needs_follow_up;
    }

    /// The outcome of a response that ended here; the turn loop adds `made_progress`.
    pub(super) fn outcome(&self) -> SamplingOutcome {
        SamplingOutcome {
            made_progress: false,
            answered: self.final_answer && !self.model_owes_step,
        }
    }
}

/// Fork: blocked-batch state of one `run_turn` loop and its compaction progress guard.
#[derive(Default)]
pub(super) struct TurnLoop {
    /// The model still owes this turn a step: a preempted response, tool output, or a Stop
    /// hook continuation. Blocked steered input must not end the turn before that.
    pub(super) model_follow_up_pending: bool,
    /// Steered input that the hooks already accepted and recorded behind a finished answer.
    /// The next step samples it; the drain and hooks at the loop top skip it.
    pub(super) accepted_pending_input: Option<Vec<TurnInput>>,
    answered: bool,
    made_progress: bool,
    consecutive_compactions_without_progress: usize,
    consecutive_optional_compactions_without_progress: usize,
    /// Compactions this turn installed, forced and optional.
    compactions: usize,
    /// Context tokens the compactions this turn installed replaced (`TurnCompactionLimit`).
    compacted_tokens: u64,
    /// Context of the compaction `check_compaction_progress` let through; it counts once the
    /// compaction installs.
    compacting_tokens: u64,
}

impl TurnLoop {
    /// After a sampled response: a user-input preemption after the terminal answer leaves the
    /// model nothing to redo, so only the queued input decides whether the turn goes on.
    /// Returns whether the model needs a follow-up.
    pub(super) fn on_sampled(
        &mut self,
        model_needs_follow_up: bool,
        outcome: SamplingOutcome,
    ) -> bool {
        let model_needs_follow_up = model_needs_follow_up && !outcome.answered;
        self.model_follow_up_pending = model_needs_follow_up;
        self.answered = outcome.answered;
        self.made_progress = outcome.made_progress;
        if outcome.made_progress {
            self.consecutive_compactions_without_progress = 0;
            self.consecutive_optional_compactions_without_progress = 0;
        }
        model_needs_follow_up
    }

    /// Fork hook of `run_turn` before a mid-turn compaction: whether it compacts now
    /// (`check_compaction_progress` with the turn's `TurnCompactionLimit`).
    pub(super) async fn before_compaction(
        &mut self,
        sess: &Session,
        turn_context: &TurnContext,
        token_status: &ContextWindowTokenStatus,
    ) -> CodexResult<bool> {
        let scope = turn_context.config.model_auto_compact_token_limit_scope;
        let history_estimate = if body_is_no_measure(scope, token_status) {
            Some(replaced_history_estimate(
                sess.clone_history().await.raw_items(),
            ))
        } else {
            None
        };
        self.check_compaction_progress(
            token_status.token_limit_reached,
            replaced_context_tokens(scope, token_status, history_estimate),
            TurnCompactionLimit::of_turn(&turn_context.config, token_status),
        )
    }

    /// Before a mid-turn compaction of `context_tokens`: after two forced compactions in a row
    /// without progress, another one cannot make the context fit. The guard checks and counts
    /// only compactions the token limit forces (docs/fork/02-compaction-context.md R9): an
    /// optional one, such as a ready background candidate or a `new_context` reset, runs
    /// while the context still fits, so it neither fails the turn nor counts.
    ///
    /// Whatever the progress, once the compactions of this turn replaced `limit` tokens of
    /// context, a forced compaction stops the turn with an error that says so, before it pays
    /// for another one. An optional one is skipped instead: the context still fits, so the
    /// turn goes on without it (review 2026-10-27, P3-1). Returns whether to compact.
    pub(super) fn check_compaction_progress(
        &mut self,
        token_limit_reached: bool,
        context_tokens: i64,
        limit: TurnCompactionLimit,
    ) -> CodexResult<bool> {
        if limit.is_reached(self.compacted_tokens) {
            if !token_limit_reached {
                return Ok(false);
            }
            return Err(CodexErr::InvalidRequest(compaction_limit_message(
                self.compactions,
                self.compacted_tokens,
            )));
        }
        if token_limit_reached
            && !self.made_progress
            && self.consecutive_compactions_without_progress >= 2
        {
            return Err(CodexErr::ContextWindowExceeded);
        }
        self.compacting_tokens = u64::try_from(context_tokens).unwrap_or(0);
        Ok(true)
    }

    pub(super) fn on_compacted(&mut self, token_limit_reached: bool) {
        self.compactions += 1;
        self.compacted_tokens = self
            .compacted_tokens
            .saturating_add(std::mem::take(&mut self.compacting_tokens));
        if self.made_progress {
            return;
        }
        if token_limit_reached {
            self.consecutive_compactions_without_progress += 1;
        } else {
            self.consecutive_optional_compactions_without_progress += 1;
        }
    }

    /// Whether an optional compaction may still start: a step without progress after an
    /// installed background candidate starts the next one while the context stays above the
    /// async threshold, so without progress they would repeat for the rest of the turn, one
    /// compaction request every few samples (review R9-1). They stop after two in a row until
    /// the model makes progress. Forced compactions do not go through this.
    /// No optional compaction starts either once the turn reached its `TurnCompactionLimit`:
    /// it could not install.
    pub(super) fn allows_optional_compaction(&self, limit: TurnCompactionLimit) -> bool {
        self.consecutive_optional_compactions_without_progress
            < OPTIONAL_COMPACTIONS_WITHOUT_PROGRESS
            && !limit.is_reached(self.compacted_tokens)
    }

    /// Only the hooks can tell whether input queued behind a finished answer continues the
    /// turn, so they run now. If they block all of it, nothing is left to sample and the
    /// answer completes the turn like any other. Returns whether the turn needs a follow-up.
    pub(super) async fn settle_queued_input(
        &mut self,
        sess: &Arc<Session>,
        turn_context: &Arc<TurnContext>,
        has_pending_input: bool,
        needs_follow_up: bool,
    ) -> bool {
        if self.model_follow_up_pending || !has_pending_input {
            return needs_follow_up;
        }
        let pending_input = sess
            .input_queue
            .get_pending_input(&sess.active_turn)
            .await
            .0;
        // An empty drain, such as a stale mailbox signal, leaves nothing to sample.
        let blocked = pending_input.is_empty()
            || run_hooks_and_record_inputs(
                sess,
                turn_context,
                &turn_context.capture_current_model_info(),
                &pending_input,
                PersistContext::SteeredUserInput,
            )
            .await;
        if !blocked {
            self.accepted_pending_input = Some(pending_input);
        } else if self.answered {
            // The queued input kept mailbox delivery open past the terminal answer. Close it
            // as that answer would have, so queue-only mail that arrives while the turn
            // completes waits for the next turn.
            sess.input_queue
                .defer_mailbox_delivery_to_next_turn(&sess.active_turn, &turn_context.sub_id)
                .await;
        }
        !blocked
    }
}

/// The context a compaction replaces, as the turn's `TurnCompactionLimit` counts it. With
/// `model_auto_compact_token_limit_scope = "body_after_prefix"` the body after the window's
/// prefix counts (review of the fixes, P3-6); otherwise the whole active context.
///
/// The body is no measure when the provider reported no usage for the window (no prefill, so
/// the body reads 0), nor when the full window, not the body, forced the compaction: a large
/// prefix filled it. Then the compaction counts at least `history_estimate`, the estimated
/// history it replaces (`replaced_history_estimate`), so the limit does not weaken
/// (review of the fixes 2, N-3).
pub(super) fn replaced_context_tokens(
    scope: AutoCompactTokenLimitScope,
    token_status: &ContextWindowTokenStatus,
    history_estimate: Option<i64>,
) -> i64 {
    match scope {
        AutoCompactTokenLimitScope::Total => token_status.active_context_tokens,
        AutoCompactTokenLimitScope::BodyAfterPrefix => {
            let body = token_status.auto_compact_scope_tokens;
            if body_is_no_measure(scope, token_status) {
                body.max(history_estimate.unwrap_or(token_status.active_context_tokens))
            } else {
                body
            }
        }
    }
}

/// Whether the body of a `body_after_prefix` window understates what a compaction replaces
/// (review of the fixes 2, N-3): no usage set its prefix, or the full window forced it.
pub(super) fn body_is_no_measure(
    scope: AutoCompactTokenLimitScope,
    token_status: &ContextWindowTokenStatus,
) -> bool {
    matches!(scope, AutoCompactTokenLimitScope::BodyAfterPrefix)
        && (token_status.auto_compact_window_prefill_tokens.is_none()
            || token_status.full_context_window_limit_reached)
}

/// The estimated history a compaction replaces: every history item but the base instructions
/// and tool declarations, which the next window keeps. Same estimate as the rest of the token
/// accounting (`estimate_item_token_count`).
pub(super) fn replaced_history_estimate<'a>(
    items: impl Iterator<Item = &'a codex_protocol::models::ResponseItem>,
) -> i64 {
    items
        .filter(|item| {
            !matches!(
                item,
                codex_protocol::models::ResponseItem::AdditionalTools { .. }
            ) && crate::context::BaseInstructionsFragment::text_from_item(item).is_none()
        })
        .map(crate::context_manager::estimate_item_token_count)
        .fold(0, i64::saturating_add)
}

/// The turn error when a turn reached its `TurnCompactionLimit`.
fn compaction_limit_message(compactions: usize, compacted_tokens: u64) -> String {
    format!(
        "Codex stopped this turn: its {compactions} compactions replaced {compacted_tokens} tokens of context, the most one turn may compact, and the context is full again. Send a new message to continue in a new turn, or raise model_auto_compact_turn_token_limit in config.toml (0 turns the limit off)."
    )
}

/// Fork: the mid-turn start of a background compaction (`async_compaction::maybe_start`, same
/// arguments after the turn loop), unless optional compactions ran out of progress or the turn
/// reached its compaction limit (`TurnLoop::allows_optional_compaction`). A candidate that is
/// already running may still install: it is paid for.
#[allow(clippy::too_many_arguments)]
pub(super) async fn maybe_start_async_compaction(
    turn_loop: &TurnLoop,
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    world_state: &Arc<WorldState>,
    phase: CompactionPhase,
    turn_cancellation: &CancellationToken,
    token_status: &ContextWindowTokenStatus,
    turn_state: (Option<String>, Option<u64>),
) {
    if !turn_loop.allows_optional_compaction(TurnCompactionLimit::of_turn(
        &step_context.turn.config,
        token_status,
    )) {
        return;
    }
    super::async_compaction::maybe_start(
        sess,
        step_context,
        world_state,
        phase,
        turn_cancellation,
        token_status,
        turn_state,
    )
    .await;
}

#[cfg(test)]
#[path = "fork_turn_tests.rs"]
mod tests;
