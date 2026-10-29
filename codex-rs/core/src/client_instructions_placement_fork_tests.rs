//! Fork: `model_instructions_placement` (specs/model-instructions-placement-2026-10-29.md).

use super::super::responses_request_properties_mismatch;
use super::ModelClient;
use super::Prompt;
use super::TestCodexResponsesRequestKind;
use super::test_model_client;
use super::test_model_info;
use super::test_responses_metadata_for_client;
use codex_api::ResponsesApiRequest;
use codex_config::config_toml::ModelInstructionsPlacement;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::SessionSource;
use pretty_assertions::assert_eq;

const BASE: &str = "You are a test agent.";

fn prompt(base: &str) -> Prompt {
    Prompt {
        input: vec![ResponseItem::Message {
            encrypted_content: None,
            status: None,
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        }],
        base_instructions: BaseInstructions {
            text: base.to_string(),
            provenance: None,
        },
        ..Default::default()
    }
}

fn build(
    placement: Option<ModelInstructionsPlacement>,
    responses_lite: bool,
    base: &str,
) -> ResponsesApiRequest {
    let mut client: ModelClient = test_model_client(SessionSource::Cli);
    if let Some(placement) = placement {
        client = client.with_model_instructions_placement(placement);
    }
    let mut model = test_model_info();
    model.use_responses_lite = responses_lite;
    let metadata = test_responses_metadata_for_client(
        &client,
        /*turn_id*/ None,
        format!("{}:0", client.state.thread_id),
        /*parent_thread_id*/ None,
        TestCodexResponsesRequestKind::Turn,
    );
    client
        .build_responses_request(
            &prompt(base),
            &model,
            /*effort*/ None,
            codex_protocol::config_types::ReasoningSummary::None,
            /*service_tier*/ None,
            &metadata,
            /*include_internal*/ true,
        )
        .expect("request builds")
}

/// Developer messages of `input` whose whole text is the base text (the request copy drops the
/// harness metadata that `BaseInstructionsFragment::text_from_item` reads for this provider).
fn base_in_input(request: &ResponsesApiRequest) -> Vec<&str> {
    request
        .input
        .iter()
        .filter_map(|item| match item {
            ResponseItem::Message { role, content, .. } if role == "developer" => {
                match content.as_slice() {
                    [ContentItem::InputText { text }] if text == BASE => Some(text.as_str()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

#[test]
fn instructions_placement_moves_base_text_to_the_field() {
    let request = build(
        Some(ModelInstructionsPlacement::Instructions),
        /*responses_lite*/ false,
        BASE,
    );

    assert_eq!(request.instructions, BASE);
    assert_eq!(base_in_input(&request), Vec::<&str>::new());
    assert_eq!(request.input.len(), 1);
    let body = serde_json::to_value(&request).expect("serialize");
    assert_eq!(body["instructions"], BASE);
}

#[test]
fn default_and_input_placement_keep_the_upstream_request() {
    let default = build(/*placement*/ None, /*responses_lite*/ false, BASE);
    let input = build(
        Some(ModelInstructionsPlacement::Input),
        /*responses_lite*/ false,
        BASE,
    );

    for request in [&default, &input] {
        assert_eq!(request.instructions, "");
        assert_eq!(base_in_input(request), vec![BASE]);
        let body = serde_json::to_value(request).expect("serialize");
        assert!(body.get("instructions").is_none(), "{body}");
    }
}

#[test]
fn responses_lite_keeps_base_text_in_input_whatever_the_placement() {
    let request = build(
        Some(ModelInstructionsPlacement::Instructions),
        /*responses_lite*/ true,
        BASE,
    );

    assert_eq!(request.instructions, "");
    assert_eq!(base_in_input(&request), vec![BASE]);
}

#[test]
fn empty_base_text_sends_neither_field_nor_prefix() {
    for placement in [
        ModelInstructionsPlacement::Input,
        ModelInstructionsPlacement::Instructions,
    ] {
        let request = build(Some(placement), /*responses_lite*/ false, "");
        assert_eq!(request.instructions, "");
        assert_eq!(base_in_input(&request), Vec::<&str>::new());
        assert_eq!(request.input.len(), 1);
        let body = serde_json::to_value(&request).expect("serialize");
        assert!(body.get("instructions").is_none(), "{body}");
    }
}

#[test]
fn changed_instructions_prevent_websocket_continuation() {
    let previous = build(
        Some(ModelInstructionsPlacement::Instructions),
        /*responses_lite*/ false,
        BASE,
    );
    let mut current = previous.clone();
    assert_eq!(
        responses_request_properties_mismatch(&previous, &current),
        None
    );

    current.instructions.push_str(" Updated.");
    assert_eq!(
        responses_request_properties_mismatch(&previous, &current),
        Some("instructions_changed")
    );
}

#[test]
fn placement_parses_from_config_toml() {
    let parse = |text: &str| {
        toml::from_str::<codex_config::config_toml::ConfigToml>(text)
            .map(|config| config.model_instructions_placement)
    };

    assert_eq!(parse("").expect("empty config"), None);
    assert_eq!(
        parse(r#"model_instructions_placement = "instructions""#).expect("instructions"),
        Some(ModelInstructionsPlacement::Instructions)
    );
    assert_eq!(
        parse(r#"model_instructions_placement = "input""#).expect("input"),
        Some(ModelInstructionsPlacement::Input)
    );
    assert!(parse(r#"model_instructions_placement = "system""#).is_err());
}
