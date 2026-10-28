//! Fork: the Code Mode cell output budget (specs/code-mode-cell-output-budget-2026-10-29.md).

use codex_protocol::models::FunctionCallOutputContentItem;
use codex_utils_output_truncation::TruncationPolicy;

use super::super::truncate_code_mode_result;

/// About 20 000 tokens of text: more than the bundled 10 000-token budget.
fn large_text() -> String {
    "x".repeat(80_000)
}

fn text_items(text: String) -> Vec<FunctionCallOutputContentItem> {
    vec![FunctionCallOutputContentItem::InputText { text }]
}

fn only_text(items: &[FunctionCallOutputContentItem]) -> &str {
    match items {
        [FunctionCallOutputContentItem::InputText { text }] => text,
        other => panic!("expected one text item, got {other:?}"),
    }
}

/// Asserts that the cell kept its items; a failure shows only the head of each text item.
fn assert_untouched(
    truncated: Vec<FunctionCallOutputContentItem>,
    items: &[FunctionCallOutputContentItem],
) {
    let heads = truncated
        .iter()
        .map(|item| match item {
            FunctionCallOutputContentItem::InputText { text } => format!("{text:.200}"),
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>();
    assert!(truncated == items, "the cell was truncated: {heads:#?}");
}

/// R1.1: without the pragma a raised configured policy keeps a 20 000-token cell whole.
#[test]
fn cell_without_pragma_keeps_output_within_configured_policy() {
    let items = text_items(large_text());

    assert_untouched(
        truncate_code_mode_result(
            items.clone(),
            /*max_output_tokens*/ None,
            TruncationPolicy::Tokens(400_000),
        ),
        &items,
    );
}

/// R1.3: the bundled 10 000-token policy cuts the same cell as before.
#[test]
fn cell_without_pragma_uses_bundled_token_policy() {
    let truncated = truncate_code_mode_result(
        text_items(large_text()),
        /*max_output_tokens*/ None,
        TruncationPolicy::Tokens(10_000),
    );

    let text = only_text(&truncated);
    assert!(
        text.starts_with(
            "Warning: truncated output (original token count: 20000)\nTotal output lines: 1\n\n"
        ),
        "unexpected header: {}",
        &text[..text.len().min(120)]
    );
    assert!(text.contains("…10000 tokens truncated…"), "{text:.200}");
}

/// R1.2: the pragma budget wins over any configured policy.
#[test]
fn pragma_budget_wins_over_configured_policy() {
    for policy in [
        TruncationPolicy::Tokens(400_000),
        TruncationPolicy::Tokens(10),
        TruncationPolicy::Bytes(1_000_000),
    ] {
        let truncated = truncate_code_mode_result(
            text_items(large_text()),
            /*max_output_tokens*/ Some(1_000),
            policy,
        );

        let text = only_text(&truncated);
        assert!(
            text.contains("…19000 tokens truncated…"),
            "policy {policy:?}: {text:.200}"
        );
    }
}

/// R1.1: a byte policy (`[output_truncation] max_bytes`) cuts the cell by bytes.
#[test]
fn cell_without_pragma_follows_byte_policy() {
    let truncated = truncate_code_mode_result(
        text_items(large_text()),
        /*max_output_tokens*/ None,
        TruncationPolicy::Bytes(1_000),
    );

    let text = only_text(&truncated);
    assert!(text.contains("…79000 chars truncated…"), "{text:.200}");
}

/// R1.1, R1.5: a cell with media keeps its items and follows the configured policy too.
#[test]
fn cell_with_media_follows_configured_policy() {
    let items = vec![
        FunctionCallOutputContentItem::InputText { text: large_text() },
        FunctionCallOutputContentItem::InputAudio {
            audio_url: "data:audio/wav;base64,AAAA".to_string(),
        },
    ];

    assert_untouched(
        truncate_code_mode_result(
            items.clone(),
            /*max_output_tokens*/ None,
            TruncationPolicy::Tokens(400_000),
        ),
        &items,
    );
}
