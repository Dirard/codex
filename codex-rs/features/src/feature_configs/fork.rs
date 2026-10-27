//! Fork: settings of the fork's features, moved out of `feature_configs.rs` unchanged;
//! `lib.rs` re-exports them, so their paths and schema names stay the same.

use crate::FeatureConfig;
use codex_protocol::openai_models::ReasoningEffort;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// How multi-agent V2 task and message arguments are delivered to child agents.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MultiAgentMessageDelivery {
    /// Keep OpenAI's encrypted collaboration arguments.
    #[default]
    Encrypted,
    /// Deliver provider-neutral plain text for cross-provider agents.
    Plaintext,
}

/// Fork: settings for `explorer` children (read-only, cheaper, capped result).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExplorerProfileConfigToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Model for explorer children. Defaults to the parent's model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Reasoning effort for explorer children. Defaults to the cheapest effort, not below
    /// `low`, that the selected model supports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Largest explorer final answer, in tokens, delivered to the parent. Defaults to 2000.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 200))]
    pub max_result_tokens: Option<usize>,
}

impl FeatureConfig for ExplorerProfileConfigToml {
    fn enabled(&self) -> Option<bool> {
        self.enabled
    }
}

/// Fork: settings of background exec notifications
/// (`specs/background-exec-notifications-2026-10-25.md`, UC7).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecExitNotificationsConfigToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// How long `codex exec` waits for background terminals whose exit or matching output is
    /// still owed to the model before it stops them and exits with code 1, in milliseconds.
    /// Defaults to 1800000 (30 minutes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exec_wait_timeout_ms: Option<u64>,
    /// How long an output watch alone keeps `codex exec`, `wait_agent` and agent residency
    /// waiting after its process last printed a matching line (or after the watch started),
    /// in milliseconds; `notify_on_exit` waits for the exit instead. Defaults to 60000.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_watch_quiet_ms: Option<u64>,
}

impl FeatureConfig for ExecExitNotificationsConfigToml {
    fn enabled(&self) -> Option<bool> {
        self.enabled
    }
}

// Fork: worktree isolation for MultiAgentV2 children.
/// Where a MultiAgentV2 child runs relative to its parent's checkout.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentIsolation {
    /// The child shares its parent's checkout.
    #[default]
    None,
    /// The child works in its own managed git worktree.
    Worktree,
}

/// The commit an isolated child's worktree starts from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorktreeBase {
    /// The parent's HEAD plus tracked modifications and untracked, non-ignored files. Those
    /// files are written to the repository's object store and become part of the agent
    /// branch's history, as with `git stash --include-untracked`; keep secrets in ignored
    /// files or use `head`, and do not push `codex/agents/*` branches.
    #[default]
    WorkingTree,
    /// The parent's HEAD commit.
    Head,
}

/// `[features.agent_worktree_isolation]` settings for isolated MultiAgentV2 children. The
/// feature requires `features.multi_agent_v2.tool_namespace` to name a custom namespace such
/// as `agents`. Agent worktrees share the managed root with Codex Desktop, whose automatic
/// cleanup removes them like the worktrees of inactive threads once the root holds more than
/// `[desktop] worktree-keep-count`; the agent's work stays on its branch and Codex restores
/// the worktree, but ignored files such as build output are lost. Raise
/// `worktree-keep-count` or set `worktree-auto-cleanup-enabled = false` to keep them.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentWorktreeIsolationConfigToml {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Isolation used when `spawn_agent` omits `isolation`. Role config files may set it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_isolation: Option<AgentIsolation>,
    /// Commit an isolated child's worktree starts from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<AgentWorktreeBase>,
    /// Remove forgotten unchanged agent worktrees of the repository when a session starts and
    /// when an agent tree stops (default `true`). Changed worktrees and agent branches are
    /// never removed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup: Option<bool>,
    /// Unchanged agent worktrees of a repository kept among the most recently active ones
    /// (default 15); older ones beyond the count are removed after an hour without activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_keep_count: Option<u32>,
    /// Hours without activity after which an unchanged agent worktree is removed (default
    /// 24, at least 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_max_idle_hours: Option<u32>,
}

impl FeatureConfig for AgentWorktreeIsolationConfigToml {
    fn enabled(&self) -> Option<bool> {
        self.enabled
    }
}
