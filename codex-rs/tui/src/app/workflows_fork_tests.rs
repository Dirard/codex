//! Fork: `/subagents` groups workflow agents under their run (R8.6.7).

use super::super::agent_navigation::AgentNavigationState;
use super::super::agent_picker::AGENT_PICKER_VIEW_ID;
use super::super::tests::make_test_app_with_channels;
use super::super::thread_events::ThreadEventChannel;
use super::*;
use crate::chatwidget::tests::render_bottom_popup;
use crate::multi_agents::SubAgentActivityDisplay;
use crate::workflows::test_fixtures;
use codex_app_server_protocol::ItemStartedNotification;
use codex_app_server_protocol::SubAgentActivityKind;
use codex_app_server_protocol::ThreadAgentWorktree;
use codex_app_server_protocol::ThreadAgentWorktreeStatus;
use codex_app_server_protocol::ThreadWorkflowUpdatedNotification;
use codex_utils_absolute_path::AbsolutePathBuf;
use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

const ROOT: &str = "00000000-0000-0000-0000-000000000100";

fn thread(id: &str) -> ThreadId {
    ThreadId::from_string(id).expect("valid thread id")
}

const WORKER: &str = "00000000-0000-0000-0000-000000000101";
const FIND: &str = "00000000-0000-0000-0000-000000000102";
const VERIFY: &str = "00000000-0000-0000-0000-000000000103";

/// A worker and two workflow agents of run `wf-1a2b3c4d`, in first-seen order.
const WORKFLOW_AGENTS: [(&str, Option<&str>); 3] = [
    (WORKER, Some("/root/worker")),
    (FIND, Some("/root/wf_1a2b3c4d_1_find_src_a_py")),
    (VERIFY, Some("/root/wf_1a2b3c4d_2_verify")),
];

/// An app whose root thread knows run `wf-1a2b3c4d` and whose picker holds a worker and two
/// workflow agents of that run.
async fn app_with_workflow_agents(
    workflows: bool,
) -> (
    Box<App>,
    tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    AppServerSession,
) {
    Box::pin(app_with_agents(workflows, &WORKFLOW_AGENTS)).await
}

/// An app whose root thread knows run `wf-1a2b3c4d` and whose picker holds `agents` in
/// first-seen order; an agent without a path is known by its thread id only, as when its
/// approval request comes before its metadata.
async fn app_with_agents(
    workflows: bool,
    agents: &[(&str, Option<&str>)],
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
    for (id, path) in agents {
        match path {
            Some(path) => app
                .agent_navigation
                .record_sub_agent_activity(SubAgentActivityDisplay {
                    thread_id: thread(id),
                    agent_path: path.to_string(),
                    is_running_hint: true,
                }),
            None => app.upsert_agent_picker_thread(
                thread(id),
                /*agent_nickname*/ None,
                /*agent_role*/ None,
                /*is_closed*/ false,
            ),
        }
    }
    (app, app_event_rx, app_server)
}

/// The thread of the selected picker row, through the hooks `refresh_agent_picker` uses.
fn selected_thread(app: &App) -> Option<ThreadId> {
    let display = app
        .chat_widget
        .selected_index_for_present_view(AGENT_PICKER_VIEW_ID)?;
    let row = app.fork_picker_visible_index(display)?;
    app.agent_navigation.picker_row_ids.get(row).copied()
}

fn drain(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) {
    while rx.try_recv().is_ok() {}
}

fn selected_row(app: &App) -> Option<usize> {
    app.chat_widget
        .selected_index_for_present_view(AGENT_PICKER_VIEW_ID)
}

fn down(app: &mut App) {
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
}

/// The root thread reports `thread_id`'s agent path, as a `SubAgentActivity` item does.
fn agent_path_arrives(app: &mut App, thread_id: ThreadId, agent_path: &str) {
    app.cache_collab_receiver_threads_for_notification(&ServerNotification::ItemStarted(
        ItemStartedNotification::new(
            ROOT.to_string(),
            "turn-1".to_string(),
            ThreadItemPayload::SubAgentActivity {
                id: format!("activity-{thread_id}"),
                kind: SubAgentActivityKind::Started,
                agent_thread_id: thread_id.to_string(),
                agent_path: agent_path.to_string(),
                model: None,
                reasoning_effort: None,
            },
            /*started_at_ms*/ 0,
        ),
    ));
}

/// `thread_id` starts waiting for approval, as `refresh_pending_thread_approvals` records it.
fn approval_requested(app: &mut App, thread_id: ThreadId) {
    app.agent_navigation.pending_approvals.insert(thread_id);
    app.refresh_agent_picker();
}

fn worktree(
    path: &str,
    branch: Option<&str>,
    status: ThreadAgentWorktreeStatus,
) -> ThreadAgentWorktree {
    ThreadAgentWorktree {
        path: AbsolutePathBuf::from_absolute_path(path).expect("absolute path"),
        branch: branch.map(str::to_string),
        status,
    }
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
    // Display rows are the upstream rows.
    assert_eq!(app.agent_navigation.fork_picker_layout, None);
    assert_eq!(app.fork_picker_visible_index(1), Some(1));
    assert_eq!(app.fork_picker_visible_index(2), Some(2));
}

/// Review 2026-10-27, item 8: a picker refresh keeps the cursor on a run row instead of
/// moving it to the current thread.
#[tokio::test]
async fn picker_refresh_keeps_the_cursor_on_a_run_row() {
    let (mut app, mut app_event_rx, mut app_server) = app_with_workflow_agents(true).await;
    Box::pin(app.open_agent_picker(&mut app_server)).await;
    drain(&mut app_event_rx);
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let before = app
        .chat_widget
        .selected_index_for_present_view(AGENT_PICKER_VIEW_ID);
    assert_eq!(before, Some(1), "the run row");
    app.set_agent_picker_thread_visibility(
        thread("00000000-0000-0000-0000-000000000101"),
        super::super::agent_navigation::AgentPickerThreadVisibility::Visible,
    );
    assert_eq!(
        app.chat_widget
            .selected_index_for_present_view(AGENT_PICKER_VIEW_ID),
        before,
        "cursor left the run row"
    );

    // A thread row keeps working as before.
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.set_agent_picker_thread_visibility(
        thread("00000000-0000-0000-0000-000000000101"),
        super::super::agent_navigation::AgentPickerThreadVisibility::Visible,
    );
    assert_eq!(
        selected_thread(&app),
        Some(thread("00000000-0000-0000-0000-000000000102"))
    );
}

/// Upstream #58720 lists threads waiting for approval first. Groups follow those rows
/// (`picker_row_ids`): a waiting agent stays above a run, a waiting workflow agent lifts its run
/// and keeps its mark, worktrees stay on their own rows, and a run row keeps the cursor while
/// the rows move.
#[tokio::test]
async fn subagents_list_waiting_agents_first_around_workflow_groups() {
    const REVIEWER: &str = "00000000-0000-0000-0000-000000000104";
    let mut agents = WORKFLOW_AGENTS.to_vec();
    agents.push((REVIEWER, None));
    let (mut app, mut app_event_rx, mut app_server) =
        Box::pin(app_with_agents(true, &agents)).await;
    app.agent_navigation
        .pending_approvals
        .insert(thread(REVIEWER));
    app.agent_navigation.fork_worktrees.insert(
        thread(REVIEWER),
        worktree(
            "/home/u/.codex/worktrees/6d88/demo",
            Some("codex/agents/review-8a1b2c3d4e5f6071"),
            ThreadAgentWorktreeStatus::Changed,
        ),
    );
    app.agent_navigation.fork_worktrees.insert(
        thread(VERIFY),
        worktree(
            "/home/u/.codex/worktrees/1f2e/demo",
            /*branch*/ None,
            ThreadAgentWorktreeStatus::Clean,
        ),
    );

    Box::pin(app.open_agent_picker(&mut app_server)).await;
    insta::with_settings!({snapshot_path => "snapshots", prepend_module_to_snapshot => false}, {
        insta::assert_snapshot!(
            "codex_tui__app__workflows_fork__subagents_waiting_agent_above_workflow_group",
            render_bottom_popup(&app.chat_widget, /*width*/ 150)
        );
    });
    assert_eq!(selected_thread(&app), Some(thread(REVIEWER)));

    // Rows: the reviewer, worker, the run, #1, #2, main. The cursor goes to the run row.
    drain(&mut app_event_rx);
    down(&mut app);
    down(&mut app);
    assert_eq!(selected_row(&app), Some(2));
    assert_eq!(selected_thread(&app), None);

    // #2 starts waiting. Waiting threads keep their first-seen order, so #2 comes before the
    // reviewer and lifts its run to the top: the run, #2, #1, the reviewer, worker, main. The
    // cursor stays on the run row.
    approval_requested(&mut app, thread(VERIFY));
    assert_eq!(selected_row(&app), Some(0), "cursor left the run row");
    assert_eq!(selected_thread(&app), None);

    // The reviewer's path arrives; `sync_agent_picker_thread` rebuilds the picker for it.
    agent_path_arrives(&mut app, thread(REVIEWER), "/root/reviewer");
    assert_eq!(selected_row(&app), Some(0), "cursor left the run row");
    insta::with_settings!({snapshot_path => "snapshots", prepend_module_to_snapshot => false}, {
        insta::assert_snapshot!(
            "codex_tui__app__workflows_fork__subagents_waiting_workflow_agent_lifts_its_run",
            render_bottom_popup(&app.chat_widget, /*width*/ 150)
        );
    });
    down(&mut app);
    assert_eq!(selected_thread(&app), Some(thread(VERIFY)));
}

/// A refresh reads the cursor through the rows on screen: the path of a workflow agent that
/// arrives while the picker is open regroups the rows only at the next rebuild.
#[tokio::test]
async fn picker_refresh_reads_the_cursor_through_the_rendered_rows() {
    const EXTRA: &str = "00000000-0000-0000-0000-000000000105";
    let mut agents = vec![(EXTRA, None)];
    agents.extend(WORKFLOW_AGENTS);
    let (mut app, mut app_event_rx, mut app_server) =
        Box::pin(app_with_agents(true, &agents)).await;
    Box::pin(app.open_agent_picker(&mut app_server)).await;

    // Rows: the extra agent, worker, the run, #1, #2, main. The cursor goes to the worker.
    drain(&mut app_event_rx);
    down(&mut app);
    assert_eq!(selected_thread(&app), Some(thread(WORKER)));

    // Not waiting for approval, so upstream does not rebuild the picker for it.
    agent_path_arrives(&mut app, thread(EXTRA), "/root/wf_1a2b3c4d_3_extra");
    let popup = render_bottom_popup(&app.chat_widget, /*width*/ 90);
    assert!(popup.contains("Agent (000000000105)"), "{popup}");
    assert_eq!(selected_thread(&app), Some(thread(WORKER)));

    // The next rebuild puts the extra agent first in its run; the cursor stays on the worker.
    approval_requested(&mut app, thread(EXTRA));
    let popup = render_bottom_popup(&app.chat_widget, /*width*/ 90);
    assert!(popup.contains("#3 extra (needs approval)"), "{popup}");
    assert!(popup.contains("3 agents"), "{popup}");
    assert_eq!(selected_row(&app), Some(4));
    assert_eq!(selected_thread(&app), Some(thread(WORKER)));
}
