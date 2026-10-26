//! Fork: app side of the `/workflows` screen (`specs/dynamic-workflow-2026-10-25.md`, UC 8.1).
//!
//! `thread/workflow/*` requests run in background tasks, as the agent picker refresh does,
//! because `thread/workflow/cancel` can hold the thread's request queue for up to 30 seconds.
//! Their answers come back as `AppEvent::Workflows` and go to the chat widget, which owns the
//! views.

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadWorkflowCancelParams;
use codex_app_server_protocol::ThreadWorkflowCancelResponse;
use codex_app_server_protocol::ThreadWorkflowListParams;
use codex_app_server_protocol::ThreadWorkflowListResponse;
use codex_app_server_protocol::ThreadWorkflowReadParams;
use codex_app_server_protocol::ThreadWorkflowReadResponse;
use codex_protocol::ThreadId;
use uuid::Uuid;

use super::App;
use crate::app_event::AppEvent;
use crate::app_server_session::AppServerSession;
use crate::workflows::RunLoadOrigin;
use crate::workflows::WorkflowsEvent;

impl App {
    /// UC 8.1: route a `/workflows` event. User actions start background requests; their
    /// answers go to the chat widget (R8.6.2, R8.6.3, R8.6.5, R8.6.6).
    pub(super) fn handle_workflows_event(
        &mut self,
        app_server: &AppServerSession,
        event: WorkflowsEvent,
    ) {
        match event {
            WorkflowsEvent::OpenList => {
                // Q35: runs live on the session's root thread, not on the agent being watched.
                let Some(thread_id) = self
                    .primary_thread_id
                    .or_else(|| self.current_displayed_thread_id())
                else {
                    self.chat_widget.add_info_message(
                        "No workflow runs yet.".to_string(),
                        Some("Start a conversation first.".to_string()),
                    );
                    return;
                };
                self.spawn_workflow_list(app_server, thread_id);
            }
            WorkflowsEvent::OpenRun { thread_id, run_id } => {
                self.spawn_workflow_read(app_server, thread_id, run_id, RunLoadOrigin::Open);
            }
            WorkflowsEvent::Refresh { thread_id, run_id } => {
                self.spawn_workflow_read(app_server, thread_id, run_id, RunLoadOrigin::Refresh);
            }
            WorkflowsEvent::Cancel { thread_id, run_id } => {
                self.chat_widget
                    .note_workflow_cancel_started(thread_id, &run_id);
                self.spawn_workflow_cancel(app_server, thread_id, run_id);
            }
            WorkflowsEvent::ListLoaded { thread_id, result } => {
                self.chat_widget.on_workflows_list_loaded(thread_id, result);
            }
            WorkflowsEvent::RunLoaded {
                thread_id,
                run_id,
                origin,
                result,
            } => self
                .chat_widget
                .on_workflow_run_loaded(thread_id, run_id, origin, result),
        }
    }

    fn spawn_workflow_list(&self, app_server: &AppServerSession, thread_id: ThreadId) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = request_handle
                .request_typed::<ThreadWorkflowListResponse>(ClientRequest::ThreadWorkflowList {
                    request_id: RequestId::String(Uuid::new_v4().to_string()),
                    params: ThreadWorkflowListParams {
                        thread_id: thread_id.to_string(),
                    },
                })
                .await
                .map(|response| response.runs)
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::Workflows(WorkflowsEvent::ListLoaded {
                thread_id,
                result,
            }));
        });
    }

    fn spawn_workflow_read(
        &self,
        app_server: &AppServerSession,
        thread_id: ThreadId,
        run_id: String,
        origin: RunLoadOrigin,
    ) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = request_handle
                .request_typed::<ThreadWorkflowReadResponse>(ClientRequest::ThreadWorkflowRead {
                    request_id: RequestId::String(Uuid::new_v4().to_string()),
                    params: ThreadWorkflowReadParams {
                        thread_id: thread_id.to_string(),
                        run_id: run_id.clone(),
                        include_result: false,
                    },
                })
                .await
                .map(|response| response.run)
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::Workflows(WorkflowsEvent::RunLoaded {
                thread_id,
                run_id,
                origin,
                result,
            }));
        });
    }

    fn spawn_workflow_cancel(
        &self,
        app_server: &AppServerSession,
        thread_id: ThreadId,
        run_id: String,
    ) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = request_handle
                .request_typed::<ThreadWorkflowCancelResponse>(
                    ClientRequest::ThreadWorkflowCancel {
                        request_id: RequestId::String(Uuid::new_v4().to_string()),
                        params: ThreadWorkflowCancelParams {
                            thread_id: thread_id.to_string(),
                            run_id: run_id.clone(),
                        },
                    },
                )
                .await
                .map(|response| response.run)
                .map_err(|err| err.to_string());
            app_event_tx.send(AppEvent::Workflows(WorkflowsEvent::RunLoaded {
                thread_id,
                run_id,
                origin: RunLoadOrigin::Cancel,
                result,
            }));
        });
    }
}
