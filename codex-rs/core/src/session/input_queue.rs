use crate::agent::api::AgentControl;
use crate::agent::types::TurnSpawnBudget;
use crate::agent_communication::PENDING_MAILBOX_MESSAGES;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::state::ActiveTurn;
use crate::state::MailboxDeliveryPhase;
use crate::state::TurnState;
use codex_diagnostics::GaugeGuard;
use codex_history::ResponseItemEnvelope;
use codex_protocol::ThreadId;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::InterAgentCommunication;
use codex_protocol::turn_input::TurnStartOptions;
use codex_protocol::user_input::UserInput;
use serde::Deserialize;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

/// Host capture metadata belonging to one input, including steers within another turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance_order: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "codex_history::UserInputOrigin::is_user"
    )]
    pub origin: codex_history::UserInputOrigin,
}

/// Input consumed by a regular turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TurnInput {
    UserInput {
        content: Vec<UserInput>,
        client_id: Option<String>,
        #[serde(flatten)]
        metadata: UserInputMetadata,
    },
    FunctionCallOutput(#[serde(with = "turn_input_response_item")] ResponseItemEnvelope),
    // Preserve the existing serialized format while carrying injection API metadata
    // through the in-memory queue.
    ResponseItem(#[serde(with = "turn_input_response_item")] ResponseItemEnvelope),
    InterAgentCommunication(crate::PendingAgentMessage),
}

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
                ) || BackgroundTerminalNotification::is_notification_item(&envelope.item)
            }
            TurnInput::InterAgentCommunication(mail) => mail.trigger_turn,
        }
    }
}

mod turn_input_response_item {
    use super::ResponseItem;
    use super::ResponseItemEnvelope;
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use serde::Serializer;
    use serde::ser::Error as _;

    pub(super) fn serialize<S>(
        item: &ResponseItemEnvelope,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if item.metadata.is_some() {
            return Err(S::Error::custom(
                "annotated response items cannot cross the turn-input serialization boundary",
            ));
        }
        item.item.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<ResponseItemEnvelope, D::Error>
    where
        D: Deserializer<'de>,
    {
        ResponseItem::deserialize(deserializer).map(ResponseItemEnvelope::new)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InputQueueActivity {
    Mailbox,
    Steer,
    /// Fork: a background terminal exit notification is waiting for the model.
    BackgroundTerminal,
}

/// Fork: queued notifications beyond this count omit their output tail.
pub(crate) const DETAILED_PENDING_BACKGROUND_NOTIFICATIONS: usize = 8;
/// Fork: queued notifications beyond this count collapse into one summary line.
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
struct BackgroundNotificationQueue {
    items: VecDeque<BackgroundNotificationEntry>,
    dropped: usize,
    dropped_session_ids: Vec<i32>,
    dropped_wakes: bool,
}

struct BackgroundNotificationEntry {
    item: ResponseItem,
    wakes: bool,
    /// The `ExitNotifier` key, so a poll that shows the exit can withdraw it (R2.10).
    key: Option<u64>,
    on_sampled: Option<SampledCallback>,
}

impl BackgroundNotificationQueue {
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.dropped == 0
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

/// Turn-local pending input storage owned by the input queue flow.
#[derive(Default)]
pub(crate) struct TurnInputQueue {
    items: Vec<TurnInput>,
    /// Response items queued through a notifying injection, unlike passive context.
    has_pending_injection: bool,
}

/// Session-scoped pending input storage and active-turn mailbox delivery coordination.
pub(crate) struct InputQueue {
    activity_tx: watch::Sender<InputQueueActivity>,
    controller: Option<(ThreadId, Arc<dyn AgentControl>, watch::Receiver<bool>)>,
    mailbox_pending_mails: Mutex<VecDeque<PendingMailboxCommunication>>,
    background_notifications: Mutex<BackgroundNotificationQueue>,
}

pub(crate) struct PendingMailboxCommunication {
    pub(crate) communication: crate::PendingAgentMessage,
    start_options: TurnStartOptions,
    turn_spawn_budget: Option<TurnSpawnBudget>,
    _diagnostics_guard: GaugeGuard,
}

impl InputQueue {
    pub(crate) fn new() -> Self {
        let (activity_tx, _) = watch::channel(InputQueueActivity::Mailbox);
        Self {
            activity_tx,
            controller: None,
            mailbox_pending_mails: Mutex::new(VecDeque::new()),
            background_notifications: Mutex::new(BackgroundNotificationQueue::default()),
        }
    }

    pub(crate) fn with_controller(thread_id: ThreadId, control: Arc<dyn AgentControl>) -> Self {
        let updates = control.watch_mailbox(thread_id);
        Self {
            controller: Some((thread_id, control, updates)),
            ..Self::new()
        }
    }

    pub(crate) fn mailbox_updates(&self) -> Option<watch::Receiver<bool>> {
        self.controller
            .as_ref()
            .map(|(_, _, updates)| updates.clone())
    }

    pub(crate) fn notify_mailbox(&self) {
        self.activity_tx.send_replace(InputQueueActivity::Mailbox);
    }

    fn read_mailbox(&self, pending: &mut VecDeque<PendingMailboxCommunication>) {
        if let Some((thread_id, control, _)) = &self.controller {
            pending.extend(
                control
                    .take_mailbox(*thread_id)
                    .into_iter()
                    .map(|communication| PendingMailboxCommunication {
                        communication,
                        start_options: TurnStartOptions::default(),
                        turn_spawn_budget: None,
                        _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
                    }),
            );
        }
    }

    pub(crate) async fn subscribe_activity(
        &self,
        turn_state: Option<&Mutex<TurnState>>,
    ) -> (
        watch::Receiver<InputQueueActivity>,
        Option<InputQueueActivity>,
    ) {
        let activity_rx = self.activity_tx.subscribe();
        let turn_activity = if let Some(turn_state) = turn_state {
            turn_state.lock().await.pending_input.pending_activity()
        } else {
            None
        };
        let pending_activity = if let Some(activity) = turn_activity {
            Some(activity)
        } else if self.has_background_notifications().await {
            Some(InputQueueActivity::BackgroundTerminal)
        } else if self.has_pending_mailbox_items().await {
            Some(InputQueueActivity::Mailbox)
        } else {
            None
        };
        (activity_rx, pending_activity)
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state updates must remain atomic"
    )]
    pub(crate) async fn deliver_mailbox_communication_to_current_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        communication: InterAgentCommunication,
        expected_turn_id: Option<&str>,
    ) -> bool {
        let active = active_turn.lock().await;
        let Some(active_turn) = active.as_ref().filter(|turn| {
            turn.task.as_ref().is_some_and(|task| {
                expected_turn_id.is_none_or(|id| {
                    task.turn_context.sub_id == id && !task.cancellation_token.is_cancelled()
                })
            })
        }) else {
            return false;
        };
        let mut turn_state = active_turn.turn_state.lock().await;
        if !turn_state.accepts_mailbox_delivery_for_current_turn() {
            return false;
        }
        turn_state
            .pending_input
            .items
            .push(TurnInput::InterAgentCommunication(communication.into()));
        self.activity_tx.send_replace(InputQueueActivity::Mailbox);
        true
    }

    pub(crate) async fn enqueue_mailbox_communication(
        &self,
        communication: crate::PendingAgentMessage,
        start_options: TurnStartOptions,
        turn_spawn_budget: Option<TurnSpawnBudget>,
    ) {
        let mut pending = self.mailbox_pending_mails.lock().await;
        // Mail retained while unloaded precedes new submissions to the loaded session.
        self.read_mailbox(&mut pending);
        pending.push_back(PendingMailboxCommunication {
            communication,
            start_options,
            turn_spawn_budget,
            _diagnostics_guard: PENDING_MAILBOX_MESSAGES.track(),
        });
        self.activity_tx.send_replace(InputQueueActivity::Mailbox);
    }

    pub(crate) async fn has_pending_mailbox_items(&self) -> bool {
        !self.mailbox_pending_mails.lock().await.is_empty()
            || self
                .controller
                .as_ref()
                .is_some_and(|(_, _, updates)| *updates.borrow())
    }

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
            if queue.items.len() >= MAX_PENDING_BACKGROUND_NOTIFICATIONS {
                queue.dropped += 1;
                if queue.dropped_session_ids.len() < MAX_LISTED_DROPPED_SESSIONS {
                    queue.dropped_session_ids.extend(session_id);
                }
                queue.dropped_wakes |= wakes;
                BackgroundNotificationEnqueue::Dropped
            } else {
                let with_output = queue.items.len() < DETAILED_PENDING_BACKGROUND_NOTIFICATIONS;
                let QueuedBackgroundNotification { item, on_sampled } = render(with_output).into();
                queue.items.push_back(BackgroundNotificationEntry {
                    item,
                    wakes,
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

    /// Fork: put notifications that an aborted or non-regular task took back in front of
    /// the thread queue. They do not wake the thread (R3.7, R3.8).
    pub(crate) async fn requeue_background_notifications(&self, items: Vec<ResponseItem>) {
        if items.is_empty() {
            return;
        }
        {
            let mut queue = self.background_notifications.lock().await;
            for item in items.into_iter().rev() {
                queue.items.push_front(BackgroundNotificationEntry {
                    item,
                    wakes: false,
                    key: None,
                    on_sampled: None,
                });
            }
        }
        self.activity_tx
            .send_replace(InputQueueActivity::BackgroundTerminal);
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

    pub(crate) async fn has_trigger_turn_mailbox_items(&self) -> bool {
        self.mailbox_pending_mails
            .lock()
            .await
            .iter()
            .any(|mail| mail.communication.trigger_turn)
    }

    pub(crate) async fn drain_mailbox(&self) -> Vec<PendingMailboxCommunication> {
        let mut pending = self.mailbox_pending_mails.lock().await;
        self.read_mailbox(&mut pending);
        pending.drain(..).collect()
    }

    pub(crate) async fn drain_mailbox_input_items(
        &self,
    ) -> (Vec<TurnInput>, TurnStartOptions, Option<TurnSpawnBudget>) {
        let pending_mails = self.drain_mailbox().await;
        // A later follow-up supersedes the earlier choice, including an omitted choice.
        let turn_spawn_budget = pending_mails
            .iter()
            .rev()
            .find(|mail| mail.communication.trigger_turn)
            .and_then(|mail| mail.turn_spawn_budget.clone());
        let mut start_options = pending_mails
            .iter()
            .rev()
            .find(|mail| mail.communication.trigger_turn)
            .map(|mail| mail.start_options.clone())
            .unwrap_or_default();
        start_options.parent_turn_id = pending_mails
            .iter()
            .filter(|mail| mail.communication.trigger_turn)
            .map(|mail| mail.start_options.parent_turn_id.as_deref())
            .reduce(|expected, candidate| expected.filter(|id| candidate == Some(*id)))
            .and_then(|id| id.filter(|id| !id.trim().is_empty()).map(str::to_string));
        start_options.root_turn_id = pending_mails
            .iter()
            .find(|mail| mail.communication.trigger_turn)
            .and_then(|mail| {
                mail.start_options
                    .parent_turn_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .and(mail.start_options.root_turn_id.as_deref())
                    .filter(|id| !id.trim().is_empty())
            })
            .map(str::to_string);
        let mut items: Vec<TurnInput> = pending_mails
            .into_iter()
            .map(|mail| TurnInput::InterAgentCommunication(mail.communication))
            .collect();
        let (notifications, sampled) = self.background_notifications.lock().await.drain();
        items.extend(notifications);
        for callback in sampled {
            callback();
        }
        (items, start_options, turn_spawn_budget)
    }

    pub(crate) async fn turn_state_for_sub_id(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) -> Option<Arc<Mutex<TurnState>>> {
        let active = active_turn.lock().await;
        active.as_ref().and_then(|active_turn| {
            active_turn
                .task
                .as_ref()
                .is_some_and(|task| task.turn_context.sub_id == sub_id)
                .then(|| Arc::clone(&active_turn.turn_state))
        })
    }

    /// Signal once a user message is queued for this sampling request.
    pub(crate) async fn watch_user_input(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
        interrupt: CancellationToken,
    ) -> Option<AbortOnDropHandle<()>> {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await?;
        // Subscribe before inspecting the queue so an arrival cannot be missed.
        let mut activity = self.activity_tx.subscribe();
        Some(AbortOnDropHandle::new(tokio::spawn(async move {
            loop {
                if turn_state.lock().await.pending_input.has_user_input() {
                    interrupt.cancel();
                    return;
                }
                if activity.changed().await.is_err() {
                    return;
                }
            }
        })))
    }

    /// Clear any pending waiters and input buffered for the current turn.
    ///
    /// Fork: background terminal notifications in that input go back to the thread queue,
    /// so an abort before the turn sampled them does not lose them (R3.7).
    pub(crate) async fn clear_pending(&self, active_turn: &ActiveTurn) {
        let pending_input = {
            let mut turn_state = active_turn.turn_state.lock().await;
            turn_state.clear_pending_waiters();
            std::mem::take(&mut turn_state.pending_input)
        };
        // Pending mail can own extension attachments whose destructors need the lock released.
        let (notifications, pending_input) =
            take_background_notification_items(pending_input.items);
        drop(pending_input);
        self.requeue_background_notifications(notifications).await;
    }

    pub(crate) async fn defer_mailbox_delivery_to_next_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await;
        let Some(turn_state) = turn_state else {
            return;
        };
        let mut turn_state = turn_state.lock().await;
        // Explicit same-turn work still needs a follow-up. Queue-only child mail does not: keep
        // it pending so task completion records it for the next turn without sampling again.
        if turn_state.pending_input.items.iter().any(|input| {
            !matches!(
                input,
                TurnInput::InterAgentCommunication(communication) if !communication.trigger_turn
            )
        }) {
            return;
        }
        turn_state.set_mailbox_delivery_phase(MailboxDeliveryPhase::NextTurn);
    }

    pub(crate) async fn accept_mailbox_delivery_for_current_turn(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
        sub_id: &str,
    ) {
        let turn_state = self.turn_state_for_sub_id(active_turn, sub_id).await;
        let Some(turn_state) = turn_state else {
            return;
        };
        self.accept_mailbox_delivery_for_turn_state(turn_state.as_ref())
            .await;
    }

    pub(super) async fn accept_mailbox_delivery_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
    ) {
        turn_state
            .lock()
            .await
            .accept_mailbox_delivery_for_current_turn();
    }

    pub(super) async fn extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
        input: Vec<TurnInput>,
    ) {
        {
            let mut turn_state = turn_state.lock().await;
            turn_state.pending_input.has_pending_injection |= input
                .iter()
                .any(|input| matches!(input, TurnInput::ResponseItem(_)));
            turn_state.pending_input.items.extend(input);
            turn_state.accept_mailbox_delivery_for_current_turn();
        }
        self.activity_tx.send_replace(InputQueueActivity::Steer);
    }

    pub(crate) async fn extend_pending_input_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
        input: Vec<TurnInput>,
    ) {
        turn_state.lock().await.pending_input.items.extend(input);
    }

    pub(crate) async fn take_pending_input_for_turn_state(
        &self,
        turn_state: &Mutex<TurnState>,
    ) -> Vec<TurnInput> {
        std::mem::take(&mut turn_state.lock().await.pending_input).items
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state updates must remain atomic"
    )]
    pub(crate) async fn get_pending_input(
        &self,
        active_turn: &Mutex<Option<ActiveTurn>>,
    ) -> (Vec<TurnInput>, TurnStartOptions, Option<TurnSpawnBudget>) {
        let (pending_input, accepts_mailbox_delivery) = {
            let mut active = active_turn.lock().await;
            match active.as_mut() {
                Some(active_turn) => {
                    let mut turn_state = active_turn.turn_state.lock().await;
                    let accepts_mailbox_delivery =
                        turn_state.accepts_mailbox_delivery_for_current_turn();
                    let pending_input = if accepts_mailbox_delivery {
                        std::mem::take(&mut turn_state.pending_input).items
                    } else {
                        Vec::new()
                    };
                    (pending_input, accepts_mailbox_delivery)
                }
                None => (Vec::new(), true),
            }
        };
        if !accepts_mailbox_delivery {
            return (pending_input, TurnStartOptions::default(), None);
        }
        let (mailbox_items, start_options, turn_spawn_budget) =
            self.drain_mailbox_input_items().await;
        if pending_input.is_empty() {
            (mailbox_items, start_options, turn_spawn_budget)
        } else {
            let mut pending_input = pending_input;
            pending_input.extend(mailbox_items);
            (pending_input, start_options, turn_spawn_budget)
        }
    }

    #[expect(
        clippy::await_holding_invalid_type,
        reason = "active turn checks and turn state reads must remain atomic"
    )]
    pub(crate) async fn has_pending_input(&self, active_turn: &Mutex<Option<ActiveTurn>>) -> bool {
        let (has_turn_pending_input, accepts_mailbox_delivery) = {
            let active = active_turn.lock().await;
            match active.as_ref() {
                Some(active_turn) => {
                    let turn_state = active_turn.turn_state.lock().await;
                    (
                        !turn_state.pending_input.is_empty(),
                        turn_state.accepts_mailbox_delivery_for_current_turn(),
                    )
                }
                None => (false, true),
            }
        };
        if !accepts_mailbox_delivery {
            return false;
        }
        if has_turn_pending_input {
            return true;
        }
        self.has_pending_queued_input().await
    }
}

/// Fork: split background terminal notifications from other turn input, keeping order.
pub(crate) fn take_background_notification_items(
    items: Vec<TurnInput>,
) -> (Vec<ResponseItem>, Vec<TurnInput>) {
    let mut notifications = Vec::new();
    let mut rest = Vec::with_capacity(items.len());
    for item in items {
        match item {
            TurnInput::ResponseItem(envelope)
                if BackgroundTerminalNotification::is_notification_item(&envelope.item) =>
            {
                notifications.push(envelope.item);
            }
            item => rest.push(item),
        }
    }
    (notifications, rest)
}

impl TurnInputQueue {
    fn has_user_input(&self) -> bool {
        self.items
            .iter()
            .any(|input| matches!(input, TurnInput::UserInput { .. }))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn pending_activity(&self) -> Option<InputQueueActivity> {
        if self.has_pending_injection
            || self.items.iter().any(|input| {
                matches!(
                    input,
                    TurnInput::UserInput { .. } | TurnInput::FunctionCallOutput(_)
                )
            })
        {
            Some(InputQueueActivity::Steer)
        } else if self.items.iter().any(|input| {
            matches!(
                input,
                TurnInput::ResponseItem(envelope)
                    if BackgroundTerminalNotification::is_notification_item(&envelope.item)
            )
        }) {
            Some(InputQueueActivity::BackgroundTerminal)
        } else if self
            .items
            .iter()
            .any(|input| matches!(input, TurnInput::InterAgentCommunication(_)))
        {
            Some(InputQueueActivity::Mailbox)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_history::CodexHarnessMetadata;
    use codex_protocol::AgentPath;
    use codex_protocol::user_input::UserInput;
    use pretty_assertions::assert_eq;

    /// Fork: the one rule for which input keeps a batch with a blocked prompt going.
    #[test]
    fn needs_model_step_covers_prompts_tool_output_and_turn_mail() {
        let prompt = |content: Vec<UserInput>| TurnInput::UserInput {
            content,
            client_id: None,
            metadata: UserInputMetadata::default(),
        };
        let item = |value: serde_json::Value| {
            ResponseItemEnvelope::new(serde_json::from_value(value).expect("response item"))
        };
        let mail = |trigger_turn| {
            TurnInput::InterAgentCommunication(
                make_mail(AgentPath::root(), AgentPath::root(), "task", trigger_turn).into(),
            )
        };
        let cases = [
            (
                "prompt",
                prompt(vec![UserInput::Text {
                    text: "hello".to_string(),
                    text_elements: Vec::new(),
                }]),
                true,
            ),
            ("empty prompt", prompt(Vec::new()), false),
            (
                "standalone tool output",
                TurnInput::FunctionCallOutput(item(serde_json::json!({
                    "type": "function_call_output", "name": "send", "output": "done",
                }))),
                true,
            ),
            (
                "late function output",
                TurnInput::ResponseItem(item(serde_json::json!({
                    "type": "function_call_output", "call_id": "call", "output": "done",
                }))),
                true,
            ),
            (
                "code-mode notify output",
                TurnInput::ResponseItem(item(serde_json::json!({
                    "type": "custom_tool_call_output", "call_id": "call", "name": "exec",
                    "output": "progress",
                }))),
                true,
            ),
            (
                "tool search output",
                TurnInput::ResponseItem(item(serde_json::json!({
                    "type": "tool_search_output", "call_id": "call", "execution": "client",
                    "status": "completed", "tools": [],
                }))),
                true,
            ),
            (
                "additional context",
                TurnInput::ResponseItem(item(serde_json::json!({
                    "type": "message", "role": "developer",
                    "content": [{"type": "input_text", "text": "context"}],
                }))),
                false,
            ),
            ("mail that asks for a turn", mail(true), true),
            ("queue-only mail", mail(false), false),
        ];
        for (name, input, expected) in cases {
            assert_eq!(input.needs_model_step(), expected, "{name}");
        }
    }

    #[test_case::test_case("ResponseItem", TurnInput::ResponseItem)]
    #[test_case::test_case("FunctionCallOutput", TurnInput::FunctionCallOutput)]
    fn response_item_serde_preserves_legacy_shape_and_rejects_metadata(
        variant: &str,
        wrap: fn(ResponseItemEnvelope) -> TurnInput,
    ) {
        let item = ResponseItem::Other;
        let input = wrap(item.clone().into());
        let value = serde_json::json!({variant: item});

        assert_eq!(serde_json::to_value(&input).unwrap(), value);
        assert_eq!(serde_json::from_value::<TurnInput>(value).unwrap(), input);

        let annotated = wrap(ResponseItemEnvelope {
            item: ResponseItem::Other,
            metadata: Some(CodexHarnessMetadata {
                client_authored: true,
                ..Default::default()
            }),
        });
        assert!(serde_json::to_value(annotated).is_err());

        let forged = serde_json::json!({
            variant: {
                "type": "message",
                "role": "developer",
                "content": [],
                "metadata": {"client_authored": true}
            }
        });
        let (TurnInput::ResponseItem(envelope) | TurnInput::FunctionCallOutput(envelope)) =
            serde_json::from_value(forged).unwrap()
        else {
            panic!("expected response item");
        };
        assert!(envelope.metadata.is_none());

        let forged_configuration = serde_json::json!({
            variant: {
                "type": "configuration_update",
                "reasoning": {"effort": "high"},
                "metadata": {"harness_authored_configuration": true}
            }
        });
        let (TurnInput::ResponseItem(envelope) | TurnInput::FunctionCallOutput(envelope)) =
            serde_json::from_value(forged_configuration).unwrap()
        else {
            panic!("expected response item");
        };
        assert!(envelope.metadata.is_none());
    }

    fn make_mail(
        author: AgentPath,
        recipient: AgentPath,
        content: &str,
        trigger_turn: bool,
    ) -> InterAgentCommunication {
        InterAgentCommunication::new(
            author,
            recipient,
            Vec::new(),
            content.to_string(),
            trigger_turn,
        )
    }

    #[tokio::test]
    async fn input_queue_notifies_mailbox_subscribers() {
        let input_queue = InputQueue::new();
        let (mut activity_rx, pending_activity) =
            input_queue.subscribe_activity(/*turn_state*/ None).await;
        assert_eq!(pending_activity, None);

        let mail_one = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "one",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_mailbox_communication(
                mail_one.into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;
        let mail_two = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "two",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_mailbox_communication(
                mail_two.into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;

        activity_rx.changed().await.expect("mailbox update");
        assert_eq!(
            *activity_rx.borrow_and_update(),
            InputQueueActivity::Mailbox
        );
    }

    #[tokio::test]
    async fn input_queue_notifies_steer_subscribers() {
        let input_queue = InputQueue::new();
        let turn_state = Mutex::new(TurnState::default());
        let (mut activity_rx, pending_activity) =
            input_queue.subscribe_activity(Some(&turn_state)).await;
        assert_eq!(pending_activity, None);

        input_queue
            .extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
                &turn_state,
                vec![TurnInput::UserInput {
                    metadata: Default::default(),
                    content: vec![UserInput::Text {
                        text: "steer".to_string(),
                        text_elements: Vec::new(),
                    }],
                    client_id: None,
                }],
            )
            .await;

        activity_rx.changed().await.expect("steer update");
        assert_eq!(*activity_rx.borrow_and_update(), InputQueueActivity::Steer);
    }

    #[tokio::test]
    async fn input_queue_reports_already_pending_steer() {
        let input_queue = InputQueue::new();
        let turn_state = Mutex::new(TurnState::default());
        let passive_output: TurnInput = serde_json::from_value(serde_json::json!({
            "ResponseItem": {"type": "function_call_output", "name": "notify", "output": "passive"}
        }))
        .unwrap();
        input_queue
            .extend_pending_input_for_turn_state(&turn_state, vec![passive_output.clone()])
            .await;
        assert_eq!(
            input_queue.subscribe_activity(Some(&turn_state)).await.1,
            None
        );
        input_queue
            .extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
                &turn_state,
                vec![passive_output.clone()],
            )
            .await;
        assert_eq!(
            input_queue.subscribe_activity(Some(&turn_state)).await.1,
            Some(InputQueueActivity::Steer)
        );
        // Consuming the injection must not make later passive context wake a wait.
        input_queue
            .take_pending_input_for_turn_state(&turn_state)
            .await;
        input_queue
            .extend_pending_input_for_turn_state(&turn_state, vec![passive_output])
            .await;
        assert_eq!(
            input_queue.subscribe_activity(Some(&turn_state)).await.1,
            None
        );
        let communication = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "already pending mail",
            /*trigger_turn*/ false,
        );
        input_queue
            .extend_pending_input_for_turn_state(
                &turn_state,
                vec![TurnInput::InterAgentCommunication(communication.into())],
            )
            .await;
        assert_eq!(
            input_queue.subscribe_activity(Some(&turn_state)).await.1,
            Some(InputQueueActivity::Mailbox)
        );
        input_queue
            .extend_pending_input_and_accept_mailbox_delivery_for_turn_state(
                &turn_state,
                vec![TurnInput::UserInput {
                    metadata: Default::default(),
                    content: vec![UserInput::Text {
                        text: "already pending".to_string(),
                        text_elements: Vec::new(),
                    }],
                    client_id: None,
                }],
            )
            .await;

        let (_activity_rx, pending_activity) =
            input_queue.subscribe_activity(Some(&turn_state)).await;

        assert_eq!(pending_activity, Some(InputQueueActivity::Steer));
    }

    #[tokio::test]
    async fn input_queue_drains_mailbox_in_delivery_order() {
        let input_queue = InputQueue::new();
        let mail_one = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "one",
            /*trigger_turn*/ false,
        );
        let mail_two = make_mail(
            AgentPath::try_from("/root/worker").expect("agent path"),
            AgentPath::root(),
            "two",
            /*trigger_turn*/ true,
        );

        input_queue
            .enqueue_mailbox_communication(
                mail_one.clone().into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;
        input_queue
            .enqueue_mailbox_communication(
                mail_two.clone().into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;

        assert_eq!(
            input_queue.drain_mailbox_input_items().await.0,
            vec![
                TurnInput::InterAgentCommunication(mail_one.into()),
                TurnInput::InterAgentCommunication(mail_two.into())
            ]
        );
        assert!(!input_queue.has_pending_mailbox_items().await);
    }

    #[tokio::test]
    async fn input_queue_uses_unambiguous_trigger_parent_and_first_root() {
        let (parent, peer, root, root2) = (Some("a"), Some("b"), Some("r"), Some("s"));
        for (pending_mails, expected_parent_turn_id, expected_root_turn_id) in [
            (Vec::new(), None, None),
            (vec![(false, Some("q"), root)], None, None),
            (vec![(true, Some(""), root)], None, None),
            (vec![(true, Some("   "), root)], None, None),
            (vec![(true, None, root)], None, None),
            (vec![(true, parent, None)], parent, None),
            (vec![(true, parent, Some(""))], parent, None),
            (vec![(true, parent, root), (true, peer, root)], None, root),
            (vec![(true, parent, root), (true, peer, root2)], None, root),
            (vec![(true, parent, root), (true, None, root)], None, root),
            (
                vec![(true, parent, root), (true, parent, root)],
                parent,
                root,
            ),
            (
                vec![(false, Some("q"), root2), (true, parent, root)],
                parent,
                root,
            ),
        ] {
            let input_queue = InputQueue::new();
            for (trigger_turn, parent_turn_id, root_turn_id) in pending_mails {
                input_queue
                    .enqueue_mailbox_communication(
                        make_mail(AgentPath::root(), AgentPath::root(), "task", trigger_turn)
                            .into(),
                        TurnStartOptions {
                            parent_turn_id: parent_turn_id.map(str::to_string),
                            root_turn_id: root_turn_id.map(str::to_string),
                            ..Default::default()
                        },
                        /*turn_spawn_budget*/ None,
                    )
                    .await;
            }
            let (_, start_options, _) = input_queue.drain_mailbox_input_items().await;
            assert_eq!(
                start_options.parent_turn_id.as_deref(),
                expected_parent_turn_id
            );
            assert_eq!(start_options.root_turn_id.as_deref(), expected_root_turn_id);
        }
    }

    #[tokio::test]
    async fn input_queue_uses_latest_followup_choice_and_ignores_queue_only_mail() {
        use codex_protocol::turn_input::CyberAccessProgram;

        for latest in [Some(CyberAccessProgram::Standard), None] {
            let input_queue = InputQueue::new();
            for (trigger_turn, program) in [
                (true, Some(CyberAccessProgram::DaybreakBlue)),
                (true, latest),
                (false, Some(CyberAccessProgram::DaybreakRed)),
            ] {
                input_queue
                    .enqueue_mailbox_communication(
                        make_mail(AgentPath::root(), AgentPath::root(), "task", trigger_turn)
                            .into(),
                        TurnStartOptions {
                            cyber_access_program: program,
                            ..Default::default()
                        },
                        /*turn_spawn_budget*/ None,
                    )
                    .await;
            }
            let (_, start_options, _) = input_queue.drain_mailbox_input_items().await;
            assert_eq!(start_options.cyber_access_program, latest);
        }
    }

    #[tokio::test]
    async fn input_queue_tracks_pending_trigger_turn_mail() {
        let input_queue = InputQueue::new();

        let queued_mail = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "queued",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_mailbox_communication(
                queued_mail.into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;
        assert!(!input_queue.has_trigger_turn_mailbox_items().await);

        let trigger_mail = make_mail(
            AgentPath::root(),
            AgentPath::try_from("/root/worker").expect("agent path"),
            "wake",
            /*trigger_turn*/ true,
        );
        input_queue
            .enqueue_mailbox_communication(
                trigger_mail.into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;
        assert!(input_queue.has_trigger_turn_mailbox_items().await);
    }

    fn background_notification(body: &str) -> ResponseItem {
        ContextualUserFragment::into(BackgroundTerminalNotification::new(body))
    }

    /// Fork: background terminal notifications wait at thread level and wake waits.
    #[tokio::test]
    async fn background_notifications_queue_after_mail_and_report_activity() {
        let input_queue = InputQueue::new();
        let (mut activity_rx, pending_activity) =
            input_queue.subscribe_activity(/*turn_state*/ None).await;
        assert_eq!(pending_activity, None);
        assert!(!input_queue.has_pending_queued_input().await);

        let mail = make_mail(
            AgentPath::try_from("/root/worker").expect("agent path"),
            AgentPath::root(),
            "mail",
            /*trigger_turn*/ false,
        );
        input_queue
            .enqueue_background_notification(
                /*key*/ None,
                /*session_id*/ None,
                /*wakes*/ false,
                |with_output| {
                    assert!(with_output);
                    background_notification("passive")
                },
            )
            .await;
        input_queue
            .enqueue_mailbox_communication(
                mail.clone().into(),
                Default::default(),
                /*turn_spawn_budget*/ None,
            )
            .await;
        activity_rx.changed().await.expect("activity");
        assert!(input_queue.has_pending_queued_input().await);
        assert!(!input_queue.has_waking_background_notifications().await);
        assert!(!input_queue.has_trigger_turn_mailbox_items().await);
        assert_eq!(
            input_queue.subscribe_activity(/*turn_state*/ None).await.1,
            Some(InputQueueActivity::BackgroundTerminal)
        );

        input_queue
            .enqueue_background_notification(
                /*key*/ None,
                /*session_id*/ None,
                /*wakes*/ true,
                |_| background_notification("waking"),
            )
            .await;
        assert!(input_queue.has_waking_background_notifications().await);

        let (items, _, _) = input_queue.drain_mailbox_input_items().await;
        assert_eq!(
            items,
            vec![
                TurnInput::InterAgentCommunication(mail.into()),
                TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                    "passive"
                ))),
                TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                    "waking"
                ))),
            ]
        );
        assert!(items[1].needs_model_step());
        assert!(!input_queue.has_pending_queued_input().await);
        assert!(!input_queue.has_waking_background_notifications().await);
    }

    /// Fork: many pending notifications drop their output, then collapse into a summary.
    #[tokio::test]
    async fn background_notifications_are_bounded() {
        let input_queue = InputQueue::new();
        let mut with_output_flags = Vec::new();
        let mut outcomes = Vec::new();
        for index in 0..MAX_PENDING_BACKGROUND_NOTIFICATIONS + 2 {
            let session_id = 1000 + i32::try_from(index).expect("small index");
            outcomes.push(
                input_queue
                    .enqueue_background_notification(
                        /*key*/ None,
                        Some(session_id),
                        /*wakes*/ index == MAX_PENDING_BACKGROUND_NOTIFICATIONS + 1,
                        |with_output| {
                            with_output_flags.push(with_output);
                            background_notification(&format!("notification {index}"))
                        },
                    )
                    .await,
            );
        }
        assert_eq!(
            outcomes[MAX_PENDING_BACKGROUND_NOTIFICATIONS - 1..],
            [
                BackgroundNotificationEnqueue::Queued,
                BackgroundNotificationEnqueue::Dropped,
                BackgroundNotificationEnqueue::Dropped,
            ]
        );
        assert_eq!(
            with_output_flags.len(),
            MAX_PENDING_BACKGROUND_NOTIFICATIONS
        );
        assert_eq!(
            with_output_flags.iter().filter(|flag| **flag).count(),
            DETAILED_PENDING_BACKGROUND_NOTIFICATIONS
        );
        // The dropped notification still asks for a turn.
        assert!(input_queue.has_waking_background_notifications().await);

        let (items, _, _) = input_queue.drain_mailbox_input_items().await;
        assert_eq!(items.len(), MAX_PENDING_BACKGROUND_NOTIFICATIONS + 1);
        assert_eq!(
            items.last(),
            Some(&TurnInput::ResponseItem(ResponseItemEnvelope::new(
                background_notification(
                    "2 more background terminals stopped while too many notifications were pending (sessions 1032, 1033); call write_stdin with one of these session IDs to read the exit code and output of a terminal that exited on its own."
                )
            )))
        );
        assert!(!input_queue.has_pending_queued_input().await);
    }

    /// Fork (R3.4): the summary lists a bounded number of dropped sessions.
    #[test]
    fn dropped_summary_lists_a_bounded_number_of_sessions() {
        let ids = (1000..1000 + MAX_LISTED_DROPPED_SESSIONS as i32).collect::<Vec<_>>();
        let summary = dropped_summary(MAX_LISTED_DROPPED_SESSIONS + 3, &ids);
        assert!(
            summary.starts_with("19 more background terminals stopped while too many notifications were pending (sessions 1000, 1001,"),
            "{summary}"
        );
        assert!(
            summary.contains("1015 and 3 more); call write_stdin"),
            "{summary}"
        );
        assert_eq!(
            dropped_summary(2, &[]),
            "2 more background terminals stopped while too many notifications were pending; call write_stdin with one of these session IDs to read the exit code and output of a terminal that exited on its own."
        );
    }

    /// Fork (R2.10): a poll that showed the exit withdraws exactly its own notification.
    #[tokio::test]
    async fn withdraw_removes_only_the_keyed_notification() {
        let input_queue = InputQueue::new();
        for (key, body) in [(Some(7), "seven"), (Some(8), "eight"), (None, "summary")] {
            input_queue
                .enqueue_background_notification(
                    key,
                    /*session_id*/ None,
                    /*wakes*/ true,
                    |_| background_notification(body),
                )
                .await;
        }
        assert!(input_queue.withdraw_background_notification(7).await);
        assert!(!input_queue.withdraw_background_notification(7).await);
        assert_eq!(
            input_queue.take_background_notifications().await,
            vec![
                background_notification("eight"),
                background_notification("summary")
            ]
        );
    }

    /// Fork (R2.12): a turn that drains a notification runs its release callback; a
    /// withdrawn notification drops it unrun.
    #[tokio::test]
    async fn draining_a_notification_runs_its_release_callback() {
        let input_queue = InputQueue::new();
        let released = Arc::new(std::sync::Mutex::new(Vec::new()));
        for (key, body) in [(1, "sampled"), (2, "withdrawn")] {
            let released = Arc::clone(&released);
            input_queue
                .enqueue_background_notification(
                    Some(key),
                    /*session_id*/ None,
                    /*wakes*/ true,
                    move |_| QueuedBackgroundNotification {
                        item: background_notification(body),
                        on_sampled: Some(Box::new(move || {
                            released.lock().expect("released").push(key);
                        })),
                    },
                )
                .await;
        }
        assert!(input_queue.withdraw_background_notification(2).await);
        assert!(released.lock().expect("released").is_empty());
        let (items, _, _) = input_queue.drain_mailbox_input_items().await;
        assert_eq!(items.len(), 1);
        assert_eq!(*released.lock().expect("released"), vec![1]);
    }

    /// Fork (R3.7): an abort returns the turn's background terminal notifications to the
    /// thread queue without a wake and drops the rest of its input as before.
    #[tokio::test]
    async fn clear_pending_returns_background_notifications_to_the_thread_queue() {
        let input_queue = InputQueue::new();
        input_queue
            .enqueue_background_notification(
                /*key*/ None,
                /*session_id*/ None,
                /*wakes*/ false,
                |_| background_notification("still queued"),
            )
            .await;
        let active_turn = ActiveTurn::default();
        input_queue
            .extend_pending_input_for_turn_state(
                active_turn.turn_state.as_ref(),
                vec![
                    TurnInput::ResponseItem(ResponseItemEnvelope::new(background_notification(
                        "taken by the aborted turn",
                    ))),
                    TurnInput::UserInput {
                        content: vec![UserInput::Text {
                            text: "dropped".to_string(),
                            text_elements: Vec::new(),
                        }],
                        client_id: None,
                        metadata: UserInputMetadata::default(),
                    },
                ],
            )
            .await;

        input_queue.clear_pending(&active_turn).await;

        assert!(active_turn.turn_state.lock().await.pending_input.is_empty());
        assert!(!input_queue.has_waking_background_notifications().await);
        assert_eq!(
            input_queue.take_background_notifications().await,
            vec![
                background_notification("taken by the aborted turn"),
                background_notification("still queued"),
            ]
        );
    }

    #[tokio::test]
    async fn turn_local_background_notification_reports_its_activity() {
        let input_queue = InputQueue::new();
        let turn_state = Mutex::new(TurnState::default());
        input_queue
            .extend_pending_input_for_turn_state(
                &turn_state,
                vec![TurnInput::ResponseItem(ResponseItemEnvelope::new(
                    background_notification("exited"),
                ))],
            )
            .await;
        assert_eq!(
            input_queue.subscribe_activity(Some(&turn_state)).await.1,
            Some(InputQueueActivity::BackgroundTerminal)
        );
    }
}
