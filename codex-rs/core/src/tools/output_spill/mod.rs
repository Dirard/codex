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
//! - Use case 7: [`spill_truncated_output`] also decides for nested Code Mode calls whether the
//!   text the script receives lost part of the full text; [`SpilledToolOutput`] then appends the
//!   notice to that text and leaves the rest of the result as it was.

mod notice;
pub(crate) mod store;

use std::borrow::Cow;

use codex_features::Feature;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ResponseInputItem;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
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
/// Use case 7, the same steps for a nested Code Mode call: when the text the script receives
/// lost part of the full text. Otherwise, or when saving fails, the result stays as it is
/// (R1.1, R1.2, R1.5, R7.1, R7.6).
pub(crate) async fn spill_truncated_output(
    invocation: &ToolInvocation,
    result: &mut AnyToolResult,
) {
    let step = invocation.step_context.as_ref();
    let config = &step.turn.config;
    if !config.features.enabled(Feature::ToolOutputSpill) {
        return;
    }
    let Some(full) = result.result.spill_text() else {
        return;
    };
    if full.is_empty() {
        return;
    }
    let target = match invocation.source {
        ToolCallSource::Direct | ToolCallSource::DirectPlaintextMessage => {
            let truncation = history_truncation(result.result.as_ref(), step);
            if !model_item_is_cut(result, truncation, &full) {
                return;
            }
            SpillTarget::Model {
                truncation,
                payload: result.payload.clone(),
            }
        }
        ToolCallSource::CodeMode { .. } => {
            if !script_text_is_cut(result, &full) {
                return;
            }
            SpillTarget::Script
        }
    };
    // After the in-memory decision: the reader check resolves the sandbox policy for every
    // directory above the file and touches the disk.
    let Some(thread_dir) = spill_reader_dir(step, invocation.session.thread_id) else {
        return;
    };
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
    let notice = notice::spill_notice(&record, &exec_command_reference(step, &invocation.source));
    let original = std::mem::replace(
        &mut result.result,
        Box::new(FunctionToolOutput::from_text(String::new(), None)),
    );
    result.result = Box::new(SpilledToolOutput {
        original,
        notice,
        target,
    });
}

/// Use case 1, step "apply the history truncation to the model-visible item": whether the item
/// the model will see, after `truncation`, lost part of `full`.
fn model_item_is_cut(result: &AnyToolResult, truncation: OutputTruncation, full: &str) -> bool {
    let (ResponseInputItem::FunctionCallOutput { mut output, .. }
    | ResponseInputItem::CustomToolCallOutput { mut output, .. }) = result
        .result
        .to_response_item(&result.call_id, &result.payload)
    else {
        return false;
    };
    apply_output_truncation(&mut output, truncation, estimate_audio_token_count);
    !visible_text(&output).contains(full)
}

/// Use case 7, step "compare the full text with what the script receives": whether the text of
/// the nested result that the script gets (R7.2, R7.3) lost part of `full`. The result is the
/// one the script would receive, so the call's own `max_output_tokens` is taken into account.
fn script_text_is_cut(result: &AnyToolResult, full: &str) -> bool {
    let value = result.result.code_mode_result(&result.payload);
    script_text(&value).is_some_and(|text| !text.contains(full))
}

/// The text of a nested result's Code Mode value that Codex may cut before the script gets it:
/// the `output` of an `exec_command` or `write_stdin` result, which the call's
/// `max_output_tokens` bounds (R7.2). An MCP result (the whole `CallToolResult`) and a dynamic
/// tool result (its whole text) have none: the script receives them uncut (R7.3).
fn script_text(value: &JsonValue) -> Option<&str> {
    value.get(SCRIPT_TEXT_FIELD)?.as_str()
}

/// [`script_text`], to append the notice to.
fn script_text_mut(value: &mut JsonValue) -> Option<&mut String> {
    match value.get_mut(SCRIPT_TEXT_FIELD)? {
        JsonValue::String(text) => Some(text),
        _ => None,
    }
}

/// The field of the `exec_command` and `write_stdin` Code Mode result that holds the command
/// output (`ExecCommandToolOutput::code_mode_result`).
const SCRIPT_TEXT_FIELD: &str = "output";

/// Use case 7, R7.4: how the model calls `exec_command` to read the file. A notice that reaches
/// a Code Mode script, or a notice on a step that offers `exec_command` only inside Code Mode,
/// names the nested tool (`tools.exec_command`); otherwise `exec_command`.
fn exec_command_reference(step: &StepContext, source: &ToolCallSource) -> String {
    const EXEC_COMMAND: &str = "exec_command";
    let router = &step.tool_router;
    let exec_command = ToolName::plain(EXEC_COMMAND).with_default_namespace();
    let nested = router
        .code_mode_tool_names()
        .iter()
        .find(|(_, name)| (*name).clone().with_default_namespace() == exec_command)
        .map(|(nested, _)| nested);
    let Some(nested) = nested else {
        return EXEC_COMMAND.to_string();
    };
    let direct = router.model_visible_specs().iter().any(|spec| {
        matches!(spec, ToolSpec::Function(_) | ToolSpec::Freeform(_)) && spec.name() == EXEC_COMMAND
    });
    if matches!(source, ToolCallSource::CodeMode { .. }) || !direct {
        format!("tools.{nested}")
    } else {
        EXEC_COMMAND.to_string()
    }
}

/// Use case 3, step "find a spill directory that the step's exec_command can read": the
/// thread directory when the step's primary environment is local, `exec_command` is available
/// to the model (directly, through Code Mode or tool search) and its sandbox can read the
/// directory but cannot redirect it (R3.4–R3.6).
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
        FunctionCallOutputBody::Text(text) => append_text_notice(text, notice),
        FunctionCallOutputBody::ContentItems(items) => match items.last_mut() {
            Some(FunctionCallOutputContentItem::InputText { text }) if !text.is_empty() => {
                append_text_notice(text, notice);
            }
            _ => items.push(FunctionCallOutputContentItem::InputText {
                text: notice.to_string(),
            }),
        },
    }
}

/// Appends the notice to `text`, after an empty line unless `text` is empty (R4.2, R7.1).
fn append_text_notice(text: &mut String, notice: &str) {
    if !text.is_empty() {
        text.push_str("\n\n");
    }
    text.push_str(notice);
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

/// Use case 4: a spilled result. For a direct call ([`SpillTarget::Model`]) the model-visible
/// item is the item of the original output after the history truncation, with the spill
/// notice; it is computed when the item is built, so a later `set_handler_duration_ms` is
/// reflected (R4.4), and recording keeps it unchanged on every replay (R4.3). For a nested Code
/// Mode call ([`SpillTarget::Script`], use case 7) the script's result carries the notice after
/// its text (R7.1, R7.5). Everything else is the original output's.
pub(crate) struct SpilledToolOutput {
    original: Box<dyn ToolOutput>,
    /// The spill notice (Data of use case 4), naming the file and how to read it.
    notice: String,
    target: SpillTarget,
}

/// Who receives the notice of a [`SpilledToolOutput`].
enum SpillTarget {
    /// The model, through the history item of a direct call (use cases 1 and 4).
    Model {
        /// The history truncation the preview was cut with (`t_h`).
        truncation: OutputTruncation,
        /// The call's payload, for the recording policy, which has no payload argument.
        payload: ToolPayload,
    },
    /// The script of a Code Mode cell, through the result of a nested call (use case 7). Nested
    /// results are not recorded as history items: what the script prints is, as cell output.
    Script,
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
        let record = store::SpillRecord {
            path: std::path::PathBuf::from(
                "/home/user/.codex/tool_outputs/thread/0123456789abcdef.txt",
            ),
            lines: 5000,
            bytes: 38_893,
            tokens: 9_724,
            capped: None,
        };
        Self {
            original,
            notice: notice::spill_notice(&record, "exec_command"),
            target: SpillTarget::Model {
                truncation,
                payload,
            },
        }
    }

    fn preview(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        let mut item = self.original.to_response_item(call_id, payload);
        if let SpillTarget::Model { truncation, .. } = &self.target
            && let ResponseInputItem::FunctionCallOutput { output, .. }
            | ResponseInputItem::CustomToolCallOutput { output, .. } = &mut item
        {
            apply_output_truncation(output, *truncation, estimate_audio_token_count);
            append_notice(output, &self.notice);
        }
        item
    }

    fn recording_truncation(
        truncation: OutputTruncation,
        item: &ResponseInputItem,
    ) -> OutputTruncation {
        let policy = match item {
            ResponseInputItem::FunctionCallOutput { output, .. }
            | ResponseInputItem::CustomToolCallOutput { output, .. } => {
                recording_policy(truncation.policy, output)
            }
            _ => truncation.policy,
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
        match &self.target {
            SpillTarget::Model {
                truncation,
                payload,
            } => Some(Self::recording_truncation(
                *truncation,
                &self.preview("", payload),
            )),
            SpillTarget::Script => self.original.history_truncation_override(),
        }
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        let item = self.preview(call_id, payload);
        #[cfg(debug_assertions)]
        if let SpillTarget::Model { truncation, .. } = &self.target
            && let ResponseInputItem::FunctionCallOutput { output, .. }
            | ResponseInputItem::CustomToolCallOutput { output, .. } = &item
        {
            let mut recorded = output.clone();
            apply_output_truncation(
                &mut recorded,
                Self::recording_truncation(*truncation, &item),
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
        let mut value = self.original.code_mode_result(payload);
        if matches!(self.target, SpillTarget::Script)
            && let Some(text) = script_text_mut(&mut value)
        {
            append_text_notice(text, &self.notice);
        }
        value
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

/// Use case 5, step "sweep old files and the total size": a starting session sweeps the saved
/// outputs of all threads in the background with the feature on or off, so files saved while
/// it was on keep their age and size limits after it is turned off (R5.2; review 2026-11-07,
/// S-3). A sweep runs at most once an hour per `CODEX_HOME` in a process; the handle lets
/// tests wait for it, callers drop it.
pub(crate) fn sweep_on_session_start(
    config: &crate::config::Config,
) -> Option<tokio::task::JoinHandle<()>> {
    store::sweep_in_background(config.codex_home.as_path(), *config.tool_output_spill)
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
