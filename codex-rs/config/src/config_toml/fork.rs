//! Fork: config tables of the fork, moved out of `config_toml.rs` unchanged;
//! `config_toml.rs` re-exports them, so their paths and schema names stay the same.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Limits applied when formatting tool and command output for model and UI consumption.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct OutputTruncationToml {
    /// Maximum bytes retained in formatted output.
    pub max_bytes: Option<usize>,
    /// Maximum lines retained in formatted output.
    pub max_lines: Option<usize>,
    /// Overrides max_lines for MCP tool output and entire emitted Code Mode cells.
    pub mcp_max_lines: Option<usize>,
}

/// Approval of a workflow run before it starts.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowApprovalToml {
    /// Ask unless the approval policy is `never`.
    #[default]
    Auto,
    Always,
    Never,
}

/// Fork: limits and defaults of background workflow runs (`features.workflows`).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct WorkflowsToml {
    /// Non-terminal runs allowed per thread. Default: 3.
    pub max_active_runs: Option<usize>,
    /// Agents of one run that work at the same time. Default: min(16, CPUs - 2).
    pub max_concurrent_agents: Option<usize>,
    /// Agents one run may start. Default: 1000.
    pub max_agents_per_run: Option<usize>,
    /// How long a call waits for agent capacity before resolving to null, in milliseconds.
    /// Default: 600000.
    pub capacity_wait_ms: Option<u64>,
    /// Token budget of a run without `budget_tokens`, as non-cached input plus output.
    pub default_budget_tokens: Option<u64>,
    /// Token limit of the result preview in the completion notification. Default: 2000.
    pub notification_max_tokens: Option<usize>,
    /// V8 heap limit of a run's script cell, in bytes. Default: 268435456.
    pub max_heap_bytes: Option<usize>,
    /// How long `codex exec` waits for active runs before cancelling them, in milliseconds.
    /// Default: 7200000.
    pub exec_wait_timeout_ms: Option<u64>,
    /// Whether the user approves a run before it starts. Default: `auto`.
    pub require_approval: Option<WorkflowApprovalToml>,
}
