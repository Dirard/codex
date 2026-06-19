use crate::auth::SharedAuthProvider;
use crate::common::ResponseEvent;
use crate::common::ResponseStream;
use crate::common::ResponsesApiRequest;
use crate::common::TextControls;
use crate::endpoint::responses::ResponsesOptions;
use crate::endpoint::session::EndpointSession;
use crate::error::ApiError;
use crate::provider::Provider;
use crate::requests::headers::build_session_headers;
use crate::requests::headers::insert_header;
use crate::requests::headers::subagent_header;
use crate::telemetry::SseTelemetry;
use codex_client::ByteStream;
use codex_client::EncodedJsonBody;
use codex_client::HttpTransport;
use codex_client::RequestTelemetry;
use codex_client::StreamResponse;
use codex_protocol::models::AgentMessageInputContent;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::MessagePhase;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::formatted_truncate_text;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::HeaderMap;
use http::HeaderValue;
use http::Method;
use serde::Deserialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio::time::timeout;
use tracing::debug;
use tracing::instrument;
use tracing::trace;

const CHAT_COMPLETIONS_ENDPOINT: &str = "chat/completions";
const REQUEST_ID_HEADER: &str = "x-request-id";
const CHAT_TOOL_SEARCH_OUTPUT_MAX_BYTES: usize = 8 * 1024;
const LEGACY_FUNCTION_CALL_INDEX: usize = usize::MAX;
const LEGACY_FUNCTION_CALL_ID: &str = "chatcmpl_function_call";

pub type ChatCompletionsOptions = ResponsesOptions;

pub struct ChatCompletionsClient<T: HttpTransport> {
    session: EndpointSession<T>,
    sse_telemetry: Option<Arc<dyn SseTelemetry>>,
}

impl<T: HttpTransport> ChatCompletionsClient<T> {
    pub fn new(transport: T, provider: Provider, auth: SharedAuthProvider) -> Self {
        Self {
            session: EndpointSession::new(transport, provider, auth),
            sse_telemetry: None,
        }
    }

    pub fn with_telemetry(
        self,
        request: Option<Arc<dyn RequestTelemetry>>,
        sse: Option<Arc<dyn SseTelemetry>>,
    ) -> Self {
        Self {
            session: self.session.with_request_telemetry(request),
            sse_telemetry: sse,
        }
    }

    #[instrument(
        name = "chat_completions.stream_request",
        level = "info",
        skip_all,
        fields(
            transport = "chat_completions_http",
            http.method = "POST",
            api.path = CHAT_COMPLETIONS_ENDPOINT
        )
    )]
    pub async fn stream_request(
        &self,
        request: ResponsesApiRequest,
        options: ChatCompletionsOptions,
    ) -> Result<ResponseStream, ApiError> {
        let ResponsesOptions {
            session_id,
            thread_id,
            session_source,
            extra_headers,
            compression: _,
            turn_state: _,
        } = options;

        let (body, tool_catalog) = chat_completions_request_from_responses(&request);
        let body = serde_json::to_value(body)
            .map_err(|e| ApiError::Stream(format!("failed to encode chat request: {e}")))?;

        let mut headers = extra_headers;
        if let Some(ref thread_id) = thread_id {
            insert_header(&mut headers, "x-client-request-id", thread_id);
        }
        headers.extend(build_session_headers(session_id, thread_id));
        if let Some(subagent) = subagent_header(&session_source) {
            insert_header(&mut headers, "x-openai-subagent", &subagent);
        }

        self.stream(body, headers, tool_catalog).await
    }

    #[instrument(
        name = "chat_completions.stream",
        level = "info",
        skip_all,
        fields(
            transport = "chat_completions_http",
            http.method = "POST",
            api.path = CHAT_COMPLETIONS_ENDPOINT
        )
    )]
    async fn stream(
        &self,
        body: Value,
        extra_headers: HeaderMap,
        tool_catalog: ChatToolCatalog,
    ) -> Result<ResponseStream, ApiError> {
        let body = EncodedJsonBody::encode(&body)
            .map_err(|e| ApiError::Stream(format!("failed to encode chat request: {e}")))?;
        let stream_response = self
            .session
            .stream_encoded_json_with(
                Method::POST,
                CHAT_COMPLETIONS_ENDPOINT,
                extra_headers,
                Some(body),
                |req| {
                    req.headers.insert(
                        http::header::ACCEPT,
                        HeaderValue::from_static("text/event-stream"),
                    );
                },
            )
            .await?;

        Ok(spawn_chat_completions_stream(
            stream_response,
            self.session.provider().stream_idle_timeout,
            self.sse_telemetry.clone(),
            tool_catalog,
        ))
    }
}

#[derive(Debug, PartialEq)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<Value>,
    tools: Vec<Value>,
    tool_choice: Option<String>,
    parallel_tool_calls: bool,
    stream: bool,
    stream_options: Option<Value>,
    response_format: Option<Value>,
    service_tier: Option<String>,
}

impl serde::Serialize for ChatCompletionsRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut object = Map::new();
        object.insert("model".to_string(), Value::String(self.model.clone()));
        object.insert("messages".to_string(), Value::Array(self.messages.clone()));
        if !self.tools.is_empty() {
            object.insert("tools".to_string(), Value::Array(self.tools.clone()));
        }
        if let Some(tool_choice) = &self.tool_choice {
            object.insert(
                "tool_choice".to_string(),
                Value::String(tool_choice.clone()),
            );
        }
        if !self.tools.is_empty() {
            object.insert(
                "parallel_tool_calls".to_string(),
                Value::Bool(self.parallel_tool_calls),
            );
        }
        object.insert("stream".to_string(), Value::Bool(self.stream));
        if let Some(stream_options) = &self.stream_options {
            object.insert("stream_options".to_string(), stream_options.clone());
        }
        if let Some(response_format) = &self.response_format {
            object.insert("response_format".to_string(), response_format.clone());
        }
        if let Some(service_tier) = &self.service_tier {
            object.insert(
                "service_tier".to_string(),
                Value::String(service_tier.clone()),
            );
        }
        Value::Object(object).serialize(serializer)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ChatToolCatalog {
    tools: HashMap<String, ChatToolMapping>,
}

impl ChatToolCatalog {
    fn insert(&mut self, chat_name: String, mapping: ChatToolMapping) {
        self.tools.insert(chat_name, mapping);
    }

    fn allocate_chat_name(&self, namespace: Option<&str>, name: &str) -> String {
        let base = sanitize_chat_tool_name(namespace, name);
        if !self.tools.contains_key(&base) {
            return base;
        }

        for index in 2.. {
            let suffix = format!("_{index}");
            let prefix_len = CHAT_TOOL_NAME_MAX_LEN.saturating_sub(suffix.len());
            let mut prefix = base.clone();
            prefix.truncate(prefix_len);
            let candidate = format!("{prefix}{suffix}");
            if !self.tools.contains_key(&candidate) {
                return candidate;
            }
        }

        unreachable!("unbounded suffix loop must find an unused chat tool name");
    }

    fn chat_name_for_function(&self, namespace: Option<&str>, name: &str) -> String {
        self.tools
            .iter()
            .find_map(|(chat_name, mapping)| match mapping {
                ChatToolMapping::Function {
                    name: mapped_name,
                    namespace: mapped_namespace,
                } if mapped_name == name && mapped_namespace.as_deref() == namespace => {
                    Some(chat_name.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| sanitize_chat_tool_name(namespace, name))
    }

    fn chat_name_for_custom(&self, name: &str) -> String {
        self.tools
            .iter()
            .find_map(|(chat_name, mapping)| match mapping {
                ChatToolMapping::Custom { name: mapped_name } if mapped_name == name => {
                    Some(chat_name.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| sanitize_chat_tool_name(None, name))
    }

    fn chat_name_for_tool_search(&self) -> String {
        self.tools
            .iter()
            .find_map(|(chat_name, mapping)| {
                matches!(mapping, ChatToolMapping::ToolSearch).then(|| chat_name.clone())
            })
            .unwrap_or_else(|| sanitize_chat_tool_name(None, "tool_search"))
    }

    fn mapping(&self, chat_name: &str) -> ChatToolMapping {
        self.tools
            .get(chat_name)
            .cloned()
            .unwrap_or_else(|| ChatToolMapping::Function {
                name: chat_name.to_string(),
                namespace: None,
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ChatToolMapping {
    Function {
        name: String,
        namespace: Option<String>,
    },
    Custom {
        name: String,
    },
    ToolSearch,
}

fn chat_completions_request_from_responses(
    request: &ResponsesApiRequest,
) -> (ChatCompletionsRequest, ChatToolCatalog) {
    let (tools, tool_catalog) = convert_responses_tools_to_chat_tools(&request.tools);
    let mut messages = Vec::new();
    if !request.instructions.trim().is_empty() {
        messages.push(chat_message("system", request.instructions.clone()));
    }

    let mut pending_tool_calls = Vec::new();
    let mut active_tool_call_ids = HashSet::new();
    for item in &request.input {
        append_response_item_to_chat_messages(
            &mut messages,
            &mut pending_tool_calls,
            &mut active_tool_call_ids,
            item,
            &tool_catalog,
        );
    }
    flush_pending_tool_calls(&mut messages, &mut pending_tool_calls);

    let response_format = response_format_from_text_controls(request.text.as_ref());
    let tool_choice = (!tools.is_empty()).then(|| request.tool_choice.clone());

    (
        ChatCompletionsRequest {
            model: request.model.clone(),
            messages,
            tools,
            tool_choice,
            parallel_tool_calls: request.parallel_tool_calls,
            stream: true,
            stream_options: Some(json!({ "include_usage": true })),
            response_format,
            service_tier: request.service_tier.clone(),
        },
        tool_catalog,
    )
}

fn append_response_item_to_chat_messages(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    active_tool_call_ids: &mut HashSet<String>,
    item: &ResponseItem,
    tool_catalog: &ChatToolCatalog,
) {
    match item {
        ResponseItem::Message { role, content, .. } => {
            flush_pending_tool_calls(messages, pending_tool_calls);
            let role = chat_role(role);
            messages.push(chat_message(role, content_items_to_text(content)));
            active_tool_call_ids.clear();
        }
        ResponseItem::FunctionCall {
            name,
            namespace,
            arguments,
            call_id,
            ..
        } => {
            active_tool_call_ids.insert(call_id.clone());
            pending_tool_calls.push(chat_tool_call(
                call_id,
                &tool_catalog.chat_name_for_function(namespace.as_deref(), name),
                arguments.clone(),
            ));
        }
        ResponseItem::FunctionCallOutput {
            call_id, output, ..
        } => push_chat_tool_output_if_matched(
            messages,
            pending_tool_calls,
            active_tool_call_ids,
            call_id,
            output_payload_to_text(output),
            "function_call_output",
        ),
        ResponseItem::CustomToolCall {
            name,
            input,
            call_id,
            ..
        } => {
            active_tool_call_ids.insert(call_id.clone());
            pending_tool_calls.push(chat_tool_call(
                call_id,
                &tool_catalog.chat_name_for_custom(name),
                json!({ "input": input }).to_string(),
            ));
        }
        ResponseItem::CustomToolCallOutput {
            call_id, output, ..
        } => push_chat_tool_output_if_matched(
            messages,
            pending_tool_calls,
            active_tool_call_ids,
            call_id,
            output_payload_to_text(output),
            "custom_tool_call_output",
        ),
        ResponseItem::ToolSearchCall {
            call_id: Some(call_id),
            execution,
            arguments,
            ..
        } if execution == "client" => {
            let chat_name = tool_catalog.chat_name_for_tool_search();
            active_tool_call_ids.insert(call_id.clone());
            pending_tool_calls.push(chat_tool_call(call_id, &chat_name, arguments.to_string()));
        }
        ResponseItem::ToolSearchOutput {
            status,
            execution,
            tools,
            ..
        } if execution == "server" => {
            flush_pending_tool_calls(messages, pending_tool_calls);
            messages.push(chat_message(
                "assistant",
                format!(
                    "Server-side tool search output:\n{}",
                    capped_chat_tool_search_output(json!({
                        "status": status,
                        "execution": execution,
                        "tools": tools,
                    }))
                ),
            ));
            active_tool_call_ids.clear();
        }
        ResponseItem::ToolSearchOutput {
            call_id: Some(call_id),
            status,
            execution,
            tools,
            ..
        } if execution == "client" => {
            push_chat_tool_output_if_matched(
                messages,
                pending_tool_calls,
                active_tool_call_ids,
                call_id,
                capped_chat_tool_search_output(json!({
                    "status": status,
                    "execution": execution,
                    "tools": tools,
                })),
                "tool_search_output",
            );
        }
        ResponseItem::AgentMessage {
            author,
            recipient,
            content,
            ..
        } => {
            flush_pending_tool_calls(messages, pending_tool_calls);
            active_tool_call_ids.clear();
            if let Some(text) = agent_message_content_to_text(author, recipient, content) {
                messages.push(chat_message("assistant", text));
            }
        }
        ResponseItem::Reasoning { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::Compaction { .. }
        | ResponseItem::CompactionTrigger { .. }
        | ResponseItem::ContextCompaction { .. }
        | ResponseItem::Other => {}
    }
}

fn push_chat_tool_output_if_matched(
    messages: &mut Vec<Value>,
    pending_tool_calls: &mut Vec<Value>,
    active_tool_call_ids: &mut HashSet<String>,
    call_id: &str,
    content: String,
    output_kind: &'static str,
) {
    if active_tool_call_ids.remove(call_id) {
        flush_pending_tool_calls(messages, pending_tool_calls);
        messages.push(chat_tool_output(call_id, content));
    } else {
        tracing::warn!(
            call_id,
            output_kind,
            "skipping orphan chat completions tool output without matching call"
        );
    }
}

fn chat_role(role: &str) -> &str {
    match role {
        "developer" | "system" => "system",
        "assistant" => "assistant",
        _ => "user",
    }
}

fn chat_message(role: &str, content: String) -> Value {
    json!({
        "role": role,
        "content": content,
    })
}

fn flush_pending_tool_calls(messages: &mut Vec<Value>, pending_tool_calls: &mut Vec<Value>) {
    if pending_tool_calls.is_empty() {
        return;
    }

    messages.push(chat_assistant_tool_calls(std::mem::take(
        pending_tool_calls,
    )));
}

fn chat_assistant_tool_calls(tool_calls: Vec<Value>) -> Value {
    json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": tool_calls,
    })
}

fn chat_tool_call(call_id: &str, name: &str, arguments: String) -> Value {
    json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": arguments,
        },
    })
}

fn chat_tool_output(call_id: &str, content: String) -> Value {
    json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": content,
    })
}

fn content_items_to_text(content: &[ContentItem]) -> String {
    let mut parts = Vec::new();
    for item in content {
        match item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                parts.push(text.as_str());
            }
            ContentItem::InputImage { .. } => {
                parts.push("[image input omitted for Chat Completions provider]");
            }
        }
    }
    parts.join("\n")
}

fn agent_message_content_to_text(
    author: &str,
    recipient: &str,
    content: &[AgentMessageInputContent],
) -> Option<String> {
    let text = content
        .iter()
        .filter_map(|content| match content {
            AgentMessageInputContent::InputText { text } => Some(text.as_str()),
            AgentMessageInputContent::EncryptedContent { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    (!text.trim().is_empty())
        .then(|| format!("Agent message from {author} to {recipient}:\n{text}"))
}

fn output_payload_to_text(output: &FunctionCallOutputPayload) -> String {
    output
        .body
        .to_text()
        .unwrap_or_else(|| "[structured tool output omitted]".to_string())
}

fn capped_chat_tool_search_output(output: Value) -> String {
    formatted_truncate_text(
        &output.to_string(),
        TruncationPolicy::Bytes(CHAT_TOOL_SEARCH_OUTPUT_MAX_BYTES),
    )
}

fn convert_responses_tools_to_chat_tools(tools: &[Value]) -> (Vec<Value>, ChatToolCatalog) {
    let mut chat_tools = Vec::new();
    let mut catalog = ChatToolCatalog::default();

    for tool in tools {
        match tool.get("type").and_then(Value::as_str) {
            Some("function") => {
                if let Some(name) = tool.get("name").and_then(Value::as_str) {
                    let chat_name = catalog.allocate_chat_name(None, name);
                    chat_tools.push(chat_function_tool_from_responses_tool(&chat_name, tool));
                    catalog.insert(
                        chat_name,
                        ChatToolMapping::Function {
                            name: name.to_string(),
                            namespace: None,
                        },
                    );
                }
            }
            Some("custom") => {
                if let Some(name) = tool.get("name").and_then(Value::as_str) {
                    let chat_name = catalog.allocate_chat_name(None, name);
                    chat_tools.push(chat_function_tool_for_custom_tool(&chat_name, tool));
                    catalog.insert(
                        chat_name,
                        ChatToolMapping::Custom {
                            name: name.to_string(),
                        },
                    );
                }
            }
            Some("namespace") => {
                let Some(namespace) = tool.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let namespace_description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let Some(namespace_tools) = tool.get("tools").and_then(Value::as_array) else {
                    continue;
                };
                for namespace_tool in namespace_tools {
                    if namespace_tool.get("type").and_then(Value::as_str) != Some("function") {
                        continue;
                    }
                    let Some(name) = namespace_tool.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    let chat_name = catalog.allocate_chat_name(Some(namespace), name);
                    chat_tools.push(chat_function_tool_from_namespace_tool(
                        &chat_name,
                        namespace_description,
                        namespace_tool,
                    ));
                    catalog.insert(
                        chat_name,
                        ChatToolMapping::Function {
                            name: name.to_string(),
                            namespace: Some(namespace.to_string()),
                        },
                    );
                }
            }
            Some("tool_search") => {
                let chat_name = catalog.allocate_chat_name(None, "tool_search");
                chat_tools.push(chat_function_tool_from_responses_tool(&chat_name, tool));
                catalog.insert(chat_name, ChatToolMapping::ToolSearch);
            }
            Some("web_search" | "image_generation") | Some(_) | None => {}
        }
    }

    (chat_tools, catalog)
}

fn chat_function_tool_from_responses_tool(name: &str, tool: &Value) -> Value {
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parameters = tool
        .get("parameters")
        .cloned()
        .unwrap_or_else(empty_object_schema);
    let strict = tool.get("strict").and_then(Value::as_bool);
    chat_function_tool(name, description, parameters, strict)
}

fn chat_function_tool_from_namespace_tool(
    chat_name: &str,
    namespace_description: &str,
    tool: &Value,
) -> Value {
    let tool_description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let description = if namespace_description.is_empty() {
        tool_description.to_string()
    } else if tool_description.is_empty() {
        namespace_description.to_string()
    } else {
        format!("{namespace_description}\n\n{tool_description}")
    };
    let parameters = tool
        .get("parameters")
        .cloned()
        .unwrap_or_else(empty_object_schema);
    let strict = tool.get("strict").and_then(Value::as_bool);
    chat_function_tool(chat_name, &description, parameters, strict)
}

fn chat_function_tool_for_custom_tool(name: &str, tool: &Value) -> Value {
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    chat_function_tool(
        name,
        description,
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Raw input for the tool.",
                },
            },
            "required": ["input"],
            "additionalProperties": false,
        }),
        Some(false),
    )
}

fn chat_function_tool(
    name: &str,
    description: &str,
    parameters: Value,
    strict: Option<bool>,
) -> Value {
    let mut function = Map::new();
    function.insert("name".to_string(), Value::String(name.to_string()));
    function.insert(
        "description".to_string(),
        Value::String(description.to_string()),
    );
    function.insert("parameters".to_string(), parameters);
    if let Some(strict) = strict {
        function.insert("strict".to_string(), Value::Bool(strict));
    }

    json!({
        "type": "function",
        "function": Value::Object(function),
    })
}

fn empty_object_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
    })
}

const CHAT_TOOL_NAME_MAX_LEN: usize = 64;

fn sanitize_chat_tool_name(namespace: Option<&str>, name: &str) -> String {
    let mut sanitized = String::new();
    let raw = namespace
        .map(|namespace| format!("{namespace}{name}"))
        .unwrap_or_else(|| name.to_string());

    for character in raw.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            sanitized.push(character);
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        sanitized.push_str("tool");
    }
    if sanitized.len() > CHAT_TOOL_NAME_MAX_LEN {
        sanitized.truncate(CHAT_TOOL_NAME_MAX_LEN);
    }
    sanitized
}

fn response_format_from_text_controls(text: Option<&TextControls>) -> Option<Value> {
    let format = text.and_then(|text| text.format.as_ref())?;
    Some(json!({
        "type": "json_schema",
        "json_schema": {
            "name": format.name.clone(),
            "strict": format.strict,
            "schema": format.schema.clone(),
        },
    }))
}

fn spawn_chat_completions_stream(
    stream_response: StreamResponse,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    tool_catalog: ChatToolCatalog,
) -> ResponseStream {
    let upstream_request_id = stream_response
        .headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (tx_event, rx_event) = mpsc::channel::<Result<ResponseEvent, ApiError>>(1600);
    tokio::spawn(process_chat_completions_sse(
        stream_response.bytes,
        tx_event,
        idle_timeout,
        telemetry,
        tool_catalog,
    ));

    ResponseStream {
        rx_event,
        upstream_request_id,
    }
}

#[derive(Debug, Default)]
struct ChatStreamState {
    response_id: Option<String>,
    created_emitted: bool,
    text_item_started: bool,
    text: String,
    tool_calls: BTreeMap<usize, AccumulatedToolCall>,
    finish_reason: Option<String>,
    token_usage: Option<TokenUsage>,
    last_server_model: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct AccumulatedToolCall {
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsChunk {
    id: Option<String>,
    model: Option<String>,
    #[serde(default)]
    choices: Vec<ChatChoice>,
    #[serde(default)]
    usage: Option<ChatUsage>,
    #[serde(default)]
    error: Option<ChatError>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    #[serde(default)]
    delta: ChatDelta,
    finish_reason: Option<String>,
}

#[derive(Default, Debug, Deserialize)]
struct ChatDelta {
    content: Option<String>,
    function_call: Option<ChatDeltaFunction>,
    #[serde(default)]
    tool_calls: Vec<ChatDeltaToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatDeltaToolCall {
    index: usize,
    id: Option<String>,
    function: Option<ChatDeltaFunction>,
}

#[derive(Debug, Deserialize)]
struct ChatDeltaFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
}

impl From<ChatUsage> for TokenUsage {
    fn from(usage: ChatUsage) -> Self {
        let input_tokens = usage.prompt_tokens.unwrap_or_default();
        let output_tokens = usage.completion_tokens.unwrap_or_default();
        TokenUsage {
            input_tokens,
            cached_input_tokens: 0,
            output_tokens,
            reasoning_output_tokens: 0,
            total_tokens: usage
                .total_tokens
                .unwrap_or_else(|| input_tokens + output_tokens),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChatError {
    message: Option<String>,
}

async fn process_chat_completions_sse(
    stream: ByteStream,
    tx_event: mpsc::Sender<Result<ResponseEvent, ApiError>>,
    idle_timeout: Duration,
    telemetry: Option<Arc<dyn SseTelemetry>>,
    tool_catalog: ChatToolCatalog,
) {
    let mut stream = stream.eventsource();
    let mut state = ChatStreamState::default();

    loop {
        let start = Instant::now();
        let response = timeout(idle_timeout, stream.next()).await;
        if let Some(t) = telemetry.as_ref() {
            t.on_sse_poll(&response, start.elapsed());
        }

        let sse = match response {
            Ok(Some(Ok(sse))) => sse,
            Ok(Some(Err(e))) => {
                debug!("SSE Error: {e:#}");
                let _ = tx_event.send(Err(ApiError::Stream(e.to_string()))).await;
                return;
            }
            Ok(None) => {
                if state.finish_reason.is_some() {
                    complete_chat_stream(&tx_event, &mut state, &tool_catalog).await;
                    return;
                }
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        "stream closed before chat completion finished".to_string(),
                    )))
                    .await;
                return;
            }
            Err(_) => {
                if state.finish_reason.is_some() {
                    complete_chat_stream(&tx_event, &mut state, &tool_catalog).await;
                    return;
                }
                let _ = tx_event
                    .send(Err(ApiError::Stream(
                        "idle timeout waiting for SSE".to_string(),
                    )))
                    .await;
                return;
            }
        };

        trace!("Chat Completions SSE event: {}", &sse.data);
        if sse.data.trim() == "[DONE]" {
            complete_chat_stream(&tx_event, &mut state, &tool_catalog).await;
            return;
        }

        let chunk: ChatCompletionsChunk = match serde_json::from_str(&sse.data) {
            Ok(chunk) => chunk,
            Err(e) => {
                debug!(
                    "Failed to parse Chat Completions SSE event: {e}, data: {}",
                    &sse.data
                );
                continue;
            }
        };

        if let Some(error) = chunk.error {
            let message = error
                .message
                .unwrap_or_else(|| "chat completion failed".to_string());
            let _ = tx_event.send(Err(ApiError::Stream(message))).await;
            return;
        }

        if !state.created_emitted {
            state.response_id = chunk.id.clone();
            state.created_emitted = true;
            if tx_event.send(Ok(ResponseEvent::Created)).await.is_err() {
                return;
            }
        } else if state.response_id.is_none() {
            state.response_id = chunk.id.clone();
        }
        if let Some(model) = chunk.model
            && state.last_server_model.as_deref() != Some(model.as_str())
        {
            if tx_event
                .send(Ok(ResponseEvent::ServerModel(model.clone())))
                .await
                .is_err()
            {
                return;
            }
            state.last_server_model = Some(model);
        }
        if let Some(usage) = chunk.usage {
            state.token_usage = Some(usage.into());
        }

        for choice in chunk.choices {
            if let Some(content) = choice.delta.content {
                if content.is_empty() {
                    continue;
                }
                if !state.text_item_started {
                    state.text_item_started = true;
                    if tx_event
                        .send(Ok(ResponseEvent::OutputItemAdded(assistant_message(
                            String::new(),
                        ))))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                state.text.push_str(&content);
                if tx_event
                    .send(Ok(ResponseEvent::OutputTextDelta(content)))
                    .await
                    .is_err()
                {
                    return;
                }
            }

            if let Some(function) = choice.delta.function_call {
                let accumulated = state
                    .tool_calls
                    .entry(LEGACY_FUNCTION_CALL_INDEX)
                    .or_default();
                accumulated.call_id = Some(LEGACY_FUNCTION_CALL_ID.to_string());
                accumulate_chat_delta_function(accumulated, function);
            }

            for tool_call in choice.delta.tool_calls {
                let accumulated = state.tool_calls.entry(tool_call.index).or_default();
                if let Some(id) = tool_call.id {
                    accumulated.call_id = Some(id);
                }
                if let Some(function) = tool_call.function {
                    accumulate_chat_delta_function(accumulated, function);
                }
            }

            if let Some(reason) = choice.finish_reason {
                state.finish_reason = Some(reason);
            }
        }
    }
}

async fn complete_chat_stream(
    tx_event: &mpsc::Sender<Result<ResponseEvent, ApiError>>,
    state: &mut ChatStreamState,
    tool_catalog: &ChatToolCatalog,
) {
    let has_pending_tool_calls = !state.tool_calls.is_empty()
        || matches!(
            state.finish_reason.as_deref(),
            Some("tool_calls" | "function_call")
        );

    if !state.text.is_empty()
        && tx_event
            .send(Ok(ResponseEvent::OutputItemDone(assistant_message(
                std::mem::take(&mut state.text),
            ))))
            .await
            .is_err()
    {
        return;
    }

    for (index, tool_call) in std::mem::take(&mut state.tool_calls) {
        let item = accumulated_tool_call_to_response_item(index, tool_call, tool_catalog);
        if tx_event
            .send(Ok(ResponseEvent::OutputItemDone(item)))
            .await
            .is_err()
        {
            return;
        }
    }

    let end_turn = Some(!has_pending_tool_calls);
    let response_id = state
        .response_id
        .clone()
        .unwrap_or_else(|| "chatcmpl_unknown".to_string());
    let _ = tx_event
        .send(Ok(ResponseEvent::Completed {
            response_id,
            token_usage: state.token_usage.take(),
            end_turn,
        }))
        .await;
}

fn accumulate_chat_delta_function(
    accumulated: &mut AccumulatedToolCall,
    function: ChatDeltaFunction,
) {
    if let Some(name) = function.name {
        accumulated.name = Some(name);
    }
    if let Some(arguments) = function.arguments {
        accumulated.arguments.push_str(&arguments);
    }
}

fn assistant_message(text: String) -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText { text }],
        phase: Some(MessagePhase::FinalAnswer),
        metadata: None,
    }
}

fn accumulated_tool_call_to_response_item(
    index: usize,
    tool_call: AccumulatedToolCall,
    tool_catalog: &ChatToolCatalog,
) -> ResponseItem {
    let call_id = tool_call
        .call_id
        .unwrap_or_else(|| format!("chatcmpl_call_{index}"));
    let chat_name = tool_call.name.unwrap_or_else(|| "unknown_tool".to_string());
    let arguments = if tool_call.arguments.trim().is_empty() {
        "{}".to_string()
    } else {
        tool_call.arguments
    };

    match tool_catalog.mapping(&chat_name) {
        ChatToolMapping::Function { name, namespace } => ResponseItem::FunctionCall {
            id: None,
            name,
            namespace,
            arguments,
            call_id,
            metadata: None,
        },
        ChatToolMapping::Custom { name } => ResponseItem::CustomToolCall {
            id: None,
            status: None,
            call_id,
            name,
            input: custom_tool_input_from_arguments(&arguments),
            metadata: None,
        },
        ChatToolMapping::ToolSearch => ResponseItem::ToolSearchCall {
            id: None,
            call_id: Some(call_id),
            status: Some("completed".to_string()),
            execution: "client".to_string(),
            arguments: serde_json::from_str(&arguments).unwrap_or_else(|_| json!({})),
            metadata: None,
        },
    }
}

fn custom_tool_input_from_arguments(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| arguments.to_string())
}

#[cfg(test)]
#[path = "chat_completions_tests.rs"]
mod tests;
