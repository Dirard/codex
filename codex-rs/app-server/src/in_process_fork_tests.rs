use super::*;

/// Fork: moved out of `tests::guaranteed_delivery_helpers_cover_required_server_notifications`
/// unchanged (`specs/dynamic-workflow-2026-10-25.md`, R8.2).
#[test]
fn guaranteed_delivery_covers_only_terminal_workflow_snapshots() {
    // Fork: only the terminal workflow snapshot must not be dropped.
    let mut tracker = codex_workflow::RunTracker::new(
        "wf-1".to_string(),
        "name".to_string(),
        "description".to_string(),
        &[],
        /*budget_tokens*/ None,
        /*started_at_ms*/ 0,
    );
    let workflow_update = |snapshot| {
        ServerNotification::ThreadWorkflowUpdated(
            codex_app_server_protocol::ThreadWorkflowUpdatedNotification {
                thread_id: "thread-1".to_string(),
                run: codex_app_server_protocol::WorkflowRun::from_snapshot(
                    snapshot, /*include_agents*/ false,
                ),
            },
        )
    };
    assert!(!server_notification_requires_delivery(&workflow_update(
        tracker.snapshot(/*include_agents*/ false)
    )));
    tracker.finish(codex_workflow::RunStatus::Completed, 1, None, None);
    assert!(server_notification_requires_delivery(&workflow_update(
        tracker.snapshot(/*include_agents*/ false)
    )));
}
