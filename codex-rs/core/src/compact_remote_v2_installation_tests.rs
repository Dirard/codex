//! Stale prefixes are skipped or replaced with a fresh request according to caller intent.

use super::*;
use crate::AgentRef;
use crate::AgentSessionReader;
use crate::compact_remote_v2::RemoteCompactV2Attempt;
use crate::context::world_state::EnvironmentsState;
use crate::context::world_state::WorldState;
use crate::session::CompactionPrefix;
use codex_login::CodexAuth;
use codex_protocol::models::ResponseItem;
use core_test_support::responses;
use futures::future::BoxFuture;
use pretty_assertions::assert_eq;
use test_case::test_case;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy)]
enum HistoryChange {
    Replacement,
    Account,
}

#[derive(Default)]
struct FailingRosterRead {
    started: Notify,
    release: Notify,
}

impl AgentSessionReader for FailingRosterRead {
    fn root(&self) -> BoxFuture<'_, CodexResult<AgentRef>> {
        unreachable!("roster rendering reads children")
    }

    fn parent(&self) -> BoxFuture<'_, CodexResult<Option<AgentRef>>> {
        unreachable!("roster rendering reads children")
    }

    fn children(&self) -> BoxFuture<'_, CodexResult<Vec<AgentRef>>> {
        Box::pin(async {
            self.started.notify_one();
            self.release.notified().await;
            Err(CodexErr::InvalidRequest("roster unavailable".into()))
        })
    }
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
    let auth = sess.services.model_client.auth_owner();
    let prefix = CompactionPrefix {
        generation: history.history_version(),
        len: history.annotated_items().len(),
        auth_owner: match change {
            HistoryChange::Replacement => auth,
            HistoryChange::Account => Some(
                codex_login::AuthManager::from_auth_for_testing_with_home(
                    CodexAuth::from_api_key("other-account"),
                    std::path::PathBuf::from("unused"),
                )
                .auth_owner(),
            ),
        },
    };
    let roster = Arc::new(FailingRosterRead::default());
    let mut cutoff_world_state = WorldState::default();
    if matches!(change, HistoryChange::Replacement) {
        cutoff_world_state.add_section(
            EnvironmentsState::from_turn_context_with_environments(
                &step.turn,
                &step.environments,
                /*current_date*/ Some("2026-01-01".into()),
            )
            .await,
        );
        cutoff_world_state.host_roster_reader = Some(roster.clone());
    }
    let candidate = BackgroundCompactionCandidate {
        prefix: prefix.clone(),
        model_info: Arc::clone(&step.settings.model_info),
        cutoff_world_state: Arc::new(cutoff_world_state),
        attempt: RemoteCompactV2Attempt {
            rejected_audio_file_ids: Default::default(),
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
    let world_state = WorldState::default();
    let mut analytics = CompactionAnalyticsDetails::default();
    let mut compaction = Box::pin(run_compaction(
        &sess,
        &step,
        &step,
        /*client_session*/ None,
        &world_state,
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
    ));
    if matches!(change, HistoryChange::Replacement) {
        tokio::select! {
            _ = roster.started.notified() => {}
            result = &mut compaction => panic!("expected a pending roster read, got {result:?}"),
        }
        sess.replace_history(
            vec![original.clone(), original],
            /*reference_context_item*/ None,
        )
        .await;
    }
    // The replacement still fits the captured length, but changes the generation.
    let before = sess.clone_history().await;
    let window_before = sess.current_window().await;
    assert!(before.annotated_items().len() >= prefix.len);
    if matches!(change, HistoryChange::Replacement) {
        assert_ne!(before.history_version(), prefix.generation);
        roster.release.notify_one();
    }
    let installed = compaction
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
    let after = sess.clone_history().await;
    match fallback {
        CompactionFallback::Skip => {
            assert!(!installed);
            assert_eq!(analytics.retained_image_count, None);
            assert_eq!(requests.requests().len(), 0);
            assert_eq!(after.annotated_items(), before.annotated_items());
            assert_eq!(sess.current_window().await, window_before);
            assert_eq!(
                after.world_state_checkpoint(),
                before.world_state_checkpoint()
            );
        }
        CompactionFallback::Foreground => {
            assert!(installed);
            assert_eq!(requests.requests().len(), 1);
            let summaries = after
                .raw_items()
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
            auth_owner: sess.services.model_client.auth_owner(),
        },
        model_info: Arc::clone(&step.settings.model_info),
        cutoff_world_state: Arc::new(WorldState::default()),
        attempt: RemoteCompactV2Attempt {
            rejected_audio_file_ids: Default::default(),
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
            auth_owner: sess.services.model_client.auth_owner(),
        },
        model_info: Arc::clone(&step.settings.model_info),
        cutoff_world_state: Arc::new(WorldState::default()),
        attempt: RemoteCompactV2Attempt {
            rejected_audio_file_ids: Default::default(),
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

async fn hopeless_check_session(
    context_window: Option<i64>,
) -> (Arc<Session>, Arc<TurnContext>, Arc<StepContext>) {
    let (sess, turn, _events) =
        crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
            CodexAuth::create_dummy_chatgpt_auth_for_testing(),
            Vec::new(),
            move |config| {
                config.model = Some("gpt-5.2".to_string());
                config.model_context_window = context_window;
                // The request copy of model instructions then omits the update_plan section.
                config.update_plan_enabled = false;
                config.model_catalog = None;
            },
        )
        .await;
    sess.set_base_instructions_provenance_for_test(Some(
        codex_protocol::models::BaseInstructionsProvenance::Model {
            model: turn.model_info().slug.clone(),
        },
    ))
    .await;
    let step = sess
        .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
        .await
        .expect("capture step");
    (sess, turn, step)
}

/// The fail-fast lower bound counts the instructions the compaction request sends: a window
/// that fits them and the tools is not hopeless, even though the persisted instructions with
/// the update_plan section would not fit.
#[tokio::test]
async fn hopeless_auto_compaction_counts_the_sent_instructions() {
    let (sess, turn, step) = hopeless_check_session(/*context_window*/ None).await;
    let token_count = |text: &str| {
        i64::try_from(codex_utils_output_truncation::approx_token_count(text)).expect("tokens")
    };
    let persisted = token_count(&sess.get_base_instructions().await.text);
    let sent = token_count(&sess.get_prompt_base_instructions().await.text);
    assert!(sent < persisted, "sent={sent} persisted={persisted}");
    let tools = tool_declaration_tokens(&step);
    let percent = turn.model_info().effective_context_window_percent;
    // A full window halfway between the sent and the persisted lower bounds.
    let context_window = ((sent + tools + (persisted - sent) / 2) * 100 + percent - 1) / percent;

    let (sess, turn, step) = hopeless_check_session(Some(context_window)).await;
    let full_window = step
        .turn
        .model_info()
        .usable_context_window()
        .expect("configured window");
    assert!(
        sent + tools < full_window && full_window <= persisted + tools,
        "sent={sent} persisted={persisted} tools={tools} window={full_window}"
    );
    reject_hopeless_auto_compaction(&sess, &turn, &step, CompactionTrigger::Auto)
        .await
        .expect("the sent instructions and tools fit the window");

    let (sess, turn, step) = hopeless_check_session(Some(context_window / 2)).await;
    let error = reject_hopeless_auto_compaction(&sess, &turn, &step, CompactionTrigger::Auto)
        .await
        .expect_err("half the window cannot hold the instructions and tools");
    assert!(matches!(
        error.details(),
        CodexErrorDetails::ContextWindowExceeded
    ));
    reject_hopeless_auto_compaction(&sess, &turn, &step, CompactionTrigger::Manual)
        .await
        .expect("an explicit /compact still sends its request");
}

const RESTORED_CONTEXT_KEYS: usize = 40;
const RETAINED_MESSAGE_CHARS: usize = 160_000;

/// A history with a retained user message and active additional context that compaction does
/// not retain, with one foreground compaction response mounted.
struct RestoredContextCase {
    _server: wiremock::MockServer,
    _events: async_channel::Receiver<codex_protocol::protocol::Event>,
    requests: responses::ResponseMock,
    sess: Arc<Session>,
    step: Arc<StepContext>,
    retained: ResponseItem,
    context: Vec<ResponseItemEnvelope>,
}

fn fresh_summary() -> ResponseItem {
    ResponseItem::Compaction {
        id: None,
        encrypted_content: "fresh-summary".to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}

impl RestoredContextCase {
    async fn start(auto_compact_limit: Option<i64>) -> Self {
        let server = responses::start_mock_server().await;
        let mut provider =
            codex_model_provider_info::built_in_model_providers(/*openai_base_url*/ None)["openai"]
                .clone();
        provider.base_url = Some(format!("{}/v1", server.uri()));
        provider.supports_websockets = false;
        let server_uri = server.uri();
        let (sess, turn, events) =
            crate::session::tests::make_session_and_context_with_auth_and_config_and_rx(
                CodexAuth::create_dummy_chatgpt_auth_for_testing(),
                Vec::new(),
                move |config| {
                    config.model = Some("gpt-5.2".to_string());
                    config.model_context_window = Some(200_000);
                    config.model_auto_compact_token_limit = auto_compact_limit;
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
        let retained: ResponseItem = serde_json::from_value(serde_json::json!({
            "type": "message", "role": "user",
            "content": [{"type": "input_text", "text": "r".repeat(RETAINED_MESSAGE_CHARS)}]
        }))
        .expect("user message");
        sess.replace_history(vec![retained.clone()], /*reference_context_item*/ None)
            .await;
        // Each value is bounded to about 1k tokens, so the context comes from many keys.
        let context = sess
            .fork_merge_additional_context_for_test(
                &turn,
                (0..RESTORED_CONTEXT_KEYS)
                    .map(|index| {
                        (
                            format!("context-{index:02}"),
                            codex_protocol::protocol::AdditionalContextEntry::new(
                                "c".repeat(3_900),
                                codex_protocol::protocol::AdditionalContextKind::Untrusted,
                            ),
                        )
                    })
                    .collect(),
            )
            .await;
        assert_eq!(context.len(), RESTORED_CONTEXT_KEYS);
        sess.record_annotated_conversation_items(&turn, &step.settings.model_info, context.clone())
            .await;
        Self {
            _server: server,
            _events: events,
            requests,
            sess,
            step,
            retained,
            context,
        }
    }

    /// The first candidate as the headroom loop builds it, without the restored context, and
    /// the restored context that the complete installation adds.
    async fn candidate_and_context_tokens(&self) -> (i64, i64) {
        let (prefix, context, _) = build_compaction_context_for_window(
            self.sess.as_ref(),
            &self.step,
            &WorldState::default(),
            /*window_ids*/ None,
        )
        .await;
        let candidate = assemble_compaction_history(
            vec![
                ResponseItemEnvelope::new(self.retained.clone()),
                ResponseItemEnvelope::new(fresh_summary()),
            ],
            prefix,
            context,
        );
        let candidate_tokens = estimate_candidate_request_tokens(
            &candidate,
            &self.sess.get_base_instructions().await,
            self.step.turn.model_info().as_ref(),
            tool_declaration_tokens(&self.step),
        );
        let context_tokens = self
            .context
            .iter()
            .map(|envelope| estimate_item_token_count(&envelope.item))
            .fold(0i64, i64::saturating_add);
        (candidate_tokens, context_tokens)
    }

    fn headroom_overflow(&self, tokens: i64) -> i64 {
        let turn = &self.step.turn;
        let scope = turn.config.model_auto_compact_token_limit_scope;
        let status = context_window_token_status_for_usage(
            turn.config.as_ref(),
            turn.model_info().as_ref(),
            tokens,
            matches!(scope, AutoCompactTokenLimitScope::BodyAfterPrefix).then_some(tokens),
        );
        compaction_candidate_overflow(&status, scope)
    }
}

/// A complete installation restores the active additional context that the candidate does
/// not retain (#61390), so the headroom loop must count it: this candidate leaves the headroom
/// alone but not with the restored context, and must shrink its retained message instead of
/// installing past the target.
#[tokio::test]
async fn complete_install_counts_restored_additional_context_in_headroom() {
    // Calibrate the limit on an identical history, so the target lies halfway between the
    // candidate with and without the restored context.
    let calibration = RestoredContextCase::start(/*auto_compact_limit*/ None).await;
    let (candidate_tokens, context_tokens) = calibration.candidate_and_context_tokens().await;
    let target = candidate_tokens + context_tokens / 2;
    // The headroom below a limit is min(64k, limit / 4).
    let auto_compact_limit = if target >= 192_000 {
        target + 64_000
    } else {
        (target * 4 + 2) / 3
    };

    let case = RestoredContextCase::start(Some(auto_compact_limit)).await;
    let (candidate_tokens, context_tokens) = case.candidate_and_context_tokens().await;
    assert_eq!(
        case.headroom_overflow(candidate_tokens),
        0,
        "the candidate alone must fit: candidate={candidate_tokens} limit={auto_compact_limit}"
    );
    assert!(
        case.headroom_overflow(candidate_tokens + context_tokens) > 0,
        "the restored context must overflow: candidate={candidate_tokens} \
         context={context_tokens} limit={auto_compact_limit}"
    );

    let mut analytics = CompactionAnalyticsDetails::default();
    let installed = run_compaction(
        &case.sess,
        &case.step,
        &case.step,
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
        /*candidate*/ None,
        CompactionFallback::Foreground,
    )
    .await
    .expect("foreground compaction should install");

    assert!(installed);
    assert_eq!(case.requests.requests().len(), 1);
    let history = case.sess.clone_history().await;
    let context_ids = case
        .context
        .iter()
        .filter_map(|envelope| envelope.item.id())
        .collect::<Vec<_>>();
    assert_eq!(context_ids.len(), RESTORED_CONTEXT_KEYS);
    assert_eq!(
        history
            .raw_items()
            .filter(|item| item.id().is_some_and(|id| context_ids.contains(&id)))
            .count(),
        RESTORED_CONTEXT_KEYS,
        "the installer restores every active context item"
    );
    let installed_tokens = estimate_candidate_request_tokens(
        history.annotated_items(),
        &case.sess.get_base_instructions().await,
        case.step.turn.model_info().as_ref(),
        tool_declaration_tokens(&case.step),
    );
    assert_eq!(
        case.headroom_overflow(installed_tokens),
        0,
        "the installed history must leave the headroom: installed={installed_tokens} \
         limit={auto_compact_limit}"
    );
    let retained_chars = history
        .raw_items()
        .filter_map(|item| match item {
            ResponseItem::Message { role, content, .. } if role == "user" => Some(content),
            _ => None,
        })
        .flatten()
        .filter_map(|content| match content {
            ContentItem::InputText { text } if text.starts_with('r') => Some(text.len()),
            _ => None,
        })
        .sum::<usize>();
    assert!(
        retained_chars < RETAINED_MESSAGE_CHARS,
        "the retained message must shrink to make room: {retained_chars} chars"
    );
}
