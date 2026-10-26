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
