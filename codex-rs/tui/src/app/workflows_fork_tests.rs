//! Fork: `/subagents` groups workflow agents under their run (R8.6.7).

use super::super::agent_navigation::AgentNavigationState;
use super::super::agent_picker::AGENT_PICKER_VIEW_ID;
use super::super::tests::make_test_app_with_channels;
use super::super::thread_events::ThreadEventChannel;
use super::*;
use crate::chatwidget::tests::render_bottom_popup;
use crate::multi_agents::SubAgentActivityDisplay;
use crate::workflows::test_fixtures;
use codex_app_server_protocol::ThreadWorkflowUpdatedNotification;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

const ROOT: &str = "00000000-0000-0000-0000-000000000100";

fn thread(id: &str) -> ThreadId {
    ThreadId::from_string(id).expect("valid thread id")
}

/// An app whose root thread knows run `wf-1a2b3c4d` and whose picker holds a worker and two
/// workflow agents of that run.
async fn app_with_workflow_agents(
    workflows: bool,
) -> (
    Box<App>,
    tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    AppServerSession,
) {
    let (mut app, app_event_rx, _op_rx) = Box::pin(make_test_app_with_channels()).await;
    app.config
        .features
        .set_enabled(Feature::Workflows, workflows)
        .expect("toggle workflows");
    let app_server = Box::pin(crate::start_embedded_app_server_for_picker(
        app.chat_widget.config_ref(),
    ))
    .await
    .expect("embedded app server");
    let root = thread(ROOT);
    app.primary_thread_id = Some(root);
    let channel = ThreadEventChannel::new(/*capacity*/ 16);
    channel
        .store
        .lock()
        .await
        .push_notification(ServerNotification::ThreadWorkflowUpdated(
            ThreadWorkflowUpdatedNotification {
                thread_id: ROOT.to_string(),
                run: test_fixtures::running_run(),
            },
        ));
    app.thread_event_channels.insert(root, channel);
    app.agent_navigation = AgentNavigationState::default();
    for (id, path) in [
        ("00000000-0000-0000-0000-000000000101", "/root/worker"),
        (
            "00000000-0000-0000-0000-000000000102",
            "/root/wf_1a2b3c4d_1_find_src_a_py",
        ),
        (
            "00000000-0000-0000-0000-000000000103",
            "/root/wf_1a2b3c4d_2_verify",
        ),
    ] {
        app.agent_navigation
            .record_sub_agent_activity(SubAgentActivityDisplay {
                thread_id: thread(id),
                agent_path: path.to_string(),
                is_running_hint: true,
            });
    }
    (app, app_event_rx, app_server)
}

/// The thread of the selected picker row, through the hooks the upstream picker uses.
fn selected_thread(app: &App) -> Option<ThreadId> {
    let display = app
        .chat_widget
        .selected_index_for_present_view(AGENT_PICKER_VIEW_ID)?;
    let visible = app.fork_picker_visible_index(display)?;
    app.agent_navigation
        .visible_threads()
        .get(visible)
        .map(|(thread_id, _)| *thread_id)
}

fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) {
    while rx.try_recv().is_ok() {}
}

#[tokio::test]
async fn subagents_group_workflow_agents_under_their_run() {
    let (mut app, mut app_event_rx, mut app_server) = app_with_workflow_agents(true).await;
    Box::pin(app.open_agent_picker(&mut app_server)).await;
    insta::with_settings!({snapshot_path => "snapshots", prepend_module_to_snapshot => false}, {
        insta::assert_snapshot!(
            "codex_tui__app__workflows_fork__subagents_group_workflow_agents",
            render_bottom_popup(&app.chat_widget, /*width*/ 90)
        );
    });

    // Rows: worker, the run, #1, #2. The run row opens its run view.
    drain(&mut app_event_rx);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(selected_thread(&app), None);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let opened = std::iter::from_fn(|| app_event_rx.try_recv().ok()).any(|event| {
        matches!(
            event,
            AppEvent::Workflows(WorkflowsEvent::OpenRun { thread_id, run_id })
                if thread_id == thread(ROOT) && run_id == "wf-1a2b3c4d"
        )
    });
    assert!(opened);

    // A workflow agent row switches to that agent, as any picker row does.
    Box::pin(app.open_agent_picker(&mut app_server)).await;
    drain(&mut app_event_rx);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(
        selected_thread(&app),
        Some(thread("00000000-0000-0000-0000-000000000102"))
    );
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let selected = std::iter::from_fn(|| app_event_rx.try_recv().ok()).any(|event| {
        matches!(
            event,
            AppEvent::SelectAgentThread(id) if id == thread("00000000-0000-0000-0000-000000000102")
        )
    });
    assert!(selected);
}

#[tokio::test]
async fn subagents_rows_are_unchanged_without_the_feature() {
    let (mut app, _app_event_rx, mut app_server) = app_with_workflow_agents(false).await;
    Box::pin(app.open_agent_picker(&mut app_server)).await;
    let popup = render_bottom_popup(&app.chat_widget, /*width*/ 90);
    assert!(!popup.contains("Workflow find-bugs"), "{popup}");
    assert!(
        popup.contains("/root/wf_1a2b3c4d_1_find_src_a_py"),
        "{popup}"
    );
    assert_eq!(app.fork_picker_visible_index(1), Some(1));
    assert_eq!(app.fork_picker_display_index(2), Some(2));
}
