//! Fork: turn state queries of the fork. A child of `state::turn`, so it reaches the private
//! fields.

use super::TurnState;

impl TurnState {
    /// Fork: whether the turn waits for an approval or a permission decision (dynamic
    /// workflow, R10.2).
    pub(crate) fn has_pending_approvals(&self) -> bool {
        !self.pending_approvals.is_empty() || !self.pending_request_permissions.is_empty()
    }
}
