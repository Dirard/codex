//! Fork: output line limits for MCP results and exec hook payloads
//! (docs/fork/03-tools-mcp-code-mode.md F1, F3), and the header line of an exec session
//! whose exit will be reported (specs/background-exec-notifications-2026-10-25.md).
//!
//! Raw events and traces keep the upstream byte/token bound only: history applies the line
//! limits, and repeating them earlier would truncate the omission marker.

use codex_protocol::mcp::CallToolResult;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
pub(super) use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::formatted_truncate_text_with_config;
use codex_utils_output_truncation::truncate_text_with_config;
use serde_json::Value as JsonValue;

use super::ExecCommandToolOutput;
use super::McpToolOutput;
use super::format_output_omission_marker;
use crate::unified_exec::OUTPUT_WATCH_MAX_ACTIVE;
use crate::unified_exec::OutputWatchRefusal;
use crate::unified_exec::OutputWatchState;

/// Header line telling the model that the exit will be reported without polling.
pub(super) const EXIT_NOTIFICATION_ARMED_LINE: &str = "Exit notification: on (Codex will report when this process exits while this Codex session is running; do not poll for completion)";
/// Added to the output watch line when the pattern was too large with Unicode classes and is
/// matched with ASCII classes (review of the fixes, P3-4).
pub(super) const OUTPUT_WATCH_ASCII_NOTE: &str = ". The pattern was too large with Unicode classes, so it is matched with ASCII classes: \\w, \\d, \\s, \\b and (?i) see ASCII letters and digits only, and . matches one byte.";

impl McpToolOutput {
    /// With `mcp_max_lines` configured, a text-only result becomes one text so the history
    /// line limit cuts it as a whole. Structured or encrypted results keep their shape.
    pub(super) fn join_text_for_mcp_line_limit(&self, payload: &mut FunctionCallOutputPayload) {
        if self.truncation.mcp_max_lines.is_some()
            && !payload.content_items().is_some_and(|items| {
                items.iter().any(|item| {
                    matches!(item, FunctionCallOutputContentItem::EncryptedContent { .. })
                })
            })
            && let Some(text_content) = mcp_text_content_for_line_limit(&self.result)
        {
            *payload = FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text(text_content),
                success: Some(self.result.success()),
            };
        }
    }
}

fn mcp_text_content_for_line_limit(result: &CallToolResult) -> Option<String> {
    if result
        .structured_content
        .as_ref()
        .is_some_and(|structured_content| !structured_content.is_null())
    {
        return None;
    }

    let mut text_segments = Vec::new();
    for content in &result.content {
        let object = content.as_object()?;
        if object.get("type").and_then(JsonValue::as_str)? != "text" {
            return None;
        }
        text_segments.push(object.get("text").and_then(JsonValue::as_str)?.to_string());
    }

    if text_segments.is_empty() {
        None
    } else {
        Some(text_segments.join("\n"))
    }
}

impl ExecCommandToolOutput {
    /// The process outlived this call and will report its exit (R1.2) and matching output
    /// (R6.3, R6.4, R6.8) to the thread.
    pub(super) fn fork_notification_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.armed_notifications.exit {
            lines.push(EXIT_NOTIFICATION_ARMED_LINE.to_string());
        }
        // R6.15 (live recheck 2026-10-27, N1): `write_stdin` turned `notify_on_exit` off.
        match self.armed_notifications.exit_off {
            Some(true) => lines.push(
                "Exit notification: off (codex exec and wait_agent no longer wait for this process to exit; its output watch still reports matching lines and the exit)".to_string(),
            ),
            Some(false) => lines.push(
                "Exit notification: off (the exit of this process is no longer reported, and nothing waits for it)".to_string(),
            ),
            None => {}
        }
        match &self.armed_notifications.output_watch {
            OutputWatchState::Unchanged => {}
            OutputWatchState::On(pattern) => lines.push(format!(
                "Output watch: on for lines matching \"{pattern}\" (Codex reports new matching lines while this Codex session is running; do not poll for them){}",
                if self.armed_notifications.output_watch_ascii {
                    // Review of the fixes P3-4: the model learns that its pattern changed.
                    OUTPUT_WATCH_ASCII_NOTE
                } else {
                    ""
                }
            )),
            OutputWatchState::Off => lines.push("Output watch: off".to_string()),
            OutputWatchState::NotStarted(OutputWatchRefusal::Limit) => lines.push(format!(
                "Output watch: not started (at most {OUTPUT_WATCH_MAX_ACTIVE} terminals can watch their output at a time; stop a watch with write_stdin stop_output_watch true)"
            )),
            OutputWatchState::NotStarted(OutputWatchRefusal::NoNotifications) => lines.push(
                "Output watch: not started (this session started before background notifications were enabled; run the command again with exec_command notify_on_output to watch it)".to_string(),
            ),
            OutputWatchState::NotStarted(OutputWatchRefusal::Exited) => lines.push(
                "Output watch: not started (the process exited; call write_stdin with this session_id to read its exit code and remaining output)".to_string(),
            ),
        }
        lines
    }

    /// The pattern of a running session's output watch for the Code Mode result.
    pub(super) fn fork_code_mode_output_watch(&self) -> Option<String> {
        match &self.armed_notifications.output_watch {
            OutputWatchState::On(pattern) if self.process_id.is_some() => Some(pattern.clone()),
            _ => None,
        }
    }

    /// The hook payload: the history line limits apply before `policy`.
    pub(super) fn fork_truncated_with_lines(&self, policy: TruncationPolicy) -> String {
        self.truncated_output_with_config(self.truncation.with_policy(policy))
    }

    fn truncated_output_with_config(&self, truncation: OutputTruncation) -> String {
        let text = String::from_utf8_lossy(&self.raw_output).to_string();
        let Some(omitted_bytes) = self.output_omitted_bytes else {
            return formatted_truncate_text_with_config(&text, truncation);
        };

        let marker = format_output_omission_marker(omitted_bytes.get());
        let truncated = truncate_text_with_config(&text, truncation);
        if truncated == text {
            return if text.contains(&marker) {
                text
            } else {
                format!("{marker}\n{text}")
            };
        }

        let original_token_count = self
            .original_token_count
            .unwrap_or_else(|| approx_token_count(&text));
        let omission_notice = if truncated.contains(&marker) {
            String::new()
        } else {
            format!("{marker}\n")
        };
        format!(
            "Warning: truncated output (original token count: {original_token_count})\n{omission_notice}\n{truncated}"
        )
    }
}
