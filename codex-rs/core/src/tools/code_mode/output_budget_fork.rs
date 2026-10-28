//! Fork: the default output budget of a Code Mode cell follows the configured output truncation
//! (`tool_output_token_limit`, `[output_truncation]`), see
//! specs/code-mode-cell-output-budget-2026-10-29.md and docs/fork/03-tools-mcp-code-mode.md F1.
//! `truncate_code_mode_result` cuts the cell with the step's policy.

#[cfg(test)]
#[path = "output_budget_fork_tests.rs"]
mod tests;
