//! Fork: minimal workflow display (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.4).
//!
//! The status line shows active runs of the shown thread; the turn items of a run's start and
//! of its completion notification become history cells, live and on replay.

use std::collections::BTreeMap;

use codex_app_server_protocol::ThreadWorkflowUpdatedNotification;
use codex_app_server_protocol::WorkflowItem;
use codex_app_server_protocol::WorkflowRun;
use codex_app_server_protocol::WorkflowRunStatus;

use super::ChatWidget;

impl ChatWidget {
    /// R8.4: follow run snapshots of the shown thread in the status line.
    pub(super) fn on_workflow_updated(&mut self, notification: ThreadWorkflowUpdatedNotification) {
        if self
            .thread_id
            .is_some_and(|thread_id| thread_id.to_string() != notification.thread_id)
        {
            return;
        }
        let run = notification.run;
        if run.status.is_terminal() {
            self.workflow_runs.remove(&run.run_id);
        } else {
            self.workflow_runs.insert(run.run_id.clone(), run);
        }
        self.bottom_pane
            .set_workflow_status_indicator(workflow_status_indicator(&self.workflow_runs));
    }

    /// R8.4: history cells for a run's start and for its completion notification.
    pub(super) fn on_workflow_item(&mut self, item: WorkflowItem) {
        if item.status.is_terminal() {
            self.workflow_runs.remove(&item.run_id);
            self.bottom_pane
                .set_workflow_status_indicator(workflow_status_indicator(&self.workflow_runs));
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
