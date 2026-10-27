//! Fork: workflow display (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.4 and UC 8.1).
//!
//! The status line shows active runs of the shown thread; the turn items of a run's start and
//! of its completion notification become history cells, live and on replay. The `/workflows`
//! screen keeps its state here because run updates reach the widget of the shown thread; the
//! app-server requests behind it live in `app/workflows_fork.rs`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::time::Duration;
use std::time::Instant;

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

/// How long a run opened from `/subagents` may take to load and still open its run view; a
/// later answer only says that the run is ready (R8.6.3).
const OPEN_GRACE: Duration = Duration::from_secs(3);
/// Reads of an open run view are at least this far apart (R8.6.6) ...
const REFRESH_MIN_INTERVAL: Duration = Duration::from_millis(500);
/// ... plus this much per agent row, up to `REFRESH_MAX_INTERVAL`, so a large fan-out does not
/// keep the thread's request queue busy with reads of the whole run.
const REFRESH_PER_AGENT: Duration = Duration::from_millis(2);
const REFRESH_MAX_INTERVAL: Duration = Duration::from_secs(3);

/// Workflow state of a chat widget: active runs for the status line and the `/workflows`
/// screen once it was opened.
#[derive(Default)]
pub(super) struct WorkflowState {
    /// R8.4: non-terminal runs of the shown thread by run id.
    active: BTreeMap<String, WorkflowRun>,
    /// UC 8.1: runs and the open run of the screen.
    screen: Option<WorkflowScreen>,
    /// The run the user asked to open and still waits for (R8.6.3).
    pending_open: Option<PendingOpen>,
    /// Tests render durations against a fixed clock.
    fixed_now_ms: Option<i64>,
    /// Tests count how often a view was built.
    #[cfg(test)]
    views_built: usize,
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
    /// A read is scheduled or in flight.
    refresh_in_flight: bool,
    refresh_pending: bool,
    /// When the last read was sent (or will be, when it waits for the interval).
    last_refresh_at: Option<Instant>,
}

/// UC 8.1, type `PendingOpen`: a run the user opened whose read has not answered yet.
struct PendingOpen {
    thread_id: ThreadId,
    run_id: String,
    requested_at: Instant,
    /// The run list was open when the user asked; the answer then waits for the list.
    from_list: bool,
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

    /// Whether `run` is older than what the screen already shows of that run.
    fn is_stale(&self, run: &WorkflowRun) -> bool {
        let newer = |known: &WorkflowRun| known.revision > run.revision;
        self.runs.get(&run.run_id).is_some_and(newer)
            || self
                .open_run
                .as_ref()
                .is_some_and(|open| open.run.run_id == run.run_id && newer(&open.run))
    }
}

/// Keeps the newer of the stored summary of a run and `run` (review 2026-10-27, item 7): an
/// answer computed before an update must not replace the update.
fn merge_summary(runs: &mut BTreeMap<String, WorkflowRun>, run: &mut WorkflowRun) {
    if runs
        .get(&run.run_id)
        .is_some_and(|known| known.revision > run.revision)
    {
        return;
    }
    let agents = run.agents.take();
    runs.insert(run.run_id.clone(), run.clone());
    run.agents = agents;
}

/// Time between two reads of an open run (R8.6.6).
fn refresh_interval(run: &WorkflowRun) -> Duration {
    let agents = run.agents.as_ref().map_or(0, Vec::len);
    let per_agents = REFRESH_PER_AGENT.saturating_mul(u32::try_from(agents).unwrap_or(u32::MAX));
    per_agents.clamp(REFRESH_MIN_INTERVAL, REFRESH_MAX_INTERVAL)
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

    /// Makes the pending open look `age` old, as if its read waited behind a cancel.
    #[cfg(test)]
    pub(super) fn age_pending_open(&mut self, age: Duration) {
        if let Some(pending) = self.pending_open.as_mut()
            && let Some(requested_at) = Instant::now().checked_sub(age)
        {
            pending.requested_at = requested_at;
        }
    }

    #[cfg(test)]
    pub(super) fn views_built(&self) -> usize {
        self.views_built
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

    /// R8.6.6: the app asked for the runs of `thread_id`. The screen state exists from now
    /// on, so an update that arrives before the answer is kept against it.
    pub(crate) fn note_workflows_list_requested(&mut self, thread_id: ThreadId) {
        self.workflows.screen_for(thread_id);
    }

    /// R8.6.3: the user opened `run_id`; its answer opens the run view only while the user
    /// still waits for it (review 2026-10-27, item 9).
    pub(crate) fn note_workflow_open_requested(&mut self, thread_id: ThreadId, run_id: &str) {
        self.workflows.pending_open = Some(PendingOpen {
            thread_id,
            run_id: run_id.to_string(),
            requested_at: Instant::now(),
            from_list: self.bottom_pane.has_view(RUN_LIST_VIEW_ID),
        });
    }

    /// UC 8.1, "список run'ов": show the answer of `thread/workflow/list` (R8.6.2). A run
    /// that an update already moved past the answer keeps the update (R8.6.6).
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
        let screen = self.workflows.screen_for(thread_id);
        for mut run in runs {
            merge_summary(&mut screen.runs, &mut run);
        }
        if screen.runs.is_empty() {
            self.add_info_message(
                "No workflow runs in this thread.".to_string(),
                Some("Codex starts them with the workflow tool.".to_string()),
            );
            return;
        }
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
            RunLoadOrigin::Open => self.on_workflow_run_opened(thread_id, run_id, result),
            RunLoadOrigin::Refresh => {
                let Some(screen) = self.workflows.screen_of(thread_id) else {
                    return;
                };
                let WorkflowScreen { runs, open_run, .. } = screen;
                let Some(open) = open_run.as_mut().filter(|open| open.run.run_id == run_id) else {
                    return;
                };
                open.refresh_in_flight = false;
                match result {
                    // A read that an update already moved past keeps the update's fields
                    // and only fills in agent rows the view does not have yet.
                    Ok(mut run) if run.revision < open.run.revision => {
                        if open.run.agents.is_none() {
                            open.run.agents = run.agents.take();
                        }
                    }
                    Ok(mut run) => {
                        merge_summary(runs, &mut run);
                        open.run = run;
                    }
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

    /// R8.6.3: the answer of a read the user asked for. It shows the run view while the user
    /// waits: over the run list that is still open, or, opened from `/subagents`, within
    /// `OPEN_GRACE` and without another popup. An open run view of the run is replaced.
    fn on_workflow_run_opened(
        &mut self,
        thread_id: ThreadId,
        run_id: String,
        result: Result<WorkflowRun, String>,
    ) {
        let pending = self
            .workflows
            .pending_open
            .take_if(|pending| pending.thread_id == thread_id && pending.run_id == run_id);
        let shown = self.bottom_pane.has_view(RUN_VIEW_ID)
            && self
                .workflows
                .screen_of(thread_id)
                .and_then(|screen| screen.open_run.as_ref())
                .is_some_and(|open| open.run.run_id == run_id);
        let wanted = pending.as_ref().is_some_and(|pending| {
            if pending.from_list {
                self.bottom_pane.has_view(RUN_LIST_VIEW_ID)
            } else {
                pending.requested_at.elapsed() <= OPEN_GRACE
                    && self.bottom_pane.no_modal_or_popup_active()
            }
        });
        match result {
            Ok(run) if shown || wanted => self.open_workflow_run(thread_id, run),
            Ok(mut run) => {
                if let Some(screen) = self.workflows.screen_of(thread_id) {
                    merge_summary(&mut screen.runs, &mut run);
                }
                if pending.is_some_and(|pending| !pending.from_list) {
                    self.add_info_message(
                        format!("Workflow run {run_id} is ready."),
                        Some("Open it with /workflows.".to_string()),
                    );
                }
            }
            Err(err) => {
                if shown || pending.is_some() {
                    self.add_error_message(format!("Failed to read workflow run {run_id}: {err}"));
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

    /// Name and status of the runs this widget knows, for the `/subagents` run rows.
    pub(crate) fn known_workflow_runs(&self) -> Vec<(String, String, WorkflowRunStatus)> {
        let mut known: BTreeMap<&str, &WorkflowRun> = BTreeMap::new();
        if let Some(screen) = &self.workflows.screen {
            known.extend(screen.runs.iter().map(|(id, run)| (id.as_str(), run)));
        }
        known.extend(
            self.workflows
                .active
                .iter()
                .map(|(id, run)| (id.as_str(), run)),
        );
        known
            .into_iter()
            .map(|(id, run)| (id.to_string(), run.name.clone(), run.status))
            .collect()
    }

    fn open_workflow_run(&mut self, thread_id: ThreadId, mut run: WorkflowRun) {
        let screen = self.workflows.screen_for(thread_id);
        merge_summary(&mut screen.runs, &mut run);
        let previous = screen
            .open_run
            .take()
            .filter(|open| open.run.run_id == run.run_id);
        let same_run = previous.is_some();
        let (refresh_in_flight, refresh_pending, last_refresh_at) = previous
            .as_ref()
            .map(|open| {
                (
                    open.refresh_in_flight,
                    open.refresh_pending,
                    open.last_refresh_at,
                )
            })
            .unwrap_or_default();
        // A newer update already in the view keeps its fields; the answer brings the agents.
        let run = match previous {
            Some(mut open) if open.run.revision > run.revision => {
                open.run.agents = run.agents.take();
                open.run
            }
            _ => run,
        };
        let rows = run_view_rows(&run);
        screen.open_run = Some(OpenRun {
            run,
            rows,
            refresh_in_flight,
            refresh_pending,
            last_refresh_at,
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
    /// A snapshot older than the one the screen holds changes nothing.
    fn apply_workflow_screen_update(&mut self, thread_id: ThreadId, mut run: WorkflowRun) {
        let Some(screen) = self.workflows.screen_of(thread_id) else {
            return;
        };
        if screen.is_stale(&run) {
            return;
        }
        if run.status.is_terminal() {
            screen.cancelling.remove(&run.run_id);
        }
        merge_summary(&mut screen.runs, &mut run);
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

    /// At most one read scheduled or in flight per open run, and reads at least
    /// `refresh_interval` apart; later updates wait for the answer.
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
        let now = Instant::now();
        let delay = open
            .last_refresh_at
            .map(|last| (last + refresh_interval(&open.run)).saturating_duration_since(now))
            .unwrap_or_default();
        open.refresh_in_flight = true;
        open.last_refresh_at = Some(now + delay);
        self.app_event_tx
            .send(AppEvent::Workflows(WorkflowsEvent::Refresh {
                thread_id,
                run_id: run_id.to_string(),
                delay,
            }));
    }

    /// Rebuild the list view if it is open; never reopens a closed one and builds nothing
    /// while it is closed.
    fn replace_workflow_list_view(&mut self) -> bool {
        if !self.bottom_pane.has_view(RUN_LIST_VIEW_ID) {
            return false;
        }
        let Some(params) = self.workflow_list_params() else {
            return false;
        };
        self.replace_selection_view_if_present(RUN_LIST_VIEW_ID, params)
    }

    /// Rebuild the run view if it is open; a closed one ends the refreshes of its run.
    fn replace_workflow_run_view(&mut self) -> bool {
        if !self.bottom_pane.has_view(RUN_VIEW_ID) {
            if let Some(screen) = self.workflows.screen.as_mut() {
                screen.open_run = None;
            }
            return false;
        }
        let Some(params) = self.workflow_run_params(/*keep_selection*/ true) else {
            return false;
        };
        self.replace_selection_view_if_present(RUN_VIEW_ID, params)
    }

    fn workflow_list_params(&mut self) -> Option<crate::bottom_pane::SelectionViewParams> {
        let now_ms = self.workflows.now_ms();
        let previous = self.selected_index_for_present_view(RUN_LIST_VIEW_ID);
        #[cfg(test)]
        {
            self.workflows.views_built += 1;
        }
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
        #[cfg(test)]
        {
            self.workflows.views_built += 1;
        }
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
