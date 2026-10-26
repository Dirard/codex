//! Fork: history replays the output truncation a tool output was recorded with
//! (docs/fork/03-tools-mcp-code-mode.md F2).

use codex_history::CodexHarnessMetadata;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_function_output_payload_lines;

/// Cuts the lines of a recorded tool output and returns the byte/token policy that bounds it
/// next. A policy or line limits recorded with the output win over the session's, so replay
/// after a model or config change truncates exactly as the originating turn did. `policy` is
/// the upstream choice: the recorded token limit, else the session policy with its allowance.
pub(super) fn apply_recorded_lines(
    output: &mut FunctionCallOutputPayload,
    metadata: Option<&CodexHarnessMetadata>,
    session: OutputTruncation,
    policy: TruncationPolicy,
) -> TruncationPolicy {
    let recorded = metadata.filter(|metadata| {
        metadata.history_truncation_policy.is_some()
            || metadata.history_truncation_max_lines.is_some()
            || metadata.history_truncation_mcp_max_lines.is_some()
    });
    let max_lines = match recorded {
        Some(metadata) => metadata.history_truncation_max_lines,
        None => session.max_lines,
    };
    truncate_function_output_payload_lines(output, max_lines);
    recorded
        .and_then(|metadata| metadata.history_truncation_policy)
        .unwrap_or(policy)
}
