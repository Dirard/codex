//! Fork: the managed git worktree an isolated MultiAgentV2 child works in
//! (`specs/agent-worktree-isolation-2026-10-25.md`, UC9). Reported only through the
//! experimental `Thread.extra`.

use crate::JsonSchema;
use crate::TS;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;

/// The managed git worktree of an isolated sub-agent thread.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadAgentWorktree {
    /// Root of the worktree.
    pub path: AbsolutePathBuf,
    /// The agent branch that holds the agent's recorded changes; null until it exists.
    pub branch: Option<String>,
    pub status: ThreadAgentWorktreeStatus,
}

/// Whether an agent worktree holds work.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ThreadAgentWorktreeStatus {
    /// The agent branch exists, or the worktree has uncommitted changes or new commits.
    Changed,
    /// No branch and no changes.
    Clean,
    /// The worktree checkout no longer exists.
    Removed,
}

#[cfg(test)]
#[path = "agent_worktree_tests.rs"]
mod tests;
