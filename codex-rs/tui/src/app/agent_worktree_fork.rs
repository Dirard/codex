//! Fork: `/subagents` rows of isolated agents name their worktree
//! (`specs/agent-worktree-isolation-2026-10-25.md`, UC9, R9.5).
//!
//! The app-server reports an isolated agent's worktree in the experimental
//! `Thread.extra.agentWorktree` of `thread/list`. The picker refresh remembers the last report
//! per thread, so an agent keeps its label after it is unloaded, and the picker adds it to the
//! row description. Without reports the rows stay as upstream builds them.

use super::App;
use crate::bottom_pane::SelectionItem;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadAgentWorktree;
use codex_app_server_protocol::ThreadAgentWorktreeStatus;
use codex_protocol::ThreadId;

impl App {
    /// Hook of `apply_agent_picker_thread_refresh`: remembers a reported worktree.
    pub(super) fn fork_record_agent_worktree(&mut self, thread_id: ThreadId, thread: &Thread) {
        if let Some(worktree) = thread
            .extra
            .as_ref()
            .and_then(|extra| extra.agent_worktree.clone())
        {
            self.agent_navigation
                .fork_worktrees
                .insert(thread_id, worktree);
        }
    }

    /// Hook of `agent_picker_selection_view_params`: appends the worktree to the description
    /// of each row whose agent has one. `items` follow `visible_threads()`.
    pub(super) fn fork_annotate_agent_worktrees(
        &self,
        mut items: Vec<SelectionItem>,
    ) -> Vec<SelectionItem> {
        let worktrees = &self.agent_navigation.fork_worktrees;
        if worktrees.is_empty() {
            return items;
        }
        for (item, (thread_id, _)) in items
            .iter_mut()
            .zip(self.agent_navigation.visible_threads())
        {
            if let Some(worktree) = worktrees.get(&thread_id) {
                let label = worktree_label(worktree);
                item.description = Some(match item.description.take() {
                    Some(description) => format!("{description} · {label}"),
                    None => label,
                });
            }
        }
        items
    }
}

/// `worktree <branch, or the worktree directory without a branch> · <status>`.
fn worktree_label(worktree: &ThreadAgentWorktree) -> String {
    let name = worktree.branch.clone().unwrap_or_else(|| {
        let path = worktree.path.as_path();
        match (
            path.parent().and_then(|bucket| bucket.file_name()),
            path.file_name(),
        ) {
            (Some(bucket), Some(name)) => {
                format!("{}/{}", bucket.to_string_lossy(), name.to_string_lossy())
            }
            _ => path.display().to_string(),
        }
    });
    let status = match worktree.status {
        ThreadAgentWorktreeStatus::Changed => "changed",
        ThreadAgentWorktreeStatus::Clean => "clean",
        ThreadAgentWorktreeStatus::Removed => "removed",
    };
    format!("worktree {name} · {status}")
}

#[cfg(test)]
#[path = "agent_worktree_fork_tests.rs"]
mod tests;
