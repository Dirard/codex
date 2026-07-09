use super::*;
use crate::INVALID_PARAMS_ERROR_CODE;
use pretty_assertions::assert_eq;
use std::collections::HashMap;

const MODEL_VISIBLE_ITEM_TOKEN_CAP: usize = 10_000;
const _: () = assert!(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES < MODEL_VISIBLE_ITEM_TOKEN_CAP);

#[test]
fn validate_additional_context_limits_accepts_below_and_at_limits() {
    assert!(validate_additional_context_limits(&None).is_ok());

    let cases = [
        context_with_entry_count(MAX_ADDITIONAL_CONTEXT_ENTRIES),
        context_with_key_len(MAX_ADDITIONAL_CONTEXT_KEY_BYTES),
        context_with_value_len(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES),
        context_at_total_limit(),
    ];

    for additional_context in cases {
        assert!(validate_additional_context_limits(&Some(additional_context)).is_ok());
    }
}

#[test]
fn validate_additional_context_limits_rejects_over_limits() {
    let cases = [
        context_with_entry_count(MAX_ADDITIONAL_CONTEXT_ENTRIES + 1),
        context_with_key_len(MAX_ADDITIONAL_CONTEXT_KEY_BYTES + 1),
        context_with_value_len(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES + 1),
        context_over_total_limit(),
    ];

    for additional_context in cases {
        let err = validate_additional_context_limits(&Some(additional_context))
            .expect_err("over-limit additional context should be rejected");
        assert_eq!(err.code, INVALID_PARAMS_ERROR_CODE);
        assert_eq!(
            err.data
                .as_ref()
                .and_then(|data| data.get("input_error_code"))
                .and_then(serde_json::Value::as_str),
            Some(ADDITIONAL_CONTEXT_TOO_LARGE_ERROR_CODE)
        );
    }
}

fn context_with_entry_count(count: usize) -> HashMap<String, AdditionalContextEntry> {
    (0..count)
        .map(|i| (format!("source_{i}"), context_entry("x")))
        .collect()
}

fn context_with_key_len(len: usize) -> HashMap<String, AdditionalContextEntry> {
    HashMap::from([(key_with_len(len, 0), context_entry("x"))])
}

fn context_with_value_len(len: usize) -> HashMap<String, AdditionalContextEntry> {
    HashMap::from([("source".to_string(), context_entry(&"x".repeat(len)))])
}

fn context_at_total_limit() -> HashMap<String, AdditionalContextEntry> {
    let context = (0..4)
        .map(|i| (key_with_len(24, i), context_entry(&"x".repeat(1000))))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        total_context_bytes(&context),
        MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES
    );
    context
}

fn context_over_total_limit() -> HashMap<String, AdditionalContextEntry> {
    let context = (0..5)
        .map(|i| (key_with_len(24, i), context_entry(&"x".repeat(900))))
        .collect::<HashMap<_, _>>();
    assert!(total_context_bytes(&context) > MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES);
    context
}

fn context_entry(value: &str) -> AdditionalContextEntry {
    AdditionalContextEntry {
        value: value.to_string(),
        kind: AdditionalContextKind::Untrusted,
    }
}

fn key_with_len(len: usize, index: usize) -> String {
    let prefix = format!("{index:02}");
    format!("{prefix}{}", "k".repeat(len - prefix.len()))
}

fn total_context_bytes(context: &HashMap<String, AdditionalContextEntry>) -> usize {
    context
        .iter()
        .map(|(key, entry)| key.len() + entry.value.len())
        .sum()
}
