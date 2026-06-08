//! Helpers for truncating tool and exec output using [`TruncationPolicy`](codex_protocol::protocol::TruncationPolicy).

use codex_protocol::models::FunctionCallOutputContentItem;
pub use codex_utils_string::approx_bytes_for_tokens;
pub use codex_utils_string::approx_token_count;
pub use codex_utils_string::approx_tokens_from_byte_count;
use codex_utils_string::truncate_middle_chars;
use codex_utils_string::truncate_middle_with_token_budget;

pub use codex_protocol::protocol::TruncationPolicy;

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

    pub fn for_mcp_output(self) -> Self {
        Self {
            max_lines: self.mcp_max_lines,
            ..self
        }
    }
}

pub fn formatted_truncate_text(content: &str, policy: TruncationPolicy) -> String {
    if content.len() <= policy.byte_budget() {
        return content.to_string();
    }

    let original_token_count = approx_token_count(content);
    let total_lines = content.lines().count();
    let result = truncate_text(content, policy);
    format!(
        "Warning: truncated output (original token count: {original_token_count})\nTotal output lines: {total_lines}\n\n{result}"
    )
}

pub fn formatted_truncate_text_with_config(content: &str, config: OutputTruncation) -> String {
    let total_lines = content.lines().count();
    let result = truncate_text_with_config(content, config);
    if result == content {
        return content.to_string();
    }

    format!("Total output lines: {total_lines}\n\n{result}")
}

pub fn truncate_text(content: &str, policy: TruncationPolicy) -> String {
    match policy {
        TruncationPolicy::Bytes(bytes) => truncate_middle_chars(content, bytes),
        TruncationPolicy::Tokens(tokens) => truncate_middle_with_token_budget(content, tokens).0,
    }
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
    let marker = format!("…{omitted_lines} lines truncated…");
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

pub fn formatted_truncate_text_content_items_with_policy(
    items: &[FunctionCallOutputContentItem],
    policy: TruncationPolicy,
) -> (Vec<FunctionCallOutputContentItem>, Option<usize>) {
    let text_segments = items
        .iter()
        .filter_map(|item| match item {
            FunctionCallOutputContentItem::InputText { text } => Some(text.as_str()),
            FunctionCallOutputContentItem::InputImage { .. }
            | FunctionCallOutputContentItem::EncryptedContent { .. } => None,
        })
        .collect::<Vec<_>>();

    if text_segments.is_empty() {
        return (items.to_vec(), None);
    }

    let mut combined = String::new();
    for text in &text_segments {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(text);
    }

    if combined.len() <= policy.byte_budget() {
        return (items.to_vec(), None);
    }

    let original_token_count = approx_token_count(&combined);
    let mut out = vec![FunctionCallOutputContentItem::InputText {
        text: formatted_truncate_text(&combined, policy),
    }];
    out.extend(items.iter().filter_map(|item| match item {
        FunctionCallOutputContentItem::InputImage { image_url, detail } => {
            Some(FunctionCallOutputContentItem::InputImage {
                image_url: image_url.clone(),
                detail: *detail,
            })
        }
        FunctionCallOutputContentItem::EncryptedContent { encrypted_content } => {
            Some(FunctionCallOutputContentItem::EncryptedContent {
                encrypted_content: encrypted_content.clone(),
            })
        }
        FunctionCallOutputContentItem::InputText { .. } => None,
    }));

    (out, Some(original_token_count))
}

pub fn truncate_function_output_items_with_policy(
    items: &[FunctionCallOutputContentItem],
    policy: TruncationPolicy,
) -> Vec<FunctionCallOutputContentItem> {
    let mut out: Vec<FunctionCallOutputContentItem> = Vec::with_capacity(items.len());
    let mut remaining_budget = match policy {
        TruncationPolicy::Bytes(_) => policy.byte_budget(),
        TruncationPolicy::Tokens(_) => policy.token_budget(),
    };
    let mut omitted_text_items = 0usize;

    for item in items {
        match item {
            FunctionCallOutputContentItem::InputText { text } => {
                if remaining_budget == 0 {
                    omitted_text_items += 1;
                    continue;
                }

                let cost = match policy {
                    TruncationPolicy::Bytes(_) => text.len(),
                    TruncationPolicy::Tokens(_) => approx_token_count(text),
                };

                if cost <= remaining_budget {
                    out.push(FunctionCallOutputContentItem::InputText { text: text.clone() });
                    remaining_budget = remaining_budget.saturating_sub(cost);
                } else {
                    let snippet_policy = match policy {
                        TruncationPolicy::Bytes(_) => TruncationPolicy::Bytes(remaining_budget),
                        TruncationPolicy::Tokens(_) => TruncationPolicy::Tokens(remaining_budget),
                    };
                    let snippet = truncate_text(text, snippet_policy);
                    if snippet.is_empty() {
                        omitted_text_items += 1;
                    } else {
                        out.push(FunctionCallOutputContentItem::InputText { text: snippet });
                    }
                    remaining_budget = 0;
                }
            }
            FunctionCallOutputContentItem::InputImage { image_url, detail } => {
                out.push(FunctionCallOutputContentItem::InputImage {
                    image_url: image_url.clone(),
                    detail: *detail,
                });
            }
            FunctionCallOutputContentItem::EncryptedContent { encrypted_content } => {
                out.push(FunctionCallOutputContentItem::EncryptedContent {
                    encrypted_content: encrypted_content.clone(),
                });
            }
        }
    }

    if omitted_text_items > 0 {
        out.push(FunctionCallOutputContentItem::InputText {
            text: format!("[omitted {omitted_text_items} text items ...]"),
        });
    }

    out
}

pub fn truncate_function_output_items_with_config(
    items: &[FunctionCallOutputContentItem],
    config: OutputTruncation,
) -> Vec<FunctionCallOutputContentItem> {
    let line_limited_items = match config.max_lines {
        Some(max_lines) => truncate_function_output_item_lines(items, max_lines),
        None => items.to_vec(),
    };

    truncate_function_output_items_with_policy(&line_limited_items, config.policy)
}

fn truncate_function_output_item_lines(
    items: &[FunctionCallOutputContentItem],
    max_lines: usize,
) -> Vec<FunctionCallOutputContentItem> {
    let text_segments = items
        .iter()
        .filter_map(|item| match item {
            FunctionCallOutputContentItem::InputText { text } => Some(text.as_str()),
            FunctionCallOutputContentItem::InputImage { .. }
            | FunctionCallOutputContentItem::EncryptedContent { .. } => None,
        })
        .collect::<Vec<_>>();

    if text_segments.is_empty() {
        return items.to_vec();
    }

    let mut combined = String::new();
    for text in &text_segments {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(text);
    }

    let truncated = truncate_middle_lines(&combined, max_lines);
    if truncated == combined {
        return items.to_vec();
    }

    let mut out = vec![FunctionCallOutputContentItem::InputText { text: truncated }];
    out.extend(items.iter().filter_map(|item| match item {
        FunctionCallOutputContentItem::InputImage { image_url, detail } => {
            Some(FunctionCallOutputContentItem::InputImage {
                image_url: image_url.clone(),
                detail: *detail,
            })
        }
        FunctionCallOutputContentItem::EncryptedContent { encrypted_content } => {
            Some(FunctionCallOutputContentItem::EncryptedContent {
                encrypted_content: encrypted_content.clone(),
            })
        }
        FunctionCallOutputContentItem::InputText { .. } => None,
    }));

    out
}

pub fn approx_tokens_from_byte_count_i64(bytes: i64) -> i64 {
    if bytes <= 0 {
        return 0;
    }

    let bytes = usize::try_from(bytes).unwrap_or(usize::MAX);
    i64::try_from(approx_tokens_from_byte_count(bytes)).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod truncate_tests;
