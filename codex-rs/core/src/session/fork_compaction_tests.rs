//! Fork R7 (docs/fork/02-compaction-context.md): the context estimate of a sampled response
//! that reports no usable usage.

use std::sync::Arc;

use codex_login::CodexAuth;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;
use tokio_util::sync::CancellationToken;

use super::*;

fn message(role: &str, text: &str) -> ResponseItem {
    let kind = if role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };
    serde_json::from_value(serde_json::json!({
        "type": "message",
        "role": role,
        "content": [{ "type": kind, "text": text }],
    }))
    .expect("message")
}

async fn session(scope: AutoCompactTokenLimitScope) -> (Arc<Session>, Arc<StepContext>) {
    let (sess, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
            CodexAuth::from_api_key("Test API Key"),
            Vec::new(),
            move |config| config.model_auto_compact_token_limit_scope = scope,
        )
        .await;
    let step = sess
        .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
        .await
        .expect("capture step");
    (sess, step)
}

/// The estimate of the whole request the history and tools make now.
async fn request_estimate(sess: &Session, step: &StepContext) -> i64 {
    let base_instructions = sess.get_base_instructions().await;
    let history = sess.clone_history().await;
    crate::compact_remote_v2::estimate_candidate_request_tokens(
        history.annotated_items(),
        &base_instructions,
        &step.settings.model_info,
        crate::compact_remote_v2::tool_declaration_tokens(step),
    )
}

#[tokio::test]
async fn unreported_usage_counts_the_estimated_context_once() {
    let (sess, step) = session(AutoCompactTokenLimitScope::Total).await;
    // Steered input after the latest model output is counted by the token check on its own.
    sess.replace_history(
        vec![
            message("user", "draft the notes"),
            message("assistant", "working"),
            message("user", &"large ".repeat(4_000)),
        ],
        /*reference_context_item*/ None,
    )
    .await;

    // Reported usage stays on the upstream path.
    sess.fork_estimate_usage(&step, &Some(TokenUsage::default()))
        .await;
    assert_eq!(sess.token_usage_info().await, None);

    sess.fork_estimate_usage(&step, &None).await;
    let expected = request_estimate(&sess, &step).await;
    assert!(expected > 6_000, "{expected}");
    assert_eq!(sess.get_total_token_usage().await, expected);
    assert_eq!(
        sess.auto_compact_window_snapshot()
            .await
            .prefill_input_tokens,
        None
    );
}

#[tokio::test]
async fn unreported_usage_sets_only_a_missing_window_prefill() {
    let (sess, step) = session(AutoCompactTokenLimitScope::BodyAfterPrefix).await;
    sess.replace_history(
        vec![
            message("user", "draft the notes"),
            message("assistant", "working"),
        ],
        /*reference_context_item*/ None,
    )
    .await;

    // Without server usage the first estimate of the window starts its auto-compact scope.
    sess.fork_estimate_usage(&step, &None).await;
    let prefill = request_estimate(&sess, &step).await;
    assert_eq!(
        sess.auto_compact_window_snapshot()
            .await
            .prefill_input_tokens,
        Some(prefill)
    );

    // Later growth counts against the scope instead of moving its start.
    sess.record_conversation_items(
        &step.turn,
        &step.settings.model_info,
        &[
            message("user", "more"),
            message("assistant", &"long answer ".repeat(1_000)),
        ],
    )
    .await;
    sess.fork_estimate_usage(&step, &None).await;
    assert_eq!(
        sess.auto_compact_window_snapshot()
            .await
            .prefill_input_tokens,
        Some(prefill)
    );
    assert!(sess.get_total_token_usage().await > prefill);
}
