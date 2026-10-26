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

/// Header line telling the model that the exit will be reported without polling.
pub(super) const EXIT_NOTIFICATION_ARMED_LINE: &str = "Exit notification: on (Codex will report when this process exits while this Codex session is running; do not poll for completion)";

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
    /// The process outlived this call and will report its exit to the thread.
    pub(super) fn fork_exit_notification_line(&self) -> Option<String> {
        self.exit_notification_armed
            .then(|| EXIT_NOTIFICATION_ARMED_LINE.to_string())
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
