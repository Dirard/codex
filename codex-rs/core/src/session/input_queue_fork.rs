//! Fork: the thread queue of background notifications (terminal exits and finished workflow
//! runs) and the model-step rule of turn input. A child of `input_queue`, so it reaches the
//! queue's private fields (specs/background-exec-notifications-2026-10-25.md UC2-UC3,
//! specs/dynamic-workflow-2026-10-25.md UC7, specs/blocked-batch-turn-input-2026-10-25.md).

use std::collections::VecDeque;

use codex_history::ResponseItemEnvelope;
use codex_protocol::models::ResponseItem;
use codex_protocol::turn_input::TurnStartOptions;

use super::InputQueue;
use super::InputQueueActivity;
use super::TurnInput;
use super::TurnInputQueue;
/// The parent names the spawn budget of queued mail through `fork::*`, without a `use`.
pub(super) use crate::agent::types::TurnSpawnBudget;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::context::WorkflowNotification;
use crate::context::is_background_notification_item;

#[cfg(test)]
#[path = "input_queue_fork_tests.rs"]
mod tests;

impl TurnInput {
    /// Fork: whether this input needs a model step of its own, so that a prompt blocked beside
    /// it cannot end the turn. Tool output, including a late code-mode `notify` result, needs
    /// one, and so does mail that asks for a turn or a background terminal exit notification.
    /// Messages such as additional context or hook context and queue-only mail only join the
    /// next model request.
    pub(crate) fn needs_model_step(&self) -> bool {
        match self {
            TurnInput::UserInput { content, .. } => !content.is_empty(),
            TurnInput::FunctionCallOutput(_) => true,
            TurnInput::ResponseItem(envelope) => {
                matches!(
                    envelope.item,
                    ResponseItem::FunctionCallOutput { .. }
                        | ResponseItem::CustomToolCallOutput { .. }
                        | ResponseItem::ToolSearchOutput { .. }
                ) || is_background_notification_item(&envelope.item)
            }
            TurnInput::InterAgentCommunication(mail) => mail.trigger_turn,
        }
    }
}

/// Fork: queued terminal notifications beyond this count omit their output tail. Workflow
/// notifications in the same queue do not count (dynamic workflow R7.8).
pub(crate) const DETAILED_PENDING_BACKGROUND_NOTIFICATIONS: usize = 8;
/// Fork: queued terminal notifications beyond this count collapse into one summary line.
/// Workflow notifications in the same queue do not count and are never dropped (R7.8).
pub(crate) const MAX_PENDING_BACKGROUND_NOTIFICATIONS: usize = 32;
/// Fork: the summary line names at most this many dropped session IDs.
const MAX_LISTED_DROPPED_SESSIONS: usize = 16;

/// Fork: runs once the model's input took a queued notification (R2.12).
pub(crate) type SampledCallback = Box<dyn FnOnce() + Send>;

/// Fork: a background terminal notification as the queue stores it.
pub(crate) struct QueuedBackgroundNotification {
    pub(crate) item: ResponseItem,
    /// Called when a turn drains the notification; dropped when it is withdrawn.
    pub(crate) on_sampled: Option<SampledCallback>,
}

impl From<ResponseItem> for QueuedBackgroundNotification {
    fn from(item: ResponseItem) -> Self {
        Self {
            item,
            on_sampled: None,
        }
    }
}

/// Fork: whether the queue kept a background terminal notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackgroundNotificationEnqueue {
    Queued,
    /// Over the cap: only the summary line names the session (R3.4).
    Dropped,
}

/// Fork: background terminal exit notifications waiting for a model step.
///
/// They wait at thread level, like mail, so a turn that already decided to finish cannot
/// record one without sampling it; the next turn drains them.
#[derive(Default)]
pub(super) struct BackgroundNotificationQueue {
    items: VecDeque<BackgroundNotificationEntry>,
    dropped: usize,
    dropped_session_ids: Vec<i32>,
    dropped_wakes: bool,
}

struct BackgroundNotificationEntry {
    item: ResponseItem,
    wakes: bool,
    /// A workflow run notification rather than a background terminal one.
    workflow: bool,
    /// The `ExitNotifier` key, so a poll that shows the exit can withdraw it (R2.10).
    key: Option<u64>,
    on_sampled: Option<SampledCallback>,
}

impl BackgroundNotificationQueue {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.dropped == 0
    }

    /// Queued background terminal notifications, which the terminal limits count (R7.8).
    fn terminal_count(&self) -> usize {
        self.items.iter().filter(|entry| !entry.workflow).count()
    }

    fn wakes(&self) -> bool {
        self.dropped_wakes || self.items.iter().any(|entry| entry.wakes)
    }

    /// Takes every notification and the callbacks to run once the lock is released.
    fn drain(&mut self) -> (Vec<TurnInput>, Vec<SampledCallback>) {
        let mut sampled = Vec::new();
        let mut items: Vec<TurnInput> = self
            .items
            .drain(..)
            .map(|entry| {
                sampled.extend(entry.on_sampled);
                TurnInput::ResponseItem(ResponseItemEnvelope::new(entry.item))
            })
            .collect();
        if self.dropped > 0 {
            let summary = BackgroundTerminalNotification::new(dropped_summary(
                self.dropped,
                &self.dropped_session_ids,
            ));
            items.push(TurnInput::ResponseItem(ResponseItemEnvelope::new(
                ContextualUserFragment::into(summary),
            )));
        }
        self.dropped = 0;
        self.dropped_session_ids.clear();
        self.dropped_wakes = false;
        (items, sampled)
    }
}

/// Fork: the overflow summary names the dropped sessions (R3.4).
fn dropped_summary(dropped: usize, session_ids: &[i32]) -> String {
    let listed = session_ids
        .iter()
        .take(MAX_LISTED_DROPPED_SESSIONS)
        .map(i32::to_string)
        .collect::<Vec<_>>();
    let sessions = match dropped.saturating_sub(listed.len()) {
        _ if listed.is_empty() => String::new(),
        0 => format!(" (sessions {})", listed.join(", ")),
        unlisted => format!(" (sessions {} and {unlisted} more)", listed.join(", ")),
    };
    format!(
        "{dropped} more background terminals stopped while too many notifications were pending{sessions}; call write_stdin with one of these session IDs to read the exit code and output of a terminal that exited on its own."
    )
}

impl InputQueue {
    /// Fork: queue a background terminal exit notification for the next model step.
    ///
    /// `render` receives whether the notification may include its output tail. `key`
    /// identifies the notification for `withdraw_background_notification`.
    pub(crate) async fn enqueue_background_notification<R: Into<QueuedBackgroundNotification>>(
        &self,
        key: Option<u64>,
        session_id: Option<i32>,
        wakes: bool,
        render: impl FnOnce(/*with_output*/ bool) -> R,
    ) -> BackgroundNotificationEnqueue {
        let outcome = {
            let mut queue = self.background_notifications.lock().await;
            let terminals = queue.terminal_count();
            if terminals >= MAX_PENDING_BACKGROUND_NOTIFICATIONS {
                queue.dropped += 1;
                if queue.dropped_session_ids.len() < MAX_LISTED_DROPPED_SESSIONS {
                    queue.dropped_session_ids.extend(session_id);
                }
                queue.dropped_wakes |= wakes;
                BackgroundNotificationEnqueue::Dropped
            } else {
                let with_output = terminals < DETAILED_PENDING_BACKGROUND_NOTIFICATIONS;
                let QueuedBackgroundNotification { item, on_sampled } = render(with_output).into();
                queue.items.push_back(BackgroundNotificationEntry {
                    item,
                    wakes,
                    workflow: false,
                    key,
                    on_sampled,
                });
                BackgroundNotificationEnqueue::Queued
            }
        };
        self.activity_tx
            .send_replace(InputQueueActivity::BackgroundTerminal);
        outcome
    }

    /// Fork: remove a queued notification that a tool response made redundant (R2.10).
    pub(crate) async fn withdraw_background_notification(&self, key: u64) -> bool {
        // Dropping a withdrawn entry drops its release callback: the poll that showed the
        // exit already removed the process entry.
        let withdrawn: VecDeque<BackgroundNotificationEntry> = {
            let mut queue = self.background_notifications.lock().await;
            let (withdrawn, kept) = std::mem::take(&mut queue.items)
                .into_iter()
                .partition(|entry| entry.key == Some(key));
            queue.items = kept;
            withdrawn
        };
        !withdrawn.is_empty()
    }

    /// Fork: put notifications that an aborted turn or task took back in front of the
    /// thread queue (R3.7). A background terminal notification no longer wakes the thread;
    /// a workflow notification still asks for a turn, which the user's interrupt or Plan
    /// mode hold back like any wake (dynamic workflow R7.7).
    pub(crate) async fn requeue_background_notifications(&self, items: Vec<ResponseItem>) {
        if items.is_empty() {
            return;
        }
        let mut workflow_requeued = false;
        {
            let mut queue = self.background_notifications.lock().await;
            for item in items.into_iter().rev() {
                let workflow = WorkflowNotification::is_notification_item(&item);
                workflow_requeued |= workflow;
                queue.items.push_front(BackgroundNotificationEntry {
                    item,
                    wakes: workflow,
                    workflow,
                    key: None,
                    on_sampled: None,
                });
            }
        }
        self.activity_tx.send_replace(if workflow_requeued {
            InputQueueActivity::Workflow
        } else {
            InputQueueActivity::BackgroundTerminal
        });
    }

    /// Fork: take every queued notification, with the overflow summary, for a shutdown
    /// that records them in the thread history (R2.5).
    pub(crate) async fn take_background_notifications(&self) -> Vec<ResponseItem> {
        // The shutdown already stopped every process, so nothing is left to release.
        let (items, _sampled) = self.background_notifications.lock().await.drain();
        items
            .into_iter()
            .filter_map(|input| match input {
                TurnInput::ResponseItem(envelope) => Some(envelope.item),
                _ => None,
            })
            .collect()
    }

    /// Fork: queue the completion notification of a workflow run. It always asks for a turn
    /// and is never dropped: a thread has few runs (dynamic workflow, UC7, R7.1).
    pub(crate) async fn enqueue_workflow_notification(&self, item: ResponseItem) {
        self.background_notifications
            .lock()
            .await
            .items
            .push_back(BackgroundNotificationEntry {
                item,
                wakes: true,
                workflow: true,
                key: None,
                on_sampled: None,
            });
        self.activity_tx.send_replace(InputQueueActivity::Workflow);
    }

    /// Fork: whether any background terminal notification is queued.
    pub(crate) async fn has_background_notifications(&self) -> bool {
        !self.background_notifications.lock().await.is_empty()
    }

    /// Fork: whether a queued background terminal notification asks for a turn.
    pub(crate) async fn has_waking_background_notifications(&self) -> bool {
        self.background_notifications.lock().await.wakes()
    }

    /// Fork: mail or background terminal notifications wait for the next model step.
    pub(crate) async fn has_pending_queued_input(&self) -> bool {
        self.has_background_notifications().await || self.has_pending_mailbox_items().await
    }

    /// Fork: drains mail and the queued background notifications; the body is upstream's
    /// `drain_mailbox_input_items`, renamed `drain_mailbox_input_items_for_task`.
    pub(crate) async fn drain_mailbox_input_items(
        &self,
    ) -> (Vec<TurnInput>, TurnStartOptions, Option<TurnSpawnBudget>) {
        self.drain_mailbox_input_items_for_task(/*take_background_notifications*/ true)
            .await
    }

    /// Fork: when `take`, appends the queued background notifications to drained mail. A task
    /// other than a regular turn leaves them queued with their wake, withdrawal key and release
    /// callback, so a waking notification still starts its own turn after that task
    /// (background-exec R3.8, dynamic workflow R7.7). Release callbacks run after the queue
    /// lock is released (R2.12).
    pub(super) async fn fork_take_background_notifications(
        &self,
        take: bool,
        items: &mut Vec<TurnInput>,
    ) {
        if !take {
            return;
        }
        let (notifications, sampled) = self.background_notifications.lock().await.drain();
        items.extend(notifications);
        for callback in sampled {
            callback();
        }
    }

    /// Fork: the activity of queued background notifications; a queued workflow notification
    /// names its own activity.
    pub(super) async fn fork_background_activity(&self) -> Option<InputQueueActivity> {
        if !self.has_background_notifications().await {
            return None;
        }
        let workflow = self
            .background_notifications
            .lock()
            .await
            .items
            .iter()
            .any(|entry| WorkflowNotification::is_notification_item(&entry.item));
        Some(if workflow {
            InputQueueActivity::Workflow
        } else {
            InputQueueActivity::BackgroundTerminal
        })
    }

    /// Fork: background notifications in the input of an aborted turn or task, terminal exits
    /// and finished workflow runs alike, go back to the thread queue, so an abort before the
    /// turn sampled them does not lose them (R3.7, dynamic workflow R7.7). The rest of that
    /// input is dropped first, with the turn state lock already released.
    pub(super) async fn fork_requeue_background_notifications(
        &self,
        pending_input: TurnInputQueue,
    ) {
        let (notifications, pending_input) =
            take_background_notification_items(pending_input.items);
        drop(pending_input);
        self.requeue_background_notifications(notifications).await;
    }
}

/// Fork: split background notifications (terminal exits and finished workflow runs) from
/// other turn input, keeping order (R3.7, dynamic workflow R7.7).
fn take_background_notification_items(
    items: Vec<TurnInput>,
) -> (Vec<ResponseItem>, Vec<TurnInput>) {
    let mut notifications = Vec::new();
    let mut rest = Vec::with_capacity(items.len());
    for item in items {
        match item {
            TurnInput::ResponseItem(envelope)
                if is_background_notification_item(&envelope.item) =>
            {
                notifications.push(envelope.item);
            }
            item => rest.push(item),
        }
    }
    (notifications, rest)
}

impl TurnInputQueue {
    /// Fork: the activity of background notifications in turn-local input; a workflow
    /// notification names its own.
    pub(super) fn fork_notification_activity(&self) -> Option<InputQueueActivity> {
        if self.items.iter().any(|input| {
            matches!(
                input,
                TurnInput::ResponseItem(envelope)
                    if WorkflowNotification::is_notification_item(&envelope.item)
            )
        }) {
            Some(InputQueueActivity::Workflow)
        } else if self.items.iter().any(|input| {
            matches!(
                input,
                TurnInput::ResponseItem(envelope)
                    if is_background_notification_item(&envelope.item)
            )
        }) {
            Some(InputQueueActivity::BackgroundTerminal)
        } else {
            None
        }
    }
}
