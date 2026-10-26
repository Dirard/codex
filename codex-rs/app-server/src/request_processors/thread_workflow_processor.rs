//! Fork: `thread/workflow/*` requests over the dynamic workflow runs of a loaded thread
//! (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.2, UC9 R9.1).

use std::sync::Arc;

use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadWorkflowCancelParams;
use codex_app_server_protocol::ThreadWorkflowCancelResponse;
use codex_app_server_protocol::ThreadWorkflowListParams;
use codex_app_server_protocol::ThreadWorkflowListResponse;
use codex_app_server_protocol::ThreadWorkflowReadParams;
use codex_app_server_protocol::ThreadWorkflowReadResponse;
use codex_app_server_protocol::WorkflowRun;
use codex_core::CodexThread;
use codex_core::ThreadManager;
use codex_protocol::ThreadId;

use crate::error_code::invalid_request;

pub(crate) struct ThreadWorkflowRequestProcessor {
    thread_manager: Arc<ThreadManager>,
}

impl ThreadWorkflowRequestProcessor {
    pub(crate) fn new(thread_manager: Arc<ThreadManager>) -> Self {
        Self { thread_manager }
    }

    pub(crate) async fn list(
        &self,
        params: ThreadWorkflowListParams,
    ) -> Result<ThreadWorkflowListResponse, JSONRPCErrorError> {
        let thread = self.loaded_thread(&params.thread_id).await?;
        Ok(ThreadWorkflowListResponse {
            runs: thread
                .workflow_runs()
                .into_iter()
                .map(|snapshot| WorkflowRun::from_snapshot(snapshot, /*include_agents*/ false))
                .collect(),
        })
    }

    pub(crate) async fn read(
        &self,
        params: ThreadWorkflowReadParams,
    ) -> Result<ThreadWorkflowReadResponse, JSONRPCErrorError> {
        let thread = self.loaded_thread(&params.thread_id).await?;
        let (snapshot, result) = thread
            .workflow_run(&params.run_id, params.include_result)
            .ok_or_else(|| run_not_found(&params.run_id))?;
        Ok(ThreadWorkflowReadResponse {
            run: WorkflowRun::from_snapshot(snapshot, /*include_agents*/ true),
            result,
        })
    }

    /// Waits until the run settles (core bounds the wait); a finished run is unchanged.
    pub(crate) async fn cancel(
        &self,
        params: ThreadWorkflowCancelParams,
    ) -> Result<ThreadWorkflowCancelResponse, JSONRPCErrorError> {
        let thread = self.loaded_thread(&params.thread_id).await?;
        let snapshot = thread
            .cancel_workflow_run(&params.run_id)
            .await
            .ok_or_else(|| run_not_found(&params.run_id))?;
        Ok(ThreadWorkflowCancelResponse {
            run: WorkflowRun::from_snapshot(snapshot, /*include_agents*/ false),
        })
    }

    /// Runs live with the thread's session, so only a loaded thread has them.
    async fn loaded_thread(
        &self,
        raw_thread_id: &str,
    ) -> Result<Arc<CodexThread>, JSONRPCErrorError> {
        let thread_id = ThreadId::from_string(raw_thread_id)
            .map_err(|error| invalid_request(format!("invalid thread id: {error}")))?;
        self.thread_manager
            .get_thread(thread_id)
            .await
            .map_err(|_| invalid_request(format!("thread not loaded: {thread_id}")))
    }
}

fn run_not_found(run_id: &str) -> JSONRPCErrorError {
    invalid_request(format!("workflow run not found: {run_id}"))
}
