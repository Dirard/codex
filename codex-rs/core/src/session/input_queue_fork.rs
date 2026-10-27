//! Fork: the thread queue of background notifications (terminal exits, output watch events and
//! finished workflow runs) and the model-step rule of turn input. A child of `input_queue`, so it reaches the
//! queue's private fields (specs/background-exec-notifications-2026-10-25.md UC2-UC3,
//! specs/dynamic-workflow-2026-10-25.md UC7, specs/blocked-batch-turn-input-2026-10-25.md).

use std::collections::VecDeque;

use codex_history::ResponseItemEnvelope;
use codex_protocol::models::ResponseItem;
use codex_protocol::turn_input::TurnStartOptions;

use super::InputQueue;
use super::InputQueueActivity;
use super::PendingMailboxCommunication;
use super::TurnInput;
use super::TurnInputQueue;
use crate::agent_communication::PENDING_MAILBOX_MESSAGES;
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

/// Fork: queued terminal notifications beyond this count omit their output tail. Workflow and
/// output watch notifications in the same queue do not count (dynamic workflow R7.8, R6.10).
pub(crate) const DETAILED_PENDING_BACKGROUND_NOTIFICATIONS: usize = 8;
/// Fork: queued terminal notifications beyond this count collapse into one summary line.
/// Workflow and output watch notifications in the same queue do not count and are never
/// dropped (R7.8, R6.10).
pub(crate) const MAX_PENDING_BACKGROUND_NOTIFICATIONS: usize = 32;
/// Fork: the summary line names at most this many dropped session IDs.
const MAX_LISTED_DROPPED_SESSIONS: usize = 16;
/// Fork: notifications a turn took whose queue metadata an abort can still restore (P3-3).
const MAX_REMEMBERED_TAKEN_NOTIFICATIONS: usize = 128;

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
    /// Queue metadata of notifications a turn took, newest last, so an abort or a lost wake
    /// returns them with their key and limit class (review 2026-10-27, P3-3).
    taken: VecDeque<TakenNotification>,
}

/// Fork: what the queue knew about a notification a turn took.
struct TakenNotification {
    fingerprint: u64,
    counted: bool,
    key: Option<u64>,
}

/// Fork: identifies a notification by its content; every rendering carries a fresh nonce.
fn fingerprint(item: &ResponseItem) -> u64 {
    use std::hash::Hash;
    use std::hash::Hasher;
    let mut hasher = std::hash::DefaultHasher::new();
    serde_json::to_vec(item)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

/// Fork: whether `item` is an output watch notification; the limits of R3.4 do not count it
/// (R6.10).
fn is_output_watch_item(item: &ResponseItem) -> bool {
    let ResponseItem::Message { content, .. } = item else {
        return false;
    };
    BackgroundTerminalNotification::is_notification_item(item)
        && content.iter().any(|content| {
            matches!(
                content,
                codex_protocol::models::ContentItem::InputText { text }
                    if crate::unified_exec::is_output_watch_text(text)
            )
        })
}

struct BackgroundNotificationEntry {
    item: ResponseItem,
    wakes: bool,
    /// A background terminal exit notification, which the limits of R3.4 count; workflow
    /// and output watch notifications are not counted (R7.8, R6.10).
    counted: bool,
    /// The `ExitNotifier` key, so a poll that shows the exit can withdraw it (R2.10), or the
    /// `OutputWatch` key, so later events join the queued notification (R6.7).
    key: Option<u64>,
    on_sampled: Option<SampledCallback>,
}

impl BackgroundNotificationQueue {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.dropped == 0
    }

    /// Queued background terminal notifications, which the terminal limits count (R7.8).
    fn terminal_count(&self) -> usize {
        self.items.iter().filter(|entry| entry.counted).count()
    }

    fn wakes(&self) -> bool {
        self.dropped_wakes || self.items.iter().any(|entry| entry.wakes)
    }

    /// Takes every notification and the callbacks to run once the lock is released.
    fn drain(&mut self) -> (Vec<TurnInput>, Vec<SampledCallback>) {
        let mut sampled = Vec::new();
        let entries: Vec<_> = self.items.drain(..).collect();
        let mut items = Vec::with_capacity(entries.len());
        for entry in entries {
            sampled.extend(entry.on_sampled);
            if entry.key.is_some() || !entry.counted {
                self.taken.push_back(TakenNotification {
                    fingerprint: fingerprint(&entry.item),
                    counted: entry.counted,
                    key: entry.key,
                });
            }
            items.push(TurnInput::ResponseItem(ResponseItemEnvelope::new(
                entry.item,
            )));
        }
        while self.taken.len() > MAX_REMEMBERED_TAKEN_NOTIFICATIONS {
            self.taken.pop_front();
        }
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
                    counted: true,
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

    /// Fork: put the events of an output watch in the queue (R6.7, R6.10). `render` learns
    /// whether the watch's previous notification still waits there and returns the text to
    /// put in its place, a new notification, or nothing. Returns whether the queue changed.
    pub(crate) async fn offer_output_notification(
        &self,
        key: u64,
        render: impl FnOnce(/*queued*/ bool) -> Option<ResponseItem>,
    ) -> bool {
        let changed = {
            let mut queue = self.background_notifications.lock().await;
            let position = queue.items.iter().position(|entry| entry.key == Some(key));
            match (render(position.is_some()), position) {
                (None, _) => false,
                (Some(item), Some(position)) => {
                    queue.items[position].item = item;
                    true
                }
                (Some(item), None) => {
                    queue.items.push_back(BackgroundNotificationEntry {
                        item,
                        wakes: true,
                        counted: false,
                        key: Some(key),
                        on_sampled: None,
                    });
                    true
                }
            }
        };
        if changed {
            self.activity_tx
                .send_replace(InputQueueActivity::BackgroundTerminal);
        }
        changed
    }

    /// Fork: under the queue lock, remove the notification with `key` when `redundant` says
    /// so; the output watch decides there, so its delivery cannot add lines in between (P3-2).
    pub(crate) async fn withdraw_background_notification_if(
        &self,
        key: u64,
        redundant: impl FnOnce() -> bool,
    ) {
        let withdrawn: Vec<BackgroundNotificationEntry> = {
            let mut queue = self.background_notifications.lock().await;
            let Some(position) = queue.items.iter().position(|entry| entry.key == Some(key)) else {
                return;
            };
            if !redundant() {
                return;
            }
            queue.items.remove(position).into_iter().collect()
        };
        drop(withdrawn);
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
    /// mode hold back like any wake (dynamic workflow R7.7). Each keeps the withdrawal or
    /// merge key and the limit class it had in the queue (R2.10, R6.7, R6.10; P3-3).
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
                let print = fingerprint(&item);
                let taken = queue
                    .taken
                    .iter()
                    .rposition(|taken| taken.fingerprint == print)
                    .and_then(|position| queue.taken.remove(position));
                let (counted, key) = match taken {
                    Some(taken) => (taken.counted, taken.key),
                    None => (!workflow && !is_output_watch_item(&item), None),
                };
                // A newer notification of the same output watch already holds the key.
                let key =
                    key.filter(|key| !queue.items.iter().any(|entry| entry.key == Some(*key)));
                queue.items.push_front(BackgroundNotificationEntry {
                    item,
                    wakes: workflow,
                    counted,
                    key,
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

    /// Fork (H5 in `docs/fork/01-multi-agent-runtime.md`): a wake that lost its turn
    /// reservation before its turn started puts the input it took back in front of the thread
    /// queues; nothing else would deliver it. Mail that asks for a turn gets the start options
    /// of the wake (queue-only mail has none); background notifications go back as an abort
    /// returns them. Other input of the reservation is dropped, as `clear_pending` drops it.
    pub(crate) async fn requeue_lost_wake_input(
        &self,
        input: Vec<TurnInput>,
        start_options: TurnStartOptions,
    ) {
        let (notifications, rest) = take_background_notification_items(input);
        let mails: Vec<_> = rest
            .into_iter()
            .filter_map(|item| match item {
                TurnInput::InterAgentCommunication(communication) => Some(communication),
                // Dropped before any queue lock is taken.
                _ => None,
            })
            .collect();
        if !mails.is_empty() {
            let mut pending = self.mailbox_pending_mails.lock().await;
            for communication in mails.into_iter().rev() {
                let start_options = if communication.trigger_turn {
                    start_options.clone()
                } else {
                    TurnStartOptions::default()
                };
                pending.push_front(PendingMailboxCommunication {
                    communication,
                    start_options,
                    _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
                });
            }
            drop(pending);
            self.activity_tx.send_replace(InputQueueActivity::Mailbox);
        }
        self.requeue_background_notifications(notifications).await;
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
                counted: false,
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
    pub(crate) async fn drain_mailbox_input_items(&self) -> (Vec<TurnInput>, TurnStartOptions) {
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
