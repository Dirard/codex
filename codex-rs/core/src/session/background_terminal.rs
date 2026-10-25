//! Fork: delivery of background terminal exit notifications into the thread.
//!
//! Realizes UC3 of `specs/background-exec-notifications-2026-10-25.md`.

use std::sync::Arc;

use super::session::Session;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::unified_exec::PendingExitNotification;

/// Turn trigger recorded for a turn started by a background terminal exit.
pub(crate) const BACKGROUND_TERMINAL_TURN_TRIGGER: &str = "background_terminal";

impl Session {
    /// Deliver an exit notification as pending work of this thread.
    ///
    /// The notification waits in the thread-level input queue: a running turn that accepts
    /// delivery takes it into its next model request, and an idle thread starts a turn for a
    /// waking notification through the same scheduler as mail with `trigger_turn`. A session
    /// that is shutting down drops it.
    pub(crate) async fn deliver_background_terminal_notification(
        self: &Arc<Self>,
        notification: PendingExitNotification,
    ) {
        if self.state.lock().await.shutting_down {
            return;
        }
        let wakes = notification.wakes();
        self.input_queue
            .enqueue_background_notification(wakes, |with_output| {
                ContextualUserFragment::into(BackgroundTerminalNotification::new(
                    notification.render(with_output),
                ))
            })
            .await;
        if wakes {
            self.maybe_start_turn_for_pending_work().await;
        }
    }

    /// Whether this thread still expects or holds a background terminal notification.
    pub(crate) async fn has_pending_background_terminal_work(&self) -> bool {
        self.input_queue.has_background_notifications().await
            || self
                .services
                .unified_exec_manager
                .has_pending_exit_notifications()
                .await
    }
}
