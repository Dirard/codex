use std::sync::Arc;

use crate::Prompt;
use crate::ResponseStream;
use crate::client::ModelClientSession;
use crate::client_common::ResponseEvent;
use crate::compact::CompactedHistoryMetadata;
use crate::compact::CompactionAnalyticsAttempt;
use crate::compact::CompactionAnalyticsDetails;
use crate::compact::InitialContextInjection;
use crate::compact::build_compaction_initial_context;
use crate::compact::compaction_status_from_result;
use crate::compact::insert_initial_context_before_last_real_user_or_summary;
use crate::compact_model_fallback::record_model_fallback;
use crate::compact_model_fallback::should_retry_with_current_model;
use crate::compact_remote::should_keep_compacted_history_item;
use crate::compact_remote_history::HistoryItemGroup;
use crate::compact_remote_history::history_item_groups;
use crate::context_manager::estimate_history_token_count;
use crate::context_manager::estimate_item_token_count;
use crate::hook_runtime::PostCompactHookOutcome;
use crate::hook_runtime::PreCompactHookOutcome;
use crate::hook_runtime::run_post_compact_hooks;
use crate::hook_runtime::run_pre_compact_hooks;
use crate::responses_metadata::CodexResponsesMetadata;
use crate::responses_metadata::CompactionTurnMetadata;
use crate::responses_retry::ResponsesStreamRequest;
use crate::responses_retry::ResponsesStreamRetryState;
use crate::responses_retry::handle_retryable_response_stream_error;
use crate::session::context_window::context_window_token_status_for_usage;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::session::turn_context::TurnContext;
use codex_analytics::CompactionImplementation;
use codex_analytics::CompactionPhase;
use codex_analytics::CompactionReason;
use codex_analytics::CompactionTrigger;
use codex_features::Feature;
use codex_history::CodexHarnessMetadata;
use codex_history::ResponseItemEnvelope;
use codex_protocol::config_types::AutoCompactTokenLimitScope;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::items::ContextCompactionItem;
use codex_protocol::items::TurnItem;
use codex_protocol::models::AgentMessageInputContent;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::protocol::TruncationPolicy;
use codex_protocol::protocol::TurnStartedEvent;
use codex_rollout_trace::CompactionCheckpointTracePayload;
use codex_rollout_trace::InferenceTraceContext;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::truncate_text;
use futures::StreamExt;
use tokio_util::sync::CancellationToken;

#[path = "compact_remote_v2_attempt.rs"]
mod attempt;
use attempt::RemoteCompactV2Attempt;
use attempt::run_remote_compact_v2_attempt;

// Mirror the current /responses/compact retained-message default while the
// server-side path remains the reference implementation.
pub(crate) const RETAINED_MESSAGE_TOKEN_BUDGET: usize = 64_000;
const MAX_RETAINED_AGENT_MESSAGE_TOKENS: i64 = 10_000;
// Compact attempts can run much longer than normal turns, so keep the per-transport
// retry budget smaller than the general Responses stream retry budget.
const MAX_REMOTE_COMPACTION_V2_STREAM_RETRIES: u64 = 2;

fn required_headroom(limit: i64) -> i64 {
    RETAINED_MESSAGE_TOKEN_BUDGET
        .min(usize::try_from(limit.saturating_div(4).max(0)).unwrap_or(usize::MAX))
        .try_into()
        .unwrap_or(i64::MAX)
}

fn overflow(used: i64, limit: Option<i64>) -> i64 {
    limit.map_or(0, |limit| {
        used.saturating_sub(limit.saturating_sub(required_headroom(limit)))
            .max(0)
    })
}

fn compaction_candidate_overflow(
    status: &crate::session::context_window::ContextWindowTokenStatus,
    scope: AutoCompactTokenLimitScope,
) -> i64 {
    match scope {
        AutoCompactTokenLimitScope::Total => overflow(
            status.auto_compact_scope_tokens,
            status.auto_compact_scope_limit,
        )
        .max(overflow(
            status.active_context_tokens,
            status.full_context_window_limit,
        )),
        AutoCompactTokenLimitScope::BodyAfterPrefix => overflow(
            status.active_context_tokens,
            status.full_context_window_limit,
        ),
    }
}

pub(crate) async fn run_inline_remote_auto_compact_task(
    sess: Arc<Session>,
    step_context: Arc<StepContext>,
    fallback_step_context: Option<Arc<StepContext>>,
    client_session: &mut ModelClientSession,
    initial_context_injection: InitialContextInjection,
    reason: CompactionReason,
    phase: CompactionPhase,
) -> CodexResult<()> {
    let compaction_metadata = CompactionTurnMetadata::new(
        CompactionTrigger::Auto,
        reason,
        CompactionImplementation::ResponsesCompactionV2,
        phase,
    );
    run_remote_compact_task_inner(
        &sess,
        &step_context,
        fallback_step_context.as_ref(),
        Some(client_session),
        initial_context_injection,
        compaction_metadata,
    )
    .await
}

pub(crate) async fn run_remote_compact_task(
    sess: Arc<Session>,
    turn_context: Arc<TurnContext>,
) -> CodexResult<()> {
    // Standalone compaction is its own request boundary, so it captures a fresh step.
    let step_context = sess
        .capture_step_context(Arc::clone(&turn_context), &CancellationToken::new())
        .await?;
    let start_event = EventMsg::TurnStarted(TurnStartedEvent {
        turn_id: turn_context.sub_id.clone(),
        trace_id: turn_context.trace_id.clone(),
        started_at: turn_context.turn_timing_state.started_at_unix_secs().await,
        model_context_window: turn_context.model_context_window(),
        collaboration_mode_kind: turn_context.mode,
    });
    sess.send_event(&turn_context, start_event).await;

    let compaction_metadata = CompactionTurnMetadata::new(
        CompactionTrigger::Manual,
        CompactionReason::UserRequested,
        CompactionImplementation::ResponsesCompactionV2,
        CompactionPhase::StandaloneTurn,
    );
    run_remote_compact_task_inner(
        &sess,
        &step_context,
        /*fallback_step_context*/ None,
        /*client_session*/ None,
        InitialContextInjection::DoNotInject,
        compaction_metadata,
    )
    .await
}

async fn run_remote_compact_task_inner(
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    fallback_step_context: Option<&Arc<StepContext>>,
    client_session: Option<&mut ModelClientSession>,
    initial_context_injection: InitialContextInjection,
    compaction_metadata: CompactionTurnMetadata,
) -> CodexResult<()> {
    let turn_context = &step_context.turn;
    let trigger = compaction_metadata.trigger();
    let reason = compaction_metadata.reason();
    let implementation = compaction_metadata.implementation();
    let phase = compaction_metadata.phase();
    let mut analytics_details = CompactionAnalyticsDetails {
        active_context_tokens_before: Some(sess.get_total_token_usage().await),
        ..Default::default()
    };
    let attempt = CompactionAnalyticsAttempt::begin(
        sess.as_ref(),
        turn_context.as_ref(),
        trigger,
        reason,
        implementation,
        phase,
    )
    .await;
    let pre_compact_outcome = run_pre_compact_hooks(sess, turn_context, trigger).await;
    match pre_compact_outcome {
        PreCompactHookOutcome::Continue => {}
        PreCompactHookOutcome::Stopped => {
            let error = CodexErr::TurnAborted;
            attempt
                .track(
                    sess.as_ref(),
                    codex_analytics::CompactionStatus::Interrupted,
                    Some(&error),
                    analytics_details,
                )
                .await;
            return Err(error);
        }
    }
    let result = run_remote_compact_task_inner_impl(
        sess,
        step_context,
        fallback_step_context,
        client_session,
        initial_context_injection,
        compaction_metadata,
        &mut analytics_details,
    )
    .await;
    let status = compaction_status_from_result(&result);
    let codex_error = result.as_ref().err();
    if result.is_ok() {
        let post_compact_outcome = run_post_compact_hooks(sess, turn_context, trigger).await;
        if let PostCompactHookOutcome::Stopped = post_compact_outcome {
            attempt
                .track(sess.as_ref(), status, codex_error, analytics_details)
                .await;
            return Err(CodexErr::TurnAborted);
        }
    }
    attempt
        .track(sess.as_ref(), status, codex_error, analytics_details)
        .await;
    match result {
        Ok(()) => Ok(()),
        Err(err) if matches!(err.details(), CodexErrorDetails::TurnAborted) => Err(err),
        Err(err) => {
            sess.track_turn_codex_error(turn_context, &err);
            let event = EventMsg::Error(
                err.to_error_event(Some("Error running remote compact task".to_string())),
            );
            sess.send_event(turn_context, event).await;
            Err(err)
        }
    }
}

async fn run_remote_compact_task_inner_impl(
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    fallback_step_context: Option<&Arc<StepContext>>,
    mut client_session: Option<&mut ModelClientSession>,
    initial_context_injection: InitialContextInjection,
    compaction_metadata: CompactionTurnMetadata,
    analytics_details: &mut CompactionAnalyticsDetails,
) -> CodexResult<()> {
    let turn_context = &step_context.turn;
    let context_compaction_item = ContextCompactionItem::new();
    let compaction_id = context_compaction_item.id.clone();
    let compaction_trace = sess.services.rollout_thread_trace.compaction_trace_context(
        turn_context.sub_id.as_str(),
        compaction_id.as_str(),
        turn_context.model_info.slug.as_str(),
        turn_context.provider.info().name.as_str(),
    );
    let compaction_item = TurnItem::ContextCompaction(context_compaction_item);
    sess.emit_turn_item_started(turn_context, &compaction_item)
        .await;

    let attempt = run_remote_compact_v2_attempt(
        sess,
        step_context,
        client_session.as_deref_mut(),
        &compaction_trace,
        compaction_metadata,
        analytics_details,
    )
    .await;
    let (attempt, compaction_turn_context) = match attempt {
        Ok(attempt) => (attempt, turn_context),
        Err(error) => {
            let Some(fallback_step_context) = fallback_step_context else {
                return Err(error);
            };
            if !should_retry_with_current_model(&error) {
                return Err(error);
            }
            let fallback_turn_context = &fallback_step_context.turn;
            let fallback_compaction_trace =
                sess.services.rollout_thread_trace.compaction_trace_context(
                    fallback_turn_context.sub_id.as_str(),
                    compaction_id.as_str(),
                    fallback_turn_context.model_info.slug.as_str(),
                    fallback_turn_context.provider.info().name.as_str(),
                );
            let fallback_result = run_remote_compact_v2_attempt(
                sess,
                fallback_step_context,
                client_session,
                &fallback_compaction_trace,
                compaction_metadata,
                analytics_details,
            )
            .await;
            record_model_fallback(
                &sess.services.session_telemetry,
                turn_context.model_info.slug.as_str(),
                fallback_turn_context.model_info.slug.as_str(),
                compaction_metadata.reason(),
                compaction_metadata.implementation(),
                fallback_result.as_ref().err(),
            );
            match fallback_result {
                Ok(attempt) => (attempt, fallback_turn_context),
                Err(_) => return Err(error),
            }
        }
    };
    let RemoteCompactV2Attempt {
        trace_input_history,
        prompt_input,
        prompt_input_metadata,
        compaction_output,
        token_usage,
        owned_client_session: _owned_client_session,
    } = attempt;
    if let Some(token_usage) = token_usage {
        sess.record_rollout_budget_usage(&token_usage)?;
        analytics_details.active_context_tokens_before = Some(token_usage.input_tokens);
        analytics_details.compaction_summary_tokens = Some(token_usage.output_tokens);
        analytics_details.cached_input_tokens = Some(token_usage.cached_input_tokens);
        analytics_details.cache_write_input_tokens = Some(token_usage.cache_write_input_tokens);
    }
    let prepared_window = sess.prepare_auto_compact_window().await;
    let (initial_context, world_state_baseline) = build_compaction_initial_context(
        sess.as_ref(),
        &initial_context_injection,
        Some(prepared_window.ids),
    )
    .await;
    let base_instructions = sess.get_base_instructions().await;
    let retain_client_developer_messages =
        sess.enabled(Feature::RetainClientDeveloperMessages);
    let mut retained_message_token_budget = RETAINED_MESSAGE_TOKEN_BUDGET;
    let mut validated_candidate = None;

    for _ in 0..3 {
        let (compacted_history, retained_images) = build_v2_compacted_history(
            &prompt_input,
            &prompt_input_metadata,
            compaction_output.clone(),
            retain_client_developer_messages,
            retained_message_token_budget,
        );
        let new_history = insert_initial_context_before_last_real_user_or_summary(
            compacted_history,
            initial_context.clone(),
        );
        let candidate_tokens = estimate_history_token_count(&new_history, &base_instructions);
        let status = context_window_token_status_for_usage(
            compaction_turn_context,
            candidate_tokens,
            matches!(
                compaction_turn_context
                    .config
                    .model_auto_compact_token_limit_scope,
                AutoCompactTokenLimitScope::BodyAfterPrefix
            )
            .then_some(candidate_tokens),
        );
        let overflow = compaction_candidate_overflow(
            &status,
            compaction_turn_context
                .config
                .model_auto_compact_token_limit_scope,
        );
        if overflow == 0 {
            validated_candidate = Some((new_history, retained_images));
            break;
        }
        retained_message_token_budget = retained_message_token_budget
            .saturating_sub(usize::try_from(overflow).unwrap_or(usize::MAX));
    }

    let (new_history, retained_images) = match validated_candidate {
        Some(candidate) => candidate,
        None => {
            let (compacted_history, retained_images) = build_v2_compacted_history(
                &prompt_input,
                &prompt_input_metadata,
                compaction_output,
                retain_client_developer_messages,
                /*retained_message_token_budget*/ 0,
            );
            let new_history = insert_initial_context_before_last_real_user_or_summary(
                compacted_history,
                initial_context,
            );
            let candidate_tokens = estimate_history_token_count(&new_history, &base_instructions);
            let status = context_window_token_status_for_usage(
                compaction_turn_context,
                candidate_tokens,
                matches!(
                    compaction_turn_context
                        .config
                        .model_auto_compact_token_limit_scope,
                    AutoCompactTokenLimitScope::BodyAfterPrefix
                )
                .then_some(candidate_tokens),
            );
            if compaction_candidate_overflow(
                &status,
                compaction_turn_context
                    .config
                    .model_auto_compact_token_limit_scope,
            ) > 0
            {
                return Err(CodexErr::ContextWindowExceeded);
            }
            (new_history, retained_images)
        }
    };
    analytics_details.retained_image_count = Some(retained_images);

    if !sess
        .commit_prepared_auto_compact_window(prepared_window)
        .await
    {
        return Err(CodexErr::Fatal(
            "auto-compact window changed while validating replacement history".to_string(),
        ));
    }

    let reference_context_item = match initial_context_injection {
        InitialContextInjection::DoNotInject => None,
        InitialContextInjection::BeforeLastUserMessage { .. } => {
            Some(compaction_turn_context.to_turn_context_item())
        }
    };
    if let Some(trace_input_history) = trace_input_history.as_deref() {
        let replacement_history = new_history
            .iter()
            .map(|envelope| envelope.item.clone())
            .collect::<Vec<_>>();
        compaction_trace.record_installed(&CompactionCheckpointTracePayload {
            input_history: trace_input_history,
            replacement_history: &replacement_history,
        });
    }
    sess.replace_compacted_history(
        new_history,
        reference_context_item,
        world_state_baseline,
        CompactedHistoryMetadata {
            message: String::new(),
            window_number: prepared_window.window_number,
            window_ids: prepared_window.ids,
        },
    )
    .await;
    sess.recompute_token_usage(compaction_turn_context).await;
    let installed_status = crate::session::context_window::context_window_token_status(
        sess.as_ref(),
        compaction_turn_context,
    )
    .await;
    debug_assert_eq!(
        compaction_candidate_overflow(
            &installed_status,
            compaction_turn_context
                .config
                .model_auto_compact_token_limit_scope,
        ),
        0,
    );

    sess.emit_turn_item_completed(compaction_turn_context, compaction_item)
        .await;
    Ok(())
}

struct RemoteCompactionV2Output {
    compaction_output: ResponseItem,
    response_id: String,
    token_usage: Option<TokenUsage>,
}

async fn run_remote_compaction_request_v2(
    sess: &Session,
    turn_context: &TurnContext,
    client_session: &mut ModelClientSession,
    prompt: &Prompt,
    responses_metadata: &CodexResponsesMetadata,
) -> CodexResult<RemoteCompactionV2Output> {
    let max_retries = turn_context
        .provider
        .info()
        .stream_max_retries()
        .min(MAX_REMOTE_COMPACTION_V2_STREAM_RETRIES);
    let mut retry_state = ResponsesStreamRetryState::default();
    loop {
        let result = match client_session
            .stream(
                prompt,
                &turn_context.model_info,
                &turn_context.session_telemetry,
                turn_context.reasoning_effort.clone(),
                turn_context.reasoning_summary,
                turn_context.config.service_tier.clone(),
                responses_metadata,
                &InferenceTraceContext::disabled(),
            )
            .await
        {
            Ok(stream) => collect_compaction_output(stream).await,
            Err(err) => Err(err),
        };

        match result {
            Ok(compaction_output) => return Ok(compaction_output),
            Err(err) if !err.is_retryable() => return Err(err),
            Err(err) => {
                handle_retryable_response_stream_error(
                    &mut retry_state,
                    max_retries,
                    err,
                    client_session,
                    sess,
                    turn_context,
                    ResponsesStreamRequest::RemoteCompactionV2,
                )
                .await?;
            }
        }
    }
}

async fn collect_compaction_output(
    mut stream: ResponseStream,
) -> CodexResult<RemoteCompactionV2Output> {
    let mut output_item_count = 0usize;
    let mut compaction_count = 0usize;
    let mut compaction_output = None;
    let mut saw_completed = false;
    let mut completed_response_id = None;
    let mut completed_token_usage = None;
    while let Some(event) = stream.next().await {
        match event? {
            ResponseEvent::OutputItemDone(item) => {
                output_item_count += 1;
                if let ResponseItem::Compaction { .. } = item {
                    compaction_count += 1;
                    if compaction_output.is_none() {
                        compaction_output = Some(item);
                    }
                }
            }
            ResponseEvent::Completed {
                response_id,
                token_usage,
                ..
            } => {
                saw_completed = true;
                completed_response_id = Some(response_id);
                completed_token_usage = token_usage;
                break;
            }
            _ => {}
        }
    }

    if !saw_completed {
        return Err(CodexErr::Stream(
            "remote compaction v2 stream closed before response.completed".to_string(),
        ));
    }

    if compaction_count != 1 {
        return Err(CodexErr::Fatal(format!(
            "remote compaction v2 expected exactly one compaction output item, got {compaction_count} from {output_item_count} output items"
        )));
    }

    let Some(compaction_output) = compaction_output else {
        unreachable!("compaction output must exist when count is exactly one");
    };
    let Some(response_id) = completed_response_id else {
        unreachable!("response id must exist after response.completed");
    };
    Ok(RemoteCompactionV2Output {
        compaction_output,
        response_id,
        token_usage: completed_token_usage,
    })
}

fn build_v2_compacted_history(
    prompt_input: &[ResponseItem],
    prompt_input_metadata: &[Option<CodexHarnessMetadata>],
    compaction_output: ResponseItem,
    retain_client_developer_messages: bool,
    retained_message_token_budget: usize,
) -> (Vec<ResponseItemEnvelope>, usize) {
    debug_assert_eq!(prompt_input.len(), prompt_input_metadata.len());
    let prompt_input = prompt_input
        .iter()
        .cloned()
        .zip(prompt_input_metadata.iter().cloned())
        .map(|(item, metadata)| ResponseItemEnvelope { item, metadata })
        .collect::<Vec<_>>();
    let retained = v2_history_item_groups(prompt_input)
        .filter(|group| is_retained_for_remote_compaction_v2(&group.source.item))
        .filter(|group| {
            should_keep_compacted_history_item(&group.source.item)
                || (retain_client_developer_messages
                    && is_client_authored_developer_message(&group.source))
        })
        .flat_map(HistoryItemGroup::into_items)
        .collect::<Vec<_>>();
    let mut retained =
        truncate_retained_messages_for_remote_compaction(retained, retained_message_token_budget);
    let retained_image_count = retained
        .iter()
        .map(|envelope| retained_input_image_count(&envelope.item))
        .sum::<usize>();
    retained.push(ResponseItemEnvelope::new(compaction_output));
    (retained, retained_image_count)
}

pub(crate) fn is_client_authored_developer_message(item: &ResponseItemEnvelope) -> bool {
    item.metadata
        .as_ref()
        .is_some_and(|metadata| metadata.client_authored)
        && matches!(&item.item, ResponseItem::Message { role, .. } if role == "developer")
}

fn v2_history_item_groups(
    items: Vec<ResponseItemEnvelope>,
) -> impl Iterator<Item = HistoryItemGroup<ResponseItemEnvelope>> {
    history_item_groups(items).flat_map(|mut group| {
        let client_message = group
            .attached_notice
            .take_if(|item| is_client_authored_developer_message(item))
            .map(|source| HistoryItemGroup {
                source,
                attached_notice: None,
            });
        std::iter::once(group).chain(client_message)
    })
}

fn is_retained_for_remote_compaction_v2(item: &ResponseItem) -> bool {
    if let ResponseItem::AgentMessage {
        author,
        recipient,
        content,
        ..
    } = item
    {
        let is_descendant_progress = author
            .strip_prefix(recipient)
            .is_some_and(|suffix| suffix.starts_with('/'))
            && matches!(
                content.first(),
                Some(AgentMessageInputContent::InputText { text })
                    if text.starts_with("Message Type: MESSAGE\n")
            );
        let is_completion = matches!(
            content.first(),
            Some(AgentMessageInputContent::InputText { text })
                if text.starts_with("Message Type: FINAL_ANSWER\n")
        );
        return !is_descendant_progress
            && !is_completion
            && estimate_item_token_count(item) <= MAX_RETAINED_AGENT_MESSAGE_TOKENS;
    }

    let ResponseItem::Message { role, .. } = item else {
        return false;
    };

    matches!(role.as_str(), "user" | "developer" | "system")
}

fn retained_input_image_count(item: &ResponseItem) -> usize {
    let ResponseItem::Message { content, .. } = item else {
        return 0;
    };

    content
        .iter()
        .filter(|item| matches!(item, ContentItem::InputImage { .. }))
        .count()
}

pub(crate) fn truncate_retained_messages_for_remote_compaction(
    items: Vec<ResponseItemEnvelope>,
    max_tokens: usize,
) -> Vec<ResponseItemEnvelope> {
    let mut remaining = max_tokens;
    let mut truncated_reversed = Vec::with_capacity(items.len());
    for group in v2_history_item_groups(items)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        if remaining == 0 {
            continue;
        }

        let group_tokens =
            usize::try_from(group.estimated_token_count()).unwrap_or(usize::MAX);
        if group_tokens <= remaining {
            if let Some(notice) = group.attached_notice {
                truncated_reversed.push(notice);
            }
            truncated_reversed.push(group.source);
            remaining -= group_tokens;
            continue;
        }

        let notice_tokens = group.attached_notice.as_ref().map_or(0, |notice| {
            usize::try_from(estimate_item_token_count(&notice.item)).unwrap_or(usize::MAX)
        });
        if notice_tokens > remaining {
            continue;
        }

        let source_budget = remaining - notice_tokens;
        let fixed_item =
            truncate_message_text_to_token_budget(group.source.clone(), /*max_tokens*/ 0).or_else(
                || {
                    let mut text_wrapper = group.source.clone();
                    let ResponseItem::Message { content, .. } = &mut text_wrapper.item else {
                        return None;
                    };
                    let text_index = content.iter().position(|content_item| {
                        matches!(
                            content_item,
                            ContentItem::InputText { .. } | ContentItem::OutputText { .. }
                        )
                    })?;
                    let mut text_item = content.remove(text_index);
                    match &mut text_item {
                        ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                            text.clear();
                        }
                        ContentItem::InputImage { .. } | ContentItem::InputAudio { .. } => {
                            unreachable!("text index must identify text content")
                        }
                    }
                    content.clear();
                    content.push(text_item);
                    Some(text_wrapper)
                },
            );
        let fixed_tokens = fixed_item
            .as_ref()
            .map(|fixed_item| {
                usize::try_from(estimate_item_token_count(&fixed_item.item)).unwrap_or(usize::MAX)
            })
            .unwrap_or(0);
        if fixed_tokens > source_budget {
            continue;
        }

        let mut text_budget = source_budget - fixed_tokens;
        while let Some(candidate) =
            truncate_message_text_to_token_budget(group.source.clone(), text_budget)
        {
            let final_tokens =
                usize::try_from(estimate_item_token_count(&candidate.item)).unwrap_or(usize::MAX);
            if final_tokens <= source_budget {
                if let Some(notice) = group.attached_notice {
                    truncated_reversed.push(notice);
                }
                truncated_reversed.push(candidate);
                remaining -= final_tokens.saturating_add(notice_tokens);
                break;
            }
            let overflow = final_tokens.saturating_sub(source_budget).max(1);
            let next_text_budget = text_budget.saturating_sub(overflow);
            if next_text_budget == text_budget {
                break;
            }
            text_budget = next_text_budget;
        }
    }
    truncated_reversed.reverse();
    truncated_reversed
}

fn truncate_message_text_to_token_budget(
    envelope: ResponseItemEnvelope,
    max_tokens: usize,
) -> Option<ResponseItemEnvelope> {
    let ResponseItemEnvelope {
        item,
        metadata: harness_metadata,
    } = envelope;
    let ResponseItem::Message {
        id,
        role,
        content,
        phase,
        internal_chat_message_metadata_passthrough: passthrough_metadata,
    } = item
    else {
        return None;
    };

    let mut remaining = max_tokens;
    let mut truncated_content = Vec::with_capacity(content.len());
    for mut content_item in content {
        match &mut content_item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                if remaining == 0 {
                    continue;
                }

                let token_count = approx_token_count(text);
                if token_count <= remaining {
                    remaining = remaining.saturating_sub(token_count);
                } else {
                    *text = truncate_text(text, TruncationPolicy::Tokens(remaining));
                    remaining = 0;
                }
                if !text.is_empty() {
                    truncated_content.push(content_item);
                }
            }
            ContentItem::InputImage { .. } | ContentItem::InputAudio { .. } => {
                truncated_content.push(content_item);
            }
        }
    }

    if truncated_content.is_empty() {
        return None;
    }

    Some(ResponseItemEnvelope {
        item: ResponseItem::Message {
            id,
            role,
            content: truncated_content,
            phase,
            internal_chat_message_metadata_passthrough: passthrough_metadata,
        },
        metadata: harness_metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::MessagePhase;
    use pretty_assertions::assert_eq;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn message(role: &str, text: &str, phase: Option<MessagePhase>) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: role.to_string(),
            content: vec![ContentItem::InputText {
                text: text.to_string(),
            }],
            phase,
            internal_chat_message_metadata_passthrough: None,
        }
    }

    fn build_without_metadata(
        input: Vec<ResponseItem>,
        output: ResponseItem,
    ) -> (Vec<ResponseItemEnvelope>, usize) {
        let metadata = vec![None; input.len()];
        build_v2_compacted_history(
            &input,
            &metadata,
            output,
            /*retain_client_developer_messages*/ false,
            RETAINED_MESSAGE_TOKEN_BUDGET,
        )
    }

    fn raw(items: Vec<ResponseItemEnvelope>) -> Vec<ResponseItem> {
        items
            .into_iter()
            .map(ResponseItemEnvelope::into_item)
            .collect()
    }

    fn image_message(image_url: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputImage {
                image_url: image_url.to_string(),
                detail: None,
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }
    }

    fn mixed_text_image_message(text: &str) -> ResponseItem {
        ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: text.to_string(),
                },
                ContentItem::InputImage {
                    image_url: "data:image/png;base64,AAAA".to_string(),
                    detail: None,
                },
            ],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }
    }

    fn response_stream(events: Vec<CodexResult<ResponseEvent>>) -> ResponseStream {
        let (tx_event, rx_event) = mpsc::channel(events.len().max(1));
        for event in events {
            tx_event
                .try_send(event)
                .expect("response stream test channel should have capacity");
        }
        drop(tx_event);
        ResponseStream {
            rx_event,
            consumer_dropped: CancellationToken::new(),
        }
    }

    #[test]
    fn build_v2_compacted_history_filters_to_installed_retention_shape() {
        let input = vec![
            message("developer", "dev", /*phase*/ None),
            message("system", "sys", /*phase*/ None),
            message("user", "user", /*phase*/ None),
            message("assistant", "commentary", Some(MessagePhase::Commentary)),
            message("assistant", "final", Some(MessagePhase::FinalAnswer)),
            ResponseItem::FunctionCall {
                id: None,
                name: "shell_command".to_string(),
                namespace: None,
                arguments: "{}".to_string(),
                call_id: "call_1".to_string(),
                encrypted_function_args: None,
                internal_chat_message_metadata_passthrough: None,
            },
            ResponseItem::Compaction {
                id: None,
                encrypted_content: "old".to_string(),
                internal_chat_message_metadata_passthrough: None,
            },
        ];
        let output = ResponseItem::Compaction {
            id: None,
            encrypted_content: "new".to_string(),
            internal_chat_message_metadata_passthrough: None,
        };

        let (history, _) = build_without_metadata(input, output.clone());

        assert_eq!(
            raw(history),
            vec![message("user", "user", /*phase*/ None), output]
        );
    }

    #[test]
    fn build_v2_compacted_history_preserves_retained_metadata_sidecar() {
        let retained = message("user", "keep", /*phase*/ None);
        let generated_notice = message(
            "developer",
            "<image_resize_notice>generated</image_resize_notice>",
            /*phase*/ None,
        );
        let harness = message("developer", "drop", /*phase*/ None);
        let client = message(
            "developer",
            "<image_resize_notice>client</image_resize_notice>",
            /*phase*/ None,
        );
        let output = ResponseItem::Compaction {
            id: None,
            encrypted_content: "new".to_string(),
            internal_chat_message_metadata_passthrough: None,
        };

        for enabled in [false, true] {
            let (history, _) = build_v2_compacted_history(
                &[
                    harness.clone(),
                    client.clone(),
                    retained.clone(),
                    generated_notice.clone(),
                ],
                &[
                    None,
                    Some(CodexHarnessMetadata {
                        client_authored: true,
                    }),
                    Some(CodexHarnessMetadata::default()),
                    None,
                ],
                output.clone(),
                enabled,
                RETAINED_MESSAGE_TOKEN_BUDGET,
            );
            let mut expected = vec![
                ResponseItemEnvelope {
                    item: retained.clone(),
                    metadata: Some(CodexHarnessMetadata::default()),
                },
                ResponseItemEnvelope::new(generated_notice.clone()),
                ResponseItemEnvelope::new(output.clone()),
            ];
            if enabled {
                expected.insert(
                    0,
                    ResponseItemEnvelope {
                        item: client.clone(),
                        metadata: Some(CodexHarnessMetadata {
                            client_authored: true,
                        }),
                    },
                );
            }
            assert_eq!(history, expected);
        }
    }

    #[test]
    fn retained_history_truncation_preserves_metadata() {
        let item = ResponseItemEnvelope {
            item: message("user", "word ".repeat(200).as_str(), /*phase*/ None),
            metadata: Some(CodexHarnessMetadata::default()),
        };

        let truncated =
            truncate_retained_messages_for_remote_compaction(vec![item], /*max_tokens*/ 4);

        assert_eq!(truncated.len(), 1);
        assert_eq!(truncated[0].metadata, Some(CodexHarnessMetadata::default()));
    }

    #[test]
    fn build_v2_compacted_history_discards_messages_before_truncating() {
        let old = message("user", "old", /*phase*/ None);
        let new = message("user", "new", /*phase*/ None);
        let huge_developer_message = "d".repeat((RETAINED_MESSAGE_TOKEN_BUDGET + 1) * 4);
        let huge_contextual_message = format!(
            "<environment_context>\n{}\n</environment_context>",
            "c".repeat((RETAINED_MESSAGE_TOKEN_BUDGET + 1) * 4)
        );
        let input = vec![
            old.clone(),
            message("developer", &huge_developer_message, /*phase*/ None),
            message("user", &huge_contextual_message, /*phase*/ None),
            new.clone(),
        ];
        let output = ResponseItem::Compaction {
            id: None,
            encrypted_content: "new".to_string(),
            internal_chat_message_metadata_passthrough: None,
        };

        let (history, _) = build_without_metadata(input, output.clone());

        assert_eq!(raw(history), vec![old, new, output]);
    }

    #[test]
    fn build_v2_compacted_history_counts_retained_input_images() {
        let input = vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![
                ContentItem::InputText {
                    text: "user".to_string(),
                },
                ContentItem::InputImage {
                    image_url: "data:image/png;base64,abc".to_string(),
                    detail: None,
                },
                ContentItem::InputImage {
                    image_url: "data:image/png;base64,def".to_string(),
                    detail: None,
                },
            ],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }];
        let output = ResponseItem::Compaction {
            id: None,
            encrypted_content: "new".to_string(),
            internal_chat_message_metadata_passthrough: None,
        };

        let (_, retained_image_count) = build_without_metadata(input, output);

        assert_eq!(retained_image_count, 2);
    }

    #[test]
    fn retained_history_budget_uses_full_item_estimator_for_image_only_message() {
        let image_only = ResponseItemEnvelope::new(image_message("data:image/png;base64,AAAA"));
        let budget = usize::try_from(estimate_item_token_count(&image_only.item) - 1)
            .expect("positive image estimate");

        assert_eq!(
            truncate_retained_messages_for_remote_compaction(vec![image_only], budget),
            Vec::<ResponseItemEnvelope>::new(),
        );
    }

    #[test]
    fn retained_history_truncates_text_only_message_to_full_item_budget() {
        let original_text = "latest context ".repeat(64);
        let item = message("user", &original_text, None);
        let budget = usize::try_from(estimate_item_token_count(&item) - 1)
            .expect("positive message estimate");

        let retained = truncate_retained_messages_for_remote_compaction(vec![item], budget);

        let [ResponseItem::Message { content, .. }] = retained.as_slice() else {
            panic!("expected one retained message");
        };
        let [ContentItem::InputText { text }] = content.as_slice() else {
            panic!("expected one retained text item");
        };
        assert!(!text.is_empty());
        assert_ne!(text, &original_text);
        assert!(
            estimate_item_token_count(&retained[0])
                <= i64::try_from(budget).expect("budget fits in i64")
        );
    }

    #[test]
    fn retained_history_keeps_mixed_fixed_content_that_exactly_fits() {
        let mixed =
            ResponseItemEnvelope::new(mixed_text_image_message("keep only what fits"));
        let text_free = truncate_message_text_to_token_budget(mixed.clone(), 0)
            .expect("the image remains after text is removed");
        let fixed_cost = usize::try_from(estimate_item_token_count(&text_free.item))
            .expect("positive fixed-content estimate");
        let retained = truncate_retained_messages_for_remote_compaction(vec![mixed], fixed_cost);

        assert_eq!(retained, vec![text_free]);
    }

    #[test]
    fn retained_history_truncation_rechecks_final_item_cost() {
        let item = ResponseItemEnvelope::new(mixed_text_image_message(
            "text that crosses the estimator rounding boundary",
        ));
        let retained = truncate_retained_messages_for_remote_compaction(vec![item], 1);

        assert!(retained.is_empty());
    }

    #[tokio::test]
    async fn collect_compaction_output_accepts_additional_output_items() {
        let compaction = ResponseItem::Compaction {
            id: None,
            encrypted_content: "encrypted".to_string(),
            internal_chat_message_metadata_passthrough: None,
        };
        let stream = response_stream(vec![
            Ok(ResponseEvent::OutputItemDone(message(
                "assistant",
                "IGNORED_COMPACT_REPLY",
                Some(MessagePhase::FinalAnswer),
            ))),
            Ok(ResponseEvent::OutputItemDone(compaction.clone())),
            Ok(ResponseEvent::Completed {
                response_id: "resp-compact".to_string(),
                token_usage: Some(TokenUsage {
                    input_tokens: 123_456,
                    cached_input_tokens: 7_890,
                    cache_write_input_tokens: 0,
                    output_tokens: 42,
                    reasoning_output_tokens: 5,
                    total_tokens: 123_498,
                    codex_rollout_budget_units: None,
                }),
                end_turn: Some(true),
            }),
        ]);

        let output = collect_compaction_output(stream)
            .await
            .expect("compaction should be collected");

        assert_eq!(output.compaction_output, compaction);
        assert_eq!(output.response_id, "resp-compact");
        assert_eq!(
            output.token_usage,
            Some(TokenUsage {
                input_tokens: 123_456,
                cached_input_tokens: 7_890,
                cache_write_input_tokens: 0,
                output_tokens: 42,
                reasoning_output_tokens: 5,
                total_tokens: 123_498,
                codex_rollout_budget_units: None,
            })
        );
    }
}
