//! Fork: tolerant usage of `response.completed` and `response.incomplete`
//! (docs/fork/02-compaction-context.md F8, R7).
//!
//! OpenAI-compatible providers report partial usage, for example only `input_tokens`. Dropping
//! all of it left auto-compaction without a context size, so the reported counters are kept,
//! the missing ones are derived from them, and a malformed optional field costs only itself.

use codex_protocol::protocol::TokenUsage;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use tracing::debug;

use super::ResponseCompletedUsage;

const INPUT_TOKENS: &str = "input_tokens";
const OUTPUT_TOKENS: &str = "output_tokens";
const TOTAL_TOKENS: &str = "total_tokens";
const OUTPUT_TOKENS_DETAILS: &str = "output_tokens_details";
const REASONING_TOKENS: &str = "reasoning_tokens";

/// Optional fields of the upstream usage shape; a malformed one is dropped, not the usage.
const OPTIONAL_FIELDS: [&str; 3] = [
    "input_tokens_details",
    OUTPUT_TOKENS_DETAILS,
    "codex_rollout_budget_units",
];

/// Token usage of a response, or `None` when it reports none of input, output and total
/// tokens.
///
/// Missing counters are derived so that `total = input + output` holds, as it does for the
/// Responses API. Output that is not reported is at least the reported reasoning, which it
/// includes. A reported total below its parts, such as `0`, is not a total and is replaced.
/// Usage that reports only output keeps it for spending with `input_tokens = 0`: it says
/// nothing about the context size, and core estimates the context as for zero input
/// (docs/fork/02-compaction-context.md R7, `fork_estimate_usage`).
pub(super) fn token_usage(usage: &Value) -> Option<TokenUsage> {
    let Value::Object(fields) = usage else {
        return None;
    };
    let mut fields = fields.clone();
    // A `null` detail counter is an absent counter, as at the top level.
    for field in OPTIONAL_FIELDS {
        if let Some(Value::Object(details)) = fields.get_mut(field) {
            for counter in details.values_mut().filter(|counter| counter.is_null()) {
                *counter = 0.into();
            }
        }
    }
    let counter = |key| fields.get(key).and_then(Value::as_i64);
    let reasoning = fields
        .get(OUTPUT_TOKENS_DETAILS)
        .and_then(|details| details.get(REASONING_TOKENS))
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    let (input, output, total) = match (
        counter(INPUT_TOKENS),
        counter(OUTPUT_TOKENS),
        counter(TOTAL_TOKENS),
    ) {
        (None, None, None) => {
            debug!("response usage reports neither input, output nor total tokens: {usage}");
            return None;
        }
        (None, Some(output), None) => (0, output, None),
        (Some(input), Some(output), total) => (input, output, total),
        (Some(input), None, total) => {
            let output = total.map_or(0, |total| total.saturating_sub(input));
            (input, output.max(reasoning), total)
        }
        (None, output, Some(total)) => {
            let output = output.unwrap_or(reasoning);
            (total.saturating_sub(output).max(0), output, Some(total))
        }
    };
    let parts = input.saturating_add(output);
    let total = total.filter(|total| *total >= parts).unwrap_or(parts);
    fields.insert(INPUT_TOKENS.to_string(), input.into());
    fields.insert(OUTPUT_TOKENS.to_string(), output.into());
    fields.insert(TOTAL_TOKENS.to_string(), total.into());
    if let Err(err) = ResponseCompletedUsage::deserialize(&Value::Object(fields.clone())) {
        debug!("dropping malformed optional fields of response usage: {err}");
        drop_malformed_optional_fields(&mut fields);
    }
    ResponseCompletedUsage::deserialize(&Value::Object(fields))
        .inspect_err(|err| debug!("failed to parse response usage: {err}"))
        .ok()
        .map(Into::into)
}

/// Drops each optional field that the upstream shape rejects on its own, keeping the others.
fn drop_malformed_optional_fields(fields: &mut Map<String, Value>) {
    let counters: Map<String, Value> = [INPUT_TOKENS, OUTPUT_TOKENS, TOTAL_TOKENS]
        .into_iter()
        .filter_map(|key| Some((key.to_string(), fields.get(key)?.clone())))
        .collect();
    for field in OPTIONAL_FIELDS {
        let Some(value) = fields.get(field) else {
            continue;
        };
        let mut probe = counters.clone();
        probe.insert(field.to_string(), value.clone());
        if let Err(err) = ResponseCompletedUsage::deserialize(&Value::Object(probe)) {
            debug!("dropping malformed {field} of response usage: {err}");
            fields.remove(field);
        }
    }
}
