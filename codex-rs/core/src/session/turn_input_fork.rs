//! Fork: automatic turns for waking background notifications. A child of `turn_input`, so the
//! wake follows the same `TurnStartKind` rules as mail
//! (specs/background-exec-notifications-2026-10-25.md UC3, specs/dynamic-workflow-2026-10-25.md UC7).

use super::TurnStartKind;
use crate::session::session::Session;

impl Session {
    /// Fork: a waking background terminal notification starts an automatic turn unless the
    /// user interrupted the last turn or Plan mode forbids automatic work (UC3, R3.1, R3.3).
    pub(crate) async fn should_wake_on_background_notifications(&self) -> bool {
        if self.is_interrupted() || !self.input_queue.has_waking_background_notifications().await {
            return false;
        }
        let state = self.state.lock().await;
        TurnStartKind::Automatic.permits_mode(
            state
                .session_configuration
                .step_settings
                .collaboration_mode
                .mode,
        )
    }
}
