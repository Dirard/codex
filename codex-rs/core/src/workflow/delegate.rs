//! Host callbacks of a workflow cell (UC2, R2.2).
//!
//! Unlike `exec` cells, a workflow cell does not dispatch through the per-sampling-request
//! worker: its delegate serves the cell's host calls itself, so the run makes progress while
//! the parent thread is idle, sampling or compacting.

use std::sync::Arc;

use codex_code_mode::CellId;
use codex_code_mode::CodeModeNestedToolCall;
use codex_code_mode::CodeModeSessionDelegate;
use codex_code_mode::NotificationFuture;
use codex_code_mode::ToolInvocationFuture;
use codex_workflow::TOOL_AGENT;
use codex_workflow::TOOL_BUDGET;
use codex_workflow::TOOL_EVENT;
use tokio_util::sync::CancellationToken;

use super::agent_call::run_agent_call;
use super::run::WorkflowRun;
use crate::session::session::Session;
use crate::session::step_context::StepContext;

/// Delegate of one run's cell. It keeps the step captured by the `workflow` call, so agents
/// inherit that step's settings (UC10).
pub(super) struct WorkflowCellDelegate {
    session: Arc<Session>,
    step_context: Arc<StepContext>,
    run: Arc<WorkflowRun>,
}

impl WorkflowCellDelegate {
    pub(super) fn new(
        session: Arc<Session>,
        step_context: Arc<StepContext>,
        run: Arc<WorkflowRun>,
    ) -> Self {
        Self {
            session,
            step_context,
            run,
        }
    }
}

impl CodeModeSessionDelegate for WorkflowCellDelegate {
    fn invoke_tool<'a>(
        &'a self,
        invocation: CodeModeNestedToolCall,
        cancellation_token: CancellationToken,
    ) -> ToolInvocationFuture<'a> {
        Box::pin(async move {
            match invocation.tool_name.name.as_str() {
                TOOL_AGENT => Ok(run_agent_call(
                    &self.session,
                    &self.step_context,
                    &self.run,
                    invocation.input,
                    cancellation_token,
                )
                .await),
                TOOL_EVENT => self.run.record_event(invocation.input),
                TOOL_BUDGET => Ok(self.run.budget_json()),
                other => Err(format!("unknown workflow host call `{other}`")),
            }
        })
    }

    fn notify<'a>(
        &'a self,
        _call_id: String,
        _cell_id: CellId,
        _text: String,
        _cancellation_token: CancellationToken,
    ) -> NotificationFuture<'a> {
        // The prelude removes `notify`; nothing reaches the parent from the cell.
        Box::pin(async { Ok(()) })
    }

    fn cell_closed(&self, _cell_id: &CellId) {}
}
