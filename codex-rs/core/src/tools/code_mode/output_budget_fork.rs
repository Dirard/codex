//! Fork: the default output budget of a Code Mode cell follows the output truncation the user
//! configured (`tool_output_token_limit`, `[output_truncation] max_bytes`), and the `exec` and
//! `wait` descriptions name it (specs/code-mode-cell-output-budget-2026-10-29.md,
//! docs/fork/03-tools-mcp-code-mode.md F1).
//!
//! [`default_cell_policy`] decides the budget for both `truncate_code_mode_result` and the
//! descriptions, so they never disagree. The descriptions are rewritten after they are built, so
//! the upstream builders keep their signatures; a catalog override without the bundled sentence
//! stays as it is.

use codex_protocol::openai_models::ModelInfo;
use codex_tools::ToolSpec;
use codex_utils_output_truncation::TruncationPolicy;

use crate::session::turn_context::TurnContext;
use crate::tools::handlers::CodeModeWaitHandler;
use crate::unified_exec::DEFAULT_MAX_OUTPUT_TOKENS;

/// The budget of a Code Mode cell without the `max_output_tokens` pragma, on a step of
/// `turn_context` with `model_info`. The step's output truncation policy applies only when the
/// user configured output truncation (`tool_output_token_limit` or `[output_truncation]
/// max_bytes`); otherwise the upstream `Tokens(DEFAULT_MAX_OUTPUT_TOKENS)`, whatever the model's
/// own policy (R1.3: a fallback model cuts tool output at 10 000 bytes, its cells at 10 000
/// tokens).
pub(super) fn default_cell_policy(
    turn_context: &TurnContext,
    model_info: &ModelInfo,
) -> TruncationPolicy {
    let config = &turn_context.config;
    if config.tool_output_token_limit.is_none() && config.output_truncation.max_bytes.is_none() {
        return TruncationPolicy::Tokens(DEFAULT_MAX_OUTPUT_TOKENS);
    }
    turn_context.output_truncation_for_model(model_info).policy
}

/// The bundled default budget of every sentence below.
const BUNDLED_DEFAULT: &str = "Defaults to 10000 tokens.";
/// The budget sentence of the bundled `exec` description (`codex_code_mode` template).
const EXEC_BUDGET_SENTENCE: &str = "`max_output_tokens` sets the token budget for direct `exec` results. Defaults to 10000 tokens.";
/// The budget sentence of the bundled `wait` description (`codex_code_mode` template).
const WAIT_BUDGET_SENTENCE: &str =
    "`max_tokens` limits how much new output this wait call returns. Defaults to 10000 tokens.";
/// The bundled description of the `wait` parameter `max_tokens` (`wait_spec.rs`).
const WAIT_MAX_TOKENS_DESCRIPTION: &str =
    "Output token budget for this wait call. Defaults to 10000 tokens.";
const WAIT_MAX_TOKENS_PARAMETER: &str = "max_tokens";

/// The budget of a Code Mode cell without the `max_output_tokens` pragma ([`default_cell_policy`])
/// in tokens; `Bytes(b)` counts about `b / 4` tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CodeModeOutputBudget {
    default_tokens: usize,
}

impl CodeModeOutputBudget {
    /// The budget of a step of `turn_context` on `model_info`.
    pub(crate) fn new(turn_context: &TurnContext, model_info: &ModelInfo) -> Self {
        Self::from_tokens(default_cell_policy(turn_context, model_info).token_budget())
    }

    pub(crate) const fn from_tokens(default_tokens: usize) -> Self {
        Self { default_tokens }
    }

    /// The built `exec` description with this default budget.
    pub(crate) fn exec_description(self, description: String) -> String {
        self.replace_in(description, EXEC_BUDGET_SENTENCE)
    }

    /// `CodeModeWaitHandler::new` whose description and `max_tokens` parameter name this
    /// default budget.
    pub(crate) fn wait_handler(
        self,
        description_override: Option<&str>,
        parameters_override: Option<&str>,
    ) -> CodeModeWaitHandler {
        let mut handler = CodeModeWaitHandler::new(description_override, parameters_override);
        let ToolSpec::Function(tool) = &mut handler.spec else {
            return handler;
        };
        tool.description =
            self.replace_in(std::mem::take(&mut tool.description), WAIT_BUDGET_SENTENCE);
        if let Some(parameter) = tool
            .parameters
            .properties
            .as_mut()
            .and_then(|properties| properties.get_mut(WAIT_MAX_TOKENS_PARAMETER))
            && let Some(description) = parameter.description.as_mut()
        {
            *description = self
                .replace_in(description.to_string(), WAIT_MAX_TOKENS_DESCRIPTION)
                .into_boxed_str();
        }
        handler
    }

    /// Replaces the bundled default inside `sentence`, if `text` contains that sentence.
    fn replace_in(self, text: String, sentence: &str) -> String {
        if !text.contains(sentence) {
            return text;
        }
        let default = format!("Defaults to {} tokens.", self.default_tokens);
        if default == BUNDLED_DEFAULT {
            return text;
        }
        text.replace(sentence, &sentence.replace(BUNDLED_DEFAULT, &default))
    }
}

#[cfg(test)]
#[path = "output_budget_fork_tests.rs"]
mod tests;
