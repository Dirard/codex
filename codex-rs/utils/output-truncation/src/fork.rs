//! Fork: line limits on top of the upstream byte/token truncation, configured by
//! `[output_truncation]` (docs/fork/03-tools-mcp-code-mode.md F1).

use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;

use crate::TruncationPolicy;
use crate::approx_token_count;
use crate::truncate_function_output_items_with_policy;
use crate::truncate_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputTruncation {
    pub policy: TruncationPolicy,
    pub max_lines: Option<usize>,
    pub mcp_max_lines: Option<usize>,
}

impl OutputTruncation {
    pub const fn new(policy: TruncationPolicy, max_lines: Option<usize>) -> Self {
        Self {
            policy,
            max_lines,
            mcp_max_lines: None,
        }
    }

    pub const fn new_with_mcp_max_lines(
        policy: TruncationPolicy,
        max_lines: Option<usize>,
        mcp_max_lines: Option<usize>,
    ) -> Self {
        Self {
            policy,
            max_lines,
            mcp_max_lines,
        }
    }

    pub fn with_policy(self, policy: TruncationPolicy) -> Self {
        Self { policy, ..self }
    }

    /// MCP responses and emitted Code Mode cells use the MCP line override, if configured.
    pub fn for_mcp_output(self) -> Self {
        Self {
            max_lines: self.mcp_max_lines.or(self.max_lines),
            ..self
        }
    }
}

impl From<TruncationPolicy> for OutputTruncation {
    fn from(policy: TruncationPolicy) -> Self {
        Self::new(policy, /*max_lines*/ None)
    }
}

pub fn formatted_truncate_text_with_config(content: &str, config: OutputTruncation) -> String {
    let line_limited = match config.max_lines {
        Some(max_lines) => truncate_middle_lines(content, max_lines),
        None => content.to_string(),
    };
    let result = truncate_text(&line_limited, config.policy);
    if result == content {
        return content.to_string();
    }

    let total_lines = content.lines().count();
    if result != line_limited {
        let original_token_count = approx_token_count(content);
        return format!(
            "Warning: truncated output (original token count: {original_token_count})\nTotal output lines: {total_lines}\n\n{result}"
        );
    }

    format!("Total output lines: {total_lines}\n\n{result}")
}

pub fn truncate_text_with_config(content: &str, config: OutputTruncation) -> String {
    let line_limited = match config.max_lines {
        Some(max_lines) => truncate_middle_lines(content, max_lines),
        None => content.to_string(),
    };

    truncate_text(&line_limited, config.policy)
}

fn truncate_middle_lines(content: &str, max_lines: usize) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let total_lines = lines.len();
    if total_lines <= max_lines {
        return content.to_string();
    }

    let omitted_lines = total_lines.saturating_sub(max_lines);
    let marker = format!("... {omitted_lines} lines truncated ...");
    if max_lines == 0 {
        return marker;
    }

    let head_count = max_lines.saturating_add(1) / 2;
    let tail_count = max_lines.saturating_sub(head_count);
    let mut out = String::new();
    for line in lines.iter().take(head_count) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&marker);
    for line in lines.iter().skip(total_lines.saturating_sub(tail_count)) {
        out.push('\n');
        out.push_str(line);
    }
    out
}

pub fn truncate_function_output_items_with_config(
    items: &[FunctionCallOutputContentItem],
    config: OutputTruncation,
    estimate_audio_token_count: impl Fn(&str) -> usize,
) -> Vec<FunctionCallOutputContentItem> {
    let line_limited_items = match config.max_lines {
        Some(max_lines) => truncate_function_output_item_lines(items, max_lines),
        None => items.to_vec(),
    };

    truncate_function_output_items_with_policy(
        &line_limited_items,
        config.policy,
        estimate_audio_token_count,
    )
}

fn truncate_function_output_item_lines(
    items: &[FunctionCallOutputContentItem],
    max_lines: usize,
) -> Vec<FunctionCallOutputContentItem> {
    let total_lines = items
        .iter()
        .map(|item| match item {
            FunctionCallOutputContentItem::InputText { text } => text.lines().count(),
            FunctionCallOutputContentItem::InputImage { .. }
            | FunctionCallOutputContentItem::InputAudio { .. }
            | FunctionCallOutputContentItem::EncryptedContent { .. } => 0,
        })
        .sum::<usize>();

    if total_lines <= max_lines {
        return items.to_vec();
    }

    let omitted_lines = total_lines.saturating_sub(max_lines);
    let marker = format!("... {omitted_lines} lines truncated ...");
    let head_count = max_lines.saturating_add(1) / 2;
    let tail_count = max_lines.saturating_sub(head_count);
    let tail_start = total_lines.saturating_sub(tail_count);
    let mut out = Vec::new();
    let mut text_line_index = 0usize;
    let mut marker_inserted = false;
    let mut current_text = String::new();

    for item in items {
        match item {
            FunctionCallOutputContentItem::InputText { text } => {
                for line in text.lines() {
                    let keep_line = text_line_index < head_count || text_line_index >= tail_start;
                    if keep_line {
                        if !current_text.is_empty() {
                            current_text.push('\n');
                        }
                        current_text.push_str(line);
                    } else if !marker_inserted {
                        if !current_text.is_empty() {
                            current_text.push('\n');
                        }
                        current_text.push_str(&marker);
                        marker_inserted = true;
                    }
                    text_line_index += 1;
                }
            }
            FunctionCallOutputContentItem::InputImage { .. }
            | FunctionCallOutputContentItem::InputAudio { .. }
            | FunctionCallOutputContentItem::EncryptedContent { .. } => {
                if !current_text.is_empty() {
                    out.push(FunctionCallOutputContentItem::InputText {
                        text: std::mem::take(&mut current_text),
                    });
                }
                out.push(item.clone());
            }
        }
    }

    if !current_text.is_empty() {
        out.push(FunctionCallOutputContentItem::InputText { text: current_text });
    }

    out
}

/// Applies only the line limit, in place. Followed by the upstream
/// `truncate_function_output_payload` with the same policy, it gives exactly
/// `truncate_text_with_config` and `truncate_function_output_items_with_config`.
pub fn truncate_function_output_payload_lines(
    output: &mut FunctionCallOutputPayload,
    max_lines: Option<usize>,
) {
    let Some(max_lines) = max_lines else {
        return;
    };
    match &mut output.body {
        FunctionCallOutputBody::Text(text) => *text = truncate_middle_lines(text, max_lines),
        FunctionCallOutputBody::ContentItems(items) => {
            *items = truncate_function_output_item_lines(items, max_lines);
        }
    }
}
