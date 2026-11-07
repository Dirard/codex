//! Fork: the full text of a truncated tool result is saved to a private file, and the model sees
//! the same preview as before plus one line with the path (`specs/tool-output-spill-2026-11-07.md`,
//! feature `tool_output_spill`).
//!
//! - Use case 1: [`spill_truncated_output`] decides, after PostToolUse hooks, whether the
//!   model-visible item of a direct call lost part of the full text, saves it and wraps the
//!   result.
//! - Use case 2: outputs declare their full text through `ToolOutput::spill_text`; dynamic
//!   tools through [`ItemTextSpill`].
//! - Use case 3 and 5: [`store`] writes, bounds and removes the files.
//! - Use case 4: [`SpilledToolOutput`] pre-applies the history truncation, appends the notice
//!   and records with a policy that leaves the preview unchanged, so replay is byte-identical.

mod notice;
pub(crate) mod store;

use std::borrow::Cow;

use codex_features::Feature;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_tools::ToolName;
use codex_utils_audio::estimate_audio_token_count;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::apply_output_truncation;
use codex_utils_output_truncation::approx_bytes_for_tokens;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::with_serialization_allowance;
use codex_utils_path_uri::PathUri;
use serde_json::Value as JsonValue;
use tracing::warn;

use self::store::SpillReader;
use self::store::SpillRecord;
use crate::session::step_context::StepContext;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolCallSource;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::registry::AnyToolResult;

/// Use case 1, steps "save the full text to a private file" and "wrap the result with the spill
/// notice": when `tool_output_spill` is enabled and the model-visible item of a direct call,
/// after the formatter and the history truncation that recording will apply, lost part of the
/// result's full text, saves the full text and replaces the result with [`SpilledToolOutput`].
/// Otherwise, or when saving fails, the result stays as it is (R1.1, R1.2, R1.5).
pub(crate) async fn spill_truncated_output(
    invocation: &ToolInvocation,
    result: &mut AnyToolResult,
) {
    let step = invocation.step_context.as_ref();
    let config = &step.turn.config;
    if !config.features.enabled(Feature::ToolOutputSpill)
        || !matches!(
            invocation.source,
            ToolCallSource::Direct | ToolCallSource::DirectPlaintextMessage
        )
    {
        return;
    }
    let Some(full) = result.result.spill_text() else {
        return;
    };
    if full.is_empty() {
        return;
    }
    let Some(thread_dir) = spill_reader_dir(step, invocation.session.thread_id) else {
        return;
    };
    let truncation = history_truncation(result.result.as_ref(), step);
    let (ResponseInputItem::FunctionCallOutput { mut output, .. }
    | ResponseInputItem::CustomToolCallOutput { mut output, .. }) = result
        .result
        .to_response_item(&result.call_id, &result.payload)
    else {
        return;
    };
    apply_output_truncation(&mut output, truncation, estimate_audio_token_count);
    if visible_text(&output).contains(full.as_ref()) {
        return;
    }
    let full = full.into_owned();
    let record =
        match store::save(thread_dir, &result.call_id, full, *config.tool_output_spill).await {
            Ok(record) => record,
            Err(err) => {
                warn!(
                    call_id = %result.call_id,
                    %err,
                    "failed to save the full output of a truncated tool result"
                );
                return;
            }
        };
    let original = std::mem::replace(
        &mut result.result,
        Box::new(FunctionToolOutput::from_text(String::new(), None)),
    );
    result.result = Box::new(SpilledToolOutput {
        original,
        record,
        truncation,
        payload: result.payload.clone(),
    });
}

/// Use case 3, step "find a spill directory that the step's exec_command can read": the
/// thread directory when the step's primary environment is local, `exec_command` is available
/// to the model (directly, through Code Mode or tool search) and its sandbox can read the
/// directory (R3.4–R3.6).
fn spill_reader_dir(
    step: &StepContext,
    thread_id: codex_protocol::ThreadId,
) -> Option<std::path::PathBuf> {
    let exposes_exec_command = step
        .tool_router
        .exposes_tool(&ToolName::plain("exec_command"));
    let primary = step.environments.primary().map(|environment| SpillReader {
        remote: environment.environment().is_remote(),
        policy: environment
            .permission_profile_with_workspace_roots()
            .file_system_sandbox_policy(),
        cwd: PathUri::to_abs_path(environment.cwd())
            .map(|cwd| cwd.as_path().to_path_buf())
            .unwrap_or_default(),
    });
    store::reader_dir(
        primary,
        exposes_exec_command,
        step.turn.config.codex_home.as_path(),
        thread_id,
    )
}

/// The truncation that recording applies to `output` (R4.1): its own override, else the
/// step's output truncation with the serialization allowance, as
/// `annotate_tool_output_metadata` computes it.
fn history_truncation(output: &dyn ToolOutput, step: &StepContext) -> OutputTruncation {
    history_truncation_for(output, step.output_truncation())
}

/// [`history_truncation`] with the step's output truncation `step_truncation`.
fn history_truncation_for(
    output: &dyn ToolOutput,
    step_truncation: OutputTruncation,
) -> OutputTruncation {
    output.history_truncation_override().unwrap_or_else(|| {
        step_truncation.with_policy(with_serialization_allowance(step_truncation.policy))
    })
}

/// The non-empty texts of `items` joined with `"\n"`: what the model reads of them. Media and
/// encrypted content do not take part; history drops empty texts.
pub(crate) fn item_text(items: &[FunctionCallOutputContentItem]) -> Cow<'_, str> {
    let mut texts = items.iter().filter_map(|item| match item {
        FunctionCallOutputContentItem::InputText { text } if !text.is_empty() => {
            Some(text.as_str())
        }
        _ => None,
    });
    let Some(first) = texts.next() else {
        return Cow::Borrowed("");
    };
    let Some(second) = texts.next() else {
        return Cow::Borrowed(first);
    };
    let mut joined = format!("{first}\n{second}");
    for text in texts {
        joined.push('\n');
        joined.push_str(text);
    }
    Cow::Owned(joined)
}

/// The text the model reads of a tool output.
fn visible_text(output: &FunctionCallOutputPayload) -> Cow<'_, str> {
    match &output.body {
        FunctionCallOutputBody::Text(text) => Cow::Borrowed(text),
        FunctionCallOutputBody::ContentItems(items) => item_text(items),
    }
}

/// Appends the notice after the last text, separated by an empty line, or as a text item of
/// its own after media (R4.2).
fn append_notice(output: &mut FunctionCallOutputPayload, notice: &str) {
    match &mut output.body {
        FunctionCallOutputBody::Text(text) => {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(notice);
        }
        FunctionCallOutputBody::ContentItems(items) => match items.last_mut() {
            Some(FunctionCallOutputContentItem::InputText { text }) if !text.is_empty() => {
                text.push_str("\n\n");
                text.push_str(notice);
            }
            _ => items.push(FunctionCallOutputContentItem::InputText {
                text: notice.to_string(),
            }),
        },
    }
}

/// A policy of the kind of `base` under which history keeps `output` whole: at least the
/// cost of every text and audio item as `truncate_function_output_payload` charges it, and,
/// for a byte policy, enough bytes that its token budget covers the token cost too (R4.3).
fn recording_policy(
    base: TruncationPolicy,
    output: &FunctionCallOutputPayload,
) -> TruncationPolicy {
    let (bytes, tokens) = match &output.body {
        FunctionCallOutputBody::Text(text) => (text.len(), approx_token_count(text)),
        FunctionCallOutputBody::ContentItems(items) => {
            items
                .iter()
                .fold((0usize, 0usize), |(bytes, tokens), item| match item {
                    FunctionCallOutputContentItem::InputText { text } => (
                        bytes.saturating_add(text.len()),
                        tokens.saturating_add(approx_token_count(text)),
                    ),
                    FunctionCallOutputContentItem::InputAudio { audio_url } => {
                        let audio_tokens = estimate_audio_token_count(audio_url);
                        (
                            bytes.saturating_add(approx_bytes_for_tokens(audio_tokens)),
                            tokens.saturating_add(audio_tokens),
                        )
                    }
                    FunctionCallOutputContentItem::InputImage { .. }
                    | FunctionCallOutputContentItem::EncryptedContent { .. } => (bytes, tokens),
                })
        }
    };
    match base {
        TruncationPolicy::Bytes(limit) => {
            TruncationPolicy::Bytes(limit.max(bytes).max(approx_bytes_for_tokens(tokens)))
        }
        TruncationPolicy::Tokens(limit) => TruncationPolicy::Tokens(limit.max(tokens)),
    }
}

/// Use case 4: a spilled result. The model-visible item is the item of the original output
/// after the history truncation `truncation`, with the spill notice; it is computed when the
/// item is built, so a later `set_handler_duration_ms` is reflected (R4.4). Recording keeps it
/// unchanged on every replay (R4.3). Everything else is the original output's.
pub(crate) struct SpilledToolOutput {
    original: Box<dyn ToolOutput>,
    record: SpillRecord,
    /// The history truncation the preview was cut with (`t_h`).
    truncation: OutputTruncation,
    /// The call's payload, for the recording policy, which has no payload argument.
    payload: ToolPayload,
}

impl SpilledToolOutput {
    /// A spilled `original` with a fixed record, cut by the history truncation of a step whose
    /// output truncation is `step_truncation`.
    #[cfg(test)]
    pub(crate) fn for_test(
        original: Box<dyn ToolOutput>,
        step_truncation: OutputTruncation,
        payload: ToolPayload,
    ) -> Self {
        let truncation = history_truncation_for(original.as_ref(), step_truncation);
        Self {
            original,
            record: SpillRecord {
                path: std::path::PathBuf::from(
                    "/home/user/.codex/tool_outputs/thread/0123456789abcdef.txt",
                ),
                lines: 5000,
                bytes: 38_893,
                tokens: 9_724,
                capped: None,
            },
            truncation,
            payload,
        }
    }

    fn preview(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        let mut item = self.original.to_response_item(call_id, payload);
        if let ResponseInputItem::FunctionCallOutput { output, .. }
        | ResponseInputItem::CustomToolCallOutput { output, .. } = &mut item
        {
            apply_output_truncation(output, self.truncation, estimate_audio_token_count);
            append_notice(output, &notice::spill_notice(&self.record));
        }
        item
    }

    fn recording_truncation(&self, item: &ResponseInputItem) -> OutputTruncation {
        let policy = match item {
            ResponseInputItem::FunctionCallOutput { output, .. }
            | ResponseInputItem::CustomToolCallOutput { output, .. } => {
                recording_policy(self.truncation.policy, output)
            }
            _ => self.truncation.policy,
        };
        OutputTruncation::new(policy, /*max_lines*/ None)
    }
}

impl ToolOutput for SpilledToolOutput {
    fn log_output(&self) -> String {
        self.original.log_output()
    }

    fn success_for_logging(&self) -> bool {
        self.original.success_for_logging()
    }

    fn set_handler_duration_ms(&mut self, handler_duration_ms: u64) {
        self.original.set_handler_duration_ms(handler_duration_ms);
    }

    fn contains_external_context(&self) -> bool {
        self.original.contains_external_context()
    }

    fn history_truncation_override(&self) -> Option<OutputTruncation> {
        Some(self.recording_truncation(&self.preview("", &self.payload)))
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        let item = self.preview(call_id, payload);
        #[cfg(debug_assertions)]
        if let ResponseInputItem::FunctionCallOutput { output, .. }
        | ResponseInputItem::CustomToolCallOutput { output, .. } = &item
        {
            let mut recorded = output.clone();
            apply_output_truncation(
                &mut recorded,
                self.recording_truncation(&item),
                estimate_audio_token_count,
            );
            debug_assert_eq!(
                &recorded, output,
                "recording must leave a spilled preview unchanged"
            );
        }
        item
    }

    fn post_tool_use_id(&self, call_id: &str) -> String {
        self.original.post_tool_use_id(call_id)
    }

    fn post_tool_use_cwd(&self) -> Option<PathUri> {
        self.original.post_tool_use_cwd()
    }

    fn post_tool_use_input(&self, payload: &ToolPayload) -> Option<JsonValue> {
        self.original.post_tool_use_input(payload)
    }

    fn post_tool_use_response(&self, call_id: &str, payload: &ToolPayload) -> Option<JsonValue> {
        self.original.post_tool_use_response(call_id, payload)
    }

    fn code_mode_result(&self, payload: &ToolPayload) -> JsonValue {
        self.original.code_mode_result(payload)
    }

    fn tool_result_metadata(&self) -> Option<&JsonValue> {
        self.original.tool_result_metadata()
    }
}

/// Use case 2: the boxed output of a client-supplied dynamic tool, eligible for spilling (R2.4).
pub(crate) fn fork_dynamic_tool_output(output: FunctionToolOutput) -> Box<dyn ToolOutput> {
    Box::new(ItemTextSpill(output))
}

/// Use case 2: the output of a client-supplied dynamic tool, eligible for spilling with the
/// text of its content items (R2.4). Everything else is the wrapped [`FunctionToolOutput`]'s,
/// which `apply_patch`, `read_mcp_resource` and agent tools return without this wrapper.
pub(crate) struct ItemTextSpill(pub(crate) FunctionToolOutput);

impl ToolOutput for ItemTextSpill {
    fn log_output(&self) -> String {
        self.0.log_output()
    }

    fn success_for_logging(&self) -> bool {
        self.0.success_for_logging()
    }

    fn set_handler_duration_ms(&mut self, handler_duration_ms: u64) {
        self.0.set_handler_duration_ms(handler_duration_ms);
    }

    fn contains_external_context(&self) -> bool {
        self.0.contains_external_context()
    }

    fn history_truncation_override(&self) -> Option<OutputTruncation> {
        self.0.history_truncation_override()
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        self.0.to_response_item(call_id, payload)
    }

    fn post_tool_use_id(&self, call_id: &str) -> String {
        self.0.post_tool_use_id(call_id)
    }

    fn post_tool_use_cwd(&self) -> Option<PathUri> {
        self.0.post_tool_use_cwd()
    }

    fn post_tool_use_input(&self, payload: &ToolPayload) -> Option<JsonValue> {
        self.0.post_tool_use_input(payload)
    }

    fn post_tool_use_response(&self, call_id: &str, payload: &ToolPayload) -> Option<JsonValue> {
        self.0.post_tool_use_response(call_id, payload)
    }

    fn code_mode_result(&self, payload: &ToolPayload) -> JsonValue {
        self.0.code_mode_result(payload)
    }

    fn tool_result_metadata(&self) -> Option<&JsonValue> {
        self.0.tool_result_metadata()
    }

    fn spill_text(&self) -> Option<Cow<'_, str>> {
        Some(item_text(&self.0.body))
    }
}

/// Fork hook of `thread/delete` in app-server: removes the saved full outputs of `thread_ids`,
/// whose threads are gone (`specs/tool-output-spill-2026-11-07.md`, R5.3). `thread/archive`
/// keeps them (R5.4).
pub async fn discard_tool_output_spills(
    codex_home: &std::path::Path,
    thread_ids: &[codex_protocol::ThreadId],
) {
    store::discard_threads(codex_home, thread_ids).await;
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
