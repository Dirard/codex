//! Fork: `codex exec` stays alive while workflow runs of its thread are active
//! (`specs/dynamic-workflow-2026-10-25.md`, UC7 R7.5).
//!
//! When the first turn completes and runs are still working, exec keeps processing the
//! turns their completion notifications start. After `workflows.exec_wait_timeout_ms` it
//! cancels the remaining runs and processes the cancellation's turn for a bounded time.

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
use crate::send_request_with_response;

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
}

impl ExecWorkflowWait {
    pub(crate) fn new(config: &codex_core::config::Config, thread_id: &str) -> Self {
        Self {
            workflows_enabled: config.features.enabled(codex_features::Feature::Workflows),
            timeout: Duration::from_millis(config.workflows.exec_wait_timeout_ms),
            thread_id: thread_id.to_string(),
            wait: None,
        }
    }

    /// Whether exec waits for runs; the deadline branch of the loop is active only then.
    pub(crate) fn fork_is_waiting(&self) -> bool {
        self.wait.is_some()
    }

    /// When the wait times out; a day away while exec does not wait.
    pub(crate) fn fork_deadline(&self) -> Instant {
        self.wait.as_ref().map_or_else(
            || Instant::now() + Duration::from_secs(24 * 60 * 60),
            WorkflowWait::deadline,
        )
    }

    /// The wait timed out: cancels the remaining runs the first time, then gives up.
    /// Returns whether exec stops.
    pub(crate) async fn fork_on_timeout(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> bool {
        let Some(wait) = self.wait.as_mut() else {
            return false;
        };
        if wait.cancelled() {
            warn!("the turn after cancelling workflow runs did not finish in time");
            return true;
        }
        wait.cancel_remaining(client, request_ids, &self.thread_id)
            .await;
        false
    }

    /// Between workflow turns there is nothing to interrupt, so exec stops waiting.
    pub(crate) fn fork_stops_on_interrupt(&self) -> bool {
        self.wait
            .as_ref()
            .is_some_and(|wait| !wait.turn_in_progress)
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
    /// turns while runs of the thread are active (R7.5). Returns whether exec waits.
    pub(crate) async fn fork_keep_waiting(
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
