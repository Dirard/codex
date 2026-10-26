//! Fork: tolerant usage of `response.completed` and `response.incomplete`
//! (docs/fork/02-compaction-context.md F8, R7).
//!
//! OpenAI-compatible providers report partial usage, for example only `input_tokens`. Dropping
//! all of it left auto-compaction without a context size, so the reported counters are kept,
//! the missing ones are derived from them, and malformed details cost only the details.

use codex_protocol::protocol::TokenUsage;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use super::ResponseCompletedUsage;

const INPUT_TOKENS: &str = "input_tokens";
const OUTPUT_TOKENS: &str = "output_tokens";
const TOTAL_TOKENS: &str = "total_tokens";

/// Optional fields of the upstream usage shape; a malformed one is dropped, not the usage.
const OPTIONAL_FIELDS: [&str; 3] = [
    "input_tokens_details",
    "output_tokens_details",
    "codex_rollout_budget_units",
];

/// Token usage of a response, or `None` when it reports neither input nor total tokens: such
/// usage says nothing about the context size, and core estimates the context instead.
pub(super) fn token_usage(usage: &Value) -> Option<TokenUsage> {
    let Value::Object(fields) = usage else {
        return None;
    };
    let counter = |key| fields.get(key).and_then(Value::as_i64);
    let (input, output, total) = match (
        counter(INPUT_TOKENS),
        counter(OUTPUT_TOKENS),
        counter(TOTAL_TOKENS),
    ) {
        (None, _, None) => {
            debug!("response usage reports neither input nor total tokens: {usage}");
            return None;
        }
        (input, output, Some(total)) => {
            let input = input.unwrap_or_else(|| total.saturating_sub(output.unwrap_or(0)).max(0));
            let output = output.unwrap_or_else(|| total.saturating_sub(input).max(0));
            (input, output, total)
        }
        (Some(input), output, None) => {
            let output = output.unwrap_or(0);
            (input, output, input.saturating_add(output))
        }
    };
    let mut fields = fields.clone();
    fields.insert(INPUT_TOKENS.to_string(), input.into());
    fields.insert(OUTPUT_TOKENS.to_string(), output.into());
    fields.insert(TOTAL_TOKENS.to_string(), total.into());
    let mut usage = Value::Object(fields);
    ResponseCompletedUsage::deserialize(&usage)
        .or_else(|err| {
            debug!("dropping malformed details of response usage: {err}");
            if let Value::Object(fields) = &mut usage {
                for field in OPTIONAL_FIELDS {
                    fields.remove(field);
                }
            }
            ResponseCompletedUsage::deserialize(&usage)
        })
        .inspect_err(|err| debug!("failed to parse response usage: {err}"))
        .ok()
        .map(Into::into)
}
