mod approvals;
pub(crate) mod code_mode;
pub(crate) mod context;
pub(crate) mod events;
mod executed_tool_calls;
pub(crate) mod handlers;
pub(crate) mod hook_names;
pub(crate) mod hosted_spec;
pub(crate) mod lifecycle;
pub(crate) mod network_approval;
pub(crate) mod orchestrator;
pub(crate) mod parallel;
pub(crate) mod registry;
pub(crate) mod router;
pub(crate) mod runtimes;
pub(crate) mod sandboxing;
pub(crate) mod spec_plan;
pub(crate) mod tool_dispatch_trace;
mod tool_namespaces_info;

#[cfg(test)]
#[path = "format_exec_output_tests.rs"]
mod tests;

use std::borrow::Cow;

use crate::exec::ExecCaptureMetadata;
use crate::session::turn_context::TurnContext;
pub(crate) use approvals::ApprovalContext;
use codex_features::Feature;
use codex_protocol::exec_output::ExecToolCallOutput;
use codex_protocol::openai_models::ToolMode;
use codex_tools::ToolName;
pub(crate) use executed_tool_calls::ExecutedToolCallRecorder;
use codex_utils_output_truncation::OutputTruncation;
use codex_utils_output_truncation::formatted_truncate_text_with_config;
use codex_utils_output_truncation::truncate_text_with_config;
pub use router::ToolRouter;

// Telemetry preview limits: keep log events smaller than model budgets.
pub(crate) const TELEMETRY_PREVIEW_MAX_BYTES: usize = 2 * 1024; // 2 KiB
pub(crate) const TELEMETRY_PREVIEW_MAX_LINES: usize = 64; // lines
pub(crate) const TELEMETRY_PREVIEW_TRUNCATION_NOTICE: &str =
    "[... telemetry preview truncated ...]";

/// Legacy boundaries such as hook payloads, telemetry tags, and Responses tool
/// names still require a single flattened string. Keep comparisons and sorting
/// on `ToolName` itself; use this only when crossing those boundaries.
pub(crate) fn flat_tool_name(tool_name: &ToolName) -> Cow<'_, str> {
    if tool_name.is_default_namespace() {
        return Cow::Borrowed(tool_name.name.as_str());
    }

    match tool_name.namespace.as_deref() {
        Some(namespace) => {
            let mut name = String::with_capacity(namespace.len() + tool_name.name.len());
            name.push_str(namespace);
            name.push_str(&tool_name.name);
            Cow::Owned(name)
        }
        None => Cow::Borrowed(tool_name.name.as_str()),
    }
}

pub(crate) fn tool_user_shell_type(
    user_shell: &crate::shell::Shell,
) -> codex_tools::ToolUserShellType {
    match user_shell.shell_type {
        crate::shell::ShellType::Zsh => codex_tools::ToolUserShellType::Zsh,
        crate::shell::ShellType::Bash => codex_tools::ToolUserShellType::Bash,
        crate::shell::ShellType::PowerShell => codex_tools::ToolUserShellType::PowerShell,
        crate::shell::ShellType::Sh => codex_tools::ToolUserShellType::Sh,
        crate::shell::ShellType::Cmd => codex_tools::ToolUserShellType::Cmd,
    }
}

pub(crate) fn requested_tool_mode(turn_context: &TurnContext) -> ToolMode {
    turn_context.model_info.tool_mode.unwrap_or_else(|| {
        if turn_context.config.features.enabled(Feature::CodeModeOnly) {
            ToolMode::CodeModeOnly
        } else if turn_context.config.features.enabled(Feature::CodeMode) {
            ToolMode::CodeMode
        } else {
            ToolMode::Direct
        }
    })
}

pub(crate) fn effective_tool_mode(turn_context: &TurnContext) -> ToolMode {
    let requested_tool_mode = requested_tool_mode(turn_context);
    if !turn_context.code_mode_available
        && requested_tool_mode == ToolMode::CodeMode
        && !turn_context.config.code_mode.disable_in_process_fallback
    {
        ToolMode::Direct
    } else {
        requested_tool_mode
    }
}

/// Format the combined exec output for sending back to the model.
/// Includes exit code and duration metadata; truncates large bodies safely.
pub fn format_exec_output_for_model(
    exec_output: &ExecToolCallOutput,
    truncation: OutputTruncation,
) -> String {
    // round to 1 decimal place
    let duration_seconds = ((exec_output.duration.as_secs_f32()) * 10.0).round() / 10.0;

    let content = build_content_with_timeout(exec_output);

    let total_lines = content.lines().count();

    let formatted_output = truncate_text_with_config(&content, truncation);

    let mut sections = Vec::new();

    sections.push(format!("Exit code: {}", exec_output.exit_code));
    sections.push(format!("Wall time: {duration_seconds} seconds"));
    if total_lines != formatted_output.lines().count() {
        sections.push(format!("Total output lines: {total_lines}"));
    }

    sections.push("Output:".to_string());
    sections.push(formatted_output);

    sections.join("\n")
}

pub(crate) fn format_captured_exec_output_for_model(
    exec_output: &ExecToolCallOutput,
    metadata: Option<&ExecCaptureMetadata>,
    truncation: OutputTruncation,
) -> String {
    let formatted = format_exec_output_for_model(exec_output, truncation);
    let Some(metadata) =
        metadata.filter(|metadata| metadata.omitted_bytes > 0 || metadata.capture_incomplete)
    else {
        return formatted;
    };
    let warning = if metadata.capture_incomplete {
        format!(
            "Capture warning: observed bytes lower bound: {}; observed omitted bytes lower bound: {}; estimated observed token count lower bound: approximately {}; full size unknown.",
            metadata.observed_bytes,
            metadata.omitted_bytes,
            metadata.estimated_original_token_count,
        )
    } else {
        format!(
            "Capture warning: observed total bytes: {}; bytes omitted: {}; estimated original token count: approximately {}.",
            metadata.observed_bytes,
            metadata.omitted_bytes,
            metadata.estimated_original_token_count,
        )
    };
    format!("{warning}\n{formatted}")
}

pub fn format_exec_output_str(
    exec_output: &ExecToolCallOutput,
    truncation: OutputTruncation,
) -> String {
    let content = build_content_with_timeout(exec_output);

    // Truncate for model consumption before serialization.
    formatted_truncate_text_with_config(&content, truncation)
}

/// Extracts exec output content and prepends a timeout message if the command timed out.
fn build_content_with_timeout(exec_output: &ExecToolCallOutput) -> String {
    if exec_output.timed_out {
        format!(
            "command timed out after {} milliseconds\n{}",
            exec_output.duration.as_millis(),
            exec_output.aggregated_output.text
        )
    } else {
        exec_output.aggregated_output.text.clone()
    }
}
