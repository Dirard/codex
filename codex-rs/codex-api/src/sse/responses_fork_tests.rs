//! Fork: a partial `response.completed.usage` drops only the usage, not the response
//! (docs/fork/02-compaction-context.md F8).

use super::*;
use assert_matches::assert_matches;
use codex_client::TransportError;
use futures::TryStreamExt;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;

/// Same as the `run_sse` helper of the upstream `tests` module, which is private to it.
async fn run_sse(events: Vec<serde_json::Value>) -> Vec<ResponseEvent> {
    let mut body = String::new();
    for e in events {
        let kind = e
            .get("type")
            .and_then(|v| v.as_str())
            .expect("fixture event missing type");
        if e.as_object().map(|o| o.len() == 1).unwrap_or(false) {
            body.push_str(&format!("event: {kind}\n\n"));
        } else {
            body.push_str(&format!("event: {kind}\ndata: {e}\n\n"));
        }
    }

    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(8);
    let stream = ReaderStream::new(std::io::Cursor::new(body))
        .map_err(|err| TransportError::Network(err.to_string()));
    tokio::spawn(process_sse(
        Box::pin(stream),
        tx,
        Duration::from_millis(1000),
        /*telemetry*/ None,
    ));

    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        out.push(ev.expect("channel closed"));
    }
    out
}

#[tokio::test]
async fn parses_completed_with_partial_usage() {
    let events = run_sse(vec![json!({
        "type": "response.completed",
        "response": {
            "id": "resp1",
            "usage": { "input_tokens": 3 },
            "end_turn": true,
        },
    })])
    .await;

    assert_eq!(events.len(), 1);
    assert_matches!(
        &events[0],
        ResponseEvent::Completed {
            response_id,
            token_usage: Some(_),
            end_turn: Some(true),
            ..
        } if response_id == "resp1"
    );
}

/// Usage reported by a `response.completed` or `response.incomplete` event.
async fn reported_usage(kind: &str, usage: serde_json::Value) -> Option<TokenUsage> {
    let mut response = json!({ "id": "resp1", "usage": usage });
    if kind == "response.incomplete" {
        response["incomplete_details"] = json!({ "reason": "max_output_tokens" });
    }
    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(8);
    let body = format!(
        "event: {kind}\ndata: {}\n\n",
        json!({ "type": kind, "response": response })
    );
    let stream = ReaderStream::new(std::io::Cursor::new(body))
        .map_err(|err| TransportError::Network(err.to_string()));
    tokio::spawn(process_sse(
        Box::pin(stream),
        tx,
        Duration::from_millis(1000),
        /*telemetry*/ None,
    ));
    match rx.recv().await.expect("one event") {
        Ok(ResponseEvent::Completed { token_usage, .. }) => token_usage,
        Err(ApiError::ResponseIncomplete(incomplete)) => incomplete.token_usage,
        other => panic!("unexpected event: {other:?}"),
    }
}

fn usage(input: i64, output: i64, total: i64) -> TokenUsage {
    TokenUsage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: total,
        ..Default::default()
    }
}

/// R7 (docs/fork/02-compaction-context.md): an OpenAI-compatible provider that always reports
/// partial usage must still feed auto-compaction, so the reported counters are kept and the
/// missing ones are derived from them.
#[tokio::test]
async fn partial_usage_keeps_reported_counters() {
    for kind in ["response.completed", "response.incomplete"] {
        for (reported, expected) in [
            (json!({ "input_tokens": 3 }), usage(3, 0, 3)),
            (
                json!({ "input_tokens": 10, "output_tokens": 4 }),
                usage(10, 4, 14),
            ),
            (
                json!({ "input_tokens": 10, "output_tokens": null, "total_tokens": 15 }),
                usage(10, 5, 15),
            ),
            (json!({ "total_tokens": 7 }), usage(7, 0, 7)),
            (
                json!({ "output_tokens": 2, "total_tokens": 9 }),
                usage(7, 2, 9),
            ),
            (
                json!({
                    "input_tokens": 10,
                    "input_tokens_details": { "cached_tokens": 6 },
                    "output_tokens": 4,
                    "output_tokens_details": { "reasoning_tokens": 1 },
                }),
                TokenUsage {
                    cached_input_tokens: 6,
                    reasoning_output_tokens: 1,
                    ..usage(10, 4, 14)
                },
            ),
            // Malformed details cost only the details.
            (
                json!({
                    "input_tokens": 10,
                    "input_tokens_details": {},
                    "output_tokens": 4,
                    "total_tokens": 14,
                }),
                usage(10, 4, 14),
            ),
        ] {
            assert_eq!(
                reported_usage(kind, reported.clone()).await,
                Some(expected),
                "{kind} {reported}"
            );
        }
    }
}

/// Review R7-3 and R7-4 (docs/fork/reviews/2026-10-26-sandbox-compaction-review.md): output
/// that is not reported is at least its reported reasoning, and a total below its parts, such
/// as `0`, is not a total.
#[tokio::test]
async fn derived_counters_stay_consistent() {
    for kind in ["response.completed", "response.incomplete"] {
        for (reported, expected) in [
            (
                json!({ "input_tokens": 10, "output_tokens_details": { "reasoning_tokens": 5 } }),
                TokenUsage {
                    reasoning_output_tokens: 5,
                    ..usage(10, 5, 15)
                },
            ),
            (
                json!({
                    "input_tokens": 10,
                    "output_tokens_details": { "reasoning_tokens": 5 },
                    "total_tokens": 12,
                }),
                TokenUsage {
                    reasoning_output_tokens: 5,
                    ..usage(10, 5, 15)
                },
            ),
            (
                json!({ "input_tokens": 10, "output_tokens": 4, "total_tokens": 0 }),
                usage(10, 4, 14),
            ),
            // Zero usage stays as reported; core decides what it says about the context.
            (
                json!({ "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 }),
                usage(0, 0, 0),
            ),
        ] {
            assert_eq!(
                reported_usage(kind, reported.clone()).await,
                Some(expected),
                "{kind} {reported}"
            );
        }
    }
}

/// Review R7-5: a malformed optional field costs only that field, and a `null` detail counter
/// is an absent counter, as at the top level.
#[tokio::test]
async fn malformed_optional_field_keeps_the_others() {
    for kind in ["response.completed", "response.incomplete"] {
        for (reported, expected) in [
            (
                json!({
                    "input_tokens": 10,
                    "input_tokens_details": { "cached_tokens": null },
                    "output_tokens": 4,
                    "output_tokens_details": { "reasoning_tokens": 3 },
                    "total_tokens": 14,
                    "codex_rollout_budget_units": 7,
                }),
                TokenUsage {
                    reasoning_output_tokens: 3,
                    codex_rollout_budget_units: Some(serde_json::Number::from(7)),
                    ..usage(10, 4, 14)
                },
            ),
            (
                json!({
                    "input_tokens": 10,
                    "input_tokens_details": { "cached_tokens": "6" },
                    "output_tokens": 4,
                    "output_tokens_details": { "reasoning_tokens": 3 },
                    "codex_rollout_budget_units": 7,
                }),
                TokenUsage {
                    reasoning_output_tokens: 3,
                    codex_rollout_budget_units: Some(serde_json::Number::from(7)),
                    ..usage(10, 4, 14)
                },
            ),
            (
                json!({
                    "input_tokens": 10,
                    "input_tokens_details": { "cached_tokens": 6 },
                    "output_tokens": 4,
                    "output_tokens_details": { "reasoning_tokens": null },
                    "codex_rollout_budget_units": "seven",
                }),
                TokenUsage {
                    cached_input_tokens: 6,
                    ..usage(10, 4, 14)
                },
            ),
        ] {
            assert_eq!(
                reported_usage(kind, reported.clone()).await,
                Some(expected),
                "{kind} {reported}"
            );
        }
    }
}

/// Usage without any of input, output and total tokens says nothing; core estimates the
/// context instead.
#[tokio::test]
async fn usage_without_counters_is_dropped() {
    for kind in ["response.completed", "response.incomplete"] {
        for reported in [
            json!({}),
            json!(null),
            json!({ "output_tokens": "2" }),
            json!({ "input_tokens": "3" }),
            json!({ "input_tokens_details": { "cached_tokens": 1 } }),
            json!(42),
        ] {
            assert_eq!(
                reported_usage(kind, reported.clone()).await,
                None,
                "{kind} {reported}"
            );
        }
    }
}

/// R7 follow-up (docs/fork/02-compaction-context.md): usage that reports only output keeps it
/// for spending. `input_tokens = 0` says that the input was not counted, so core estimates the
/// context, as for zero usage of a provider that is not the first-party one.
#[tokio::test]
async fn output_only_usage_is_kept_for_spending() {
    for kind in ["response.completed", "response.incomplete"] {
        for (reported, expected) in [
            (json!({ "output_tokens": 2 }), usage(0, 2, 2)),
            (
                json!({ "input_tokens": null, "output_tokens": 7, "total_tokens": null }),
                usage(0, 7, 7),
            ),
            (
                json!({
                    "output_tokens": 9,
                    "output_tokens_details": { "reasoning_tokens": 4 },
                }),
                TokenUsage {
                    reasoning_output_tokens: 4,
                    ..usage(0, 9, 9)
                },
            ),
        ] {
            assert_eq!(
                reported_usage(kind, reported.clone()).await,
                Some(expected),
                "{kind} {reported}"
            );
        }
    }
}

/// Tails review 2026-10-27, P3-10: a negative counter is not a count. It is ignored like an
/// absent one, so it neither subtracts from spending nor makes up a usage on its own, and
/// output that is reported alone is at least its reasoning, as when output is derived.
#[tokio::test]
async fn negative_counters_are_ignored_and_output_covers_reasoning() {
    for kind in ["response.completed", "response.incomplete"] {
        for (reported, expected) in [
            (json!({ "output_tokens": -5 }), None),
            (
                json!({ "input_tokens": -3, "output_tokens": 4 }),
                Some(usage(0, 4, 4)),
            ),
            (
                json!({ "input_tokens": 10, "output_tokens": -2, "total_tokens": 15 }),
                Some(usage(10, 5, 15)),
            ),
            (
                json!({ "input_tokens": 10, "output_tokens": 2, "total_tokens": -1 }),
                Some(usage(10, 2, 12)),
            ),
            (
                json!({
                    "output_tokens": 3,
                    "output_tokens_details": { "reasoning_tokens": 9 },
                }),
                Some(TokenUsage {
                    reasoning_output_tokens: 9,
                    ..usage(0, 9, 9)
                }),
            ),
        ] {
            assert_eq!(
                reported_usage(kind, reported.clone()).await,
                expected,
                "{kind} {reported}"
            );
        }
    }
}
