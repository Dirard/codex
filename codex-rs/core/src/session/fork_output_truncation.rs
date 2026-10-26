//! Fork: replay-stable output truncation of tool output (docs/fork/03-tools-mcp-code-mode.md F1).

use codex_history::CodexHarnessMetadata;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::with_serialization_allowance;

use super::step_context::StepContext;

impl StepContext {
    pub(crate) fn output_truncation(&self) -> OutputTruncation {
        self.turn
            .output_truncation_for_model(&self.settings.model_info)
    }
}

/// Fork: saves the complete originating truncation in the metadata of a tool output, so replay
/// does not depend on the model or config selected after this output. A recorded policy wins;
/// otherwise a tool-specific token override is preserved. Always sets the token limit.
pub(super) fn annotate_tool_output_metadata(
    metadata: &mut CodexHarnessMetadata,
    originating_truncation: OutputTruncation,
) {
    if let Some(policy) = metadata.history_truncation_policy {
        metadata
            .history_truncation_token_limit
            .get_or_insert_with(|| policy.token_budget());
        return;
    }
    // Preserve a tool-specific token override, then save the complete policy so
    // replay does not depend on the model or config selected after this output.
    let truncation = if let Some(policy) = metadata
        .history_truncation_token_limit
        .map(TruncationPolicy::Tokens)
    {
        originating_truncation.with_policy(policy)
    } else {
        originating_truncation
            .with_policy(with_serialization_allowance(originating_truncation.policy))
    };
    metadata
        .history_truncation_token_limit
        .get_or_insert_with(|| {
            with_serialization_allowance(originating_truncation.policy).token_budget()
        });
    metadata.history_truncation_policy = Some(truncation.policy);
    metadata.history_truncation_max_lines = truncation.max_lines;
    metadata.history_truncation_mcp_max_lines = truncation.mcp_max_lines;
}
