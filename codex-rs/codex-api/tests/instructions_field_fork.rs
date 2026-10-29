//! Fork: the top-level `instructions` field of Responses requests
//! (specs/model-instructions-placement-2026-10-29.md).

use codex_api::ResponseCreateWsRequest;
use codex_api::ResponsesApiRequest;
use pretty_assertions::assert_eq;

fn request(instructions: &str) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "gpt-test".into(),
        instructions: instructions.into(),
        input: Vec::new(),
        tools: None,
        tool_choice: "auto".into(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream: true,
        stream_options: None,
        include: Vec::new(),
        service_tier: Some("priority".into()),
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
        access_programs: None,
    }
}

#[test]
fn instructions_follow_routing_fields_and_precede_input() {
    let http = serde_json::to_string(&request("Be brief.")).expect("serialize");
    assert!(
        http.starts_with(
            r#"{"model":"gpt-test","stream":true,"service_tier":"priority","instructions":"Be brief.","input":[]"#
        ),
        "{http}"
    );

    let api_request = request("Be brief.");
    let ws = serde_json::to_value(ResponseCreateWsRequest::from(&api_request)).expect("ws");
    assert_eq!(ws["instructions"], "Be brief.");
}

#[test]
fn empty_instructions_are_not_serialized() {
    let api_request = request("");
    let http = serde_json::to_value(&api_request).expect("serialize");
    assert!(http.get("instructions").is_none(), "{http}");
    let ws = serde_json::to_value(ResponseCreateWsRequest::from(&api_request)).expect("ws");
    assert!(ws.get("instructions").is_none(), "{ws}");
}
