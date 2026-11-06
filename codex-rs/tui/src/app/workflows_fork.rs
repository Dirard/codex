//! Fork: app side of the `/workflows` screen (`specs/dynamic-workflow-2026-10-25.md`, UC 8.1).
//!
//! `thread/workflow/*` requests run in background tasks, as the agent picker refresh does,
//! because `thread/workflow/cancel` can hold the thread's request queue for up to 30 seconds.
//! Their answers come back as `AppEvent::Workflows` and go to the chat widget, which owns the
//! views. The `/subagents` hooks group workflow agents under their run (R8.6.7).

use std::collections::BTreeMap;
use std::time::Duration;

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItemPayload;
use codex_app_server_protocol::ThreadWorkflowCancelParams;
use codex_app_server_protocol::ThreadWorkflowCancelResponse;
use codex_app_server_protocol::ThreadWorkflowListParams;
use codex_app_server_protocol::ThreadWorkflowListResponse;
use codex_app_server_protocol::ThreadWorkflowReadParams;
use codex_app_server_protocol::ThreadWorkflowReadResponse;
use codex_app_server_protocol::WorkflowRunStatus;
use codex_features::Feature;
use codex_protocol::ThreadId;
use uuid::Uuid;

use super::App;
use super::agent_picker::AGENT_PICKER_VIEW_ID;
use super::thread_events::ThreadBufferedEvent;
use crate::app_event::AppEvent;
use crate::app_server_session::AppServerSession;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::workflows::RunLoadOrigin;
use crate::workflows::WorkflowsEvent;
use crate::workflows::picker::PickerLayout;
use crate::workflows::view::one_line;
use crate::workflows::view::run_mark;
use crate::workflows::view::status_text;

/// Name and, when known, status of a run for its `/subagents` row.
struct RunLabel {
    name: String,
    status: Option<WorkflowRunStatus>,
}

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
                // Updates that arrive before the answer must not lose to it (R8.6.6).
                self.chat_widget.note_workflows_list_requested(thread_id);
                self.spawn_workflow_list(app_server, thread_id);
            }
            WorkflowsEvent::OpenRun { thread_id, run_id } => {
                // A late answer opens the run view only while the user still waits for it.
                self.chat_widget
                    .note_workflow_open_requested(thread_id, &run_id);
                self.spawn_workflow_read(
                    app_server,
                    thread_id,
                    run_id,
                    RunLoadOrigin::Open,
                    Duration::ZERO,
                );
            }
            WorkflowsEvent::Refresh {
                thread_id,
                run_id,
                delay,
            } => {
                self.spawn_workflow_read(
                    app_server,
                    thread_id,
                    run_id,
                    RunLoadOrigin::Refresh,
                    delay,
                );
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
        delay: Duration,
    ) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
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

    /// R8.6.7: the grouped layout of the rows `agent_picker_selection_view_params` just recorded
    /// in `picker_row_ids`, or `None` when the upstream rows stay as they are (feature disabled
    /// or no workflow agents).
    fn fork_workflow_picker_layout(&self) -> Option<PickerLayout> {
        if !self.config.features.enabled(Feature::Workflows) {
            return None;
        }
        PickerLayout::build(
            self.agent_navigation
                .picker_row_ids
                .iter()
                .map(|thread_id| {
                    self.agent_navigation
                        .get(thread_id)
                        .and_then(|entry| entry.agent_path.as_deref())
                }),
        )
    }

    /// Hook of `agent_picker_selection_view_params`, after it recorded `picker_row_ids`: reorder
    /// the rows (one per `picker_row_ids` entry) into the grouped layout and translate the
    /// upstream selection, a `picker_row_ids` index, into its display row. The layout is kept
    /// as the one of the rendered picker: an agent path that arrives while the picker is open
    /// does not move the rows until the next rebuild, and the hooks of `refresh_agent_picker`
    /// must read the cursor through the rows on screen.
    pub(super) fn fork_group_workflow_picker_items(
        &mut self,
        items: Vec<SelectionItem>,
        initial: Option<usize>,
    ) -> (Vec<SelectionItem>, Option<usize>) {
        let Some(layout) = self.fork_workflow_picker_layout() else {
            self.agent_navigation.fork_picker_layout = None;
            return (items, initial);
        };
        let labels = self.workflow_run_labels();
        let primary_thread_id = self.primary_thread_id;
        let row_ids = &self.agent_navigation.picker_row_ids;
        let pending = &self.agent_navigation.pending_approvals;
        let items = layout.group_items(
            items,
            |row| row_ids.get(row).is_some_and(|id| pending.contains(id)),
            |run_id, children| {
                workflow_run_row(primary_thread_id, run_id, children, labels.get(run_id))
            },
        );
        let initial = initial.and_then(|row| layout.display_index(row));
        self.agent_navigation.fork_picker_layout = Some(layout);
        (items, initial)
    }

    /// Hook of `refresh_agent_picker`: the `picker_row_ids` index shown at row `display` of the
    /// rendered picker; `None` for a run row.
    pub(super) fn fork_picker_visible_index(&self, display: usize) -> Option<usize> {
        match &self.agent_navigation.fork_picker_layout {
            Some(layout) => layout.visible_index(display),
            None => Some(display),
        }
    }

    /// Hook of `refresh_agent_picker`, before it rebuilds the rows: the run whose row has the
    /// cursor. Upstream keeps the cursor by thread id, which a run row has not (review
    /// 2026-10-27, item 8).
    pub(super) fn fork_selected_picker_run(&self) -> Option<String> {
        let display = self
            .chat_widget
            .selected_index_for_present_view(AGENT_PICKER_VIEW_ID)?;
        self.agent_navigation
            .fork_picker_layout
            .as_ref()?
            .run_at(display)
            .map(str::to_string)
    }

    /// Hook of `refresh_agent_picker`, after it rebuilt the rows: put the cursor back on the run
    /// row it was on, while the picker still holds an agent of that run.
    pub(super) fn fork_reselect_picker_run(
        &self,
        mut params: SelectionViewParams,
        run_id: Option<String>,
    ) -> SelectionViewParams {
        if let Some(row) = run_id.and_then(|run_id| {
            self.agent_navigation
                .fork_picker_layout
                .as_ref()?
                .run_row(&run_id)
        }) {
            params.initial_selected_idx = Some(row);
        }
        params
    }

    /// Q38: run names and statuses from the primary thread's stored events, then from the
    /// chat widget's live runs; later sources win.
    fn workflow_run_labels(&self) -> BTreeMap<String, RunLabel> {
        let mut labels = BTreeMap::new();
        let mut note = |run_id: &str, name: &str, status: WorkflowRunStatus| {
            labels.insert(
                run_id.to_string(),
                RunLabel {
                    name: name.to_string(),
                    status: Some(status),
                },
            );
        };
        if let Some(store) = self
            .primary_thread_id
            .and_then(|thread_id| self.thread_event_channels.get(&thread_id))
            .and_then(|channel| channel.store.try_lock().ok())
        {
            for item in store.turns.iter().flat_map(|turn| turn.items.iter()) {
                if let ThreadItemPayload::Workflow(item) = &item.payload {
                    note(&item.run_id, &item.name, item.status);
                }
            }
            for event in &store.buffer {
                let ThreadBufferedEvent::Notification(notification) = event else {
                    continue;
                };
                match notification.as_ref() {
                    ServerNotification::ItemStarted(started) => {
                        if let ThreadItemPayload::Workflow(item) = &started.item.payload {
                            note(&item.run_id, &item.name, item.status);
                        }
                    }
                    ServerNotification::ItemCompleted(completed) => {
                        if let ThreadItemPayload::Workflow(item) = &completed.item.payload {
                            note(&item.run_id, &item.name, item.status);
                        }
                    }
                    ServerNotification::ThreadWorkflowUpdated(updated) => {
                        note(&updated.run.run_id, &updated.run.name, updated.run.status);
                    }
                    _ => {}
                }
            }
        }
        if self.chat_widget.thread_id() == self.primary_thread_id {
            for (run_id, name, status) in self.chat_widget.known_workflow_runs() {
                note(&run_id, &name, status);
            }
        }
        labels
    }
}

/// The `/subagents` row of a run: opens its run view (R8.6.7).
fn workflow_run_row(
    primary_thread_id: Option<ThreadId>,
    run_id: &str,
    children: usize,
    label: Option<&RunLabel>,
) -> SelectionItem {
    let name = format!(
        "Workflow {}",
        label.map_or_else(|| run_id.to_string(), |label| one_line(&label.name))
    );
    let mut description = vec![run_id.to_string()];
    if let Some(status) = label.and_then(|label| label.status) {
        description.push(status_text(status, /*cancelling*/ false));
    }
    description.push(format!(
        "{children} {}",
        if children == 1 { "agent" } else { "agents" }
    ));
    let name_prefix_spans = match label.and_then(|label| label.status) {
        Some(status) => vec![run_mark(status), " ".into()],
        None => vec!["•".into(), " ".into()],
    };
    let actions: Vec<crate::bottom_pane::SelectionAction> = match primary_thread_id {
        Some(thread_id) => {
            let run_id = run_id.to_string();
            vec![Box::new(move |tx| {
                tx.send(AppEvent::Workflows(WorkflowsEvent::OpenRun {
                    thread_id,
                    run_id: run_id.clone(),
                }));
            })]
        }
        None => Vec::new(),
    };
    SelectionItem {
        search_value: Some(format!("{name} {run_id}")),
        name,
        name_prefix_spans,
        description: Some(description.join(" · ")),
        is_disabled: actions.is_empty(),
        actions,
        dismiss_on_select: true,
        ..Default::default()
    }
}

#[cfg(test)]
#[path = "workflows_fork_tests.rs"]
mod tests;
