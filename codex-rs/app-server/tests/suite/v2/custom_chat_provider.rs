use anyhow::Context;
use anyhow::Result;
use app_test_support::TestAppServer;
use app_test_support::to_response;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput as V2UserInput;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request as WiremockRequest;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(60);
const CHAT_PROVIDER_HEADER_TOKEN: &str = "chat-provider-token";

#[derive(Clone, Debug)]
struct ChatToolRoundTripResponder {
    requests: Arc<Mutex<Vec<WiremockRequest>>>,
}

impl ChatToolRoundTripResponder {
    fn new(requests: Arc<Mutex<Vec<WiremockRequest>>>) -> Self {
        Self { requests }
    }
}

impl Respond for ChatToolRoundTripResponder {
    fn respond(&self, request: &WiremockRequest) -> ResponseTemplate {
        if let Err(err) = serde_json::from_slice::<Value>(&request.body) {
            return ResponseTemplate::new(400)
                .set_body_string(format!("chat completions request should be json: {err}"));
        }

        let mut requests = match self.requests.lock() {
            Ok(requests) => requests,
            Err(err) => {
                return ResponseTemplate::new(500)
                    .set_body_string(format!("chat request log should not be poisoned: {err}"));
            }
        };
        let call_index = requests.len();
        requests.push(request.clone());
        drop(requests);

        match call_index {
            0 => ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(
                    "data: {\"id\":\"chatcmpl-tool-1\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-shell\",\"function\":{\"name\":\"shell_command\",\"arguments\":\"{\\\"command\\\":\\\"echo chat-tool-ok\\\",\\\"login\\\":false}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
                    "text/event-stream",
                ),
            _ => ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(
                    concat!(
                        "data: {\"id\":\"chatcmpl-tool-2\",\"model\":\"glm-5.1\",\"choices\":[{\"delta\":{\"content\":\"final\"},\"finish_reason\":null}]}\n\n",
                        "data: {\"id\":\"chatcmpl-tool-2\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                    ),
                    "text/event-stream",
                ),
        }
    }
}

#[tokio::test]
async fn custom_chat_provider_round_trips_tool_calls_without_responses_api() -> Result<()> {
    let provider_server = MockServer::start().await;
    let chat_requests = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ChatToolRoundTripResponder::new(Arc::clone(&chat_requests)))
        .expect(2)
        .mount(&provider_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&provider_server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&provider_server)
        .await;

    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &provider_server.uri())?;

    let mut mcp = TestAppServer::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;

    let thread_request_id = mcp
        .send_thread_start_request(ThreadStartParams::default())
        .await?;
    let thread_response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_request_id)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response(thread_response)?;

    let turn_request_id = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread.id,
            client_user_message_id: None,
            input: vec![V2UserInput::Text {
                text: "run a tiny shell command".to_string(),
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;
    let turn_response: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_request_id)),
    )
    .await??;
    let _: TurnStartResponse = to_response(turn_response)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    let chat_requests = chat_requests
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(chat_requests.len(), 2);
    for request in chat_requests.iter() {
        assert_eq!(request_header(request, "authorization"), None);
        assert_eq!(
            request_header(request, "x-provider-token"),
            Some(CHAT_PROVIDER_HEADER_TOKEN)
        );
        assert_eq!(request_header(request, "chatgpt-account-id"), None);
        assert_eq!(request_header(request, "x-openai-fedramp"), None);
        assert_eq!(request_header(request, "x-oai-attestation"), None);
    }

    let first_body: Value =
        serde_json::from_slice(&chat_requests[0].body).context("first chat request body")?;
    assert_eq!(first_body["model"], "glm-5.1");
    let tool_names: Vec<_> = first_body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["function"]["name"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        tool_names.contains(&"shell_command"),
        "chat tools should include shell_command, got {tool_names:?}"
    );

    let second_body =
        String::from_utf8(chat_requests[1].body.clone()).context("second chat request body")?;
    assert!(second_body.contains("\"tool_call_id\":\"call-shell\""));
    assert!(second_body.contains("chat-tool-ok"));

    Ok(())
}

fn create_config_toml(codex_home: &Path, provider_server_uri: &str) -> std::io::Result<()> {
    std::fs::write(
        codex_home.join("config.toml"),
        format!(
            r#"
model = "glm-5.1"
approval_policy = "never"
sandbox_mode = "read-only"
model_provider = "glm"
model_auto_compact_token_limit = 1000000

[model_providers.glm]
name = "GLM"
base_url = "{provider_server_uri}/v1"
wire_api = "chat"
http_headers = {{ "x-provider-token" = "{CHAT_PROVIDER_HEADER_TOKEN}" }}
request_max_retries = 0
stream_max_retries = 0
models = ["glm-5.1"]
"#
        ),
    )
}

fn request_header<'a>(request: &'a WiremockRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
}
