//! Fork: delivery of background terminal exit notifications into the thread.
//!
//! Realizes UC3 of `specs/background-exec-notifications-2026-10-25.md`.

use std::sync::Arc;
use std::time::Duration;

use codex_protocol::models::ResponseItem;

use super::session::Session;
use crate::codex_thread::CodexThread;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::session::input_queue::QueuedBackgroundNotification;
use crate::session::input_queue::SampledCallback;
use crate::unified_exec::ExitNotifier;
use crate::unified_exec::ExitWatchers;
use crate::unified_exec::OutputWatch;
use crate::unified_exec::PendingExitNotification;
use crate::unified_exec::PendingTerminal;
use crate::unified_exec::SHUTDOWN_EXIT_WATCHER_WAIT;
use crate::unified_exec::TerminatedProcesses;
use crate::unified_exec::WatchWait;
use codex_utils_string::untrusted::UntrustedBoundary;

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
        self.fork_sync_terminal_journal().await;
        if wakes {
            self.wake_for_background_notification().await;
        }
    }

    /// Starts a turn for a waking background notification. The scheduler runs only when the
    /// notification itself may wake the thread (R3.1, R3.3): after the user's interrupt or in
    /// Plan mode a background event must not start a turn through other pending work, such
    /// as mail that asks for a turn (review 2026-10-27, P3-4). A session that is shutting
    /// down does not wake (R2.5).
    async fn wake_for_background_notification(self: &Arc<Self>) {
        if self.state.lock().await.shutting_down
            || !self.should_wake_on_background_notifications().await
        {
            return;
        }
        let session = Arc::clone(self);
        tokio::spawn(async move {
            session.maybe_start_turn_for_pending_work().await;
        });
    }

    /// Deliver the due events of an output watch (UC6, R6.7, R6.10).
    ///
    /// The events join the watch's notification while it still waits in the thread queue;
    /// otherwise they become a new notification once the interval allows it. Like an exit
    /// notification, a new one starts a turn for an idle thread; a session that is shutting
    /// down queues it without a wake, and the shutdown records it (R2.5). `exited` takes the
    /// final batch of a process that exited (R6.9). Returns whether the queue changed.
    pub(crate) async fn deliver_output_watch_notification(
        self: &Arc<Self>,
        watch: &OutputWatch,
        exited: bool,
    ) -> bool {
        let now = tokio::time::Instant::now();
        let boundary = UntrustedBoundary::random();
        let changed = self
            .input_queue
            .offer_output_notification(watch.key(), |queued| {
                watch
                    .take_for_queue(queued, now, exited, &boundary)
                    .map(|text| {
                        ContextualUserFragment::into(BackgroundTerminalNotification::new(text))
                    })
            })
            .await;
        if changed {
            self.wake_for_background_notification().await;
            // The last notification stopped the watch, and with `notify_on_exit` off nothing
            // reports the exit any more: the journal drops the terminal, so a crash now does
            // not make a resume report it lost (review of the fixes 2, N-5).
            if !exited && watch.is_stopped() && watch.exit_turned_off() {
                self.fork_sync_terminal_journal().await;
            }
        }
        changed
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
    /// Like a terminal notification it does not start a turn after the user's interrupt or in
    /// Plan mode, not even through other pending work (review 2026-10-27, P3-4).
    pub(crate) async fn deliver_workflow_notification(self: &Arc<Self>, body: String) {
        if self.state.lock().await.shutting_down {
            return;
        }
        self.input_queue
            .enqueue_workflow_notification(ContextualUserFragment::into(
                crate::context::WorkflowNotification::new(body),
            ))
            .await;
        if self.should_wake_on_background_notifications().await {
            self.maybe_start_turn_for_pending_work().await;
        }
    }

    /// Fork hook of `on_task_finished` and `handle_task_abort`, before the turn's terminal
    /// event: an output watch that still waits for its first matching line waits from the end
    /// of the turn that started it, completed or interrupted, so the time the model kept
    /// working does not use up the wait, and a later turn does not extend it (R6.14, tails
    /// 2026-10-27; tails review P3-7).
    pub(crate) async fn fork_note_turn_finished(&self, turn_id: &str) {
        if self
            .features()
            .enabled(codex_features::Feature::ExecExitNotifications)
        {
            self.services
                .unified_exec_manager
                .note_turn_finished(turn_id)
                .await;
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
    /// First the exit watchers of the stopped processes record the end of their commands, so
    /// it reaches the rollout before the rollout closes; this holds without exit
    /// notifications too (live recheck 3 R10). Without exit notifications nothing else is
    /// recorded.
    pub(crate) async fn record_background_terminals_at_shutdown(
        &self,
        terminated: TerminatedProcesses,
    ) {
        if let Some(watchers) = self.services.session_extension_data.get::<ExitWatchers>() {
            watchers
                .wait_until_done(tokio::time::Instant::now() + SHUTDOWN_EXIT_WATCHER_WAIT)
                .await;
        }
        let deadline = tokio::time::Instant::now() + SHUTDOWN_DELIVERY_WAIT;
        for exit_notifier in &terminated.deliveries_in_flight {
            exit_notifier.wait_for_delivery(deadline).await;
        }
        let mut items: Vec<ResponseItem> = self.input_queue.take_background_notifications().await;
        items.extend(terminated.shutdown_notices.iter().map(|notice| {
            ContextualUserFragment::into(BackgroundTerminalNotification::new(notice.render()))
        }));
        if !items.is_empty() {
            let turn_context = self.new_inject_items_context().await;
            self.record_conversation_items(&turn_context, turn_context.model_info(), &items)
                .await;
        }
        // The notices are recorded; a resume has nothing to learn from the journal (D6).
        self.fork_clear_terminal_journal().await;
    }
}

/// Fork hook of `thread/archive` and `thread/delete` in app-server: removes the background
/// terminal journals that dead Codex processes left for `thread_ids`, since no resume will
/// report them (tails review 2026-10-27, P3-5). A process that still runs one of the threads
/// keeps its journal until its shutdown.
pub async fn discard_background_terminal_journals(
    codex_home: &std::path::Path,
    thread_ids: &[codex_protocol::ThreadId],
) {
    crate::unified_exec::discard_terminal_journals(codex_home, thread_ids).await;
}

/// What a thread still owes the model from background terminals (UC7, R7.4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PendingBackgroundTerminals {
    /// Live terminals whose exit Codex will still report, by session ID.
    pub terminals: Vec<PendingBackgroundTerminal>,
    /// A queued background notification will start a turn.
    pub notification_queued: bool,
    /// The turn running on the thread, if any.
    pub active_turn_id: Option<String>,
    /// A turn was reserved for pending work and is about to start.
    pub turn_starting: bool,
}

/// A live terminal whose exit Codex will still report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingBackgroundTerminal {
    pub process_id: i32,
    pub command: String,
    pub running_for: Duration,
    /// The pattern of its output watch, if one is on.
    pub output_watch: Option<String>,
    /// `notify_on_exit` is on: what waits for the terminal waits for its exit (live recheck
    /// 2026-10-27, N1).
    pub notify_on_exit: bool,
    /// How much longer its output watch keeps it awaited without news, if one is on, and
    /// whether that wait follows a match (live recheck 2 R2).
    pub output_watch_wait: Option<Duration>,
    pub output_watch_matched: bool,
}

impl From<PendingTerminal> for PendingBackgroundTerminal {
    fn from(terminal: PendingTerminal) -> Self {
        Self {
            process_id: terminal.process_id,
            command: terminal.command,
            running_for: tokio::time::Instant::now().saturating_duration_since(terminal.started_at),
            output_watch: terminal.output_watch,
            notify_on_exit: terminal.notify_on_exit,
            output_watch_wait: terminal.watch_wait.map(|wait| match wait {
                WatchWait::FirstMatchAfterTurn(left)
                | WatchWait::FirstMatch(left)
                | WatchWait::Quiet(left) => left,
            }),
            output_watch_matched: matches!(terminal.watch_wait, Some(WatchWait::Quiet(_))),
        }
    }
}

impl CodexThread {
    /// Fork: what the thread still owes the model from background terminals, so that
    /// `codex exec` can wait for it (UC7, R7.1, R7.4). The process store is read before the
    /// queue, as for eviction: a notification on its way into the queue still counts as a
    /// pending terminal. A turn that is only finishing counts as neither running nor
    /// starting.
    pub async fn pending_background_terminals(&self) -> PendingBackgroundTerminals {
        let terminals = self
            .session
            .services
            .unified_exec_manager
            .pending_exit_notifications()
            .await
            .into_iter()
            .map(PendingBackgroundTerminal::from)
            .collect();
        let notification_queued = self.session.should_wake_on_background_notifications().await;
        let (active_turn_id, turn_starting) = match self.session.active_turn.lock().await.as_ref() {
            Some(turn) => match &turn.task {
                Some(task) => (Some(task.turn_context.sub_id.clone()), false),
                None => (None, turn.finishing.is_none()),
            },
            None => (None, false),
        };
        PendingBackgroundTerminals {
            terminals,
            notification_queued,
            active_turn_id,
            turn_starting,
        }
    }
}
