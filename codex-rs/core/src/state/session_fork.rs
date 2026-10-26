//! Fork: session state of the cumulative spawn budget (docs/fork/01-multi-agent-runtime.md F1)
//! and of prepared compaction windows (docs/fork/02-compaction-context.md F1). A child of
//! `state::session`, so it reaches the private fields.

use super::SessionState;
use crate::agent::control::TurnSpawnBudget;
use crate::state::auto_compact_window::PreparedAutoCompactWindow;

impl SessionState {
    pub(crate) fn start_turn_spawn_budget(&mut self, limit: usize) {
        self.turn_spawn_budget = Some(TurnSpawnBudget::new(limit));
    }

    pub(crate) fn set_turn_spawn_budget(&mut self, budget: TurnSpawnBudget) {
        self.turn_spawn_budget = Some(budget);
    }

    pub(crate) fn current_turn_spawn_budget(&mut self, limit: usize) -> TurnSpawnBudget {
        self.turn_spawn_budget
            .get_or_insert_with(|| TurnSpawnBudget::new(limit))
            .clone()
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
