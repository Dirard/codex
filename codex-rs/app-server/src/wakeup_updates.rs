//! Fork: `thread/wakeup/updated` for clients subscribed to a thread
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC8 R8.4).
//!
//! Each thread listener subscribes a sink to the thread's wakeup state. The extension calls
//! it synchronously with every change, so the listener command is queued before a tool call
//! that scheduled a wakeup completes (R2.7) and the listener, which handles commands before
//! events, sends the update ahead of the call's `item/completed`. A sink stops once its
//! listener's channel closes.

use std::sync::Arc;

use codex_core::CodexThread;
use tokio::sync::mpsc;

use crate::thread_state::ThreadListenerCommand;

/// Subscribes the listener of `thread` to its wakeup changes; does nothing without the
/// extension.
pub(crate) fn forward_wakeup_updates(
    thread: &Arc<CodexThread>,
    commands: mpsc::UnboundedSender<ThreadListenerCommand>,
) {
    let Some(state) = codex_wakeup_extension::thread_state(thread) else {
        return;
    };
    let thread_id = state.thread_id().to_string();
    state.subscribe(Box::new(move |update| {
        let notification =
            crate::request_processors::wakeup_updated_notification(thread_id.clone(), update);
        commands
            .send(ThreadListenerCommand::EmitThreadWakeupUpdated {
                notification: Box::new(notification),
            })
            .is_ok()
    }));
}

/// Sends `thread/wakeup/updated` to the connections subscribed to the thread. The thread
/// listener calls it for `EmitThreadWakeupUpdated`.
pub(crate) async fn emit_thread_wakeup_updated(
    outgoing: &Arc<crate::outgoing_message::OutgoingMessageSender>,
    thread_state_manager: &crate::thread_state::ThreadStateManager,
    conversation_id: codex_protocol::ThreadId,
    notification: codex_app_server_protocol::ThreadWakeupUpdatedNotification,
) {
    let subscribed_connection_ids = thread_state_manager
        .subscribed_connection_ids(conversation_id)
        .await;
    let outgoing = crate::outgoing_message::ThreadScopedOutgoingMessageSender::new(
        Arc::clone(outgoing),
        subscribed_connection_ids,
        conversation_id,
    );
    outgoing
        .send_server_notification(
            codex_app_server_protocol::ServerNotification::ThreadWakeupUpdated(notification),
        )
        .await;
}
