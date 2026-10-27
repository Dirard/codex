//! Fork: the managed worktree of an isolated sub-agent thread in `thread/list` and
//! `thread/read` (`specs/agent-worktree-isolation-2026-10-25.md`, UC9, R9.3).
//!
//! The state comes from the thread's working directory on disk, so unloaded threads and
//! threads of an earlier process are described too. It is reported only through the
//! experimental `Thread.extra` and only while the feature is enabled in the server config.

use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadAgentWorktree;
use codex_app_server_protocol::ThreadAgentWorktreeStatus;
use codex_app_server_protocol::ThreadExtra;
use codex_core::AgentWorktreeState;
use codex_core::config::Config;
use codex_features::Feature;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SubAgentSource;
use codex_utils_absolute_path::AbsolutePathBuf;

/// Sets `extra.agentWorktree` of every spawned sub-agent thread that works in its own
/// managed worktree; other threads stay unchanged.
pub(super) async fn annotate_agent_worktrees<'a>(
    config: &Config,
    threads: impl IntoIterator<Item = &'a mut Thread>,
) {
    if !config.features.enabled(Feature::AgentWorktreeIsolation) {
        return;
    }
    futures::future::join_all(
        threads
            .into_iter()
            .filter(|thread| {
                matches!(
                    &thread.source,
                    SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. })
                )
            })
            .map(|thread| annotate(config, thread)),
    )
    .await;
}

async fn annotate(config: &Config, thread: &mut Thread) {
    let Ok(thread_id) = ThreadId::from_string(&thread.id) else {
        return;
    };
    let Some(view) =
        codex_core::describe_agent_worktree(config, thread_id, thread.cwd.as_path()).await
    else {
        return;
    };
    let Ok(path) = AbsolutePathBuf::from_absolute_path(&view.path) else {
        return;
    };
    thread
        .extra
        .get_or_insert_with(ThreadExtra::default)
        .agent_worktree = Some(ThreadAgentWorktree {
        path,
        branch: view.branch,
        status: match view.state {
            AgentWorktreeState::Changed => ThreadAgentWorktreeStatus::Changed,
            AgentWorktreeState::Clean => ThreadAgentWorktreeStatus::Clean,
            AgentWorktreeState::Removed => ThreadAgentWorktreeStatus::Removed,
        },
    });
}
