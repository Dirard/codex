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

/// What a workflow run does when its agents spend the token budget.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowBudgetModeToml {
    /// Calls that have not started resolve to null; running agents finish their turn.
    #[default]
    Soft,
    /// As `soft`, and running agents are interrupted once the run spends more than its
    /// budget times `budget_hard_overshoot`.
    Hard,
}

/// How workflow agents are asked for answers that match a JSON schema.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStructuredOutputToml {
    /// Native structured output until the provider rejects it, then the schema in the task.
    #[default]
    Auto,
    /// Native structured output (`text.format` with a JSON schema).
    Native,
    /// The schema in the agent's task, for providers without JSON schema output.
    Prompt,
}

/// Fork: limits and defaults of background workflow runs (`features.workflows`).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, JsonSchema)]
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
    /// What happens when a run spends its token budget. Default: `soft`.
    pub budget_mode: Option<WorkflowBudgetModeToml>,
    /// In `hard` budget mode, the multiple of the budget at which running agents are
    /// interrupted, at most 100; values below 1.0 count as 1.0, larger values are a
    /// configuration error. Default: 1.0.
    #[schemars(range(max = 100))]
    pub budget_hard_overshoot: Option<f64>,
    /// How agents are asked for answers that match a JSON schema. Default: `auto`.
    pub structured_output: Option<WorkflowStructuredOutputToml>,
    /// Whether reasoning effort `ultra` turns on workflow ultra mode, which tells the model
    /// to run a workflow for every substantive task. Approvals, budgets and limits are
    /// unchanged. Default: true.
    pub ultra_mode: Option<bool>,
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    /// Workflows ultra mode: `[workflows] ultra_mode` is optional, and `false` is read.
    #[test]
    fn workflows_ultra_mode_is_optional() {
        let ultra_mode = |text: &str| {
            toml::from_str::<super::super::ConfigToml>(text)
                .expect("config.toml parses")
                .workflows
                .and_then(|workflows| workflows.ultra_mode)
        };
        assert_eq!(ultra_mode(""), None);
        assert_eq!(ultra_mode("[workflows]\nmax_active_runs = 2\n"), None);
        assert_eq!(ultra_mode("[workflows]\nultra_mode = false\n"), Some(false));
        assert_eq!(ultra_mode("[workflows]\nultra_mode = true\n"), Some(true));
    }
}

/// Fork: where a standard (non-Lite) Responses request carries the base instructions
/// (`model_instructions_placement`). Responses Lite requests always carry them in `input`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModelInstructionsPlacement {
    /// The first `developer` message of `input`, as upstream sends it.
    #[default]
    Input,
    /// The top-level `instructions` field, as Codex sent it before 0.162 (upstream #51156).
    Instructions,
}
