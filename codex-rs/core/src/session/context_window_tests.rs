use super::*;
use codex_protocol::config_types::AutoCompactTokenLimitScope;
use std::sync::Arc;

async fn total_scope_turn(auto_limit: i64, full_limit: i64) -> TurnContext {
    let (_, mut turn) = crate::session::tests::make_session_and_context().await;
    let model_info = Arc::make_mut(&mut turn.model_info);
    model_info.auto_compact_token_limit = Some(auto_limit);
    model_info.context_window = Some(full_limit);
    model_info.effective_context_window_percent = 100;
    turn
}

async fn body_scope_turn(auto_limit: i64, full_limit: i64) -> TurnContext {
    let (_, mut turn) = crate::session::tests::make_session_and_context().await;
    Arc::make_mut(&mut turn.config).model_auto_compact_token_limit = Some(auto_limit);
    Arc::make_mut(&mut turn.config).model_auto_compact_token_limit_scope =
        AutoCompactTokenLimitScope::BodyAfterPrefix;
    let model_info = Arc::make_mut(&mut turn.model_info);
    model_info.context_window = Some(full_limit);
    model_info.effective_context_window_percent = 100;
    turn
}

#[tokio::test]
async fn total_scope_requires_auto_and_full_window_headroom() {
    let turn = total_scope_turn(/*auto_limit*/ 100_000, /*full_limit*/ 120_000).await;
    let status = context_window_token_status_for_usage(
        &turn, /*active_context_tokens*/ 80_001,
        /*auto_compact_window_prefill_tokens*/ None,
    );

    assert_eq!(status.auto_compact_scope_tokens, 80_001);
    assert_eq!(status.full_context_window_limit, Some(120_000));
}

#[tokio::test]
async fn body_after_prefix_uses_candidate_as_new_baseline_but_keeps_full_window_headroom() {
    let turn = body_scope_turn(/*auto_limit*/ 20_000, /*full_limit*/ 100_000).await;
    let candidate_tokens = 80_001;
    let status =
        context_window_token_status_for_usage(&turn, candidate_tokens, Some(candidate_tokens));

    assert_eq!(status.auto_compact_scope_tokens, 0);
    assert_eq!(status.full_context_window_limit, Some(100_000));
}
