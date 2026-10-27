//! Fork R7 (docs/fork/02-compaction-context.md): the context estimate of a sampled response
//! whose usage does not tell the context size, and the window baseline it starts.

use std::sync::Arc;

use codex_login::CodexAuth;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;
use tokio_util::sync::CancellationToken;

use super::*;
use crate::session::TokenUsageContext;
use crate::session::context_window::context_window_token_status;

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

/// A session of a provider that is not the first-party one when `first_party` is false.
async fn session_for(
    scope: AutoCompactTokenLimitScope,
    first_party: bool,
) -> (Arc<Session>, Arc<StepContext>) {
    let (sess, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
            CodexAuth::from_api_key("Test API Key"),
            Vec::new(),
            move |config| {
                config.model_auto_compact_token_limit_scope = scope;
                if !first_party {
                    config.model_provider.name = "OpenAI-compatible (test)".to_string();
                }
            },
        )
        .await;
    let step = sess
        .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
        .await
        .expect("capture step");
    assert_eq!(step.turn.provider.info().is_openai(), first_party);
    (sess, step)
}

async fn session(scope: AutoCompactTokenLimitScope) -> (Arc<Session>, Arc<StepContext>) {
    session_for(scope, /*first_party*/ true).await
}

/// The request input the history makes now, as the sampling step sends it.
async fn request_input(sess: &Session, step: &StepContext) -> Vec<ResponseItem> {
    sess.clone_history()
        .await
        .for_prompt(&step.settings.model_info.input_modalities)
}

/// The estimate of a request with these items, base instructions and tools.
async fn estimate_of(sess: &Session, step: &StepContext, items: &[ResponseItem]) -> i64 {
    let base_instructions = sess.get_base_instructions().await;
    let items = items
        .iter()
        .cloned()
        .map(ResponseItemEnvelope::new)
        .collect::<Vec<_>>();
    crate::compact_remote_v2::estimate_candidate_request_tokens(
        &items,
        &base_instructions,
        &step.settings.model_info,
        crate::compact_remote_v2::tool_declaration_tokens(step),
    )
}

/// The estimate of the whole request the history and tools make now.
async fn request_estimate(sess: &Session, step: &StepContext) -> i64 {
    let items = request_input(sess, step).await;
    estimate_of(sess, step, &items).await
}

async fn prefill(sess: &Session) -> Option<i64> {
    sess.auto_compact_window_snapshot()
        .await
        .prefill_input_tokens
}

/// Tokens counted against the auto-compact limit of the current scope.
async fn scope_tokens(sess: &Session, step: &StepContext) -> i64 {
    context_window_token_status(sess, &step.turn)
        .await
        .auto_compact_scope_tokens
}

/// Records a sampled response the way the sampling step does: its items, its usage, the hook.
async fn sample(
    sess: &Session,
    step: &StepContext,
    items: &[ResponseItem],
    usage: Option<TokenUsage>,
) -> Vec<ResponseItem> {
    let request = request_input(sess, step).await;
    sess.record_conversation_items(&step.turn, &step.settings.model_info, items)
        .await;
    sess.record_token_usage_info(
        &step.turn,
        &step.settings,
        usage.as_ref(),
        TokenUsageContext::Sampling,
    )
    .await
    .expect("record usage");
    sess.fork_estimate_usage(step, &usage, &request).await;
    request
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
    let request = request_input(&sess, &step).await;

    // Reported usage stays on the upstream path.
    sess.fork_estimate_usage(&step, &Some(TokenUsage::default()), &request)
        .await;
    assert_eq!(sess.token_usage_info().await, None);

    sess.fork_estimate_usage(&step, &None, &request).await;
    let expected = request_estimate(&sess, &step).await;
    assert!(expected > 6_000, "{expected}");
    assert_eq!(sess.get_total_token_usage().await, expected);
    assert_eq!(prefill(&sess).await, None);
}

/// Review R7-2: the first response of a window answers a request without its own output, so
/// that output is growth of the window, as with server usage.
#[tokio::test]
async fn unreported_usage_counts_the_response_output_as_window_growth() {
    let (sess, step) = session(AutoCompactTokenLimitScope::BodyAfterPrefix).await;
    sess.replace_history(
        vec![message("user", "draft the notes")],
        /*reference_context_item*/ None,
    )
    .await;
    let answer = message("assistant", &"long answer ".repeat(4_000));

    let request = sample(&sess, &step, std::slice::from_ref(&answer), None).await;
    let baseline = estimate_of(&sess, &step, &request).await;
    assert_eq!(prefill(&sess).await, Some(baseline));
    let growth = scope_tokens(&sess, &step).await;
    assert!(growth > 10_000, "{growth}");
    assert_eq!(sess.get_total_token_usage().await, baseline + growth);

    // Later growth counts against the scope instead of moving its start.
    sample(
        &sess,
        &step,
        &[message("assistant", &"more ".repeat(1_000))],
        None,
    )
    .await;
    assert_eq!(prefill(&sess).await, Some(baseline));
    assert!(scope_tokens(&sess, &step).await > growth);
}

/// Review R7-1: upstream estimates the baseline of a compacted or resumed window without tool
/// declarations, while the response estimate counts them; they are not growth of the window.
#[tokio::test]
async fn unreported_usage_rebases_a_baseline_without_tool_declarations() {
    let (sess, step) = session(AutoCompactTokenLimitScope::BodyAfterPrefix).await;
    sess.replace_history(
        vec![
            message("user", "draft the notes"),
            message("assistant", "working"),
        ],
        /*reference_context_item*/ None,
    )
    .await;
    sess.recompute_token_usage(&step.turn).await;
    let tools = crate::compact_remote_v2::tool_declaration_tokens(&step);
    assert!(tools > 0, "{tools}");

    // A response that adds nothing leaves the window without growth.
    sample(&sess, &step, &[], None).await;

    assert_eq!(scope_tokens(&sess, &step).await, 0);
    assert_eq!(
        prefill(&sess).await,
        Some(request_estimate(&sess, &step).await)
    );
}

/// Review R7-3: usage of another provider with input but no output counts the input it
/// reports and the estimated output of the response; spending keeps the reported counters.
#[tokio::test]
async fn usage_without_output_adds_the_estimated_output() {
    let (sess, step) = session_for(
        AutoCompactTokenLimitScope::Total,
        /*first_party*/ false,
    )
    .await;
    sess.replace_history(
        vec![message("user", "draft the notes")],
        /*reference_context_item*/ None,
    )
    .await;
    let reported = TokenUsage {
        input_tokens: 1_000,
        total_tokens: 1_000,
        ..TokenUsage::default()
    };
    let answer = message("assistant", &"long answer ".repeat(4_000));

    let request = sample(
        &sess,
        &step,
        std::slice::from_ref(&answer),
        Some(reported.clone()),
    )
    .await;

    let output = request_estimate(&sess, &step).await - estimate_of(&sess, &step, &request).await;
    assert!(output > 10_000, "{output}");
    assert_eq!(sess.get_total_token_usage().await, 1_000 + output);
    let info = sess.token_usage_info().await.expect("token info");
    assert_eq!(info.total_token_usage, reported);
    assert_eq!(
        info.last_token_usage,
        TokenUsage {
            total_tokens: 1_000 + output,
            ..reported
        }
    );
}

/// Review R7-4: zero usage of another provider counted nothing, so the context is estimated,
/// and its zero "server" baseline does not replace the window's estimated one.
#[tokio::test]
async fn zero_usage_is_estimated_and_keeps_the_window_baseline() {
    let (sess, step) = session_for(
        AutoCompactTokenLimitScope::BodyAfterPrefix,
        /*first_party*/ false,
    )
    .await;
    sess.replace_history(
        vec![message("user", "draft the notes")],
        /*reference_context_item*/ None,
    )
    .await;

    let request = sample(
        &sess,
        &step,
        &[message("assistant", "working")],
        Some(TokenUsage::default()),
    )
    .await;
    let baseline = estimate_of(&sess, &step, &request).await;
    assert_eq!(prefill(&sess).await, Some(baseline));
    assert_eq!(
        sess.get_total_token_usage().await,
        request_estimate(&sess, &step).await
    );

    sample(
        &sess,
        &step,
        &[message("assistant", &"long answer ".repeat(4_000))],
        Some(TokenUsage::default()),
    )
    .await;
    assert_eq!(prefill(&sess).await, Some(baseline));
    assert!(scope_tokens(&sess, &step).await > 10_000);
}

/// First-party usage is complete by contract: it stays as upstream reads it, also when it is
/// zero, as in test fixtures.
#[tokio::test]
async fn first_party_usage_stays_as_reported() {
    let (sess, step) = session(AutoCompactTokenLimitScope::Total).await;
    sess.replace_history(
        vec![message("user", "draft the notes")],
        /*reference_context_item*/ None,
    )
    .await;
    let reported = TokenUsage {
        input_tokens: 1_000,
        total_tokens: 1_000,
        ..TokenUsage::default()
    };

    sample(
        &sess,
        &step,
        &[message("assistant", &"long answer ".repeat(4_000))],
        Some(reported.clone()),
    )
    .await;
    assert_eq!(sess.get_total_token_usage().await, 1_000);

    sample(&sess, &step, &[], Some(TokenUsage::default())).await;
    assert_eq!(sess.get_total_token_usage().await, 0);
}

/// R7 follow-up: usage that reports only output (`codex-api` keeps it with `input_tokens = 0`)
/// does not tell the context size from any provider, so the context is estimated as without
/// usage, and the output stays for spending. Before, `codex-api` dropped such usage, and its
/// output tokens were never spent.
#[tokio::test]
async fn output_only_usage_is_estimated_and_spent() {
    for first_party in [true, false] {
        let (sess, step) =
            session_for(AutoCompactTokenLimitScope::BodyAfterPrefix, first_party).await;
        sess.replace_history(
            vec![message("user", "draft the notes")],
            /*reference_context_item*/ None,
        )
        .await;
        let reported = TokenUsage {
            output_tokens: 500,
            total_tokens: 500,
            ..TokenUsage::default()
        };

        let request = sample(
            &sess,
            &step,
            &[message("assistant", &"long answer ".repeat(4_000))],
            Some(reported),
        )
        .await;
        assert_eq!(
            sess.get_total_token_usage().await,
            request_estimate(&sess, &step).await,
            "first party {first_party}"
        );
        assert!(
            scope_tokens(&sess, &step).await > 2_000,
            "first party {first_party}"
        );
        assert_eq!(
            prefill(&sess).await,
            Some(estimate_of(&sess, &step, &request).await),
            "first party {first_party}"
        );
        let spent = sess.total_token_usage().await.expect("usage");
        assert_eq!(
            (spent.input_tokens, spent.output_tokens),
            (0, 500),
            "first party {first_party}"
        );
    }
}
