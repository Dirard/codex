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
use std::time::Duration;

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
    // The app notes the open before it reads the run (R8.6.3).
    chat.note_workflow_open_requested(thread_id, "wf-1a2b3c4d");
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
    chat.note_workflow_open_requested(thread_id, "wf-1a2b3c4d");
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

fn finished_run() -> WorkflowRun {
    let mut finished = test_fixtures::running_run();
    finished.status = WorkflowRunStatus::Completed;
    finished.finished_at_ms = Some(1_090_000);
    finished.revision += 10;
    finished
}

fn history_text(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> String {
    drain_insert_history(rx)
        .iter()
        .flat_map(|lines| lines.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Review 2026-10-27, item 7: a list answer computed before a terminal update does not replace
/// the update, also when the update arrived between the request and the answer.
#[tokio::test]
async fn stale_list_answer_keeps_a_newer_update() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    chat.on_workflows_list_loaded(thread_id, Ok(vec![test_fixtures::running_run()]));
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    app_events(&mut rx);
    update(&mut chat, thread_id, finished_run());
    // The answer of a second `/workflows` was computed while the run still ran.
    chat.on_workflows_list_loaded(thread_id, Ok(vec![test_fixtures::running_run()]));
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("wf-1a2b3c4d · completed"), "{popup}");
    assert!(!popup.contains("x cancel run"), "{popup}");
    assert!(
        chat.known_workflow_runs()
            .iter()
            .all(|(_, _, status)| *status == WorkflowRunStatus::Completed),
        "/subagents labels follow the newer update"
    );

    // The first `/workflows` of a widget: the update arrives after the request, before the
    // answer.
    let (mut chat, _rx, thread_id) = workflow_screen_chat().await;
    chat.note_workflows_list_requested(thread_id);
    update(&mut chat, thread_id, finished_run());
    chat.on_workflows_list_loaded(thread_id, Ok(vec![test_fixtures::running_run()]));
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("wf-1a2b3c4d · completed"), "{popup}");
}

/// Review 2026-10-27, item 7: a read answered before a terminal update does not bring back
/// `running` and `Cancel run`.
#[tokio::test]
async fn stale_refresh_keeps_a_newer_update() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    open_run_view(&mut chat, thread_id);
    app_events(&mut rx);
    let later = test_fixtures::running_run();
    update(&mut chat, thread_id, later.clone()); // read #1 in flight
    update(&mut chat, thread_id, finished_run()); // pending
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Refresh,
        Ok(later),
    );
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(!popup.contains("Cancel run"), "{popup}");
    assert!(popup.contains("wf-1a2b3c4d · completed"), "{popup}");
    assert!(
        popup.contains("#4 verify a.py:12"),
        "agent rows stay: {popup}"
    );
    assert_eq!(
        refreshes(&app_events(&mut rx)),
        2,
        "read #1, then the pending update reads again"
    );

    // An older update after the newer one changes nothing either.
    update(&mut chat, thread_id, test_fixtures::running_run());
    assert!(!render_bottom_popup(&chat, /*width*/ 100).contains("Cancel run"));
}

/// Review 2026-10-27, item 9: an answer that arrives after the user closed the list does not
/// open the run view; from `/subagents`, a late answer only says that the run is ready.
#[tokio::test]
async fn late_open_answer_does_not_reopen_a_closed_screen() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    chat.on_workflows_list_loaded(thread_id, Ok(vec![test_fixtures::running_run()]));
    chat.note_workflow_open_requested(thread_id, "wf-1a2b3c4d");
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Open,
        Ok(test_fixtures::running_run()),
    );
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(
        !popup.contains("Phases"),
        "closed list, no run view: {popup}"
    );
    assert!(!history_text(&mut rx).contains("is ready"));

    // From `/subagents` (no list): a quick answer opens the run view ...
    chat.note_workflow_open_requested(thread_id, "wf-1a2b3c4d");
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Open,
        Ok(test_fixtures::running_run()),
    );
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(popup.contains("Phases"), "{popup}");
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

    // ... a late one leaves the composer alone and says the run is ready.
    chat.note_workflow_open_requested(thread_id, "wf-1a2b3c4d");
    chat.workflows.age_pending_open(Duration::from_secs(30));
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Open,
        Ok(test_fixtures::running_run()),
    );
    let popup = render_bottom_popup(&chat, /*width*/ 100);
    assert!(!popup.contains("Phases"), "{popup}");
    assert!(
        history_text(&mut rx).contains("Workflow run wf-1a2b3c4d is ready."),
        "late answer"
    );

    // An answer nobody waits for (a newer open replaced it) is dropped.
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Open,
        Ok(test_fixtures::running_run()),
    );
    assert!(!render_bottom_popup(&chat, /*width*/ 100).contains("Phases"));
    assert!(history_text(&mut rx).is_empty());
}

/// Review 2026-10-27, item 10: updates build no view while the screen is closed, and reads
/// of an open run are spaced by the refresh interval.
#[tokio::test]
async fn closed_screen_builds_nothing_and_reads_are_spaced() {
    let (mut chat, mut rx, thread_id) = workflow_screen_chat().await;
    open_run_view(&mut chat, thread_id);
    let first = app_events(&mut rx);
    assert_eq!(refreshes(&first), 0);

    update(&mut chat, thread_id, test_fixtures::running_run());
    let delays = |events: &[AppEvent]| {
        events
            .iter()
            .filter_map(|event| match event {
                AppEvent::Workflows(WorkflowsEvent::Refresh { delay, .. }) => Some(*delay),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(delays(&app_events(&mut rx)), vec![Duration::ZERO]);
    update(&mut chat, thread_id, test_fixtures::running_run());
    chat.on_workflow_run_loaded(
        thread_id,
        "wf-1a2b3c4d".to_string(),
        RunLoadOrigin::Refresh,
        Ok(test_fixtures::running_run()),
    );
    let second = delays(&app_events(&mut rx));
    assert_eq!(second.len(), 1, "{second:?}");
    assert!(
        second[0] > Duration::ZERO && second[0] <= Duration::from_millis(500),
        "the next read waits for the interval: {second:?}"
    );

    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    let built = chat.workflows.views_built();
    for _ in 0..3 {
        update(&mut chat, thread_id, test_fixtures::running_run());
    }
    assert_eq!(
        chat.workflows.views_built(),
        built,
        "a closed screen builds no view"
    );
    assert_eq!(refreshes(&app_events(&mut rx)), 0);
}
