//! Fork: the output truncation of a turn, from the model policy and `[output_truncation]`
//! (docs/fork/03-tools-mcp-code-mode.md F1).

use codex_protocol::openai_models::ModelInfo;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;

use super::TurnContext;

impl TurnContext {
    pub(crate) fn output_truncation_for_model(&self, model_info: &ModelInfo) -> OutputTruncation {
        let policy = effective_output_truncation_policy(
            model_info.truncation_policy.into(),
            self.config.output_truncation.max_bytes,
        );
        OutputTruncation::new_with_mcp_max_lines(
            policy,
            self.config.output_truncation.max_lines,
            self.config.output_truncation.mcp_max_lines,
        )
    }

    /// Legacy callers without a step still follow the turn's current model.
    pub(crate) fn output_truncation(&self) -> OutputTruncation {
        let model_info = self.capture_current_model_info();
        self.output_truncation_for_model(&model_info)
    }
}

fn effective_output_truncation_policy(
    model_policy: TruncationPolicy,
    configured_max_bytes: Option<usize>,
) -> TruncationPolicy {
    match configured_max_bytes {
        Some(configured_max_bytes) => {
            TruncationPolicy::Bytes(configured_max_bytes.min(model_policy.byte_budget()))
        }
        None => model_policy,
    }
}
