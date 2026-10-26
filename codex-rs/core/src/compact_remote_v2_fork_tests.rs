//! Fork: tests of the compaction headroom (`compact_headroom.rs`) and of the retained-message
//! budget charged by full item cost (docs/fork/02-compaction-context.md F1, F2).

use super::*;
use crate::context_manager::ContextManager;
use codex_protocol::config_types::AutoCompactTokenLimitScope;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::MessagePhase;
use codex_protocol::protocol::SessionSource;
use pretty_assertions::assert_eq;

/// Same as the `message` helper of the upstream `tests` module, which is private to it.
fn message(role: &str, text: &str, phase: Option<MessagePhase>) -> ResponseItem {
    ResponseItem::Message {
        status: None,
        encrypted_content: None,
        id: None,
        role: role.to_string(),
        content: vec![ContentItem::InputText {
            text: text.to_string(),
        }],
        phase,
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn compaction_headroom_applies_to_all_but_guardian_reviewers() {
    use codex_protocol::protocol::InternalSessionSource;
    use codex_protocol::protocol::SubAgentSource;
    for (source, enforced) in [
        (SessionSource::Cli, true),
        (SessionSource::SubAgent(SubAgentSource::Review), true),
        (
            SessionSource::SubAgent(SubAgentSource::Other("worker".to_string())),
            true,
        ),
        (
            SessionSource::SubAgent(SubAgentSource::Other(
                crate::guardian::GUARDIAN_REVIEWER_NAME.to_string(),
            )),
            false,
        ),
        (
            SessionSource::Internal(InternalSessionSource::Guardian),
            false,
        ),
    ] {
        assert_eq!(
            enforces_compaction_headroom(&source),
            enforced,
            "{source:?}"
        );
    }
}

fn token_status(
    active_context_tokens: i64,
    auto_compact_scope_limit: Option<i64>,
    full_context_window_limit: Option<i64>,
) -> crate::session::context_window::ContextWindowTokenStatus {
    buffered_token_status(
        active_context_tokens,
        auto_compact_scope_limit,
        /*fallback_buffer_tokens*/ 0,
        full_context_window_limit,
    )
}

/// A `Total` status whose trigger includes a token-budget fallback buffer.
fn buffered_token_status(
    active_context_tokens: i64,
    auto_compact_scope_limit: Option<i64>,
    fallback_buffer_tokens: i64,
    full_context_window_limit: Option<i64>,
) -> crate::session::context_window::ContextWindowTokenStatus {
    let full_context_window_limit_reached =
        full_context_window_limit.is_some_and(|limit| active_context_tokens >= limit);
    let token_limit_reached = full_context_window_limit_reached
        || auto_compact_scope_limit
            .is_some_and(|limit| active_context_tokens >= limit + fallback_buffer_tokens);
    crate::session::context_window::ContextWindowTokenStatus {
        active_context_tokens,
        auto_compact_scope_tokens: active_context_tokens,
        auto_compact_scope_limit,
        full_context_window_limit,
        base_window_tokens_remaining: None,
        auto_compact_window_prefill_tokens: None,
        full_context_window_limit_reached,
        token_limit_reached,
        turn_end_compaction_threshold_reached: false,
    }
}

/// A `BodyAfterPrefix` status of a candidate that becomes its window's prefill.
fn body_after_prefix_status(
    active_context_tokens: i64,
    full_context_window_limit: Option<i64>,
) -> crate::session::context_window::ContextWindowTokenStatus {
    let full_context_window_limit_reached =
        full_context_window_limit.is_some_and(|limit| active_context_tokens >= limit);
    crate::session::context_window::ContextWindowTokenStatus {
        active_context_tokens,
        auto_compact_scope_tokens: 0,
        auto_compact_scope_limit: Some(90_000),
        full_context_window_limit,
        base_window_tokens_remaining: None,
        auto_compact_window_prefill_tokens: Some(active_context_tokens),
        full_context_window_limit_reached,
        token_limit_reached: full_context_window_limit_reached,
        turn_end_compaction_threshold_reached: false,
    }
}

#[test]
fn soft_headroom_keeps_a_quarter_of_the_headroom_below_the_compaction_trigger() {
    // Auto limit 90k: headroom 22.5k, soft margin 5_625, ceiling 84_375. Window 100k:
    // headroom 25k, soft margin 6_250, ceiling 93_750.
    for (candidate, auto_limit, window, current, accepted) in [
        (80_000, Some(90_000), Some(100_000), 105_000, true),
        (84_375, Some(90_000), Some(100_000), 105_000, true),
        (85_000, None, None, 105_000, true),
        (93_750, None, Some(100_000), 105_000, true),
        // Just below the trigger the next step would compact again at once.
        (84_376, Some(90_000), Some(100_000), 105_000, false),
        (89_999, Some(90_000), Some(100_000), 105_000, false),
        (93_751, None, Some(100_000), 105_000, false),
        // At or above the trigger, as before.
        (90_000, Some(90_000), Some(100_000), 105_000, false),
        (95_000, Some(90_000), Some(100_000), 105_000, false),
        (100_000, None, Some(100_000), 105_000, false),
        (120_000, Some(90_000), Some(100_000), 130_000, false),
        // The candidate must shrink the context it replaces.
        (80_000, Some(90_000), Some(100_000), 80_000, false),
        (80_000, Some(90_000), Some(100_000), 60_000, false),
    ] {
        assert_eq!(
            soft_headroom_accepts(
                &token_status(candidate, auto_limit, window),
                AutoCompactTokenLimitScope::Total,
                current,
            ),
            accepted,
            "candidate={candidate} auto_limit={auto_limit:?} window={window:?} \
             current={current}"
        );
    }
}

/// A token-budget fallback buffer moves the trigger, not the soft ceiling: a candidate in
/// the buffer would start the window with no base-window tokens left.
#[test]
fn soft_headroom_ignores_the_fallback_buffer() {
    for (candidate, accepted) in [(84_375, true), (88_000, false), (94_999, false)] {
        let status = buffered_token_status(
            candidate,
            Some(90_000),
            /*fallback_buffer_tokens*/ 5_000,
            Some(100_000),
        );
        assert!(!status.token_limit_reached, "candidate={candidate}");
        assert_eq!(
            soft_headroom_accepts(&status, AutoCompactTokenLimitScope::Total, 105_000),
            accepted,
            "candidate={candidate}"
        );
    }
}

/// `BodyAfterPrefix` restarts the body count from the candidate, so only the full window
/// bounds it; the soft install still leaves a quarter of that window's headroom.
#[test]
fn body_after_prefix_soft_headroom_keeps_a_quarter_of_the_window_headroom() {
    for (candidate, window, current, accepted) in [
        (85_000, Some(100_000), 105_000, true),
        (93_750, Some(100_000), 105_000, true),
        (85_000, None, 105_000, true),
        (93_751, Some(100_000), 105_000, false),
        (99_999, Some(100_000), 105_000, false),
        (100_000, Some(100_000), 105_000, false),
        (85_000, Some(100_000), 85_000, false),
    ] {
        assert_eq!(
            soft_headroom_accepts(
                &body_after_prefix_status(candidate, window),
                AutoCompactTokenLimitScope::BodyAfterPrefix,
                current,
            ),
            accepted,
            "candidate={candidate} window={window:?} current={current}"
        );
    }
}

#[test]
fn candidate_request_estimate_counts_tools_outside_history() {
    let base_instructions = BaseInstructions {
        text: String::new(),
        provenance: None,
    };
    let user = ResponseItemEnvelope::new(message("user", "hello", /*phase*/ None));
    let declared_tools = ResponseItemEnvelope::new(ResponseItem::AdditionalTools {
        id: None,
        role: "developer".to_string(),
        tools: Arc::from([serde_json::json!({"type": "function", "name": "lookup"})]),
    });
    let mut model_info = codex_models_manager::model_info::model_info_from_slug("gpt-5.4");
    model_info.use_responses_lite = false;
    let plain = vec![user.clone()];
    let history_tokens = ContextManager::estimate_history_token_count(&plain, &base_instructions);

    assert_eq!(
        estimate_candidate_request_tokens(&plain, &base_instructions, &model_info, 700),
        history_tokens + 700
    );
    // Only Responses Lite sends no top-level tools when history already declares them.
    let incremental = vec![declared_tools, user];
    let incremental_tokens =
        ContextManager::estimate_history_token_count(&incremental, &base_instructions);
    assert_eq!(
        estimate_candidate_request_tokens(&incremental, &base_instructions, &model_info, 700),
        incremental_tokens + 700
    );
    model_info.use_responses_lite = true;
    assert_eq!(
        estimate_candidate_request_tokens(&incremental, &base_instructions, &model_info, 700),
        incremental_tokens
    );
    assert_eq!(
        estimate_candidate_request_tokens(&plain, &base_instructions, &model_info, 700),
        history_tokens + 700
    );
}

fn image_message(image_url: &str) -> ResponseItem {
    ResponseItem::Message {
        status: None,
        encrypted_content: None,
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputImage {
            image: ImageReference::Inline {
                image_url: image_url.to_string(),
            },
            detail: None,
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn mixed_text_image_message(text: &str) -> ResponseItem {
    ResponseItem::Message {
        status: None,
        encrypted_content: None,
        id: None,
        role: "user".to_string(),
        content: vec![
            ContentItem::InputText {
                text: text.to_string(),
            },
            ContentItem::InputImage {
                image: ImageReference::Inline {
                    image_url: "data:image/png;base64,AAAA".to_string(),
                },
                detail: None,
            },
        ],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn retained_history_budget_uses_full_item_estimator_for_image_only_message() {
    let image_only = ResponseItemEnvelope::new(image_message("data:image/png;base64,AAAA"));
    let budget = usize::try_from(estimate_item_token_count(&image_only.item) - 1)
        .expect("positive image estimate");

    assert_eq!(
        truncate_retained_messages(vec![image_only], budget, RetainedImageBudget::Enabled),
        Vec::<ResponseItemEnvelope>::new(),
    );
}

#[test]
fn retained_history_truncates_text_only_message_to_full_item_budget() {
    let original_text = "latest context ".repeat(64);
    let item = ResponseItemEnvelope::new(message("user", &original_text, /*phase*/ None));
    let budget = usize::try_from(estimate_item_token_count(&item.item) - 1)
        .expect("positive message estimate");

    let retained = truncate_retained_messages_for_remote_compaction(vec![item], budget);

    let [
        ResponseItemEnvelope {
            item: ResponseItem::Message { content, .. },
            ..
        },
    ] = retained.as_slice()
    else {
        panic!("expected one retained message");
    };
    let [ContentItem::InputText { text }] = content.as_slice() else {
        panic!("expected one retained text item");
    };
    assert!(!text.is_empty());
    assert_ne!(text, &original_text);
    assert!(
        estimate_item_token_count(&retained[0].item)
            <= i64::try_from(budget).expect("budget fits in i64")
    );
}

#[test]
fn retained_history_keeps_mixed_fixed_content_that_exactly_fits() {
    let mixed = ResponseItemEnvelope::new(mixed_text_image_message(
        &"keep only what fits ".repeat(/*count*/ 20),
    ));
    let text_free = truncate_message_text_to_token_budget(mixed.clone(), /*max_tokens*/ 0)
        .expect("the image remains after text is removed");
    let fixed_cost = usize::try_from(estimate_item_token_count(&text_free.item))
        .expect("positive fixed-content estimate");
    let retained =
        truncate_retained_messages(vec![mixed], fixed_cost, RetainedImageBudget::Enabled);

    assert_eq!(retained, vec![text_free]);
}

#[test]
fn retained_history_truncation_rechecks_final_item_cost() {
    let item = ResponseItemEnvelope::new(mixed_text_image_message(
        "text that crosses the estimator rounding boundary",
    ));
    let retained =
        truncate_retained_messages_for_remote_compaction(vec![item], /*max_tokens*/ 1);

    assert!(retained.is_empty());
}
