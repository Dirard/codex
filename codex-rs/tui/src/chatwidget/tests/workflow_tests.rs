//! Fork: workflow runs show in the status line and as history cells (R8.4), and the
//! `/workflows` screen (UC 8.1, R8.6).

use super::*;
use crate::workflows::RunLoadOrigin;
use crate::workflows::WorkflowsEvent;
use crate::workflows::test_fixtures;
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

/// A chat widget showing a thread, with the workflow screen on a fixed clock.
async fn workflow_screen_chat() -> (
    ChatWidget,
    tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    ThreadId,
) {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    drain_insert_history(&mut rx);
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);
    chat.workflows.set_fixed_now_ms(test_fixtures::NOW_MS);
    (chat, rx, thread_id)
}

fn app_events(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> Vec<AppEvent> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn refreshes(events: &[AppEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, AppEvent::Workflows(WorkflowsEvent::Refresh { .. })))
        .count()
}

fn update(chat: &mut ChatWidget, thread_id: ThreadId, run: WorkflowRun) {
    chat.on_workflow_updated(ThreadWorkflowUpdatedNotification {
        thread_id: thread_id.to_string(),
        run: WorkflowRun {
            agents: None,
            ..run
        },
    });
}

fn open_run_view(chat: &mut ChatWidget, thread_id: ThreadId) {
    chat.on_workflows_list_loaded(
        thread_id,
        Ok(vec![
            test_fixtures::completed_run(),
            test_fixtures::running_run(),
            test_fixtures::failed_run(),
        ]),
    );
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Open,
        Ok(test_fixtures::running_run()),
    );
}

/// R8.6.1: `/workflows` exists only with `features.workflows`.
#[tokio::test]
async fn workflows_command_follows_the_feature() {
    let (mut chat, mut rx, _thread_id) = workflow_screen_chat().await;
    chat.set_feature_enabled(Feature::Workflows, /*enabled*/ false);
    chat.bottom_pane
        .set_composer_text("/workf".to_string(), Vec::new(), Vec::new());
    assert!(
        !render_bottom_popup(&chat, /*width*/ 80).contains("/workflows"),
        "{}",
        render_bottom_popup(&chat, /*width*/ 80)
    );
    chat.bottom_pane
        .set_composer_text("/workflows".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        !app_events(&mut rx)
            .iter()
            .any(|event| matches!(event, AppEvent::Workflows(_)))
    );

    chat.set_feature_enabled(Feature::Workflows, /*enabled*/ true);
    chat.bottom_pane
        .set_composer_text("/workf".to_string(), Vec::new(), Vec::new());
    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains("/workflows") && popup.contains("list and manage workflow runs"),
        "{popup}"
    );
    chat.bottom_pane
        .set_composer_text("/workflows".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app_events(&mut rx)
            .iter()
            .any(|event| matches!(event, AppEvent::Workflows(WorkflowsEvent::OpenList)))
    );
}

/// R8.6.2: the run list, newest first; an empty answer and an error become messages.
#[tokio::test]
async fn workflow_run_list_shows_runs_newest_first() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    chat.on_workflows_list_loaded(thread_id, Ok(Vec::new()));
    chat.on_workflows_list_loaded(thread_id, Err("thread not loaded".to_string()));
    let history = drain_insert_history(&mut rx)
        .iter()
        .flat_map(|lines| lines.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        history.contains("No workflow runs in this thread."),
        "{history}"
    );
    assert!(
        history.contains("Failed to load workflow runs: thread not loaded"),
        "{history}"
    );

    chat.on_workflows_list_loaded(
        thread_id,
        Ok(vec![
            test_fixtures::completed_run(),
            test_fixtures::running_run(),
            test_fixtures::failed_run(),
        ]),
    );
    assert_chatwidget_snapshot!(
        "workflow_runs_list",
        render_bottom_popup(&chat, /*width*/ 100)
    );
}

/// R8.6.3, R8.6.4: the run view over the list; Esc returns to the list and an agent row
/// opens the agent's transcript.
#[tokio::test]
async fn workflow_run_view_details_and_navigation() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    open_run_view(&mut chat, thread_id);
    assert_chatwidget_snapshot!(
        "workflow_run_view",
        render_bottom_popup(&chat, /*width*/ 100)
    );

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        popup.contains("Runs of this thread, newest first."),
        "{popup}"
    );

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app_events(&mut rx).iter().any(|event| matches!(
        event,
        AppEvent::Workflows(WorkflowsEvent::OpenRun { run_id, .. })
            if run_id == "wf-1a2b3c4d"
    )));
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Open,
        Ok(test_fixtures::running_run()),
    );
    // `Cancel run`, then #2: #1 answered from the journal and has no thread.
    chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let expected = ThreadId::from_string(&test_fixtures::agent_thread(2)).expect("thread id");
    assert!(
        app_events(&mut rx)
            .iter()
            .any(|event| matches!(event, AppEvent::SelectAgentThread(id) if *id == expected))
    );
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        !popup.contains("Runs of this thread") && !popup.contains("Phases"),
        "selecting an agent closes the screen: {popup}"
    );
}

/// R8.6.6: updates rebuild the open views, keep one read in flight and never reopen a closed
/// view.
#[tokio::test]
async fn workflow_updates_refresh_open_views_once_at_a_time() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    open_run_view(&mut chat, thread_id);
    app_events(&mut rx);

    let mut later = test_fixtures::running_run();
    later.current_phase = Some("Synthesize".to_string());
    later.agent_counts.completed = 6;
    update(&mut chat, thread_id, later.clone());
    update(&mut chat, thread_id, later.clone());
    assert_eq!(refreshes(&app_events(&mut rx)), 1);
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("6 done"), "{popup}");
    assert!(
        popup.contains("#4 verify a.py:12"),
        "agent rows stay until the read: {popup}"
    );

    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Refresh,
        Ok(later.clone()),
    );
    assert_eq!(
        refreshes(&app_events(&mut rx)),
        1,
        "the pending update reads again"
    );
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Refresh,
        Ok(later.clone()),
    );
    assert_eq!(refreshes(&app_events(&mut rx)), 0);

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let screen_shown = |chat: &ChatWidget| {
        let popup = render_bottom_popup(chat, /*width*/ 100);
        popup.contains("Runs of this thread") || popup.contains("Phases")
    };
    assert!(!screen_shown(&chat));
    update(&mut chat, thread_id, later);
    assert_eq!(refreshes(&app_events(&mut rx)), 0);
    assert!(!screen_shown(&chat));
}

/// R8.6.5: a cancel marks the run until it settles, then shows the settled snapshot.
#[tokio::test]
async fn workflow_cancel_marks_the_run_until_it_settles() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    open_run_view(&mut chat, thread_id);
    app_events(&mut rx);

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(app_events(&mut rx).iter().any(|event| matches!(
        event,
        AppEvent::Workflows(WorkflowsEvent::Cancel { run_id, .. }) if run_id == "wf-1a2b3c4d"
    )));
    chat.note_workflow_cancel_started(thread_id, "wf-1a2b3c4d");
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        popup.contains("Cancelling…") && popup.contains("wf-1a2b3c4d · cancelling…"),
        "{popup}"
    );

    let mut cancelled = test_fixtures::running_run();
    cancelled.status = WorkflowRunStatus::Cancelled;
    cancelled.finished_at_ms = Some(1_090_000);
    cancelled.agents = None;
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Cancel,
        Ok(cancelled),
    );
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        popup.contains("wf-1a2b3c4d · cancelled · 1m 30s"),
        "{popup}"
    );
    assert!(!popup.contains("Cancel"), "{popup}");
    assert_eq!(
        refreshes(&app_events(&mut rx)),
        1,
        "agent rows are read again"
    );

    chat.on_workflow_run_loaded(
        thread_id,
        "wf-12345678".to_string(),
        RunLoadOrigin::Cancel,
        Err("run not found".to_string()),
    );
    let history = drain_insert_history(&mut rx)
        .iter()
        .flat_map(|lines| lines.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        history.contains("Failed to cancel workflow run wf-12345678: run not found"),
        "{history}"
    );
}
