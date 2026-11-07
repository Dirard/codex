//! Checks code-mode timing headers, including absent and zero host measurements.

use super::*;
use codex_protocol::models::FunctionCallOutputBody;
use codex_protocol::models::ImageReference;
use pretty_assertions::assert_eq;

#[test]
fn completed_timing_preserves_content_and_distinguishes_zero_from_unavailable() {
    let content = vec![
        FunctionCallOutputContentItem::InputText {
            text: "result".to_string(),
        },
        FunctionCallOutputContentItem::InputImage {
            image: ImageReference::Inline {
                image_url: "data:image/png;base64,image".to_string(),
            },
            detail: None,
        },
        FunctionCallOutputContentItem::InputAudio {
            audio_url: "data:audio/wav;base64,audio".to_string(),
        },
    ];
    for (host_duration, expected_timing) in [
        (
            Some(Duration::from_millis(/*millis*/ 750)),
            "Wall time 1.250 seconds (code-mode 0.750 seconds; overhead 0.500 seconds)",
        ),
        (
            Some(Duration::ZERO),
            "Wall time 1.250 seconds (code-mode 0.000 seconds; overhead 1.250 seconds)",
        ),
        (
            Some(Duration::from_micros(/*micros*/ 1_251_400)),
            "Wall time 1.250 seconds (code-mode 1.251 seconds; overhead -0.001 seconds)",
        ),
        (None, "Wall time 0.8 seconds"),
    ] {
        let mut output = CodeModeToolOutput::new(
            FunctionToolOutput::from_content(content.clone(), Some(false)),
            "Script failed".to_string(),
            Duration::from_millis(/*millis*/ 750),
            host_duration,
            OutputTruncation::from(codex_utils_output_truncation::TruncationPolicy::Bytes(4096)),
        );
        output.set_handler_duration_ms(/*handler_duration_ms*/ 1_250);
        let mut expected_content = vec![FunctionCallOutputContentItem::InputText {
            text: format!("Script failed\n{expected_timing}\nOutput:\n"),
        }];
        expected_content.extend(content.clone());
        let expected = FunctionToolOutput::from_content(expected_content, Some(false));
        for payload in [
            ToolPayload::Custom {
                input: "text('result')".to_string(),
            },
            ToolPayload::Function {
                arguments: r#"{"cell_id":"1"}"#.to_string(),
            },
        ] {
            let response = output.to_response_item("call", &payload);
            assert_eq!(response, expected.to_response_item("call", &payload));
            // Serialization must reuse the completed value, including on retries.
            assert_eq!(output.to_response_item("call", &payload), response);
        }
        assert_eq!(output.success_for_logging(), false);
    }
}

/// A runtime result whose content is `text` and, when it failed, `error_text`.
fn cell_result(text: &str, error_text: Option<&str>) -> codex_code_mode::RuntimeResponse {
    codex_code_mode::RuntimeResponse::Result {
        cell_id: codex_code_mode::CellId::new("cell-1".to_string()),
        content_items: vec![codex_code_mode::FunctionCallOutputContentItem::InputText {
            text: text.to_string(),
        }],
        error_text: error_text.map(str::to_string),
        result_buffered_for: Duration::ZERO,
        code_mode_host_duration: None,
    }
}

/// A test step whose turn has `tool_output_spill` on or off.
async fn spill_step(enabled: bool) -> std::sync::Arc<crate::session::step_context::StepContext> {
    let (_session, mut turn) = crate::session::tests::make_session_and_context().await;
    let mut config = (*turn.config).clone();
    if enabled {
        config
            .features
            .enable(codex_features::Feature::ToolOutputSpill)
            .expect("enable tool_output_spill");
    }
    turn.config = std::sync::Arc::new(config);
    crate::session::step_context::StepContext::for_test(std::sync::Arc::new(turn))
}

/// Tool-output-spill R2.5: with Codex-side truncation the cell's full text, including the
/// `Script error`, is kept from before its budget; without the feature it is not copied, and
/// host-side truncation leaves the cell ineligible.
#[tokio::test]
async fn cell_spill_text_is_the_text_before_the_budget() {
    let long = (1..=5000)
        .map(|line| format!("row {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let step = spill_step(/*enabled*/ true).await;
    let output = super::super::handle_runtime_response(
        &step,
        cell_result(&long, Some("boom")),
        /*max_output_tokens*/ Some(100),
        codex_code_mode::CodeModeOutputTruncation::Client,
        Duration::from_millis(10),
        /*experimental_show_cell_overhead*/ false,
    );
    let expected = format!("{long}\nScript error:\nboom");
    assert_eq!(output.spill_text().as_deref(), Some(expected.as_str()));
    let visible = output.log_output();
    assert!(!visible.contains("row 2500"), "the budget cut the middle");

    let off = spill_step(/*enabled*/ false).await;
    let output = super::super::handle_runtime_response(
        &off,
        cell_result(&long, None),
        /*max_output_tokens*/ Some(100),
        codex_code_mode::CodeModeOutputTruncation::Client,
        Duration::from_millis(10),
        /*experimental_show_cell_overhead*/ false,
    );
    assert_eq!(output.spill_text(), None);

    let output = super::super::handle_runtime_response(
        &step,
        cell_result(&long, None),
        /*max_output_tokens*/ Some(100),
        codex_code_mode::CodeModeOutputTruncation::Server,
        Duration::from_millis(10),
        /*experimental_show_cell_overhead*/ false,
    );
    assert_eq!(output.spill_text(), None);
}

/// Tool-output-spill R4.4, R4.1: the preview of a spilled cell is built from the original when
/// the item is built, so the handler duration set after the spill reaches its header, and the
/// text before the notice is the cell's own output.
#[test]
fn a_later_handler_duration_reaches_the_preview() {
    let long = (1..=3000)
        .map(|line| format!("row {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let truncation = OutputTruncation::new(
        codex_utils_output_truncation::TruncationPolicy::Tokens(10_000),
        Some(50),
    );
    let original = CodeModeToolOutput::new(
        FunctionToolOutput::from_content(
            vec![FunctionCallOutputContentItem::InputText { text: long }],
            Some(true),
        ),
        "Script completed".to_string(),
        Duration::from_millis(750),
        Some(Duration::from_millis(700)),
        truncation,
    );
    let payload = ToolPayload::Custom {
        input: "text(rows)".to_string(),
    };
    let mut spilled = crate::tools::output_spill::SpilledToolOutput::for_test(
        Box::new(original),
        truncation,
        payload.clone(),
    );
    spilled.set_handler_duration_ms(/*handler_duration_ms*/ 1_250);
    let ResponseInputItem::CustomToolCallOutput { output, .. } =
        spilled.to_response_item("call", &payload)
    else {
        panic!("expected a custom tool output");
    };
    let FunctionCallOutputBody::ContentItems(items) = &output.body else {
        panic!("expected content items");
    };
    let text = crate::tools::output_spill::item_text(items);
    assert!(
        text.starts_with(
            "Script completed\nWall time 1.250 seconds (code-mode 0.700 seconds; overhead 0.550 seconds)\nOutput:\n"
        ),
        "{text}"
    );
    assert!(text.contains("lines truncated"), "{text}");
    assert!(text.contains("Full output saved to: "), "{text}");
}
