// Aggregates all former standalone integration tests as modules.
mod add_dir;
mod agents_md;
mod apply_patch;
mod approval_policy;
mod auth_env;
#[path = "completion_backfill_tests.rs"]
mod completion_backfill;
#[path = "cyber_access_program_tests.rs"]
mod cyber_access_program;
mod daybreak;
mod ephemeral;
mod hooks;
mod mcp_required_exit;
mod originator;
mod output_schema;
mod prompt_stdin;
// Fork: request_user_input in exec (specs/explorer-and-questions-2026-10-25.md).
mod request_user_input;
mod resume;
mod sandbox;
#[cfg(target_os = "macos")]
mod seatbelt;
mod server_error_exit;
// Fork: a startup warning is reported once (live recheck 2026-10-27, R4).
mod startup_warnings;
// Fork: exec waits for workflow runs.
mod workflow;
// Fork: exec waits for background terminal notifications.
mod background_terminals;
mod worktree;
