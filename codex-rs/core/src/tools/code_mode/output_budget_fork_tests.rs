//! Fork: the Code Mode cell output budget (specs/code-mode-cell-output-budget-2026-10-29.md).

use std::collections::BTreeMap;

use codex_protocol::models::FunctionCallOutputContentItem;
use codex_tools::ToolSpec;
use codex_utils_output_truncation::TruncationPolicy;
use pretty_assertions::assert_eq;

use super::super::truncate_code_mode_result;
use super::super::wait_spec::create_wait_tool;
use super::*;

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

fn bundled_exec_description() -> String {
    codex_code_mode::build_exec_tool_description(
        &[],
        &[],
        &BTreeMap::new(),
        /*default_exec_yield_time_ms*/ 10_000,
        /*code_mode_only*/ false,
        codex_code_mode::ImageDetailVisibility::Visible,
        codex_code_mode::DeferredToolDiscovery::Catalog,
        /*messages*/ None,
    )
}

/// The description and the `max_tokens` parameter description of a `wait` spec.
fn wait_texts(spec: &ToolSpec) -> (&str, &str) {
    let ToolSpec::Function(tool) = spec else {
        panic!("expected a function wait tool, got {spec:?}");
    };
    let max_tokens = tool
        .parameters
        .properties
        .as_ref()
        .and_then(|properties| properties.get(WAIT_MAX_TOKENS_PARAMETER))
        .and_then(|parameter| parameter.description.as_deref())
        .expect("wait declares max_tokens with a description");
    (&tool.description, max_tokens)
}

/// Upstream drift guard: every sentence this module rewrites is in the bundled descriptions.
/// If upstream rewords one, update the constants in `output_budget_fork.rs`.
#[test]
fn bundled_descriptions_contain_the_replaced_sentences() {
    for sentence in [
        EXEC_BUDGET_SENTENCE,
        WAIT_BUDGET_SENTENCE,
        WAIT_MAX_TOKENS_DESCRIPTION,
    ] {
        assert!(sentence.ends_with(BUNDLED_DEFAULT), "{sentence}");
    }
    assert!(
        bundled_exec_description().contains(EXEC_BUDGET_SENTENCE),
        "the bundled exec description lost: {EXEC_BUDGET_SENTENCE}"
    );
    assert!(
        codex_code_mode::build_wait_tool_description().contains(WAIT_BUDGET_SENTENCE),
        "the bundled wait description lost: {WAIT_BUDGET_SENTENCE}"
    );
    let spec = create_wait_tool(
        /*description_override*/ None, /*parameters_override*/ None,
    );
    let (description, max_tokens) = wait_texts(&spec);
    assert!(description.contains(WAIT_BUDGET_SENTENCE), "{description}");
    assert_eq!(max_tokens, WAIT_MAX_TOKENS_DESCRIPTION);
}

/// R1.4: the bundled 10 000-token budget keeps the bundled descriptions.
#[test]
fn bundled_budget_keeps_descriptions() {
    let budget = CodeModeOutputBudget::from_tokens(10_000);

    assert_eq!(
        budget.exec_description(bundled_exec_description()),
        bundled_exec_description()
    );
    assert_eq!(
        budget
            .wait_handler(
                /*description_override*/ None, /*parameters_override*/ None
            )
            .spec,
        create_wait_tool(
            /*description_override*/ None, /*parameters_override*/ None
        )
    );
}

/// R1.4: a raised budget is named by `exec`, the `wait` description and its `max_tokens`.
#[test]
fn raised_budget_is_named_in_descriptions() {
    let budget = CodeModeOutputBudget::from_tokens(400_000);

    let exec = budget.exec_description(bundled_exec_description());
    assert!(exec.contains(
        "`max_output_tokens` sets the token budget for direct `exec` results. Defaults to 400000 tokens."
    ));
    assert!(!exec.contains(EXEC_BUDGET_SENTENCE));
    assert_eq!(
        exec.replace("Defaults to 400000 tokens.", BUNDLED_DEFAULT),
        bundled_exec_description()
    );

    let handler = budget.wait_handler(
        /*description_override*/ None, /*parameters_override*/ None,
    );
    let (description, max_tokens) = wait_texts(&handler.spec);
    assert!(description.contains(
        "`max_tokens` limits how much new output this wait call returns. Defaults to 400000 tokens."
    ));
    assert!(!description.contains(BUNDLED_DEFAULT), "{description}");
    assert_eq!(
        max_tokens,
        "Output token budget for this wait call. Defaults to 400000 tokens."
    );
}

/// A catalog description without the bundled sentence stays as it is.
#[test]
fn catalog_wait_description_stays() {
    let handler = CodeModeOutputBudget::from_tokens(400_000)
        .wait_handler(Some("Catalog wait."), /*parameters_override*/ None);

    let (description, max_tokens) = wait_texts(&handler.spec);
    assert_eq!(description, "Catalog wait.");
    assert_eq!(
        max_tokens,
        "Output token budget for this wait call. Defaults to 400000 tokens."
    );
}

/// R1.1, R1.3: the step's policy cuts a cell only once the user configured output truncation;
/// otherwise the upstream 10 000 tokens, also for a fallback model that cuts tool output at
/// 10 000 bytes.
#[tokio::test]
async fn default_cell_policy_needs_configured_truncation() {
    use codex_models_manager::ModelsManagerConfig;
    use codex_models_manager::model_info::model_info_from_slug;
    use codex_models_manager::model_info::with_config_overrides;

    let bundled = codex_models_manager::bundled_models_response()
        .expect("bundled model catalog should load")
        .models
        .into_iter()
        .find(|model| model.slug == "gpt-5.5")
        .expect("bundled catalog should list gpt-5.5");
    let fallback = model_info_from_slug("unknown-fallback-model");
    for (model, tool_output_token_limit, max_bytes, expected) in [
        (&bundled, None, None, TruncationPolicy::Tokens(10_000)),
        (
            &bundled,
            Some(400_000),
            None,
            TruncationPolicy::Tokens(400_000),
        ),
        (&bundled, None, Some(8_000), TruncationPolicy::Bytes(8_000)),
        (&fallback, None, None, TruncationPolicy::Tokens(10_000)),
        // A byte-mode model gets the limit as bytes, about four per token.
        (
            &fallback,
            Some(400_000),
            None,
            TruncationPolicy::Bytes(1_600_000),
        ),
    ] {
        let (_session, mut turn) = crate::session::tests::make_session_and_context().await;
        let mut config = (*turn.config).clone();
        config.tool_output_token_limit = tool_output_token_limit;
        config.output_truncation.max_bytes = max_bytes;
        turn.config = std::sync::Arc::new(config);
        // The models manager applies `tool_output_token_limit` to the model policy.
        let model_info = with_config_overrides(
            model.clone(),
            &ModelsManagerConfig {
                tool_output_token_limit,
                ..ModelsManagerConfig::default()
            },
        );

        assert_eq!(
            default_cell_policy(&turn, &model_info),
            expected,
            "{} {tool_output_token_limit:?} {max_bytes:?}",
            model.slug
        );
    }
}
