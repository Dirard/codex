//! Fork: delivery of background terminal exit notifications into the thread.
//!
//! Realizes UC3 of `specs/background-exec-notifications-2026-10-25.md`.

use std::sync::Arc;
use std::time::Duration;

use codex_protocol::models::ResponseItem;

use super::session::Session;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::session::input_queue::QueuedBackgroundNotification;
use crate::session::input_queue::SampledCallback;
use crate::unified_exec::ExitNotifier;
use crate::unified_exec::PendingExitNotification;
use crate::unified_exec::TerminatedProcesses;

/// Turn trigger recorded for a turn started by a background terminal exit.
pub(crate) const BACKGROUND_TERMINAL_TURN_TRIGGER: &str = "background_terminal";

/// How long a shutdown waits for notifications that were already on their way.
const SHUTDOWN_DELIVERY_WAIT: Duration = Duration::from_secs(1);

impl Session {
    /// Deliver an exit notification as pending work of this thread.
    ///
    /// The notification waits in the thread-level input queue: a running turn that accepts
    /// delivery takes it into its next model request, and an idle thread starts a turn for a
    /// waking notification through the same scheduler as mail with `trigger_turn`. The
    /// notification is queued before this returns, so a `write_stdin` that runs afterwards
    /// can withdraw it (R2.10); only the wake runs as a separate task. A session that is
    /// shutting down queues it without a wake, and the shutdown records it (R2.5).
    pub(crate) async fn deliver_background_terminal_notification(
        self: &Arc<Self>,
        exit_notifier: &Arc<ExitNotifier>,
        notification: PendingExitNotification,
    ) {
        let wakes = notification.wakes();
        self.input_queue
            .enqueue_background_notification(
                Some(notification.key),
                Some(notification.process_id),
                wakes,
                |with_output| {
                    let rendered = notification.render(with_output);
                    QueuedBackgroundNotification {
                        item: ContextualUserFragment::into(BackgroundTerminalNotification::new(
                            rendered.text,
                        )),
                        // Once the model has everything a poll could return, the entry and
                        // its output buffers go (R2.12). Until then a poll still works.
                        on_sampled: (!rendered.leaves_readable_output)
                            .then(|| self.release_when_sampled(exit_notifier)),
                    }
                },
            )
            .await;
        exit_notifier.finish_delivery();
        if wakes && !self.state.lock().await.shutting_down {
            let session = Arc::clone(self);
            tokio::spawn(async move {
                session.maybe_start_turn_for_pending_work().await;
            });
        }
    }

    fn release_when_sampled(
        self: &Arc<Self>,
        exit_notifier: &Arc<ExitNotifier>,
    ) -> SampledCallback {
        let session = Arc::downgrade(self);
        let exit_notifier = Arc::clone(exit_notifier);
        Box::new(move || {
            let Some(session) = session.upgrade() else {
                return;
            };
            tokio::spawn(async move {
                session
                    .services
                    .unified_exec_manager
                    .release_after_notification(&exit_notifier)
                    .await;
            });
        })
    }

    /// Fork: deliver the completion notification of a workflow run through the same queue
    /// (`specs/dynamic-workflow-2026-10-25.md`, UC7, R7.2): a running turn samples it in its
    /// next model request, an idle thread starts a turn, and a turn that already decided to
    /// finish leaves it queued for the turn started after it. A shutting-down session drops it.
    pub(crate) async fn deliver_workflow_notification(self: &Arc<Self>, body: String) {
        if self.state.lock().await.shutting_down {
            return;
        }
        self.input_queue
            .enqueue_workflow_notification(ContextualUserFragment::into(
                crate::context::WorkflowNotification::new(body),
            ))
            .await;
        self.maybe_start_turn_for_pending_work().await;
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

    /// Whether unloading this V2 agent now would break a background terminal promise (R3.6):
    /// a live armed process would be killed, or a queued notification would start a turn.
    ///
    /// The process store is checked before the queue: a notifier stays pending until its
    /// notification is queued, so no notification can slip between the two checks (P3-3).
    /// Other queued notifications survive eviction through the shutdown record (R2.5).
    pub(crate) async fn background_terminals_block_eviction(&self) -> bool {
        self.services
            .unified_exec_manager
            .has_pending_exit_notifications()
            .await
            || self.should_wake_on_background_notifications().await
    }

    /// Record what a shutdown owes the model before persistence closes (R2.5): every queued
    /// notification and a notice for each armed process that the shutdown stopped.
    ///
    /// Without exit notifications both are empty and nothing is recorded.
    pub(crate) async fn record_background_terminals_at_shutdown(
        &self,
        terminated: TerminatedProcesses,
    ) {
        let deadline = tokio::time::Instant::now() + SHUTDOWN_DELIVERY_WAIT;
        for exit_notifier in &terminated.deliveries_in_flight {
            exit_notifier.wait_for_delivery(deadline).await;
        }
        let mut items: Vec<ResponseItem> = self.input_queue.take_background_notifications().await;
        items.extend(terminated.shutdown_notices.iter().map(|notice| {
            ContextualUserFragment::into(BackgroundTerminalNotification::new(notice.render()))
        }));
        if items.is_empty() {
            return;
        }
        let turn_context = self.new_inject_items_context().await;
        self.record_conversation_items(&turn_context, turn_context.model_info(), &items)
            .await;
    }
}
