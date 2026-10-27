use super::*;
use codex_extension_items::ExtensionItem as CoreExtensionItem;
use codex_protocol::ThreadId;
use codex_protocol::items::TurnItem as CoreTurnItem;
use pretty_assertions::assert_eq;

/// Fork: the start and terminal items of a workflow run replay in the turns that
/// recorded them (dynamic workflow, R8.3).
#[test]
fn rebuilds_workflow_items_from_persisted_completions() {
    use crate::protocol::v2::WorkflowItem;
    use crate::protocol::v2::WorkflowRunStatus;

    let thread_id = ThreadId::new();
    let turn_started = |turn_id: &str| {
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_attribution: None,
            turn_id: turn_id.to_string(),
            root_turn_id: None,
            trace_id: None,
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }))
    };
    let completed = |turn_id: &str, item: &WorkflowItem| {
        RolloutItem::EventMsg(EventMsg::ItemCompleted(ItemCompletedEvent {
            thread_id,
            turn_id: turn_id.to_string(),
            item: CoreTurnItem::Extension(CoreExtensionItem::Workflow(item.clone())),
            started_at_ms: Some(0),
            completed_at_ms: 0,
        }))
    };
    let turn_complete = |turn_id: &str| {
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            root_turn_id: None,
            turn_id: turn_id.to_string(),
            started_at: None,
            last_agent_message: None,
            error: None,
            completed_at: None,
            duration_ms: None,
            time_to_first_token_ms: None,
        }))
    };
    let start = WorkflowItem {
        id: "call-1".to_string(),
        run_id: "wf-0a1b2c3d".to_string(),
        name: "find-bugs".to_string(),
        status: WorkflowRunStatus::Running,
        notification: None,
    };
    let finish = WorkflowItem {
        id: "wf-0a1b2c3d-notification".to_string(),
        status: WorkflowRunStatus::Cancelled,
        notification: Some("Workflow \"find-bugs\" (wf-0a1b2c3d) cancelled in 2.0s".to_string()),
        ..start.clone()
    };
    let items = vec![
        turn_started("turn-1"),
        completed("turn-1", &start),
        turn_complete("turn-1"),
        turn_started("turn-2"),
        completed("turn-2", &finish),
        turn_complete("turn-2"),
    ];

    let turns = build_turns_from_rollout_items(&items);

    let payloads = |turn: &Turn| {
        turn.items
            .iter()
            .map(|item| item.payload.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(turns.len(), 2);
    assert_eq!(
        payloads(&turns[0]),
        vec![ThreadItemPayload::Workflow(start)]
    );
    assert_eq!(
        payloads(&turns[1]),
        vec![ThreadItemPayload::Workflow(finish)]
    );
}
