//! Fork: blocked-batch rules of the turn loop and its compaction progress guard
//! (specs/blocked-batch-turn-input-2026-10-25.md, specs/blocked-batch-review-fixes-2026-10-25.md,
//! specs/stop-hooks-blocked-batch-2026-10-25.md, docs/fork/02-compaction-context.md).

use std::sync::Arc;

use codex_analytics::CompactionPhase;
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
use crate::context::world_state::WorldState;

/// Optional compactions in a row without progress after which no new background compaction
/// starts until the model makes progress (review R9-1).
const OPTIONAL_COMPACTIONS_WITHOUT_PROGRESS: usize = 2;

/// Compactions one turn installs, forced and optional together, after which the turn stops
/// with an error instead of compacting again (docs/fork/02-compaction-context.md §4 item 11).
/// The progress guards above stop loops without progress; this backstop stops a turn whose
/// every step refills the context. A typical turn compacts at most a few times; 32 windows of
/// a 272k context are about 6M tokens of new context in one turn.
pub(super) const MAX_COMPACTIONS_PER_TURN: usize = 32;

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
    /// Compactions this turn installed, forced and optional (`MAX_COMPACTIONS_PER_TURN`).
    compactions: usize,
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

    /// Before a mid-turn compaction: after two forced compactions in a row without progress,
    /// another one cannot make the context fit. The guard checks and counts only compactions
    /// the token limit forces (docs/fork/02-compaction-context.md R9): an optional one, such as
    /// a ready background candidate, runs while the context still fits, so it neither fails
    /// the turn nor counts.
    ///
    /// Whatever the progress, a turn that already installed `MAX_COMPACTIONS_PER_TURN`
    /// compactions stops with an error that says so, before it pays for another one.
    pub(super) fn check_compaction_progress(&self, token_limit_reached: bool) -> CodexResult<()> {
        if self.compactions >= MAX_COMPACTIONS_PER_TURN {
            return Err(CodexErr::InvalidRequest(compaction_limit_message()));
        }
        if token_limit_reached
            && !self.made_progress
            && self.consecutive_compactions_without_progress >= 2
        {
            return Err(CodexErr::ContextWindowExceeded);
        }
        Ok(())
    }

    pub(super) fn on_compacted(&mut self, token_limit_reached: bool) {
        self.compactions += 1;
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
    /// No optional compaction starts either once the turn reached `MAX_COMPACTIONS_PER_TURN`:
    /// it could not install.
    pub(super) fn allows_optional_compaction(&self) -> bool {
        self.consecutive_optional_compactions_without_progress
            < OPTIONAL_COMPACTIONS_WITHOUT_PROGRESS
            && self.compactions < MAX_COMPACTIONS_PER_TURN
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

/// The turn error when a turn reached `MAX_COMPACTIONS_PER_TURN`.
fn compaction_limit_message() -> String {
    format!(
        "Codex stopped this turn after compacting its context {MAX_COMPACTIONS_PER_TURN} times in it: every step refilled the context. Send a new message to continue in a new turn, or start a new thread."
    )
}

/// Fork: the mid-turn start of a background compaction (`async_compaction::maybe_start`, same
/// arguments after the turn loop), unless optional compactions ran out of progress
/// (`TurnLoop::allows_optional_compaction`). A candidate that is already running may still
/// install: it is paid for.
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
    if !turn_loop.allows_optional_compaction() {
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
