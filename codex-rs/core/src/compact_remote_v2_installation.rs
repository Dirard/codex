//! Materializes and installs foreground or prepared V2 compaction through a shared path.
//! A rejected or still-over-budget prepared result may fall back to one foreground request.
//! Prepared prefixes are validated by the history replacement primitive before mutation.

use super::*;
use crate::compact::assemble_compaction_history;
use crate::compact::build_compaction_context_for_window;
use crate::context::world_state::split_prefix_updates;
use crate::context_manager::updates::merge_world_state_updates;
use crate::session::CompactedHistoryInstallation;
use codex_rollout_trace::CompactionCheckpointTracePayload;

#[cfg(test)]
#[path = "compact_remote_v2_installation_tests.rs"]
mod tests;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_compaction(
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    replacement_step_context: &Arc<StepContext>,
    mut client_session: Option<&mut ModelClientSession>,
    world_state: &WorldState,
    compaction_metadata: CompactionTurnMetadata,
    analytics_details: &mut CompactionAnalyticsDetails,
    mut started_item: Option<&TurnItem>,
    candidate: Option<BackgroundCompactionCandidate>,
    fallback: CompactionFallback,
) -> CodexResult<bool> {
    if let Some(candidate) = candidate {
        let installed = build_and_install_compaction(
            sess,
            step_context,
            replacement_step_context,
            client_session.as_deref_mut(),
            world_state,
            compaction_metadata,
            analytics_details,
            started_item,
            Some(candidate),
        )
        .await?;
        if installed {
            // Count the installed history once, as validation under the state lock did.
            // Session usage was just recomputed from this history and would add the live
            // suffix a second time.
            let turn = &replacement_step_context.turn;
            let scope = turn.config.model_auto_compact_token_limit_scope;
            let base_instructions = sess.get_base_instructions().await;
            let installed_tokens = crate::context_manager::estimate_history_token_count(
                sess.clone_history().await.annotated_items(),
                &base_instructions,
            );
            let status = crate::session::context_window::context_window_token_status_for_usage(
                turn.config.as_ref(),
                turn.model_info().as_ref(),
                installed_tokens,
                matches!(scope, AutoCompactTokenLimitScope::BodyAfterPrefix)
                    .then_some(installed_tokens),
            );
            if compaction_candidate_overflow(&status, scope) == 0 {
                return Ok(true);
            }
            // The live suffix can leave an adopted prefix over budget. Try the existing
            // foreground path once, with a new item: adoption already completed its item.
            started_item = None;
        } else if fallback == CompactionFallback::Skip {
            return Ok(false);
        }
    }
    build_and_install_compaction(
        sess,
        step_context,
        replacement_step_context,
        client_session,
        world_state,
        compaction_metadata,
        analytics_details,
        started_item,
        /*candidate*/ None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn build_and_install_compaction(
    sess: &Arc<Session>,
    step_context: &Arc<StepContext>,
    replacement_step_context: &Arc<StepContext>,
    mut client_session: Option<&mut ModelClientSession>,
    world_state: &WorldState,
    compaction_metadata: CompactionTurnMetadata,
    analytics_details: &mut CompactionAnalyticsDetails,
    started_item: Option<&TurnItem>,
    candidate: Option<BackgroundCompactionCandidate>,
) -> CodexResult<bool> {
    let turn_context = &step_context.turn;
    if let Some(candidate) = &candidate
        && (candidate.model_info.slug != step_context.settings.model_info.slug
            || candidate.model_info.comp_hash != step_context.settings.model_info.comp_hash)
    {
        return Ok(false);
    }
    let needs_start = started_item.is_none();
    let compaction_item = started_item
        .cloned()
        .unwrap_or_else(|| TurnItem::ContextCompaction(ContextCompactionItem::new()));
    let compaction_id = compaction_item.id();
    let compaction_trace = sess.services.rollout_thread_trace.compaction_trace_context(
        turn_context.sub_id.as_str(),
        compaction_id.as_str(),
        candidate
            .as_ref()
            .map_or(turn_context.model_info().slug.as_str(), |candidate| {
                candidate.model_info.slug.as_str()
            }),
        turn_context.provider.info().name.as_str(),
    );
    if needs_start && candidate.is_none() {
        sess.emit_turn_item_started(turn_context, &compaction_item)
            .await;
    }
    let (attempt, compaction_turn_context, prefix) = if let Some(candidate) = candidate {
        let mut attempt = candidate.attempt;
        if compaction_trace.is_enabled() {
            attempt.trace_input_history =
                Some(sess.clone_history().await.raw_items().cloned().collect());
        }
        (
            attempt,
            turn_context,
            Some((
                candidate.prefix,
                candidate.model_info,
                candidate.cutoff_world_state,
            )),
        )
    } else {
        let attempt = run_remote_compact_v2_attempt(
            sess,
            step_context,
            &compaction_trace,
            compaction_metadata,
            analytics_details,
            RemoteCompactV2AttemptInput::Foreground(client_session.as_deref_mut()),
        )
        .await;
        let (attempt, compaction_turn_context) = match attempt {
            Ok(attempt) => (attempt, turn_context),
            Err(error) => {
                let fallback_turn_context = &replacement_step_context.turn;
                if !should_retry_with_current_model(&error, turn_context, fallback_turn_context) {
                    return Err(error);
                }
                sess.set_last_known_step_context(replacement_step_context)
                    .await;
                let fallback_compaction_trace =
                    sess.services.rollout_thread_trace.compaction_trace_context(
                        fallback_turn_context.sub_id.as_str(),
                        compaction_id.as_str(),
                        fallback_turn_context.model_info().slug.as_str(),
                        fallback_turn_context.provider.info().name.as_str(),
                    );
                let fallback_result = run_remote_compact_v2_attempt(
                    sess,
                    replacement_step_context,
                    &fallback_compaction_trace,
                    compaction_metadata,
                    analytics_details,
                    RemoteCompactV2AttemptInput::Foreground(client_session),
                )
                .await;
                record_model_fallback(
                    &sess.services.session_telemetry,
                    turn_context.model_info().slug.as_str(),
                    fallback_turn_context.model_info().slug.as_str(),
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
        (attempt, compaction_turn_context, None)
    };
    let RemoteCompactV2Attempt {
        input_goal_ids,
        trace_input_history,
        prompt_input,
        prompt_input_metadata,
        compaction_output,
        compaction_response_id,
        token_usage,
        owned_client_session: _owned_client_session,
    } = attempt;
    if prefix.is_none()
        && let Some(token_usage) = token_usage.as_ref()
    {
        sess.record_rollout_budget_usage(token_usage).await?;
        analytics_details.active_context_tokens_before = Some(token_usage.input_tokens);
    }
    let retained_image_budget = if sess.enabled(Feature::CompactionImageBudget) {
        RetainedImageBudget::Enabled
    } else {
        RetainedImageBudget::Disabled
    };
    let mut retained_message_token_budget = RETAINED_MESSAGE_TOKEN_BUDGET;
    let (mut compacted_history, mut retained_images) = build_v2_compacted_history(
        &prompt_input,
        &prompt_input_metadata,
        compaction_output.clone(),
        sess.features().get(),
        retained_message_token_budget,
        retained_image_budget,
    );
    let complete_replacement = if prefix.is_none() {
        let prepared_window = sess.prepare_auto_compact_window().await;
        let (window_prefix, context, world_state_baseline) = build_compaction_context_for_window(
            sess.as_ref(),
            replacement_step_context,
            world_state,
            Some(prepared_window.ids),
        )
        .await;
        let base_instructions = sess.get_base_instructions().await;
        let mut attempts = 0;
        let new_history = loop {
            let new_history = assemble_compaction_history(
                compacted_history.clone(),
                window_prefix.clone(),
                context.clone(),
            );
            let candidate_tokens = estimate_history_token_count(&new_history, &base_instructions);
            let scope = replacement_step_context
                .turn
                .config
                .model_auto_compact_token_limit_scope;
            let status = context_window_token_status_for_usage(
                replacement_step_context.turn.config.as_ref(),
                replacement_step_context.turn.model_info().as_ref(),
                candidate_tokens,
                matches!(scope, AutoCompactTokenLimitScope::BodyAfterPrefix)
                    .then_some(candidate_tokens),
            );
            let overflow = compaction_candidate_overflow(&status, scope);
            if overflow == 0 {
                break new_history;
            }
            if retained_message_token_budget == 0 {
                return Err(CodexErr::ContextWindowExceeded);
            }
            attempts += 1;
            // Shrink from what was actually retained; a larger budget rebuilds the same
            // candidate.
            let retained_tokens = compacted_history
                .split_last()
                .map_or(&[][..], |(_compaction_output, retained)| retained)
                .iter()
                .map(|envelope| estimate_item_token_count(&envelope.item))
                .fold(0i64, i64::saturating_add);
            retained_message_token_budget = if attempts >= 3 {
                0
            } else {
                retained_message_token_budget
                    .min(usize::try_from(retained_tokens).unwrap_or(0))
                    .saturating_sub(usize::try_from(overflow).unwrap_or(usize::MAX))
            };
            (compacted_history, retained_images) = build_v2_compacted_history(
                &prompt_input,
                &prompt_input_metadata,
                compaction_output.clone(),
                sess.features().get(),
                retained_message_token_budget,
                retained_image_budget,
            );
        };
        Some((new_history, world_state_baseline, prepared_window))
    } else {
        None
    };
    let reviewer_compaction_hash = reviewer_compaction_hash(sess, turn_context).await;
    let installed = if let Some((prefix, model_info, cutoff_world_state)) = prefix {
        let (initial_context, world_state_baseline) = sess
            .build_initial_context_with_world_state(replacement_step_context, &cutoff_world_state)
            .await;
        let (prefix_context, initial_context) = split_prefix_updates(initial_context);
        let initial_context = merge_world_state_updates(initial_context)
            .into_iter()
            .map(ResponseItemEnvelope::new)
            .collect();
        sess.replace_compacted_history(
            compacted_history,
            replacement_step_context.to_turn_context_item(),
            world_state_baseline,
            CompactedHistoryInstallation::Prefix {
                prefix,
                prefix_context,
                initial_context,
                model_info: &model_info,
                compaction_response_id,
                reviewer_compaction_hash,
                turn_context,
                started_item: needs_start.then_some(&compaction_item),
            },
        )
        .await
    } else {
        let (new_history, world_state_baseline, prepared_window) =
            complete_replacement.expect("foreground compaction must have a validated history");
        if !sess
            .commit_prepared_auto_compact_window(prepared_window)
            .await
        {
            return Err(CodexErr::Fatal(
                "auto-compact window changed while validating replacement history".to_string(),
            ));
        }
        let installation = CompactedHistoryMetadata {
            input_goal_ids,
            message: String::new(),
            window_number: prepared_window.window_number,
            window_ids: prepared_window.ids,
            compaction_response_id: Some(compaction_response_id),
            compaction_model_hash: compaction_turn_context.model_info().comp_hash.clone(),
            reviewer_compaction_hash,
        };
        sess.replace_compacted_history(
            new_history,
            replacement_step_context.to_turn_context_item(),
            world_state_baseline,
            installation,
        )
        .await
    };
    if !installed {
        return Ok(false);
    }
    // Rejected candidates must not supply analytics for a later foreground attempt.
    if let Some(token_usage) = token_usage {
        analytics_details.compaction_summary_tokens = Some(token_usage.output_tokens);
        analytics_details.cached_input_tokens = Some(token_usage.cached_input_tokens);
        analytics_details.cache_write_input_tokens = Some(token_usage.cache_write_input_tokens);
    }
    analytics_details.retained_image_count = Some(retained_images);
    if let Some(trace_input_history) = trace_input_history.as_deref() {
        let replacement_history = sess
            .clone_history()
            .await
            .raw_items()
            .cloned()
            .collect::<Vec<_>>();
        compaction_trace.record_installed(&CompactionCheckpointTracePayload {
            input_history: trace_input_history,
            replacement_history: &replacement_history,
        });
    }
    sess.recompute_token_usage(&replacement_step_context.turn)
        .await;
    sess.emit_turn_item_completed(compaction_turn_context, compaction_item)
        .await;
    Ok(true)
}

async fn reviewer_compaction_hash(sess: &Session, turn_context: &TurnContext) -> Option<String> {
    if crate::context::GuardianContextMode::from_history(
        sess.conversation_history_snapshot().await.as_ref(),
    ) == crate::context::GuardianContextMode::Legacy
        && let Some(review_turn) = sess.turn_context_for_sub_id(&turn_context.sub_id).await
    {
        // Previous-model compaction must remain compatible with the continuing turn's
        // reviewer, including model changes accepted while compaction was running.
        let mut review_context = crate::guardian::GuardianReviewContext::from(&review_turn);
        review_context.model_info = review_turn.capture_current_model_info();
        let (_, reviewer) = crate::guardian::resolve_review_model(sess, &review_context).await;
        reviewer.comp_hash.clone()
    } else {
        None
    }
}
