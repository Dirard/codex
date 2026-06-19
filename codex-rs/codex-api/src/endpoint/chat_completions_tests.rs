use super::*;
use codex_client::TransportError;
use codex_protocol::models::AgentMessageInputContent;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsage;
use futures::TryStreamExt;
use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_test::io::Builder as IoBuilder;
use tokio_util::io::ReaderStream;

fn base_request(tools: Vec<Value>) -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "glm-5.1".to_string(),
        instructions: "system prompt".to_string(),
        input: vec![ResponseItem::Message {
            id: None,
            metadata: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            phase: None,
        }],
        tools,
        tool_choice: "auto".to_string(),
        parallel_tool_calls: true,
        reasoning: None,
        store: false,
        stream: true,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
    }
}

#[test]
fn converts_responses_request_to_chat_completions_request() {
    let request = base_request(vec![
        json!({
            "type": "function",
            "name": "spawn_agent",
            "description": "Spawn an agent",
            "strict": false,
            "parameters": {
                "type": "object",
                "properties": {
                    "task": { "type": "string" }
                },
                "required": ["task"]
            }
        }),
        json!({
            "type": "custom",
            "name": "exec",
            "description": "Run shell input"
        }),
        json!({
            "type": "namespace",
            "name": "mcp__demo__",
            "description": "Demo namespace",
            "tools": [{
                "type": "function",
                "name": "lookup",
                "description": "Lookup data",
                "strict": false,
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }]
        }),
    ]);

    let (chat_request, catalog) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");

    assert_eq!(
        body,
        json!({
            "model": "glm-5.1",
            "messages": [
                { "role": "system", "content": "system prompt" },
                { "role": "user", "content": "hello" }
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "spawn_agent",
                        "description": "Spawn an agent",
                        "strict": false,
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "task": { "type": "string" }
                            },
                            "required": ["task"]
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "exec",
                        "description": "Run shell input",
                        "strict": false,
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "input": {
                                    "type": "string",
                                    "description": "Raw input for the tool."
                                }
                            },
                            "required": ["input"],
                            "additionalProperties": false
                        }
                    }
                },
                {
                    "type": "function",
                    "function": {
                        "name": "mcp__demo__lookup",
                        "description": "Demo namespace\n\nLookup data",
                        "strict": false,
                        "parameters": {
                            "type": "object",
                            "properties": {}
                        }
                    }
                }
            ],
            "tool_choice": "auto",
            "parallel_tool_calls": true,
            "stream": true,
            "stream_options": { "include_usage": true }
        })
    );
    assert_eq!(
        catalog.mapping("spawn_agent"),
        ChatToolMapping::Function {
            name: "spawn_agent".to_string(),
            namespace: None,
        }
    );
    assert_eq!(
        catalog.mapping("exec"),
        ChatToolMapping::Custom {
            name: "exec".to_string(),
        }
    );
    assert_eq!(
        catalog.mapping("mcp__demo__lookup"),
        ChatToolMapping::Function {
            name: "lookup".to_string(),
            namespace: Some("mcp__demo__".to_string()),
        }
    );
}

#[test]
fn converts_parallel_tool_call_history_to_chat_tool_call_group() {
    let mut request = base_request(vec![
        json!({
            "type": "function",
            "name": "first",
            "parameters": { "type": "object", "properties": {} }
        }),
        json!({
            "type": "function",
            "name": "second",
            "parameters": { "type": "object", "properties": {} }
        }),
    ]);
    request.input = vec![
        ResponseItem::FunctionCall {
            id: None,
            metadata: None,
            name: "first".to_string(),
            namespace: None,
            arguments: r#"{"value":1}"#.to_string(),
            call_id: "call-first".to_string(),
        },
        ResponseItem::FunctionCall {
            id: None,
            metadata: None,
            name: "second".to_string(),
            namespace: None,
            arguments: r#"{"value":2}"#.to_string(),
            call_id: "call-second".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            metadata: None,
            call_id: "call-first".to_string(),
            output: FunctionCallOutputPayload::from_text("first output".to_string()),
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            metadata: None,
            call_id: "call-second".to_string(),
            output: FunctionCallOutputPayload::from_text("second output".to_string()),
        },
    ];

    let (chat_request, _) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");

    assert_eq!(
        body["messages"],
        json!([
            { "role": "system", "content": "system prompt" },
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [
                    {
                        "id": "call-first",
                        "type": "function",
                        "function": {
                            "name": "first",
                            "arguments": "{\"value\":1}"
                        }
                    },
                    {
                        "id": "call-second",
                        "type": "function",
                        "function": {
                            "name": "second",
                            "arguments": "{\"value\":2}"
                        }
                    }
                ]
            },
            { "role": "tool", "tool_call_id": "call-first", "content": "first output" },
            { "role": "tool", "tool_call_id": "call-second", "content": "second output" }
        ])
    );
}

#[test]
fn preserves_structured_function_call_output_history_for_chat_messages() {
    let mut request = base_request(vec![json!({
        "type": "function",
        "name": "inspect_image",
        "parameters": { "type": "object", "properties": {} }
    })]);
    request.input = vec![
        ResponseItem::FunctionCall {
            id: None,
            metadata: None,
            name: "inspect_image".to_string(),
            namespace: None,
            arguments: "{}".to_string(),
            call_id: "call-image".to_string(),
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            metadata: None,
            call_id: "call-image".to_string(),
            output: FunctionCallOutputPayload::from_content_items(vec![
                FunctionCallOutputContentItem::InputImage {
                    image_url: "data:image/png;base64,Zm9v".to_string(),
                    detail: None,
                },
                FunctionCallOutputContentItem::InputText {
                    text: "OCR text".to_string(),
                },
            ]),
        },
    ];

    let (chat_request, _) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");
    let content = body["messages"][2]["content"]
        .as_str()
        .expect("structured tool output should be serialized as text");
    let content_items: Value =
        serde_json::from_str(content).expect("structured output content should be JSON");

    assert_eq!(
        content_items,
        json!([
            {
                "type": "input_image",
                "image_url": "data:image/png;base64,Zm9v"
            },
            {
                "type": "input_text",
                "text": "OCR text"
            }
        ])
    );
}

#[test]
fn allocates_distinct_chat_tool_search_name_when_function_name_conflicts() {
    let mut request = base_request(vec![
        json!({
            "type": "function",
            "name": "tool_search",
            "parameters": { "type": "object", "properties": {} }
        }),
        json!({
            "type": "tool_search",
            "description": "Search hosted tools",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                }
            }
        }),
    ]);
    request.input = vec![ResponseItem::ToolSearchCall {
        id: None,
        metadata: None,
        call_id: Some("search-1".to_string()),
        status: Some("completed".to_string()),
        execution: "client".to_string(),
        arguments: json!({ "query": "calendar create" }),
    }];

    let (chat_request, catalog) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");

    assert_eq!(body["tools"][0]["function"]["name"], "tool_search");
    assert_eq!(body["tools"][1]["function"]["name"], "tool_search_2");
    assert_eq!(
        catalog.mapping("tool_search"),
        ChatToolMapping::Function {
            name: "tool_search".to_string(),
            namespace: None,
        }
    );
    assert_eq!(
        catalog.mapping("tool_search_2"),
        ChatToolMapping::ToolSearch
    );
    assert_eq!(
        body["messages"],
        json!([
            { "role": "system", "content": "system prompt" },
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "search-1",
                    "type": "function",
                    "function": {
                        "name": "tool_search_2",
                        "arguments": "{\"query\":\"calendar create\"}"
                    }
                }]
            }
        ])
    );
}

#[test]
fn truncates_tool_search_output_history_for_chat_messages() {
    let mut request = base_request(vec![]);
    request.input = vec![
        ResponseItem::ToolSearchCall {
            id: None,
            metadata: None,
            call_id: Some("search-1".to_string()),
            status: Some("completed".to_string()),
            execution: "client".to_string(),
            arguments: json!({ "query": "large tool" }),
        },
        ResponseItem::ToolSearchOutput {
            id: None,
            metadata: None,
            call_id: Some("search-1".to_string()),
            status: "completed".to_string(),
            execution: "client".to_string(),
            tools: vec![json!({
                "name": "large_tool",
                "description": "x".repeat(CHAT_TOOL_SEARCH_OUTPUT_MAX_BYTES * 2),
            })],
        },
    ];

    let (chat_request, _) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");
    let content = body["messages"][2]["content"]
        .as_str()
        .expect("tool output content should be text");

    assert!(content.len() < CHAT_TOOL_SEARCH_OUTPUT_MAX_BYTES + 1_000);
    assert!(content.starts_with("Warning: truncated output"));
    assert!(content.contains("\nTotal output lines:"));
}

#[test]
fn skips_orphan_function_call_output_history_for_chat_messages() {
    let mut request = base_request(vec![]);
    request.input = vec![
        ResponseItem::Message {
            id: None,
            metadata: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "hello".to_string(),
            }],
            phase: None,
        },
        ResponseItem::FunctionCallOutput {
            id: None,
            metadata: None,
            call_id: "orphan-call".to_string(),
            output: FunctionCallOutputPayload::from_text("orphan output".to_string()),
        },
    ];

    let (chat_request, _) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");

    assert_eq!(
        body["messages"],
        json!([
            { "role": "system", "content": "system prompt" },
            { "role": "user", "content": "hello" }
        ])
    );
}

#[test]
fn renders_server_tool_search_output_as_assistant_context() {
    let mut request = base_request(vec![]);
    request.input = vec![ResponseItem::ToolSearchOutput {
        id: None,
        metadata: None,
        call_id: Some("server-search-1".to_string()),
        status: "completed".to_string(),
        execution: "server".to_string(),
        tools: vec![json!({
            "name": "server_tool",
            "description": "Available on the server",
        })],
    }];

    let (chat_request, _) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");
    let message = &body["messages"][1];
    let content = message["content"]
        .as_str()
        .expect("server tool search output should be text context");

    assert_eq!(message["role"], "assistant");
    assert!(message.get("tool_call_id").is_none());
    assert!(message.get("tool_calls").is_none());
    assert!(content.starts_with("Server-side tool search output:\n"));
    assert!(content.contains("\"execution\":\"server\""));
    assert!(content.contains("\"name\":\"server_tool\""));
}

#[test]
fn sanitizes_namespace_tool_names_and_preserves_original_mapping() {
    let mut request = base_request(vec![json!({
        "type": "namespace",
        "name": "extension/",
        "description": "Extension namespace",
        "tools": [{
            "type": "function",
            "name": "echo",
            "parameters": { "type": "object", "properties": {} }
        }]
    })]);
    request.input = vec![ResponseItem::FunctionCall {
        id: None,
        metadata: None,
        name: "echo".to_string(),
        namespace: Some("extension/".to_string()),
        arguments: r#"{"message":"hello"}"#.to_string(),
        call_id: "call-extension".to_string(),
    }];

    let (chat_request, catalog) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");

    assert_eq!(body["tools"][0]["function"]["name"], "extension_echo");
    assert_eq!(
        catalog.mapping("extension_echo"),
        ChatToolMapping::Function {
            name: "echo".to_string(),
            namespace: Some("extension/".to_string()),
        }
    );
    assert_eq!(
        body["messages"],
        json!([
            { "role": "system", "content": "system prompt" },
            {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call-extension",
                    "type": "function",
                    "function": {
                        "name": "extension_echo",
                        "arguments": "{\"message\":\"hello\"}"
                    }
                }]
            }
        ])
    );
}

#[test]
fn converts_agent_message_history_to_chat_assistant_message() {
    let mut request = base_request(vec![]);
    request.input = vec![ResponseItem::AgentMessage {
        id: None,
        metadata: None,
        author: "worker".to_string(),
        recipient: "root".to_string(),
        content: vec![
            AgentMessageInputContent::InputText {
                text: "finished analysis".to_string(),
            },
            AgentMessageInputContent::EncryptedContent {
                encrypted_content: "opaque".to_string(),
            },
        ],
    }];

    let (chat_request, _) = chat_completions_request_from_responses(&request);
    let body = serde_json::to_value(chat_request).expect("serialize chat request");

    assert_eq!(
        body["messages"],
        json!([
            { "role": "system", "content": "system prompt" },
            {
                "role": "assistant",
                "content": "Agent message from worker to root:\nfinished analysis"
            }
        ])
    );
}

async fn collect_chat_events(
    chunks: &[&[u8]],
    tool_catalog: ChatToolCatalog,
) -> Vec<Result<ResponseEvent, ApiError>> {
    let mut builder = IoBuilder::new();
    for chunk in chunks {
        builder.read(chunk);
    }

    let reader = builder.build();
    let stream = ReaderStream::new(reader).map_err(|err| TransportError::Network(err.to_string()));
    let (tx, mut rx) = mpsc::channel::<Result<ResponseEvent, ApiError>>(16);
    tokio::spawn(process_chat_completions_sse(
        Box::pin(stream),
        tx,
        Duration::from_secs(5),
        /*telemetry*/ None,
        tool_catalog,
    ));

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

#[tokio::test]
async fn parses_chat_completions_text_stream() {
    let events = collect_chat_events(
        &[
            br#"data: {"id":"chatcmpl-1","model":"glm-5.1","choices":[{"delta":{"content":"hel"},"finish_reason":null}]}

"#,
            br#"data: {"id":"chatcmpl-1","choices":[{"delta":{"content":"lo"},"finish_reason":null}]}

"#,
            br#"data: {"id":"chatcmpl-1","choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}

"#,
        ],
        ChatToolCatalog::default(),
    )
    .await;

    assert_eq!(events.len(), 7);
    assert!(matches!(&events[0], Ok(ResponseEvent::Created)));
    assert!(matches!(
        &events[1],
        Ok(ResponseEvent::ServerModel(model)) if model == "glm-5.1"
    ));
    assert!(matches!(
        &events[2],
        Ok(ResponseEvent::OutputItemAdded(ResponseItem::Message { .. }))
    ));
    assert!(matches!(
        &events[3],
        Ok(ResponseEvent::OutputTextDelta(delta)) if delta == "hel"
    ));
    assert!(matches!(
        &events[4],
        Ok(ResponseEvent::OutputTextDelta(delta)) if delta == "lo"
    ));
    assert!(matches!(
        &events[5],
        Ok(ResponseEvent::OutputItemDone(ResponseItem::Message { content, .. }))
            if content == &vec![ContentItem::OutputText { text: "hello".to_string() }]
    ));
    assert!(matches!(
        &events[6],
        Ok(ResponseEvent::Completed {
            response_id,
            token_usage: Some(TokenUsage { total_tokens: 3, .. }),
            end_turn: Some(true),
        }) if response_id == "chatcmpl-1"
    ));
}

#[tokio::test]
async fn parses_chat_completions_usage_chunk_after_finish_reason() {
    let events = collect_chat_events(
        &[
            br#"data: {"id":"chatcmpl-usage","choices":[{"delta":{"content":"done"},"finish_reason":null}]}

"#,
            br#"data: {"id":"chatcmpl-usage","choices":[{"delta":{},"finish_reason":"stop"}],"usage":null}

"#,
            br#"data: {"id":"chatcmpl-usage","choices":[],"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6}}

"#,
            br#"data: [DONE]

"#,
        ],
        ChatToolCatalog::default(),
    )
    .await;

    assert!(matches!(
        events.last(),
        Some(Ok(ResponseEvent::Completed {
            response_id,
            token_usage: Some(TokenUsage { total_tokens: 6, .. }),
            end_turn: Some(true),
        })) if response_id == "chatcmpl-usage"
    ));
}

#[tokio::test]
async fn ignores_empty_chat_completions_content_delta() {
    let events = collect_chat_events(
        &[
            br#"data: {"id":"chatcmpl-empty","choices":[{"delta":{"content":""},"finish_reason":null}]}

"#,
            br#"data: {"id":"chatcmpl-empty","choices":[{"delta":{},"finish_reason":"stop"}]}

"#,
        ],
        ChatToolCatalog::default(),
    )
    .await;

    assert_eq!(events.len(), 2);
    assert!(matches!(&events[0], Ok(ResponseEvent::Created)));
    assert!(matches!(
        &events[1],
        Ok(ResponseEvent::Completed {
            response_id,
            end_turn: Some(true),
            ..
        }) if response_id == "chatcmpl-empty"
    ));
}

#[tokio::test]
async fn parses_chat_completions_custom_tool_call_stream() {
    let mut catalog = ChatToolCatalog::default();
    catalog.insert(
        "exec".to_string(),
        ChatToolMapping::Custom {
            name: "exec".to_string(),
        },
    );

    let events = collect_chat_events(
        &[br#"data: {"id":"chatcmpl-2","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","function":{"name":"exec","arguments":"{\"input\":\"ls\"}"}}]},"finish_reason":"tool_calls"}]}

"#],
        catalog,
    )
    .await;

    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], Ok(ResponseEvent::Created)));
    assert!(matches!(
        &events[1],
        Ok(ResponseEvent::OutputItemDone(ResponseItem::CustomToolCall {
            call_id,
            name,
            input,
            ..
        })) if call_id == "call-1" && name == "exec" && input == "ls"
    ));
    assert!(matches!(
        &events[2],
        Ok(ResponseEvent::Completed {
            response_id,
            end_turn: Some(false),
            ..
        }) if response_id == "chatcmpl-2"
    ));
}

#[tokio::test]
async fn parses_split_chat_completions_tool_call_arguments() {
    let mut catalog = ChatToolCatalog::default();
    catalog.insert(
        "exec".to_string(),
        ChatToolMapping::Custom {
            name: "exec".to_string(),
        },
    );

    let events = collect_chat_events(
        &[
            br#"data: {"id":"chatcmpl-3","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","function":{"name":"exec","arguments":"{\"input\":\""}}]},"finish_reason":null}]}

"#,
            br#"data: {"id":"chatcmpl-3","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ls -la\"}"}}]},"finish_reason":"tool_calls"}]}

"#,
        ],
        catalog,
    )
    .await;

    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], Ok(ResponseEvent::Created)));
    assert!(matches!(
        &events[1],
        Ok(ResponseEvent::OutputItemDone(ResponseItem::CustomToolCall {
            call_id,
            name,
            input,
            ..
        })) if call_id == "call-1" && name == "exec" && input == "ls -la"
    ));
    assert!(matches!(
        &events[2],
        Ok(ResponseEvent::Completed {
            response_id,
            end_turn: Some(false),
            ..
        }) if response_id == "chatcmpl-3"
    ));
}

#[tokio::test]
async fn parses_split_legacy_chat_completions_function_call() {
    let events = collect_chat_events(
        &[
            br#"data: {"id":"chatcmpl-legacy","choices":[{"delta":{"function_call":{"name":"shell_command","arguments":"{\"command\":\""}},"finish_reason":null}]}

"#,
            br#"data: {"id":"chatcmpl-legacy","choices":[{"delta":{"function_call":{"arguments":"pwd\"}"}},"finish_reason":"function_call"}]}

"#,
        ],
        ChatToolCatalog::default(),
    )
    .await;

    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], Ok(ResponseEvent::Created)));
    assert!(matches!(
        &events[1],
        Ok(ResponseEvent::OutputItemDone(ResponseItem::FunctionCall {
            call_id,
            name,
            namespace,
            arguments,
            ..
        })) if call_id == LEGACY_FUNCTION_CALL_ID
            && name == "shell_command"
            && namespace.is_none()
            && arguments == "{\"command\":\"pwd\"}"
    ));
    assert!(matches!(
        &events[2],
        Ok(ResponseEvent::Completed {
            response_id,
            end_turn: Some(false),
            ..
        }) if response_id == "chatcmpl-legacy"
    ));
}
