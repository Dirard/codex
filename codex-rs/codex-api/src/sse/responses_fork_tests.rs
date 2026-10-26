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
            token_usage: None,
            end_turn: Some(true),
            ..
        } if response_id == "resp1"
    );
}
