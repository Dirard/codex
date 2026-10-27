//! Fork: the managed worktree of an isolated sub-agent thread in `thread/list` and
//! `thread/read` (`specs/agent-worktree-isolation-2026-10-25.md`, UC9, R9.3).
//!
//! The state comes from the thread's working directory on disk, so unloaded threads and
//! threads of an earlier process are described too. It is reported only through the
//! experimental `Thread.extra` and only while the feature is enabled in the server config.
//! A direct `thread/resume` of such a thread that cannot restore its removed worktree reports
//! why (R5.9).

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

/// Worktree states read at once for one answer: each costs a few Git processes.
const ANNOTATION_CONCURRENCY: usize = 8;

/// Sets `extra.agentWorktree` of every spawned sub-agent thread that works in its own
/// managed worktree; other threads stay unchanged.
pub(super) async fn annotate_agent_worktrees<'a>(
    config: &Config,
    threads: impl IntoIterator<Item = &'a mut Thread>,
) {
    use futures::StreamExt as _;
    if !config.features.enabled(Feature::AgentWorktreeIsolation) {
        return;
    }
    let sub_agents = threads.into_iter().filter(|thread| {
        matches!(
            &thread.source,
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn { .. })
        )
    });
    futures::stream::iter(sub_agents)
        .for_each_concurrent(ANNOTATION_CONCURRENCY, |thread| annotate(config, thread))
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

/// The upstream answer to a sub-agent that cannot be resumed through its parent.
const CHILD_RESUME_ERROR: &str = "cannot resume an unloaded multi-agent v2 sub-agent through its parent; resume the parent first, or use thread/read to inspect it";

/// R5.9: the error of a direct `thread/resume` of a sub-agent. A worktree that cannot be
/// restored is named with the reason; other failures keep the upstream text, whose details
/// stay in the log.
pub(super) fn child_resume_message(err: &codex_protocol::error::CodexErr) -> String {
    match codex_core::agent_worktree_reload_error(err) {
        Some(reason) => format!("{CHILD_RESUME_ERROR}: {reason}"),
        None => CHILD_RESUME_ERROR.to_string(),
    }
}
