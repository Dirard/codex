//! Fork: `/code-review [effort] [target]` (`specs/builtin-workflows-2026-11-07.md`, UC9).
//!
//! The command submits the canonical review turn of `codex_workflow::review_turn_text`, which
//! asks the model to start the built-in `review` workflow, the same text `codex exec review
//! --workflow` sends (R9.4). It exists only while both `features.workflows` and
//! `features.builtin_workflows` are enabled (R9.1); upstream `/review` is unchanged (R9.3).

use codex_features::Feature;
use codex_workflow::REVIEW_EFFORTS;
use serde_json::Map;
use serde_json::Value;

use super::ChatWidget;

/// R9.2: a first word equal, ignoring case, to an effort level is `effort`; the trimmed rest,
/// when not empty, is `target`.
pub(crate) fn parse_code_review_args(args: &str) -> Value {
    let mut parsed = Map::new();
    let trimmed = args.trim();
    let (first, rest) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    let lowered = first.to_ascii_lowercase();
    let target = if let Some(effort) = REVIEW_EFFORTS
        .iter()
        .find(|effort| **effort == lowered.as_str())
    {
        parsed.insert("effort".to_string(), Value::String((*effort).to_string()));
        rest.trim()
    } else {
        trimmed
    };
    if !target.is_empty() {
        parsed.insert("target".to_string(), Value::String(target.to_string()));
    }
    Value::Object(parsed)
}

impl ChatWidget {
    /// R9.1: `/code-review` needs both workflow features.
    pub(crate) fn fork_code_review_enabled(&self) -> bool {
        self.config.features.enabled(Feature::Workflows)
            && self.config.features.enabled(Feature::BuiltinWorkflows)
    }

    /// R9.1: show or hide `/code-review` after either feature changes.
    pub(crate) fn fork_sync_code_review_command(&mut self) {
        let enabled = self.fork_code_review_enabled();
        self.bottom_pane.set_code_review_command_enabled(enabled);
    }

    /// UC9, R9.2, R9.4: submit the canonical review turn for the command's arguments.
    pub(super) fn fork_code_review(&mut self, args: &str) {
        if !self.fork_code_review_enabled() {
            return;
        }
        let text = codex_workflow::review_turn_text(&parse_code_review_args(args));
        self.submit_user_message(text.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    /// R9.2: the examples of the specification.
    #[test]
    fn code_review_args_follow_the_examples() {
        for (input, expected) in [
            ("", json!({})),
            ("high", json!({ "effort": "high" })),
            ("MAX main", json!({ "effort": "max", "target": "main" })),
            (
                "security of src/auth",
                json!({ "target": "security of src/auth" }),
            ),
            (
                "  low   the parser  ",
                json!({ "effort": "low", "target": "the parser" }),
            ),
            ("highest", json!({ "target": "highest" })),
        ] {
            assert_eq!(parse_code_review_args(input), expected, "{input:?}");
        }
    }
}
