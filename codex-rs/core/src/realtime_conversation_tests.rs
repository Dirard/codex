use super::RealtimeHandoffState;
use super::RealtimeSessionKind;
use super::ensure_realtime_transport_supported;
use super::realtime_api_key;
use super::realtime_auth_manager_for_provider;
use super::realtime_delegation_from_handoff;
use super::realtime_request_headers;
use super::realtime_text_from_handoff_request;
use super::wrap_realtime_delegation_input;
use async_channel::bounded;
use codex_config::config_toml::RealtimeWsVersion;
use codex_login::AuthManager;
use codex_login::CodexAuth;
use codex_model_provider_info::ModelProviderInfo;
use codex_model_provider_info::WireApi;
use codex_protocol::error::CodexErr;
use codex_protocol::protocol::ConversationStartTransport;
use codex_protocol::protocol::RealtimeHandoffRequested;
use codex_protocol::protocol::RealtimeTranscriptEntry;
use http::header::AUTHORIZATION;
use pretty_assertions::assert_eq;
use std::sync::Arc;

#[test]
fn prefers_handoff_input_transcript_over_active_transcript() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: "ignored".to_string(),
        active_transcript: vec![
            RealtimeTranscriptEntry {
                role: "user".to_string(),
                text: "hello".to_string(),
            },
            RealtimeTranscriptEntry {
                role: "assistant".to_string(),
                text: "hi there".to_string(),
            },
        ],
    };
    assert_eq!(
        realtime_text_from_handoff_request(&handoff),
        Some("ignored".to_string())
    );
}

#[test]
fn extracts_text_from_handoff_request_active_transcript_if_input_missing() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: String::new(),
        active_transcript: vec![RealtimeTranscriptEntry {
            role: "user".to_string(),
            text: "hello".to_string(),
        }],
    };
    assert_eq!(
        realtime_text_from_handoff_request(&handoff),
        Some("user: hello".to_string())
    );
}

#[test]
fn wraps_handoff_with_transcript_delta() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: "delegate this".to_string(),
        active_transcript: vec![
            RealtimeTranscriptEntry {
                role: "user".to_string(),
                text: "hello".to_string(),
            },
            RealtimeTranscriptEntry {
                role: "assistant".to_string(),
                text: "hi there".to_string(),
            },
        ],
    };
    assert_eq!(
        realtime_delegation_from_handoff(&handoff),
        Some(
            "<realtime_delegation>\n  <input>delegate this</input>\n  <transcript_delta>user: hello\nassistant: hi there</transcript_delta>\n</realtime_delegation>"
                .to_string()
        )
    );
}

#[test]
fn extracts_text_from_handoff_request_input_transcript_if_messages_missing() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: "ignored".to_string(),
        active_transcript: vec![],
    };
    assert_eq!(
        realtime_text_from_handoff_request(&handoff),
        Some("ignored".to_string())
    );
}

#[test]
fn ignores_empty_handoff_request_input_transcript() {
    let handoff = RealtimeHandoffRequested {
        handoff_id: "handoff_1".to_string(),
        item_id: "item_1".to_string(),
        input_transcript: String::new(),
        active_transcript: vec![],
    };
    assert_eq!(realtime_text_from_handoff_request(&handoff), None);
}

#[test]
fn wraps_realtime_delegation_input() {
    assert_eq!(
        wrap_realtime_delegation_input("hello", /*transcript_delta*/ None),
        "<realtime_delegation>\n  <input>hello</input>\n</realtime_delegation>"
    );
}

#[test]
fn wraps_realtime_delegation_input_with_xml_escaping() {
    assert_eq!(
        wrap_realtime_delegation_input("use a < b && c > d", Some("saw <that>")),
        "<realtime_delegation>\n  <input>use a &lt; b &amp;&amp; c &gt; d</input>\n  <transcript_delta>saw &lt;that&gt;</transcript_delta>\n</realtime_delegation>"
    );
}

#[test]
fn wraps_realtime_delegation_input_with_xml_escaping_without_transcript() {
    assert_eq!(
        wrap_realtime_delegation_input("use a < b && c > d", /*transcript_delta*/ None),
        "<realtime_delegation>\n  <input>use a &lt; b &amp;&amp; c &gt; d</input>\n</realtime_delegation>"
    );
}

#[tokio::test]
async fn clears_active_handoff_explicitly() {
    let (tx, _rx) = bounded(1);
    let state = RealtimeHandoffState::new(
        tx,
        /*client_managed_handoffs*/ false,
        /*codex_responses_as_items*/ false,
        /*codex_response_item_prefix*/ None,
        /*codex_response_handoff_prefix*/ None,
        RealtimeSessionKind::V1,
    );

    *state.active_handoff.lock().await = Some("handoff_1".to_string());
    assert_eq!(
        state.active_handoff.lock().await.clone(),
        Some("handoff_1".to_string())
    );

    *state.active_handoff.lock().await = None;
    assert_eq!(state.active_handoff.lock().await.clone(), None);
}

#[test]
fn uses_quicksilver_alpha_header_for_realtime_v1() {
    let headers =
        realtime_request_headers(Some("session_1"), Some("sk-test"), RealtimeWsVersion::V1)
            .expect("headers")
            .expect("headers");

    assert_eq!(
        headers
            .get("openai-alpha")
            .and_then(|value| value.to_str().ok()),
        Some("quicksilver=v1")
    );
}

#[test]
fn omits_quicksilver_alpha_header_for_realtime_v2() {
    let headers =
        realtime_request_headers(Some("session_1"), Some("sk-test"), RealtimeWsVersion::V2)
            .expect("headers")
            .expect("headers");

    assert!(headers.get("openai-alpha").is_none());
}

#[test]
fn rejects_websocket_provider_without_realtime_websocket_support() {
    let provider = ModelProviderInfo {
        name: "Chat provider".to_string(),
        wire_api: WireApi::Chat,
        supports_websockets: false,
        ..Default::default()
    };

    let err =
        ensure_realtime_transport_supported(&provider, &ConversationStartTransport::Websocket)
            .expect_err("provider should not support realtime without websocket capability");
    match err {
        CodexErr::InvalidRequest(message) => assert_eq!(
            message,
            "model provider 'Chat provider' does not support realtime websocket conversations"
        ),
        other => panic!("expected invalid request, got {other:?}"),
    }
}

#[test]
fn accepts_webrtc_provider_without_realtime_websocket_support() {
    let provider = ModelProviderInfo {
        name: "Chat provider".to_string(),
        wire_api: WireApi::Chat,
        supports_websockets: false,
        ..Default::default()
    };

    ensure_realtime_transport_supported(
        &provider,
        &ConversationStartTransport::Webrtc {
            sdp: "v=offer\r\n".to_string(),
        },
    )
    .expect("webrtc transport should not require websocket capability");
}

#[test]
fn accepts_chat_provider_with_realtime_websocket_support() {
    let provider = ModelProviderInfo {
        name: "Chat realtime provider".to_string(),
        wire_api: WireApi::Chat,
        supports_websockets: true,
        ..Default::default()
    };

    ensure_realtime_transport_supported(&provider, &ConversationStartTransport::Websocket)
        .expect("chat provider should support realtime when capability is enabled");
}

#[test]
fn accepts_responses_provider_with_realtime_websocket_support() {
    let provider = ModelProviderInfo {
        name: "Realtime provider".to_string(),
        wire_api: WireApi::Responses,
        supports_websockets: true,
        ..Default::default()
    };

    ensure_realtime_transport_supported(&provider, &ConversationStartTransport::Websocket)
        .expect("provider should support realtime");
}

#[test]
fn custom_realtime_provider_without_auth_does_not_use_global_auth_manager() {
    let fallback_auth_manager =
        AuthManager::from_auth_for_testing(CodexAuth::from_api_key("sk-global-openai"));
    let provider = ModelProviderInfo {
        name: "Custom realtime provider".to_string(),
        base_url: Some("http://localhost:1234/v1".to_string()),
        wire_api: WireApi::Chat,
        supports_websockets: true,
        ..Default::default()
    };

    assert!(
        realtime_auth_manager_for_provider(
            /*provider_auth_manager*/ None,
            &fallback_auth_manager,
            &provider
        )
        .is_none()
    );
}

#[test]
fn openai_realtime_provider_uses_global_auth_manager_when_provider_auth_is_missing() {
    let fallback_auth_manager =
        AuthManager::from_auth_for_testing(CodexAuth::from_api_key("sk-global-openai"));
    let provider = ModelProviderInfo::create_openai_provider(/*base_url*/ None);

    let selected = realtime_auth_manager_for_provider(
        /*provider_auth_manager*/ None,
        &fallback_auth_manager,
        &provider,
    )
    .expect("OpenAI realtime should use global auth when provider auth is absent");

    assert!(Arc::ptr_eq(&selected, &fallback_auth_manager));
}

#[test]
fn custom_realtime_provider_preserves_provider_scoped_auth_manager() {
    let provider_auth_manager =
        AuthManager::from_auth_for_testing(CodexAuth::from_api_key("sk-provider-local"));
    let fallback_auth_manager =
        AuthManager::from_auth_for_testing(CodexAuth::from_api_key("sk-global-openai"));
    let provider = ModelProviderInfo {
        name: "Custom realtime provider".to_string(),
        base_url: Some("http://localhost:1234/v1".to_string()),
        wire_api: WireApi::Chat,
        supports_websockets: true,
        ..Default::default()
    };

    let selected = realtime_auth_manager_for_provider(
        Some(Arc::clone(&provider_auth_manager)),
        &fallback_auth_manager,
        &provider,
    )
    .expect("provider-scoped auth should be preserved for custom realtime provider");

    assert!(Arc::ptr_eq(&selected, &provider_auth_manager));
}

#[test]
fn custom_openai_named_realtime_provider_without_auth_omits_authorization_header() {
    let provider = ModelProviderInfo {
        name: "OpenAI".to_string(),
        base_url: Some("http://localhost:1234/v1".to_string()),
        wire_api: WireApi::Chat,
        supports_websockets: true,
        ..Default::default()
    };

    let api_key = realtime_api_key(
        /*auth*/ None,
        &provider,
        || Some("sk-global-openai".to_string()),
    )
    .expect("custom provider without auth should not require an API key");
    assert_eq!(api_key, None);

    let headers =
        realtime_request_headers(Some("session_1"), api_key.as_deref(), RealtimeWsVersion::V2)
            .expect("headers")
            .expect("headers");
    assert!(headers.get(AUTHORIZATION).is_none());
}

#[test]
fn openai_realtime_provider_uses_env_key_when_auth_is_missing() {
    let provider = ModelProviderInfo::create_openai_provider(/*base_url*/ None);

    let api_key = realtime_api_key(
        /*auth*/ None,
        &provider,
        || Some("sk-global-openai".to_string()),
    )
    .expect("OpenAI provider should still use env fallback");

    assert_eq!(api_key.as_deref(), Some("sk-global-openai"));
}
