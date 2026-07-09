use anyhow::Result;
use app_test_support::TestAppServer;
use app_test_support::create_final_assistant_message_sse_response;
use app_test_support::create_mock_responses_server_sequence_unchecked;
use app_test_support::create_shell_command_sse_response;
use app_test_support::to_response;
use app_test_support::write_mock_responses_config_toml_with_chatgpt_base_url;
use codex_app_server::INVALID_PARAMS_ERROR_CODE;
use codex_app_server_protocol::AdditionalContextEntry;
use codex_app_server_protocol::AdditionalContextKind;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCResponse;
use codex_app_server_protocol::MAX_ADDITIONAL_CONTEXT_ENTRIES;
use codex_app_server_protocol::MAX_ADDITIONAL_CONTEXT_KEY_BYTES;
use codex_app_server_protocol::MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES;
use codex_app_server_protocol::MAX_ADDITIONAL_CONTEXT_VALUE_BYTES;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::TurnSteerParams;
use codex_app_server_protocol::UserInput as V2UserInput;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const MODEL_VISIBLE_ITEM_TOKEN_CAP: usize = 10_000;
const _: () = assert!(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES < MODEL_VISIBLE_ITEM_TOKEN_CAP);

#[tokio::test]
async fn turn_start_accepts_additional_context_at_limits() -> Result<()> {
    let server = create_mock_responses_server_sequence_unchecked(vec![
        create_final_assistant_message_sse_response("Done")?,
    ])
    .await;
    let codex_home = TempDir::new()?;
    write_mock_responses_config_toml_with_chatgpt_base_url(
        codex_home.path(),
        &server.uri(),
        &server.uri(),
    )?;

    let mut mcp = TestAppServer::new_with_auto_env(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let thread_id = start_thread(&mut mcp).await?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id,
            client_user_message_id: None,
            input: text_input("inspect context"),
            additional_context: Some(context_at_total_limit()),
            ..Default::default()
        })
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/completed"),
    )
    .await??;

    Ok(())
}

#[tokio::test]
async fn turn_start_rejects_additional_context_over_limits() -> Result<()> {
    let server = create_mock_responses_server_sequence_unchecked(Vec::new()).await;
    let codex_home = TempDir::new()?;
    write_mock_responses_config_toml_with_chatgpt_base_url(
        codex_home.path(),
        &server.uri(),
        &server.uri(),
    )?;

    let mut mcp = TestAppServer::new_with_auto_env(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let thread_id = start_thread(&mut mcp).await?;

    for (case_name, additional_context) in over_limit_cases() {
        let request_id = mcp
            .send_turn_start_request(TurnStartParams {
                thread_id: thread_id.clone(),
                client_user_message_id: None,
                input: text_input(case_name),
                additional_context: Some(additional_context),
                ..Default::default()
            })
            .await?;
        assert_invalid_params_error(&mut mcp, request_id).await?;
    }

    Ok(())
}

#[tokio::test]
async fn turn_steer_rejects_additional_context_over_limits() -> Result<()> {
    #[cfg(target_os = "windows")]
    let shell_command = vec![
        "powershell".to_string(),
        "-Command".to_string(),
        "Start-Sleep -Seconds 10".to_string(),
    ];
    #[cfg(not(target_os = "windows"))]
    let shell_command = vec!["sleep".to_string(), "10".to_string()];

    let tmp = TempDir::new()?;
    let codex_home = tmp.path().join("codex_home");
    std::fs::create_dir(&codex_home)?;
    let working_directory = tmp.path().join("workdir");
    std::fs::create_dir(&working_directory)?;
    let server =
        create_mock_responses_server_sequence_unchecked(vec![create_shell_command_sse_response(
            shell_command,
            Some(&working_directory),
            Some(10_000),
            "call_sleep",
        )?])
        .await;
    write_mock_responses_config_toml_with_chatgpt_base_url(
        &codex_home,
        &server.uri(),
        &server.uri(),
    )?;

    let mut mcp = TestAppServer::new_with_auto_env(&codex_home).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp.initialize()).await??;
    let thread_id = start_thread(&mut mcp).await?;

    let turn_req = mcp
        .send_turn_start_request(TurnStartParams {
            thread_id: thread_id.clone(),
            client_user_message_id: None,
            input: text_input("run sleep"),
            cwd: Some(working_directory),
            ..Default::default()
        })
        .await?;
    let turn_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(turn_req)),
    )
    .await??;
    let TurnStartResponse { turn } = to_response::<TurnStartResponse>(turn_resp)?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_notification_message("turn/started"),
    )
    .await??;

    for (case_name, additional_context) in over_limit_cases() {
        let request_id = mcp
            .send_turn_steer_request(TurnSteerParams {
                thread_id: thread_id.clone(),
                client_user_message_id: None,
                input: text_input(case_name),
                responsesapi_client_metadata: None,
                additional_context: Some(additional_context),
                expected_turn_id: turn.id.clone(),
            })
            .await?;
        assert_invalid_params_error(&mut mcp, request_id).await?;
    }

    mcp.interrupt_turn_and_wait_for_aborted(thread_id, turn.id, DEFAULT_READ_TIMEOUT)
        .await?;
    Ok(())
}

#[test]
fn additional_context_limit_fixtures_cover_boundaries() {
    let at_entries = context_with_entry_count(MAX_ADDITIONAL_CONTEXT_ENTRIES);
    assert_eq!(at_entries.len(), MAX_ADDITIONAL_CONTEXT_ENTRIES);

    let over_entries = context_with_entry_count(MAX_ADDITIONAL_CONTEXT_ENTRIES + 1);
    assert_eq!(over_entries.len(), MAX_ADDITIONAL_CONTEXT_ENTRIES + 1);

    let at_key = context_with_key_len(MAX_ADDITIONAL_CONTEXT_KEY_BYTES);
    assert_eq!(max_key_bytes(&at_key), MAX_ADDITIONAL_CONTEXT_KEY_BYTES);

    let over_key = context_with_key_len(MAX_ADDITIONAL_CONTEXT_KEY_BYTES + 1);
    assert_eq!(
        max_key_bytes(&over_key),
        MAX_ADDITIONAL_CONTEXT_KEY_BYTES + 1
    );

    let at_value = context_with_value_len(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES);
    assert_eq!(
        max_value_bytes(&at_value),
        MAX_ADDITIONAL_CONTEXT_VALUE_BYTES
    );

    let over_value = context_with_value_len(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES + 1);
    assert_eq!(
        max_value_bytes(&over_value),
        MAX_ADDITIONAL_CONTEXT_VALUE_BYTES + 1
    );

    let at_total = context_at_total_limit();
    assert_eq!(
        total_context_bytes(&at_total),
        MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES
    );

    let over_total = context_over_total_limit();
    assert!(total_context_bytes(&over_total) > MAX_ADDITIONAL_CONTEXT_TOTAL_BYTES);
}

async fn start_thread(mcp: &mut TestAppServer) -> Result<String> {
    let thread_req = mcp
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let thread_resp: JSONRPCResponse = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(thread_req)),
    )
    .await??;
    let ThreadStartResponse { thread, .. } = to_response::<ThreadStartResponse>(thread_resp)?;
    Ok(thread.id)
}

async fn assert_invalid_params_error(mcp: &mut TestAppServer, request_id: i64) -> Result<()> {
    let error: JSONRPCError = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert_eq!(error.error.code, INVALID_PARAMS_ERROR_CODE);
    Ok(())
}

fn over_limit_cases() -> Vec<(&'static str, HashMap<String, AdditionalContextEntry>)> {
    vec![
        (
            "too many additional context entries",
            context_with_entry_count(MAX_ADDITIONAL_CONTEXT_ENTRIES + 1),
        ),
        (
            "additional context key too large",
            context_with_key_len(MAX_ADDITIONAL_CONTEXT_KEY_BYTES + 1),
        ),
        (
            "additional context value too large",
            context_with_value_len(MAX_ADDITIONAL_CONTEXT_VALUE_BYTES + 1),
        ),
        (
            "additional context total too large",
            context_over_total_limit(),
        ),
    ]
}

fn text_input(text: &str) -> Vec<V2UserInput> {
    vec![V2UserInput::Text {
        text: text.to_string(),
        text_elements: Vec::new(),
    }]
}

fn context_with_entry_count(count: usize) -> HashMap<String, AdditionalContextEntry> {
    (0..count)
        .map(|i| (format!("source_{i}"), context_entry("x")))
        .collect()
}

fn context_with_key_len(len: usize) -> HashMap<String, AdditionalContextEntry> {
    HashMap::from([(key_with_len(len, 0), context_entry("x"))])
}

fn context_with_value_len(len: usize) -> HashMap<String, AdditionalContextEntry> {
    HashMap::from([("source".to_string(), context_entry(&"x".repeat(len)))])
}

fn context_at_total_limit() -> HashMap<String, AdditionalContextEntry> {
    (0..4)
        .map(|i| (key_with_len(24, i), context_entry(&"x".repeat(1000))))
        .collect()
}

fn context_over_total_limit() -> HashMap<String, AdditionalContextEntry> {
    (0..5)
        .map(|i| (key_with_len(24, i), context_entry(&"x".repeat(900))))
        .collect()
}

fn context_entry(value: &str) -> AdditionalContextEntry {
    AdditionalContextEntry {
        value: value.to_string(),
        kind: AdditionalContextKind::Untrusted,
    }
}

fn key_with_len(len: usize, index: usize) -> String {
    let prefix = format!("{index:02}");
    format!("{prefix}{}", "k".repeat(len - prefix.len()))
}

fn max_key_bytes(context: &HashMap<String, AdditionalContextEntry>) -> usize {
    context.keys().map(String::len).max().unwrap_or_default()
}

fn max_value_bytes(context: &HashMap<String, AdditionalContextEntry>) -> usize {
    context
        .values()
        .map(|entry| entry.value.len())
        .max()
        .unwrap_or_default()
}

fn total_context_bytes(context: &HashMap<String, AdditionalContextEntry>) -> usize {
    context
        .iter()
        .map(|(key, entry)| key.len() + entry.value.len())
        .sum()
}
