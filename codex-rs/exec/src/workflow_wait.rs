//! Fork: `codex exec` stays alive while workflow runs of its thread are active
//! (`specs/dynamic-workflow-2026-10-25.md`, UC7 R7.5).
//!
//! When the first turn completes and runs are still working, exec keeps processing the
//! turns their completion notifications start. After `workflows.exec_wait_timeout_ms` it
//! cancels the remaining runs and processes the cancellation's turn for a bounded time.
//!
//! The same hooks also wait for background terminals (`terminal_wait`,
//! `specs/background-exec-notifications-2026-10-25.md` UC7): exec stays while either wait
//! asks for it.

use std::time::Duration;

use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ThreadWorkflowCancelParams;
use codex_app_server_protocol::ThreadWorkflowCancelResponse;
use codex_app_server_protocol::ThreadWorkflowListParams;
use codex_app_server_protocol::ThreadWorkflowListResponse;
use tokio::time::Instant;
use tracing::info;
use tracing::warn;

use crate::RequestIdSequencer;
use crate::event_processor::EventProcessor;
use crate::send_request_with_response;

#[path = "terminal_wait.rs"]
mod terminal_wait;
use terminal_wait::TerminalTick;
use terminal_wait::TerminalWait;

/// How long exec processes the turn started by a cancellation before it exits anyway.
const CANCEL_GRACE: Duration = Duration::from_secs(120);

/// State of exec while it waits for workflow runs.
pub(crate) struct WorkflowWait {
    deadline: Instant,
    cancelled: bool,
    /// A turn started by a workflow notification is being processed.
    pub(crate) turn_in_progress: bool,
}

impl WorkflowWait {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            cancelled: false,
            turn_in_progress: false,
        }
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Whether the wait already cancelled its runs and only finishes their last turn.
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled
    }

    /// R7.5: the wait timed out; cancel the remaining runs and give their turn a grace period.
    pub(crate) async fn cancel_remaining(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
        thread_id: &str,
    ) {
        self.cancelled = true;
        self.deadline = Instant::now() + CANCEL_GRACE;
        for run_id in active_workflow_runs(client, request_ids, thread_id).await {
            warn!(%run_id, "exec wait for workflow runs timed out; cancelling");
            if let Err(err) = send_request_with_response::<ThreadWorkflowCancelResponse>(
                client,
                ClientRequest::ThreadWorkflowCancel {
                    request_id: request_ids.next(),
                    params: ThreadWorkflowCancelParams {
                        thread_id: thread_id.to_string(),
                        run_id: run_id.clone(),
                    },
                },
                "thread/workflow/cancel",
            )
            .await
            {
                warn!(%run_id, "failed to cancel workflow run: {err}");
            }
        }
    }
}

/// The wait of `run_exec_session` for the workflow runs of its primary thread. Its
/// `fork_*` methods are the hooks of the session loop in `lib.rs`.
pub(crate) struct ExecWorkflowWait {
    workflows_enabled: bool,
    timeout: Duration,
    thread_id: String,
    wait: Option<WorkflowWait>,
    /// Background terminals of the thread (UC7 of the background exec notifications spec).
    terminals: TerminalWait,
}

impl ExecWorkflowWait {
    pub(crate) fn new(config: &codex_core::config::Config, thread_id: &str) -> Self {
        Self {
            workflows_enabled: config.features.enabled(codex_features::Feature::Workflows),
            timeout: Duration::from_millis(config.workflows.exec_wait_timeout_ms),
            thread_id: thread_id.to_string(),
            wait: None,
            terminals: TerminalWait::new(config, thread_id),
        }
    }

    /// Whether exec waits for runs or terminals; the deadline branch of the loop is active
    /// only then.
    pub(crate) fn fork_is_waiting(&self) -> bool {
        self.wait.is_some() || self.terminals.is_waiting()
    }

    /// When a timer of a wait fires next; a day away while exec does not wait.
    pub(crate) fn fork_deadline(&self) -> Instant {
        let workflow = self.wait.as_ref().map(WorkflowWait::deadline);
        let terminals = self.terminals.next_wakeup();
        match (workflow, terminals) {
            (Some(workflow), Some(terminals)) => workflow.min(terminals),
            (Some(deadline), None) | (None, Some(deadline)) => deadline,
            (None, None) => Instant::now() + Duration::from_secs(24 * 60 * 60),
        }
    }

    /// A timer fired. The workflow wait cancels the remaining runs the first time it times
    /// out, then gives up; the terminal wait checks the thread again or gives up after its
    /// timeout with a warning and an error exit (UC7, R7.2, R7.3). Returns whether exec stops.
    pub(crate) async fn fork_on_timeout(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
        event_processor: &mut dyn EventProcessor,
        error_seen: &mut bool,
    ) -> bool {
        let now = Instant::now();
        if self.terminals.is_waiting()
            && self.terminals.next_wakeup().is_some_and(|at| at <= now)
            && self
                .terminals
                .on_tick(client, request_ids, event_processor, error_seen)
                .await
                == TerminalTick::Done
            && self.wait.is_none()
        {
            return true;
        }
        let Some(wait) = self.wait.as_mut() else {
            return !self.terminals.is_waiting();
        };
        if wait.deadline() > now {
            return false;
        }
        if wait.cancelled() {
            warn!("the turn after cancelling workflow runs did not finish in time");
            return true;
        }
        wait.cancel_remaining(client, request_ids, &self.thread_id)
            .await;
        false
    }

    /// Between workflow or terminal turns there is nothing to interrupt, so exec stops
    /// waiting.
    pub(crate) fn fork_stops_on_interrupt(&self) -> bool {
        let workflow_idle = self
            .wait
            .as_ref()
            .is_some_and(|wait| !wait.turn_in_progress);
        let terminals_idle = self.terminals.is_waiting() && !self.terminals.turn_in_progress();
        (self.wait.is_none() || workflow_idle)
            && (!self.terminals.is_waiting() || terminals_idle)
            && (self.wait.is_some() || self.terminals.is_waiting())
    }

    /// Whether exec drops `notification`. While exec waits, a turn started on its thread
    /// by a workflow notification becomes the task (R7.5).
    pub(crate) fn fork_skip_notification(
        &mut self,
        notification: &codex_app_server_protocol::ServerNotification,
        task_id: &mut String,
    ) -> bool {
        // Exec's own turn backfill reads paginated threads with `includeTurns`; while it
        // waits for workflow turns, the resulting notice would reach the output although
        // the user cannot act on it.
        if is_backfill_deprecation_notice(notification) {
            return true;
        }
        self.terminals.observe(notification, task_id);
        if let Some(wait) = self.wait.as_mut()
            && let codex_app_server_protocol::ServerNotification::TurnStarted(payload) =
                notification
            && payload.thread_id == self.thread_id
        {
            *task_id = payload.turn.id.clone();
            wait.turn_in_progress = true;
        }
        false
    }

    /// Instead of shutting down after a turn, keeps processing workflow notification
    /// turns while runs of the thread are active (R7.5), and background terminal turns
    /// while the thread still owes the model a terminal notification (background exec
    /// notifications UC7, R7.1). Returns whether exec waits.
    pub(crate) async fn fork_keep_waiting(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> bool {
        let terminals = self.terminals.keep_waiting(client, request_ids).await;
        let workflows = self.keep_waiting_for_workflows(client, request_ids).await;
        terminals || workflows
    }

    async fn keep_waiting_for_workflows(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> bool {
        if !(self.workflows_enabled
            && self.wait.as_ref().is_none_or(|wait| !wait.cancelled())
            && !active_workflow_runs(client, request_ids, &self.thread_id)
                .await
                .is_empty())
        {
            return false;
        }
        let timeout = self.timeout;
        let wait = self.wait.get_or_insert_with(|| WorkflowWait::new(timeout));
        wait.turn_in_progress = false;
        true
    }
}

/// The deprecation notice app-server sends for exec's own `thread/read` backfill.
pub(crate) fn is_backfill_deprecation_notice(
    notification: &codex_app_server_protocol::ServerNotification,
) -> bool {
    matches!(
        notification,
        codex_app_server_protocol::ServerNotification::DeprecationNotice(notice)
            if notice.summary.contains("omit `includeTurns`")
    )
}

/// Ids of the thread's runs that have not finished.
pub(crate) async fn active_workflow_runs(
    client: &InProcessAppServerClient,
    request_ids: &mut RequestIdSequencer,
    thread_id: &str,
) -> Vec<String> {
    match send_request_with_response::<ThreadWorkflowListResponse>(
        client,
        ClientRequest::ThreadWorkflowList {
            request_id: request_ids.next(),
            params: ThreadWorkflowListParams {
                thread_id: thread_id.to_string(),
            },
        },
        "thread/workflow/list",
    )
    .await
    {
        Ok(response) => {
            let active: Vec<String> = response
                .runs
                .into_iter()
                .filter(|run| !run.status.is_terminal())
                .map(|run| run.run_id)
                .collect();
            if !active.is_empty() {
                info!(runs = ?active, "waiting for workflow runs before exiting");
            }
            active
        }
        Err(err) => {
            warn!("thread/workflow/list failed: {err}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::DeprecationNoticeNotification;
    use codex_app_server_protocol::ServerNotification;

    #[test]
    fn only_the_backfill_notice_is_dropped() {
        let notice = |summary: &str| {
            ServerNotification::DeprecationNotice(DeprecationNoticeNotification {
                summary: summary.to_string(),
                details: None,
            })
        };
        assert!(is_backfill_deprecation_notice(&notice(
            "Full-history hydration is deprecated for paginated threads; omit `includeTurns` or set it to `false`, then page with `thread/turns/list` and `thread/items/list`."
        )));
        assert!(!is_backfill_deprecation_notice(&notice(
            "Some other deprecation."
        )));
    }
}
