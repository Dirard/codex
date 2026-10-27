#![cfg(not(target_os = "windows"))]
#![allow(clippy::unwrap_used)]

//! Fork: `request_user_input` in `codex exec` (specs/explorer-and-questions-2026-10-25.md).

use core_test_support::responses;
use core_test_support::test_codex_exec::test_codex_exec;

const CALL_ID: &str = "exec-user-input-call";

/// Finds a tool description anywhere in the request, including namespaced tool lists.
fn find_description(value: &serde_json::Value, name: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("name").and_then(serde_json::Value::as_str) == Some(name)
                && let Some(description) =
                    map.get("description").and_then(serde_json::Value::as_str)
            {
                return Some(description.to_string());
            }
            map.values().find_map(|value| find_description(value, name))
        }
        serde_json::Value::Array(items) => {
            items.iter().find_map(|value| find_description(value, name))
        }
        _ => None,
    }
}

/// Returns the exec output, the tool output the model receives and the tool description.
async fn run_exec_request_user_input(
    extra_args: &[&str],
) -> anyhow::Result<(std::process::Output, String, String)> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    let arguments = serde_json::json!({
        "questions": [{
            "id": "storage",
            "header": "Storage",
            "question": "Which storage backend should the cache use?",
            "options": [
                {"label": "SQLite (Recommended)", "description": "Persistent."},
                {"label": "In memory", "description": "Fast but lost on restart."}
            ]
        }]
    })
    .to_string();
    let first = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-1"),
            responses::ev_function_call(CALL_ID, "request_user_input", &arguments),
            responses::ev_completed("resp-1"),
        ]),
    )
    .await;
    let followup = responses::mount_sse_once(
        &server,
        responses::sse(vec![
            responses::ev_response_created("resp-2"),
            responses::ev_assistant_message("msg-2", "continued without asking"),
            responses::ev_completed("resp-2"),
        ]),
    )
    .await;

    let output = test
        .cmd_with_server(&server)
        .arg("--skip-git-repo-check")
        .args(extra_args)
        .arg("add a cache")
        .timeout(std::time::Duration::from_secs(60))
        .output()?;
    let tool_output = followup
        .single_request()
        .function_call_output_text(CALL_ID)
        .unwrap_or_default();
    let body = first.single_request().body_json();
    let description = find_description(&body, "request_user_input").unwrap_or_default();
    Ok((output, tool_output, description))
}

/// exec offers no question tool (combined live run 2026-10-27, D2): it rejects
/// `requestUserInput`, and the later user message that would answer
/// `request_user_input_async` never comes. A call the model makes anyway is refused.
async fn assert_exec_offers_no_question_tool(extra_args: &[&str]) -> anyhow::Result<()> {
    let (output, tool_output, description) = run_exec_request_user_input(extra_args).await?;

    assert!(output.status.success(), "exec run failed: {output:?}");
    assert_eq!(description, "", "request_user_input is offered");
    assert_eq!(tool_output, "unsupported call: request_user_input");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_never_offers_default_mode_request_user_input() -> anyhow::Result<()> {
    assert_exec_offers_no_question_tool(&["-c", "features.default_mode_request_user_input=true"])
        .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_offers_no_question_tool_without_feature() -> anyhow::Result<()> {
    assert_exec_offers_no_question_tool(&[]).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_offers_no_question_tool_even_when_configured() -> anyhow::Result<()> {
    assert_exec_offers_no_question_tool(&[
        "-c",
        "tools.experimental_request_user_input.enabled=true",
    ])
    .await
}
