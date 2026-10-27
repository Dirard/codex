use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandToolOptions {
    pub exec_permission_approvals_enabled: bool,
    /// Fork: advertise `notify_on_exit` (feature `exec_exit_notifications`).
    pub exit_notifications: bool,
}

/// Fork: `exec_command` parameter description for exit notifications.
pub(crate) const NOTIFY_ON_EXIT_DESCRIPTION: &str = "When true and the command is still running when this call returns, Codex notifies you once it exits (exit code or failure, duration and the tail of unread output), even after your turn has ended. Use it for builds, tests and other long commands, then continue other work or end your turn instead of polling write_stdin. Combine with a short yield_time_ms to background the command right away. Do not use it for servers or REPLs that are meant to keep running: codex exec and wait_agent wait for the exit; if you set it on one, turn it off with write_stdin notify_on_exit false.";
/// Fork: `write_stdin` description addition when exit notifications are enabled.
pub(crate) const WRITE_STDIN_EXIT_NOTIFICATION_GUIDANCE: &str = " Sessions started with notify_on_exit or notify_on_output report their exit (and matching output) automatically; do not poll them just to wait.";
/// Fork: `exec_command` parameter description for output watches (UC6).
pub(crate) const NOTIFY_ON_OUTPUT_DESCRIPTION: &str = "Regular expression in Rust regex syntax (at most 256 bytes; Unicode-aware: (?i) ignores case in any script and \\p{..} classes work; a pattern written only in ASCII that is too large with Unicode classes, such as .{0,255}Z, is matched with ASCII classes instead, where \\w, \\d, \\s, \\b, . and (?i) cover ASCII only, and one with non-ASCII text is refused; prefer + or * to large or nested counted repetitions, which can be refused as too costly). While the command keeps running after this call returns, Codex notifies you of new output lines that match it, even after your turn has ended, and also reports the exit as notify_on_exit does. Matches within 0.5 s arrive together, a new notification follows at most every 5 s, and a watch stops after 10 notifications. Use it for servers, watchers and long jobs, e.g. \"(?i)ready|listening|error|fail\" (include failure patterns; start with (?i) to ignore case), instead of polling write_stdin. Once you no longer need the process, stop it with write_stdin chars \"\\u0003\" (Ctrl-C) on its session_id, not by killing it by name (pkill, killall), which can stop processes of other sessions. A watch alone keeps codex exec and wait_agent waiting for its first matching line for a few minutes after your turn ends, and after that only while matching lines keep coming; also set notify_on_exit for a job whose exit you need, but not for a server that should keep running.";
/// Fork: `write_stdin` parameter description for output watches (UC6).
pub(crate) const WRITE_STDIN_NOTIFY_ON_OUTPUT_DESCRIPTION: &str = "Regular expression that starts watching this session's new output lines as exec_command notify_on_output does, replacing its previous watch and arming its exit notification. If a watch of this process already matched, the new one is awaited only while matching lines keep coming, not for a first match. An empty string changes nothing; set stop_output_watch to stop the watch, only once its matches are no longer needed.";
/// Fork: `write_stdin` parameter description for stopping an output watch (live recheck 2 R5).
pub(crate) const WRITE_STDIN_STOP_OUTPUT_WATCH_DESCRIPTION: &str = "true stops this session's output watch. Stop it only when what it reports is no longer needed; to stop waiting for the exit, set notify_on_exit false and keep the watch. Its exit is still reported unless you turned notify_on_exit off.";
/// Fork: `write_stdin` parameter description for exit notifications (R6.15).
pub(crate) const WRITE_STDIN_NOTIFY_ON_EXIT_DESCRIPTION: &str = "true: Codex reports this session's exit as exec_command notify_on_exit does, and codex exec and wait_agent wait for it. false: they no longer wait for the exit, until you set it to true again; an output watch that is on still reports the exit, so pass notify_on_exit alone and keep the watch. Use false for a server or watcher that should keep running while this Codex session runs: codex exec still stops it when it exits, and so does unloading a sub-agent that nothing waits for.";

#[cfg(test)]
pub fn create_exec_command_tool(options: CommandToolOptions) -> ToolSpec {
    create_exec_command_tool_with_environment_id(
        options,
        /*include_environment_id*/ false,
        /*include_windows_shell_guidance*/ cfg!(windows),
    )
}

pub(crate) fn create_exec_command_tool_with_environment_id(
    options: CommandToolOptions,
    include_environment_id: bool,
    include_windows_shell_guidance: bool,
) -> ToolSpec {
    let yield_time_ms_description = if cfg!(windows) {
        "Maximum time to wait before returning a session ID for a still-running command. Commands that finish sooner return immediately. For ordinary commands, omit this parameter to use the 10000 ms default. Effective range on Windows is 10000-30000 ms."
    } else {
        "Wait before yielding output. Defaults to 10000 ms; effective range is 250-30000 ms."
    };
    let mut properties = BTreeMap::from([
        (
            "cmd".to_string(),
            JsonSchema::string(Some("Shell command to execute.".to_string())),
        ),
        (
            "workdir".to_string(),
            JsonSchema::string(Some(
                "Working directory for the command. Defaults to the turn cwd."
                    .to_string(),
            )),
        ),
        (
            "tty".to_string(),
            JsonSchema::boolean(Some(
                "True allocates a PTY for the command; false or omitted uses plain pipes."
                    .to_string(),
            )),
        ),
        (
            "yield_time_ms".to_string(),
            JsonSchema::number(Some(yield_time_ms_description.to_string())),
        ),
        (
            "max_output_tokens".to_string(),
            JsonSchema::number(Some(
                "Output token budget. Defaults to 10000 tokens; larger requests may be capped by policy.".to_string(),
            )),
        ),
    ]);
    properties.insert(
        "shell".to_string(),
        JsonSchema::string(Some(
            "Shell binary to launch. Defaults to the user's default shell.".to_string(),
        )),
    );
    properties.insert(
        "login".to_string(),
        JsonSchema::boolean(Some(
            "True runs the shell with -l/-i semantics; false disables them. Defaults to true."
                .to_string(),
        )),
    );
    if include_environment_id {
        properties.insert(
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Environment id from <environment_context>. Omit to use the primary environment."
                    .to_string(),
            )),
        );
    }
    properties.extend(create_approval_parameters(
        options.exec_permission_approvals_enabled,
    ));
    if options.exit_notifications {
        properties.insert(
            "notify_on_exit".to_string(),
            JsonSchema::boolean(Some(NOTIFY_ON_EXIT_DESCRIPTION.to_string())),
        );
        properties.insert(
            "notify_on_output".to_string(),
            JsonSchema::string(Some(NOTIFY_ON_OUTPUT_DESCRIPTION.to_string())),
        );
    }

    ToolSpec::Function(ResponsesApiTool {
        name: "exec_command".to_string(),
        description: if include_windows_shell_guidance {
            format!(
                "Runs a command in a PTY, returning output or a session ID for ongoing interaction.\n\n{}",
                windows_shell_guidance()
            )
        } else {
            "Runs a command in a PTY, returning output or a session ID for ongoing interaction."
                .to_string()
        },
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["cmd".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(fork::unified_exec_output_schema(options.exit_notifications).into()),
    })
}

pub fn create_write_stdin_tool(exit_notifications: bool) -> ToolSpec {
    let mut properties = BTreeMap::from([
        (
            "session_id".to_string(),
            JsonSchema::number(Some(
                "Identifier of the running unified exec session.".to_string(),
            )),
        ),
        (
            "chars".to_string(),
            JsonSchema::string(Some(
                "Bytes to write to stdin. Defaults to empty, which polls without writing.".to_string(),
            )),
        ),
        (
            "yield_time_ms".to_string(),
            JsonSchema::number(Some(
                "Wait before yielding output. Non-empty writes default to 250 ms and cap at 30000 ms; empty polls wait 5000-300000 ms by default.".to_string(),
            )),
        ),
        (
            "max_output_tokens".to_string(),
            JsonSchema::number(Some(
                "Output token budget. Defaults to 10000 tokens; larger requests may be capped by policy.".to_string(),
            )),
        ),
    ]);
    if exit_notifications {
        properties.insert(
            "notify_on_output".to_string(),
            JsonSchema::string(Some(WRITE_STDIN_NOTIFY_ON_OUTPUT_DESCRIPTION.to_string())),
        );
        properties.insert(
            "notify_on_exit".to_string(),
            JsonSchema::boolean(Some(WRITE_STDIN_NOTIFY_ON_EXIT_DESCRIPTION.to_string())),
        );
        properties.insert(
            "stop_output_watch".to_string(),
            JsonSchema::boolean(Some(WRITE_STDIN_STOP_OUTPUT_WATCH_DESCRIPTION.to_string())),
        );
    }

    ToolSpec::Function(ResponsesApiTool {
        name: "write_stdin".to_string(),
        description: format!(
            "Writes characters to an existing unified exec session and returns recent output.{}",
            if exit_notifications {
                WRITE_STDIN_EXIT_NOTIFICATION_GUIDANCE
            } else {
                ""
            }
        ),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["session_id".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(fork::unified_exec_output_schema(exit_notifications).into()),
    })
}

pub fn create_request_permissions_tool(description: String) -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "reason".to_string(),
            JsonSchema::string(Some(
                "Optional short explanation for why additional permissions are needed.".to_string(),
            )),
        ),
        (
            "environment_id".to_string(),
            JsonSchema::string(Some(
                "Environment id from <environment_context>. Omit to use the primary environment."
                    .to_string(),
            )),
        ),
        ("permissions".to_string(), permission_profile_schema()),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "request_permissions".to_string(),
        description,
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["permissions".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

pub fn request_permissions_tool_description() -> String {
    "Request additional filesystem or network permissions from the user and wait for the client to grant a subset of the requested permission profile. Use environment_id to target a specific attached environment; omit it to use the primary environment. Relative filesystem paths resolve against the selected environment cwd. Granted permissions apply automatically to later shell-like commands in the current turn, or for the rest of the session if the client approves them at session scope."
        .to_string()
}

fn unified_exec_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "chunk_id": {
                "type": "string",
                "description": "Chunk identifier included when the response reports one."
            },
            "wall_time_seconds": {
                "type": "number",
                "description": "Elapsed wall time spent waiting for output in seconds."
            },
            "exit_code": {
                "type": "number",
                "description": "Process exit code when the command finished during this call."
            },
            "session_id": {
                "type": "number",
                "description": "Session identifier to pass to write_stdin when the process is still running."
            },
            "original_token_count": {
                "type": "number",
                "description": "Approximate token count before output truncation."
            },
            "output": {
                "type": "string",
                "description": "Command output text, possibly truncated."
            }
        },
        "required": ["wall_time_seconds", "output"],
        "additionalProperties": false
    })
}

fn create_approval_parameters(
    exec_permission_approvals_enabled: bool,
) -> BTreeMap<String, JsonSchema> {
    let mut sandbox_permission_values = vec![json!("use_default")];
    if exec_permission_approvals_enabled {
        sandbox_permission_values.push(json!("with_additional_permissions"));
    }
    sandbox_permission_values.push(json!("require_escalated"));
    let sandbox_permissions_description = if exec_permission_approvals_enabled {
        "Per-command sandbox override. Defaults to `use_default`; use `with_additional_permissions` with `additional_permissions`, or `require_escalated` for unsandboxed execution."
    } else {
        "Per-command sandbox override. Defaults to `use_default`; use `require_escalated` for unsandboxed execution."
    };

    let mut properties = BTreeMap::from([
        (
            "sandbox_permissions".to_string(),
            JsonSchema::string_enum(
                sandbox_permission_values,
                Some(sandbox_permissions_description.to_string()),
            ),
        ),
        (
            "justification".to_string(),
            JsonSchema::string(Some(
                "User-facing approval question for `require_escalated`; omit otherwise.".to_string(),
            )),
        ),
        (
            "prefix_rule".to_string(),
            JsonSchema::array(JsonSchema::string(/*description*/ None), Some(
                    r#"Reusable approval prefix for `cmd`, only with `sandbox_permissions: "require_escalated"`; for example ["git", "pull"]."#.to_string(),
                )),
        ),
    ]);

    if exec_permission_approvals_enabled {
        let mut additional_permissions = permission_profile_schema();
        additional_permissions.description = Some(
            "Sandboxed filesystem or network access for this command; only with `sandbox_permissions: \"with_additional_permissions\"`."
                .into(),
        );
        properties.insert("additional_permissions".to_string(), additional_permissions);
    }

    properties
}

fn permission_profile_schema() -> JsonSchema {
    let mut schema = JsonSchema::object(
        BTreeMap::from([
            ("network".to_string(), network_permissions_schema()),
            ("file_system".to_string(), file_system_permissions_schema()),
        ]),
        /*required*/ None,
        Some(false.into()),
    );
    schema.description = Some("Filesystem or network access request.".into());
    schema
}

fn network_permissions_schema() -> JsonSchema {
    let mut schema = JsonSchema::object(
        BTreeMap::from([(
            "enabled".to_string(),
            JsonSchema::boolean(Some(
                "True requests network access; false or omitted requests none.".to_string(),
            )),
        )]),
        /*required*/ None,
        Some(false.into()),
    );
    schema.description = Some("Network access request.".into());
    schema
}

fn file_system_permissions_schema() -> JsonSchema {
    let mut schema = JsonSchema::object(
        BTreeMap::from([
            (
                "read".to_string(),
                JsonSchema::array(
                    JsonSchema::string(/*description*/ None),
                    Some(
                        "Absolute paths to grant read access; omit when none are needed."
                            .to_string(),
                    ),
                ),
            ),
            (
                "write".to_string(),
                JsonSchema::array(
                    JsonSchema::string(/*description*/ None),
                    Some(
                        "Absolute paths to grant write access; omit when none are needed."
                            .to_string(),
                    ),
                ),
            ),
        ]),
        /*required*/ None,
        Some(false.into()),
    );
    schema.description = Some("Filesystem access request.".into());
    schema
}

fn windows_shell_guidance() -> &'static str {
    r#"Windows safety rules:
- Do not compose destructive filesystem commands across shells. Do not enumerate paths in PowerShell and then pass them to `cmd /c`, batch builtins, or another shell for deletion or moving. Use one shell end-to-end, prefer native PowerShell cmdlets such as `Remove-Item` / `Move-Item` with `-LiteralPath`, and avoid string-built shell commands for file operations.
- Before any recursive delete or move on Windows, verify the resolved absolute target paths stay within the intended workspace or explicitly named target directory. Never issue a recursive delete or move against a computed path if the final target has not been checked.
- When using `Start-Process` to launch a background helper or service, pass `-WindowStyle Hidden` unless the user explicitly asked for a visible interactive window. Use visible windows only for interactive tools the user needs to see or control."#
}

#[path = "shell_spec_fork.rs"]
mod fork;

#[cfg(test)]
#[path = "shell_spec_tests.rs"]
mod tests;
