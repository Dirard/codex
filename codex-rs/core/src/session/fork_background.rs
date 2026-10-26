//! Fork: background work around turns and shutdown: background terminal exit notifications
//! (specs/background-exec-notifications-2026-10-25.md), finished workflow runs
//! (specs/dynamic-workflow-2026-10-25.md) and agent worktrees
//! (specs/agent-worktree-isolation-2026-10-25.md).

use codex_protocol::models::ResponseItem;
use codex_protocol::turn_input::TurnStartOptions;

use super::TurnInput;
use super::session::Session;
use crate::state::TaskKind;
use crate::unified_exec::TerminatedProcesses;

impl Session {
    /// Fork: drains the thread queue for a starting task. Background notifications (terminal
    /// exits, finished workflow runs) describe events that happened before this turn, so a
    /// regular turn samples them with its own input instead of one step later. Other tasks
    /// leave them in the thread queue, with their wake: they would record a waking
    /// notification without a model step, and an abort would drop it (background-exec R3.8,
    /// dynamic workflow R7.7). Returns the pending items and the task input.
    pub(crate) async fn fork_drain_task_input(
        &self,
        task_kind: TaskKind,
        mut input: Vec<TurnInput>,
    ) -> (Vec<TurnInput>, Vec<TurnInput>) {
        let take_background_notifications = task_kind == TaskKind::Regular;
        let (mut pending_items, _, _) = self
            .input_queue
            .drain_mailbox_input_items_for_task(take_background_notifications)
            .await;
        if take_background_notifications && !input.is_empty() {
            let (notifications, rest): (Vec<_>, Vec<_>) =
                pending_items.into_iter().partition(|item| {
                    matches!(item, TurnInput::ResponseItem(envelope)
                    if crate::context::is_background_notification_item(&envelope.item))
                });
            pending_items = rest;
            input.splice(0..0, notifications);
        }
        (pending_items, input)
    }

    /// Fork: after the shutdown stopped the session's processes, records what their armed
    /// background terminals owe the model while persistence is still open, so a resumed thread
    /// learns what happened to them (background-exec-notifications R2.5); then interrupts
    /// workflow runs and their agents (dynamic workflow, R9.3).
    pub(crate) async fn fork_on_processes_terminated(&self, terminated: TerminatedProcesses) {
        self.record_background_terminals_at_shutdown(terminated)
            .await;
        if let Some(workflows) = crate::workflow::WorkflowRegistry::existing(self) {
            workflows.shutdown().await;
        }
    }

    /// Fork: with the session's processes stopped, finalizes agent worktrees once the agent
    /// tree stops; eviction alone keeps them (spec agent-worktree-isolation UC6).
    pub(crate) async fn fork_finalize_agent_worktrees(&self) {
        let runtime = &self.services.local_agent_runtime;
        runtime
            .worktrees
            .session_stopped(
                self.thread_id,
                runtime.is_tree_root(self.thread_id),
                runtime.shutdown.is_cancelled(),
            )
            .await;
    }
}

/// Fork: names a queue-only wake by a finished workflow run (dynamic workflow, UC7) or by a
/// background terminal exit (UC3, R3.1).
pub(crate) fn name_wake_trigger(input: &[TurnInput], start_options: &mut TurnStartOptions) {
    let notified = |matches: fn(&ResponseItem) -> bool| {
        input.iter().any(
            |item| matches!(item, TurnInput::ResponseItem(envelope) if matches(&envelope.item)),
        )
    };
    if notified(crate::context::WorkflowNotification::is_notification_item) {
        start_options.turn_trigger = Some(crate::workflow::WORKFLOW_TURN_TRIGGER.to_string());
    } else if notified(crate::context::BackgroundTerminalNotification::is_notification_item) {
        start_options.turn_trigger =
            Some(crate::session::BACKGROUND_TERMINAL_TURN_TRIGGER.to_string());
    }
}
