//! Fork: the result fields that background notifications add to `exec_command` and
//! `write_stdin` (`specs/background-exec-notifications-2026-10-25.md`, R1.2, R6.3, live
//! recheck 4 R12). Code Mode shows a tool's output schema to the model as the TypeScript type
//! of its result, with these descriptions, so a model that calls the tools from Code Mode learns
//! where the status lines of the text output are.

use serde_json::Value;
use serde_json::json;

/// What the Code Mode result of `exec_command` and `write_stdin` holds: the upstream fields,
/// and with exit notifications on also the exit notification, the output watch and the status
/// lines (`ExecCommandToolOutput::fork_code_mode_notes`).
pub(super) fn unified_exec_output_schema(exit_notifications: bool) -> Value {
    let mut schema = super::unified_exec_output_schema();
    if exit_notifications
        && let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut)
    {
        properties.insert(
            "exit_notification".to_string(),
            json!({
                "type": "boolean",
                "description": "True when Codex will report the exit of this running session."
            }),
        );
        properties.insert(
            "output_watch".to_string(),
            json!({
                "type": "string",
                "description": "Pattern of the output watch this call started."
            }),
        );
        properties.insert(
            "notes".to_string(),
            json!({
                "type": "array",
                "items": { "type": "string" },
                "description": "Status of this running session's notifications: whether its exit is reported and what waits for it, and whether its output watch is on, unchanged, off or not started and why. Read them before you rely on a notification."
            }),
        );
    }
    schema
}

#[cfg(test)]
#[path = "shell_spec_fork_tests.rs"]
mod tests;
