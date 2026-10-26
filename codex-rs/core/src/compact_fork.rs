//! Fork: local compaction requests carry the step's tools for prompt-cache parity
//! (docs/fork/02-compaction-context.md F7), and a prepared compaction window renders its
//! canonical context once (F1, `compact_remote_v2_installation.rs`).

use std::sync::Arc;

use crate::context::world_state::WorldState;
use crate::context::world_state::WorldStateSnapshot;
use crate::context::world_state::split_prefix_updates;
use crate::context_manager::ContextManager;
use crate::context_manager::updates::merge_world_state_updates;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::state::AutoCompactWindowIds;
use crate::stream_events_utils::last_assistant_message_from_item;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ResponseInputItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::user_input::UserInput;
use codex_tools::ToolSpec;

/// Sent only on the single retry after a local compaction response called a tool.
const LOCAL_COMPACTION_SUMMARY_REMINDER: &str =
    "Reply with the handoff summary as plain text. Do not call tools.";
/// A terminal, non-network failure once the corrective retry called a tool again.
const LOCAL_COMPACTION_TOOL_CALL_ERROR: &str = "Context compaction failed: the model called a \
     tool instead of writing a summary, even after a reminder. The conversation history was \
     left unchanged.";

/// The prefix and ordinary context of a compaction replacement, rendered for `window_ids`
/// when the window was prepared ahead of the candidate, else as upstream renders it.
pub(crate) async fn build_compaction_context_for_window(
    sess: &Session,
    step_context: &StepContext,
    world_state: &WorldState,
    window_ids: Option<AutoCompactWindowIds>,
) -> (Vec<ResponseItem>, Vec<ResponseItem>, WorldStateSnapshot) {
    let (updates, snapshot) = match window_ids {
        Some(ids) => {
            sess.build_initial_context_with_world_state_for_window(step_context, world_state, ids)
                .await
        }
        None => {
            sess.build_initial_context_with_world_state(step_context, world_state)
                .await
        }
    };
    let (prefix, context) = split_prefix_updates(updates);
    (prefix, merge_world_state_updates(context), snapshot)
}

/// Tools are sent for prompt-cache parity, but a Responses Lite request whose input already
/// declares them sends none, exactly like the ordinary request in that window. Decided per
/// attempt: trimming may drop the declaration from the input.
pub(super) fn prompt_tools(
    step_context: &StepContext,
    turn_input: &[ResponseItem],
) -> Arc<[ToolSpec]> {
    if step_context.settings.model_info.use_responses_lite
        && turn_input
            .iter()
            .any(|item| matches!(item, ResponseItem::AdditionalTools { .. }))
    {
        Arc::default()
    } else {
        step_context.tool_router.model_visible_specs()
    }
}

/// The tool call was never run; asks once more, explicitly, instead of installing an earlier
/// assistant reply as the summary. A second tool call fails the compaction.
pub(super) fn remind_once(
    summary_reminder_sent: &mut bool,
    history: &mut ContextManager,
    step_context: &StepContext,
) -> CodexResult<()> {
    if *summary_reminder_sent {
        return Err(CodexErr::InvalidRequest(
            LOCAL_COMPACTION_TOOL_CALL_ERROR.to_string(),
        ));
    }
    *summary_reminder_sent = true;
    let reminder = ResponseInputItem::from(vec![UserInput::Text {
        text: LOCAL_COMPACTION_SUMMARY_REMINDER.to_string(),
        text_elements: Vec::new(),
    }]);
    history.record_items(&[reminder.into()], step_context.output_truncation());
    Ok(())
}

/// Client-executed tool calls that a compaction response must not leave unanswered.
fn is_tool_call(item: &ResponseItem) -> bool {
    matches!(
        item,
        ResponseItem::FunctionCall { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::LocalShellCall { .. }
            | ResponseItem::ToolSearchCall { .. }
    )
}

/// Requests carry tools for prompt-cache parity, so a model may call one instead of
/// summarizing. When a response calls a tool, only its last visible assistant message can be
/// the summary, and only if it follows the last call and is not commentary: a preamble before
/// a call would otherwise replace the whole history.
pub(super) fn tool_call_without_summary(output: &[ResponseItem]) -> bool {
    let Some(last_tool_call) = output.iter().rposition(is_tool_call) else {
        return false;
    };
    let summary = output.iter().enumerate().rev().find(|(_, item)| {
        last_assistant_message_from_item(item, /*plan_mode*/ false).is_some()
    });
    !summary.is_some_and(|(index, item)| {
        index > last_tool_call
            && !matches!(
                item,
                ResponseItem::Message {
                    phase: Some(MessagePhase::Commentary),
                    ..
                }
            )
    })
}

#[cfg(test)]
#[path = "compact_fork_tests.rs"]
mod tests;
