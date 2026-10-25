//! Stale prefixes are skipped or replaced with a fresh request according to caller intent.

use super::*;
use crate::compact_remote_v2::RemoteCompactV2Attempt;
use crate::context::world_state::WorldState;
use crate::session::CompactionPrefix;
use codex_login::CodexAuth;
use codex_protocol::models::ResponseItem;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use test_case::test_case;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum HistoryChange {
    Replacement,
    Account,
}

#[test_case(HistoryChange::Replacement, CompactionFallback::Skip; "replacement optional")]
#[test_case(HistoryChange::Account, CompactionFallback::Skip; "account optional")]
#[test_case(HistoryChange::Replacement, CompactionFallback::Foreground; "replacement required")]
#[test_case(HistoryChange::Account, CompactionFallback::Foreground; "account required")]
#[tokio::test]
async fn prepared_prefix_rejection_respects_caller_intent(
    change: HistoryChange,
    fallback: CompactionFallback,
) {
    let server = responses::start_mock_server().await;
    let mut provider =
        codex_model_provider_info::built_in_model_providers(/*openai_base_url*/ None)["openai"]
            .clone();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    provider.supports_websockets = false;
    let server_uri = server.uri();
    let (sess, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
            CodexAuth::create_dummy_chatgpt_auth_for_testing(),
            Vec::new(),
            move |config| {
                config.model = Some("gpt-5.2".to_string());
                config.model_provider = provider;
                config.chatgpt_base_url = server_uri;
                let _ = config.features.disable(Feature::TokenBudget);
            },
        )
        .await;
    let requests = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            serde_json::json!({"type":"response.output_item.done", "item": {
                "type":"compaction", "encrypted_content":"fresh-summary"
            }}),
            serde_json::json!({"type":"response.completed", "response":{"id":"foreground"}}),
        ]),
    )
    .await;
    let step = sess
        .capture_step_context(turn, &CancellationToken::new())
        .await
        .expect("capture step");
    let original: ResponseItem = serde_json::from_value(serde_json::json!({
        "type": "message", "role": "user",
        "content": [{"type": "input_text", "text": "original turn"}]
    }))
    .expect("user message");
    sess.replace_history(vec![original.clone()], /*reference_context_item*/ None)
        .await;
    let history = sess.clone_history().await;
    let auth = sess.services.model_client.auth_owner_generation();
    let mut candidate = BackgroundCompactionCandidate {
        prefix: CompactionPrefix {
            generation: history.history_version(),
            len: history.annotated_items().len(),
            auth_owner_generation: auth,
        },
        model_info: Arc::clone(&step.settings.model_info),
        cutoff_world_state: Arc::new(WorldState::default()),
        attempt: RemoteCompactV2Attempt {
            input_goal_ids: Default::default(),
            trace_input_history: None,
            prompt_input: vec![original.clone()],
            prompt_input_metadata: vec![None],
            compaction_output: ResponseItem::Compaction {
                id: None,
                encrypted_content: "stale-summary".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
            compaction_response_id: "prepared".to_string(),
            token_usage: Some(TokenUsage {
                output_tokens: 777,
                cached_input_tokens: 9,
                cache_write_input_tokens: 5,
                ..Default::default()
            }),
            owned_client_session: None,
        },
    };
    match change {
        HistoryChange::Replacement => {
            sess.replace_history(
                vec![original.clone(), original.clone()],
                /*reference_context_item*/ None,
            )
            .await;
        }
        HistoryChange::Account => {
            candidate.prefix.auth_owner_generation = Some(auth.unwrap_or_default().wrapping_add(1));
        }
    }
    // Rewritten histories still fit the captured length; length alone cannot reject them.
    let history = sess.clone_history().await;
    assert!(history.annotated_items().len() >= candidate.prefix.len);
    if !matches!(change, HistoryChange::Account) {
        assert_ne!(history.history_version(), candidate.prefix.generation);
    }
    let before = sess
        .clone_history()
        .await
        .raw_items()
        .cloned()
        .collect::<Vec<_>>();
    let mut analytics = CompactionAnalyticsDetails::default();
    let installed = run_compaction(
        &sess,
        &step,
        &step,
        /*client_session*/ None,
        &WorldState::default(),
        CompactionTurnMetadata::new(
            CompactionTrigger::Auto,
            CompactionReason::ContextLimit,
            CompactionImplementation::ResponsesCompactionV2,
            CompactionPhase::PreTurn,
        ),
        &mut analytics,
        /*started_item*/ None,
        Some(candidate),
        fallback,
    )
    .await
    .expect("reject stale result without failing compaction");
    // The foreground response has no usage; stale candidate statistics must not leak.
    assert_eq!(
        (
            analytics.compaction_summary_tokens,
            analytics.cached_input_tokens,
            analytics.cache_write_input_tokens
        ),
        (None, None, None),
    );
    let after = sess
        .clone_history()
        .await
        .raw_items()
        .cloned()
        .collect::<Vec<_>>();
    match fallback {
        CompactionFallback::Skip => {
            assert!(!installed);
            assert_eq!(analytics.retained_image_count, None);
            assert_eq!(requests.requests().len(), 0);
            assert_eq!(after, before);
        }
        CompactionFallback::Foreground => {
            assert!(installed);
            assert_eq!(requests.requests().len(), 1);
            let summaries = after
                .iter()
                .filter_map(|item| match item {
                    ResponseItem::Compaction {
                        encrypted_content, ..
                    } => Some(encrypted_content.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(summaries, ["fresh-summary"]);
        }
    }
}

#[tokio::test]
async fn prepared_prefix_with_live_suffix_stays_installed() {
    let server = responses::start_mock_server().await;
    let mut provider =
        codex_model_provider_info::built_in_model_providers(/*openai_base_url*/ None)["openai"]
            .clone();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    provider.supports_websockets = false;
    let server_uri = server.uri();
    let (sess, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
            CodexAuth::create_dummy_chatgpt_auth_for_testing(),
            Vec::new(),
            move |config| {
                config.model = Some("gpt-5.2".to_string());
                config.model_context_window = Some(200_000);
                config.model_auto_compact_token_limit = Some(180_000);
                config.model_provider = provider;
                config.chatgpt_base_url = server_uri;
                let _ = config.features.disable(Feature::TokenBudget);
            },
        )
        .await;
    let requests = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            serde_json::json!({"type":"response.output_item.done", "item": {
                "type":"compaction", "encrypted_content":"fresh-summary"
            }}),
            serde_json::json!({"type":"response.completed", "response":{"id":"foreground"}}),
        ]),
    )
    .await;
    let step = sess
        .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
        .await
        .expect("capture step");
    let original: ResponseItem = serde_json::from_value(serde_json::json!({
        "type": "message", "role": "user",
        "content": [{"type": "input_text", "text": "original turn"}]
    }))
    .expect("user message");
    sess.replace_history(vec![original.clone()], /*reference_context_item*/ None)
        .await;
    let history = sess.clone_history().await;
    let candidate = BackgroundCompactionCandidate {
        prefix: CompactionPrefix {
            generation: history.history_version(),
            len: history.annotated_items().len(),
            auth_owner_generation: sess.services.model_client.auth_owner_generation(),
        },
        model_info: Arc::clone(&step.settings.model_info),
        cutoff_world_state: Arc::new(WorldState::default()),
        attempt: RemoteCompactV2Attempt {
            input_goal_ids: Default::default(),
            trace_input_history: None,
            prompt_input: vec![original],
            prompt_input_metadata: vec![None],
            compaction_output: ResponseItem::Compaction {
                id: None,
                encrypted_content: "prepared-summary".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
            compaction_response_id: "prepared".to_string(),
            token_usage: None,
            owned_client_session: None,
        },
    };
    // The live suffix fits once, but counting it twice would exceed the headroom target.
    let live_suffix: ResponseItem = serde_json::from_value(serde_json::json!({
        "type": "message", "role": "user",
        "content": [{"type": "input_text", "text": "s".repeat(320_000)}]
    }))
    .expect("live suffix message");
    sess.record_conversation_items(&turn, &step.settings.model_info, &[live_suffix])
        .await;
    assert_eq!(
        sess.clone_history().await.history_version(),
        candidate.prefix.generation
    );

    let mut analytics = CompactionAnalyticsDetails::default();
    let installed = run_compaction(
        &sess,
        &step,
        &step,
        /*client_session*/ None,
        &WorldState::default(),
        CompactionTurnMetadata::new(
            CompactionTrigger::Auto,
            CompactionReason::ContextLimit,
            CompactionImplementation::ResponsesCompactionV2,
            CompactionPhase::MidTurn,
        ),
        &mut analytics,
        /*started_item*/ None,
        Some(candidate),
        CompactionFallback::Foreground,
    )
    .await
    .expect("prepared compaction should install");

    assert!(installed);
    assert_eq!(requests.requests().len(), 0);
    let summaries = sess
        .clone_history()
        .await
        .raw_items()
        .filter_map(|item| match item {
            ResponseItem::Compaction {
                encrypted_content, ..
            } => Some(encrypted_content.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(summaries, ["prepared-summary"]);
}

/// Tool declarations travel outside history in ordinary requests, so the headroom check
/// must count them: this prepared prefix fits only while the large tool schema is ignored.
#[tokio::test]
async fn prepared_prefix_counts_tool_declarations_in_headroom() {
    let server = responses::start_mock_server().await;
    let mut provider =
        codex_model_provider_info::built_in_model_providers(/*openai_base_url*/ None)["openai"]
            .clone();
    provider.base_url = Some(format!("{}/v1", server.uri()));
    provider.supports_websockets = false;
    let server_uri = server.uri();
    let large_tool = codex_protocol::dynamic_tools::DynamicToolSpec::Function(
        codex_protocol::dynamic_tools::DynamicToolFunctionSpec {
            name: "large_schema_tool".to_string(),
            description: "d".repeat(320_000),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            defer_loading: false,
        },
    );
    let (sess, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
            CodexAuth::create_dummy_chatgpt_auth_for_testing(),
            vec![large_tool],
            move |config| {
                config.model = Some("gpt-5.2".to_string());
                config.model_context_window = Some(120_000);
                config.model_auto_compact_token_limit = Some(100_000);
                config.model_provider = provider;
                config.chatgpt_base_url = server_uri;
                let _ = config.features.disable(Feature::TokenBudget);
            },
        )
        .await;
    let requests = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            serde_json::json!({"type":"response.output_item.done", "item": {
                "type":"compaction", "encrypted_content":"fresh-summary"
            }}),
            serde_json::json!({"type":"response.completed", "response":{"id":"foreground"}}),
        ]),
    )
    .await;
    let step = sess
        .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
        .await
        .expect("capture step");
    // The headroom target is 75k tokens; the visible tool declarations alone exceed it.
    let tool_tokens = step
        .tool_router
        .model_visible_specs()
        .iter()
        .map(|spec| {
            codex_utils_output_truncation::approx_token_count(
                &serde_json::to_string(spec).expect("serialize tool spec"),
            )
        })
        .sum::<usize>();
    assert!(
        tool_tokens > 80_000,
        "large tool must be model visible: {tool_tokens}"
    );
    let original: ResponseItem = serde_json::from_value(serde_json::json!({
        "type": "message", "role": "user",
        "content": [{"type": "input_text", "text": "original turn"}]
    }))
    .expect("user message");
    sess.replace_history(vec![original.clone()], /*reference_context_item*/ None)
        .await;
    let history = sess.clone_history().await;
    let candidate = BackgroundCompactionCandidate {
        prefix: CompactionPrefix {
            generation: history.history_version(),
            len: history.annotated_items().len(),
            auth_owner_generation: sess.services.model_client.auth_owner_generation(),
        },
        model_info: Arc::clone(&step.settings.model_info),
        cutoff_world_state: Arc::new(WorldState::default()),
        attempt: RemoteCompactV2Attempt {
            input_goal_ids: Default::default(),
            trace_input_history: None,
            prompt_input: vec![original],
            prompt_input_metadata: vec![None],
            compaction_output: ResponseItem::Compaction {
                id: None,
                encrypted_content: "prepared-summary".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
            compaction_response_id: "prepared".to_string(),
            token_usage: None,
            owned_client_session: None,
        },
    };
    let before = sess
        .clone_history()
        .await
        .raw_items()
        .cloned()
        .collect::<Vec<_>>();

    let mut analytics = CompactionAnalyticsDetails::default();
    let installed = run_compaction(
        &sess,
        &step,
        &step,
        /*client_session*/ None,
        &WorldState::default(),
        CompactionTurnMetadata::new(
            CompactionTrigger::Auto,
            CompactionReason::ContextLimit,
            CompactionImplementation::ResponsesCompactionV2,
            CompactionPhase::MidTurn,
        ),
        &mut analytics,
        /*started_item*/ None,
        Some(candidate),
        CompactionFallback::Skip,
    )
    .await
    .expect("an over-budget optional candidate is declined without an error");

    assert!(!installed);
    assert_eq!(requests.requests().len(), 0);
    let after = sess
        .clone_history()
        .await
        .raw_items()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(after, before);
}
