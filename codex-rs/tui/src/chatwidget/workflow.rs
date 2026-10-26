//! Fork: workflow display (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.4 and UC 8.1).
//!
//! The status line shows active runs of the shown thread; the turn items of a run's start and
//! of its completion notification become history cells, live and on replay. The `/workflows`
//! screen keeps its state here because run updates reach the widget of the shown thread; the
//! app-server requests behind it live in `app/workflows_fork.rs`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use codex_app_server_protocol::ThreadWorkflowUpdatedNotification;
use codex_app_server_protocol::WorkflowItem;
use codex_app_server_protocol::WorkflowRun;
use codex_app_server_protocol::WorkflowRunStatus;
use codex_features::Feature;
use codex_protocol::ThreadId;

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::workflows::RunLoadOrigin;
use crate::workflows::WorkflowsEvent;
use crate::workflows::view::RUN_LIST_VIEW_ID;
use crate::workflows::view::RUN_VIEW_ID;
use crate::workflows::view::RunViewRow;
use crate::workflows::view::run_list_params;
use crate::workflows::view::run_view_params;
use crate::workflows::view::run_view_rows;
use crate::workflows::view::sorted_runs;

/// Workflow state of a chat widget: active runs for the status line and the `/workflows`
/// screen once it was opened.
#[derive(Default)]
pub(super) struct WorkflowState {
    /// R8.4: non-terminal runs of the shown thread by run id.
    active: BTreeMap<String, WorkflowRun>,
    /// UC 8.1: runs and the open run of the screen.
    screen: Option<WorkflowScreen>,
    /// Tests render durations against a fixed clock.
    fixed_now_ms: Option<i64>,
}

/// UC 8.1, type `WorkflowScreen`.
struct WorkflowScreen {
    thread_id: ThreadId,
    runs: BTreeMap<String, WorkflowRun>,
    /// Run ids in the order the list view shows them.
    list_order: Vec<String>,
    open_run: Option<OpenRun>,
    cancelling: BTreeSet<String>,
}

/// UC 8.1, type `OpenRun`.
struct OpenRun {
    run: WorkflowRun,
    /// Rows in the order the run view shows them.
    rows: Vec<RunViewRow>,
    refresh_in_flight: bool,
    refresh_pending: bool,
}

impl WorkflowScreen {
    fn new(thread_id: ThreadId) -> Self {
        Self {
            thread_id,
            runs: BTreeMap::new(),
            list_order: Vec::new(),
            open_run: None,
            cancelling: BTreeSet::new(),
        }
    }
}

impl WorkflowState {
    fn now_ms(&self) -> i64 {
        self.fixed_now_ms.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
                .unwrap_or_default()
        })
    }

    /// The screen of `thread_id`, replacing a screen of another thread.
    fn screen_for(&mut self, thread_id: ThreadId) -> &mut WorkflowScreen {
        if self
            .screen
            .as_ref()
            .is_none_or(|screen| screen.thread_id != thread_id)
        {
            self.screen = Some(WorkflowScreen::new(thread_id));
        }
        self.screen
            .get_or_insert_with(|| WorkflowScreen::new(thread_id))
    }

    fn screen_of(&mut self, thread_id: ThreadId) -> Option<&mut WorkflowScreen> {
        self.screen
            .as_mut()
            .filter(|screen| screen.thread_id == thread_id)
    }

    #[cfg(test)]
    pub(super) fn set_fixed_now_ms(&mut self, now_ms: i64) {
        self.fixed_now_ms = Some(now_ms);
    }
}

impl ChatWidget {
    /// R8.4: follow run snapshots of the shown thread in the status line; UC 8.1: keep an open
    /// `/workflows` screen current (R8.6.6).
    pub(super) fn on_workflow_updated(&mut self, notification: ThreadWorkflowUpdatedNotification) {
        if self
            .thread_id
            .is_some_and(|thread_id| thread_id.to_string() != notification.thread_id)
        {
            return;
        }
        let run = notification.run;
        if run.status.is_terminal() {
            self.workflows.active.remove(&run.run_id);
        } else {
            self.workflows
                .active
                .insert(run.run_id.clone(), run.clone());
        }
        self.bottom_pane
            .set_workflow_status_indicator(workflow_status_indicator(&self.workflows.active));
        if let Some(thread_id) = self.thread_id {
            self.apply_workflow_screen_update(thread_id, run);
        }
    }

    /// R8.4: history cells for a run's start and for its completion notification.
    pub(super) fn on_workflow_item(&mut self, item: WorkflowItem) {
        if item.status.is_terminal() {
            self.workflows.active.remove(&item.run_id);
            self.bottom_pane
                .set_workflow_status_indicator(workflow_status_indicator(&self.workflows.active));
            let summary = item
                .notification
                .as_deref()
                .and_then(|text| text.lines().next())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "Workflow \"{}\" ({}) {}",
                        item.name,
                        item.run_id,
                        status_label(item.status)
                    )
                });
            self.add_info_message(summary, /*hint*/ None);
        } else {
            self.add_info_message(
                format!(
                    "Workflow \"{}\" ({}) started in the background",
                    item.name, item.run_id
                ),
                Some("Codex continues when its notification arrives.".to_string()),
            );
        }
    }

    /// R8.6.1: `/workflows` exists only while `features.workflows` is enabled.
    pub(crate) fn fork_sync_workflows_command(&mut self) {
        self.bottom_pane
            .set_workflows_command_enabled(self.config.features.enabled(Feature::Workflows));
    }

    /// R8.6.1: a runtime toggle of `features.workflows` shows or hides `/workflows`.
    pub(super) fn fork_on_feature_toggled(&mut self, feature: Feature) {
        if feature == Feature::Workflows {
            self.fork_sync_workflows_command();
        }
    }

    /// R8.6.2: `/workflows` asks the app to list the runs of the session's primary thread.
    pub(super) fn open_workflows(&mut self) {
        if !self.config.features.enabled(Feature::Workflows) {
            return;
        }
        self.app_event_tx
            .send(AppEvent::Workflows(WorkflowsEvent::OpenList));
    }

    /// UC 8.1, "список run'ов": show the answer of `thread/workflow/list` (R8.6.2).
    pub(crate) fn on_workflows_list_loaded(
        &mut self,
        thread_id: ThreadId,
        result: Result<Vec<WorkflowRun>, String>,
    ) {
        let runs = match result {
            Ok(runs) => runs,
            Err(err) => {
                self.add_error_message(format!("Failed to load workflow runs: {err}"));
                return;
            }
        };
        if runs.is_empty() {
            self.add_info_message(
                "No workflow runs in this thread.".to_string(),
                Some("Codex starts them with the workflow tool.".to_string()),
            );
            return;
        }
        let screen = self.workflows.screen_for(thread_id);
        screen.runs = runs
            .into_iter()
            .map(|run| (run.run_id.clone(), run))
            .collect();
        if !self.replace_workflow_list_view()
            && let Some(params) = self.workflow_list_params()
        {
            self.show_selection_view(params);
        }
    }

    /// UC 8.1, "карточка run'а": apply an answer of `thread/workflow/read` or of
    /// `thread/workflow/cancel` (R8.6.3, R8.6.5, R8.6.6).
    pub(crate) fn on_workflow_run_loaded(
        &mut self,
        thread_id: ThreadId,
        run_id: String,
        origin: RunLoadOrigin,
        result: Result<WorkflowRun, String>,
    ) {
        match origin {
            RunLoadOrigin::Open => match result {
                Ok(run) => self.open_workflow_run(thread_id, run),
                Err(err) => {
                    self.add_error_message(format!("Failed to read workflow run {run_id}: {err}"));
                }
            },
            RunLoadOrigin::Refresh => {
                let Some(screen) = self.workflows.screen_of(thread_id) else {
                    return;
                };
                let Some(open) = screen
                    .open_run
                    .as_mut()
                    .filter(|open| open.run.run_id == run_id)
                else {
                    return;
                };
                open.refresh_in_flight = false;
                match result {
                    Ok(run) => open.run = run,
                    Err(err) => tracing::warn!(%err, run_id, "failed to refresh a workflow run"),
                }
                let send_again = std::mem::take(&mut open.refresh_pending);
                if self.replace_workflow_run_view() && send_again {
                    self.request_workflow_run_refresh(thread_id, &run_id);
                }
            }
            RunLoadOrigin::Cancel => {
                let Some(screen) = self.workflows.screen_of(thread_id) else {
                    if let Err(err) = result {
                        self.add_error_message(format!(
                            "Failed to cancel workflow run {run_id}: {err}"
                        ));
                    }
                    return;
                };
                screen.cancelling.remove(&run_id);
                match result {
                    Ok(run) => {
                        self.apply_workflow_screen_update(thread_id, run);
                    }
                    Err(err) => {
                        self.add_error_message(format!(
                            "Failed to cancel workflow run {run_id}: {err}"
                        ));
                        self.replace_workflow_list_view();
                        self.replace_workflow_run_view();
                    }
                }
            }
        }
    }

    /// R8.6.5: mark a run `cancelling…` while its cancel is in flight.
    pub(crate) fn note_workflow_cancel_started(&mut self, thread_id: ThreadId, run_id: &str) {
        let screen = self.workflows.screen_for(thread_id);
        screen.cancelling.insert(run_id.to_string());
        self.replace_workflow_list_view();
        self.replace_workflow_run_view();
    }

    fn open_workflow_run(&mut self, thread_id: ThreadId, run: WorkflowRun) {
        let screen = self.workflows.screen_for(thread_id);
        let mut summary = run.clone();
        summary.agents = None;
        screen.runs.insert(run.run_id.clone(), summary);
        let rows = run_view_rows(&run);
        let same_run = screen
            .open_run
            .as_ref()
            .is_some_and(|open| open.run.run_id == run.run_id);
        let (refresh_in_flight, refresh_pending) = screen
            .open_run
            .as_ref()
            .filter(|_| same_run)
            .map(|open| (open.refresh_in_flight, open.refresh_pending))
            .unwrap_or_default();
        screen.open_run = Some(OpenRun {
            run,
            rows,
            refresh_in_flight,
            refresh_pending,
        });
        let Some(params) = self.workflow_run_params(/*keep_selection*/ same_run) else {
            return;
        };
        if !self.replace_selection_view_if_present(RUN_VIEW_ID, params)
            && let Some(params) = self.workflow_run_params(/*keep_selection*/ false)
        {
            self.show_selection_view(params);
        }
    }

    /// R8.6.6: fold a run snapshot into the screen; an open run view asks for fresh agent rows.
    fn apply_workflow_screen_update(&mut self, thread_id: ThreadId, run: WorkflowRun) {
        let Some(screen) = self.workflows.screen_of(thread_id) else {
            return;
        };
        if run.status.is_terminal() {
            screen.cancelling.remove(&run.run_id);
        }
        let mut summary = run.clone();
        summary.agents = None;
        screen.runs.insert(run.run_id.clone(), summary);
        let refresh = match screen.open_run.as_mut() {
            Some(open) if open.run.run_id == run.run_id => {
                let agents = open.run.agents.take();
                open.run = run;
                if open.run.agents.is_none() {
                    open.run.agents = agents;
                }
                true
            }
            _ => false,
        };
        self.replace_workflow_list_view();
        if refresh && self.replace_workflow_run_view() {
            let run_id = self
                .workflows
                .screen
                .as_ref()
                .and_then(|screen| screen.open_run.as_ref())
                .map(|open| open.run.run_id.clone());
            if let Some(run_id) = run_id {
                self.request_workflow_run_refresh(thread_id, &run_id);
            }
        }
    }

    /// At most one read in flight per open run; later updates wait for its answer.
    fn request_workflow_run_refresh(&mut self, thread_id: ThreadId, run_id: &str) {
        let Some(open) = self
            .workflows
            .screen_of(thread_id)
            .and_then(|screen| screen.open_run.as_mut())
            .filter(|open| open.run.run_id == run_id)
        else {
            return;
        };
        if open.refresh_in_flight {
            open.refresh_pending = true;
            return;
        }
        open.refresh_in_flight = true;
        self.app_event_tx
            .send(AppEvent::Workflows(WorkflowsEvent::Refresh {
                thread_id,
                run_id: run_id.to_string(),
            }));
    }

    /// Rebuild the list view if it is open; never reopens a closed one.
    fn replace_workflow_list_view(&mut self) -> bool {
        let Some(params) = self.workflow_list_params() else {
            return false;
        };
        self.replace_selection_view_if_present(RUN_LIST_VIEW_ID, params)
    }

    /// Rebuild the run view if it is open; a closed one ends the refreshes of its run.
    fn replace_workflow_run_view(&mut self) -> bool {
        let Some(params) = self.workflow_run_params(/*keep_selection*/ true) else {
            return false;
        };
        if self.replace_selection_view_if_present(RUN_VIEW_ID, params) {
            return true;
        }
        if let Some(screen) = self.workflows.screen.as_mut() {
            screen.open_run = None;
        }
        false
    }

    fn workflow_list_params(&mut self) -> Option<crate::bottom_pane::SelectionViewParams> {
        let now_ms = self.workflows.now_ms();
        let previous = self.selected_index_for_present_view(RUN_LIST_VIEW_ID);
        let screen = self.workflows.screen.as_mut()?;
        let selected_run_id = previous.and_then(|index| screen.list_order.get(index).cloned());
        let runs = sorted_runs(screen.runs.values());
        let order: Vec<String> = runs.iter().map(|run| run.run_id.clone()).collect();
        let selected = selected_run_id
            .and_then(|run_id| order.iter().position(|id| *id == run_id))
            .or(previous.filter(|index| *index < order.len()));
        let params = run_list_params(
            screen.thread_id,
            &runs,
            &screen.cancelling,
            now_ms,
            selected,
        );
        screen.list_order = order;
        Some(params)
    }

    fn workflow_run_params(
        &mut self,
        keep_selection: bool,
    ) -> Option<crate::bottom_pane::SelectionViewParams> {
        let now_ms = self.workflows.now_ms();
        let previous = keep_selection
            .then(|| self.selected_index_for_present_view(RUN_VIEW_ID))
            .flatten();
        let screen = self.workflows.screen.as_mut()?;
        let thread_id = screen.thread_id;
        let cancelling = screen
            .open_run
            .as_ref()
            .is_some_and(|open| screen.cancelling.contains(&open.run.run_id));
        let open = screen.open_run.as_mut()?;
        let selected_row = previous.and_then(|index| open.rows.get(index).cloned());
        let rows = run_view_rows(&open.run);
        let selected = selected_row
            .and_then(|row| rows.iter().position(|candidate| *candidate == row))
            .or(previous.filter(|index| *index < rows.len()));
        let params = run_view_params(thread_id, &open.run, cancelling, now_ms, selected);
        open.rows = rows;
        Some(params)
    }
}

fn status_label(status: WorkflowRunStatus) -> &'static str {
    match status {
        WorkflowRunStatus::Running => "running",
        WorkflowRunStatus::Completed => "completed",
        WorkflowRunStatus::Failed => "failed",
        WorkflowRunStatus::Cancelled => "cancelled",
        WorkflowRunStatus::Interrupted => "interrupted",
    }
}

/// `Workflow find-bugs · Verify · 3 running, 5 done`, or a count for several runs.
pub(super) fn workflow_status_indicator(runs: &BTreeMap<String, WorkflowRun>) -> Option<String> {
    let mut active = runs.values().filter(|run| !run.status.is_terminal());
    let first = active.next()?;
    let others = active.count();
    if others > 0 {
        return Some(format!("{} workflows running", others + 1));
    }
    let counts = &first.agent_counts;
    let running = counts.running + counts.awaiting_approval;
    let done = counts.completed + counts.null_results;
    let mut text = format!("Workflow {}", first.name);
    if let Some(phase) = &first.current_phase {
        text.push_str(&format!(" · {phase}"));
    }
    text.push_str(&format!(" · {running} running, {done} done"));
    if counts.awaiting_approval > 0 {
        text.push_str(&format!(", {} awaiting approval", counts.awaiting_approval));
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::WorkflowAgentCounts;
    use pretty_assertions::assert_eq;

    fn run(run_id: &str, status: WorkflowRunStatus) -> WorkflowRun {
        WorkflowRun {
            run_id: run_id.to_string(),
            name: "find-bugs".to_string(),
            description: "d".to_string(),
            status,
            revision: 3,
            current_phase: Some("Verify".to_string()),
            phases: Vec::new(),
            agent_counts: WorkflowAgentCounts {
                queued: 2,
                running: 3,
                awaiting_approval: 1,
                completed: 4,
                null_results: 1,
            },
            null_reasons: Default::default(),
            logs_tail: Vec::new(),
            charged_tokens: 0,
            cached_tokens: 0,
            budget_tokens: None,
            started_at_ms: 0,
            finished_at_ms: None,
            result_preview: None,
            error: None,
            agents: None,
            resume: None,
        }
    }

    #[test]
    fn indicator_summarizes_active_runs() {
        let mut runs = BTreeMap::new();
        assert_eq!(workflow_status_indicator(&runs), None);
        runs.insert("wf-1".to_string(), run("wf-1", WorkflowRunStatus::Running));
        assert_eq!(
            workflow_status_indicator(&runs).as_deref(),
            Some("Workflow find-bugs · Verify · 4 running, 5 done, 1 awaiting approval")
        );
        runs.insert("wf-2".to_string(), run("wf-2", WorkflowRunStatus::Running));
        assert_eq!(
            workflow_status_indicator(&runs).as_deref(),
            Some("2 workflows running")
        );
    }
}
