//! Fork: session state of the cumulative spawn budget (docs/fork/01-multi-agent-runtime.md F1),
//! of prepared compaction windows (docs/fork/02-compaction-context.md F1) and of the estimated
//! auto-compact baseline (R7). A child of `state::session`, so it reaches the private fields.

use super::SessionState;
use crate::agent::control::TurnSpawnBudget;
use crate::state::auto_compact_window::PreparedAutoCompactWindow;

/// Fork: the fork's fields of `SessionState`, one aggregate field upstream.
#[derive(Default)]
pub(super) struct SessionStateFork {
    turn_spawn_budget: Option<TurnSpawnBudget>,
    estimated_prefill: Option<EstimatedPrefill>,
}

/// The auto-compact baseline that the fork estimated for a compaction window.
#[derive(Clone, Copy)]
struct EstimatedPrefill {
    window_number: u64,
    tokens: i64,
}

impl SessionState {
    pub(crate) fn start_turn_spawn_budget(&mut self, limit: usize) {
        self.fork.turn_spawn_budget = Some(TurnSpawnBudget::new(limit));
    }

    pub(crate) fn set_turn_spawn_budget(&mut self, budget: TurnSpawnBudget) {
        self.fork.turn_spawn_budget = Some(budget);
    }

    pub(crate) fn current_turn_spawn_budget(&mut self, limit: usize) -> TurnSpawnBudget {
        self.fork
            .turn_spawn_budget
            .get_or_insert_with(|| TurnSpawnBudget::new(limit))
            .clone()
    }

    /// Fork R7 (docs/fork/02-compaction-context.md): the `body_after_prefix` baseline of a
    /// window whose responses report no context size. Once per window it becomes the estimate
    /// of the request that a response answered (`request_tokens`), so that the tool
    /// declarations the estimate counts are not growth, while the response's own output is.
    /// Upstream estimates the baselines of recompute and resume without tool declarations, so
    /// a baseline that someone else set is rebased too. A server-observed baseline stays.
    ///
    /// Upstream records a response's zero usage as a server-observed baseline of `0`. No real
    /// request has zero input, so such a baseline is replaced, by this window's estimate when
    /// there is one.
    pub(crate) fn rebase_estimated_auto_compact_prefill(
        &mut self,
        request_tokens: impl FnOnce() -> i64,
    ) {
        let window_number = self.auto_compact_window.window_number();
        let prefill = self.auto_compact_window.snapshot().prefill_input_tokens;
        let rebased = self
            .fork
            .estimated_prefill
            .filter(|rebased| rebased.window_number == window_number)
            .map(|rebased| rebased.tokens);
        if rebased.is_some() && prefill == rebased {
            return;
        }
        if prefill == Some(0) {
            self.auto_compact_window.clear_prefill();
        }
        let tokens = match rebased {
            Some(tokens) if prefill == Some(0) => tokens,
            _ => request_tokens(),
        };
        self.auto_compact_window.set_estimated_prefill(tokens);
        self.fork.estimated_prefill = self
            .auto_compact_window
            .snapshot()
            .prefill_input_tokens
            .map(|tokens| EstimatedPrefill {
                window_number,
                tokens,
            });
    }

    pub(crate) fn prepare_auto_compact_window(&self) -> PreparedAutoCompactWindow {
        self.auto_compact_window.prepare_advance()
    }

    pub(crate) fn commit_prepared_auto_compact_window(
        &mut self,
        prepared: PreparedAutoCompactWindow,
    ) -> bool {
        self.auto_compact_window.commit_prepared(prepared)
    }
}
