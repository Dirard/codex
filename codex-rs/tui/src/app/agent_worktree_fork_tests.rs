//! Fork: `/subagents` rows of isolated agents name their worktree (R9.5).

use super::super::agent_navigation::AgentNavigationState;
use super::super::tests::make_test_app_with_channels;
use super::*;
use crate::chatwidget::tests::render_bottom_popup;
use crate::multi_agents::SubAgentActivityDisplay;
use codex_utils_absolute_path::AbsolutePathBuf;

const ROOT: &str = "00000000-0000-0000-0000-000000000200";

fn thread(id: &str) -> ThreadId {
    ThreadId::from_string(id).expect("valid thread id")
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
async fn subagents_show_agent_worktrees() {
    let (mut app, _app_event_rx, _op_rx) = Box::pin(make_test_app_with_channels()).await;
    let mut app_server = Box::pin(crate::start_embedded_app_server_for_picker(
        app.chat_widget.config_ref(),
    ))
    .await
    .expect("embedded app server");
    app.primary_thread_id = Some(thread(ROOT));
    app.agent_navigation = AgentNavigationState::default();
    for (id, path) in [
        ("00000000-0000-0000-0000-000000000201", "/root/top"),
        ("00000000-0000-0000-0000-000000000202", "/root/bottom"),
        ("00000000-0000-0000-0000-000000000203", "/root/shared"),
    ] {
        app.agent_navigation
            .record_sub_agent_activity(SubAgentActivityDisplay {
                thread_id: thread(id),
                agent_path: path.to_string(),
                is_running_hint: true,
            });
    }
    app.agent_navigation.fork_worktrees.insert(
        thread("00000000-0000-0000-0000-000000000201"),
        worktree(
            "/home/u/.codex/worktrees/6d88/demo",
            Some("codex/agents/top-8a1b2c3d4e5f6071"),
            ThreadAgentWorktreeStatus::Changed,
        ),
    );
    app.agent_navigation.fork_worktrees.insert(
        thread("00000000-0000-0000-0000-000000000202"),
        worktree(
            "/home/u/.codex/worktrees/1f2e/demo",
            None,
            ThreadAgentWorktreeStatus::Clean,
        ),
    );

    Box::pin(app.open_agent_picker(&mut app_server)).await;

    insta::with_settings!({snapshot_path => "snapshots", prepend_module_to_snapshot => false}, {
        insta::assert_snapshot!(
            "codex_tui__app__agent_worktree_fork__subagents_show_agent_worktrees",
            render_bottom_popup(&app.chat_widget, /*width*/ 120)
        );
    });
}
