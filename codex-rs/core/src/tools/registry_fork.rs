//! Fork: tool outputs carry their originating truncation into history metadata
//! (docs/fork/03-tools-mcp-code-mode.md F2), and Code Mode cannot call a direct-only tool
//! (F7, the `direct_tool_required` error).

use std::time::Duration;

use codex_history::CodexHarnessMetadata;
use codex_otel::SessionTelemetry;
use codex_utils_output_truncation::OutputTruncation;

use super::ToolExposure;
use super::ToolRegistry;
use crate::function_tool::FunctionCallError;
use crate::sandbox_tags::SandboxTags;
use crate::tools::context::ToolCallSource;
use crate::tools::context::ToolInvocation;
use crate::tools::router::tool_log_payload;
use crate::tools::tool_dispatch_trace::ToolDispatchTrace;

/// History metadata that replays the policy and line limits of the originating turn, next to
/// the upstream `history_truncation_token_limit` set by the caller.
pub(super) fn truncation_metadata(truncation: OutputTruncation) -> CodexHarnessMetadata {
    CodexHarnessMetadata {
        history_truncation_policy: Some(truncation.policy),
        history_truncation_max_lines: truncation.max_lines,
        history_truncation_mcp_max_lines: truncation.mcp_max_lines,
        ..Default::default()
    }
}

impl ToolRegistry {
    /// Rejects a Code Mode call of a direct-only tool before hooks and execution, so the
    /// model calls it directly instead of looping through `exec` or `tools.*`.
    pub(super) fn fork_reject_direct_only(
        &self,
        invocation: &ToolInvocation,
        otel: &SessionTelemetry,
        sandbox_tags: SandboxTags,
        dispatch_trace: &ToolDispatchTrace,
    ) -> Result<(), FunctionCallError> {
        let tool_name = &invocation.tool_name;
        if !matches!(&invocation.source, ToolCallSource::CodeMode { .. })
            || self
                .registered_tool(tool_name)
                .is_none_or(|tool| tool.exposure != ToolExposure::DirectModelOnly)
        {
            return Ok(());
        }
        let code_mode_name = codex_tools::code_mode_name_for_tool_name(tool_name);
        let message = serde_json::json!({
            "error": {
                "code": "direct_tool_required",
                "tool": code_mode_name,
                "message": format!(
                    "Call `{code_mode_name}` directly; it cannot be invoked through `exec` or `tools.*`."
                ),
            }
        })
        .to_string();
        let log_payload = tool_log_payload(&invocation.payload, &invocation.source);
        let mut tool_result_tags = Vec::with_capacity(2);
        sandbox_tags.append_metric_tags(&mut tool_result_tags);
        otel.tool_result_with_tags(
            tool_name,
            &invocation.call_id,
            log_payload.as_ref(),
            Duration::ZERO,
            /*success*/ false,
            &message,
            &tool_result_tags,
            /*extra_trace_fields*/ &[],
        );
        let err = FunctionCallError::RespondToModel(message);
        dispatch_trace.record_failed(&err);
        Err(err)
    }
}
