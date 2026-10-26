//! Fork: workflow runs show in the status line and as history cells (R8.4).

use super::*;
use codex_app_server_protocol::ThreadWorkflowUpdatedNotification;
use codex_app_server_protocol::WorkflowAgentCounts;
use codex_app_server_protocol::WorkflowItem;
use codex_app_server_protocol::WorkflowRun;
use codex_app_server_protocol::WorkflowRunStatus;
use pretty_assertions::assert_eq;

fn workflow_run(status: WorkflowRunStatus) -> WorkflowRun {
    WorkflowRun {
        run_id: "wf-0000abcd".to_string(),
        name: "find-bugs".to_string(),
        description: "d".to_string(),
        status,
        revision: 2,
        current_phase: Some("Find".to_string()),
        phases: Vec::new(),
        agent_counts: WorkflowAgentCounts {
            queued: 0,
            running: 3,
            awaiting_approval: 0,
            completed: 1,
            null_results: 0,
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

fn workflow_item(status: WorkflowRunStatus, notification: Option<&str>) -> WorkflowItem {
    WorkflowItem {
        id: "call-1".to_string(),
        run_id: "wf-0000abcd".to_string(),
        name: "find-bugs".to_string(),
        status,
        notification: notification.map(str::to_string),
    }
}

#[tokio::test]
async fn workflow_runs_add_history_cells_and_a_status_indicator() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    drain_insert_history(&mut rx);

    chat.on_workflow_item(workflow_item(WorkflowRunStatus::Running, None));
    chat.on_workflow_updated(ThreadWorkflowUpdatedNotification {
        thread_id: chat
            .thread_id
            .map(|thread_id| thread_id.to_string())
            .unwrap_or_default(),
        run: workflow_run(WorkflowRunStatus::Running),
    });
    assert_eq!(
        chat.bottom_pane.workflow_status_indicator(),
        Some("Workflow find-bugs · Find · 3 running, 1 done")
    );

    chat.on_workflow_item(workflow_item(
        WorkflowRunStatus::Completed,
        Some("Workflow \"find-bugs\" (wf-0000abcd) completed in 1m 2s: 4 agents (4 ok, 0 null), 10 tokens charged, 0 cached input tokens.\nResult preview:\n\"ok\""),
    ));
    assert_eq!(chat.bottom_pane.workflow_status_indicator(), None);
    let cells = drain_insert_history(&mut rx);
    let text = cells
        .iter()
        .flat_map(|lines| lines.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("Workflow \"find-bugs\" (wf-0000abcd) started in the background"),
        "{text}"
    );
    assert!(
        text.contains("Workflow \"find-bugs\" (wf-0000abcd) completed in 1m 2s: 4 agents"),
        "{text}"
    );
    assert!(!text.contains("Result preview"), "{text}");
}
