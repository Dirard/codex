//! Fork: the output budget of a Code Mode cell without the `max_output_tokens` pragma follows the
//! output truncation the user configured (specs/code-mode-cell-output-budget-2026-10-29.md,
//! R1.1, R1.3, R1.4).

use super::*;
use pretty_assertions::assert_eq;
use test_case::test_case;

/// About 20 000 tokens per output batch: more than the bundled 10 000-token budget.
const LARGE_OUTPUT_BYTES: usize = 80_000;

/// The test catalog gives this model the 10 000-byte policy of the fallback model metadata.
const BYTE_POLICY_TEST_MODEL: &str = "test-gpt-5.1-codex";

/// What the model got from a cell that prints [`LARGE_OUTPUT_BYTES`] of `x`, yields, then prints
/// as many `y`.
struct LargeCell {
    exec_description: String,
    exec_output: String,
    wait_output: String,
}

async fn run_large_cell(
    model: &'static str,
    configure: impl FnOnce(&mut Config) + Send + 'static,
) -> Result<LargeCell> {
    let server = responses::start_mock_server().await;
    let code = format!(
        r#"text("x".repeat({LARGE_OUTPUT_BYTES}));
yield_control();
text("y".repeat({LARGE_OUTPUT_BYTES}));
"#
    );
    let (test, first_completion) = run_code_mode_turn_with_model_and_config(
        &server,
        "print a large cell",
        &code,
        model,
        configure,
    )
    .await?;

    let first_request = first_completion.single_request();
    let exec_description = first_request
        .body_json()
        .get("tools")
        .and_then(Value::as_array)
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool.get("name").and_then(Value::as_str) == Some("exec"))
        })
        .and_then(|tool| tool.get("description"))
        .and_then(Value::as_str)
        .expect("the request should declare exec with a description")
        .to_string();
    let first_items = custom_tool_output_items(&first_request, "call-1");
    assert_eq!(
        first_items.len(),
        2,
        "exec should return a header and one text item"
    );
    let exec_output = text_item(&first_items, /*index*/ 1).to_string();
    let cell_id = extract_running_cell_id(text_item(&first_items, /*index*/ 0));

    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-3"),
            responses::ev_function_call(
                "call-2",
                "wait",
                &serde_json::json!({
                    "cell_id": cell_id,
                    "yield_time_ms": 1_000,
                })
                .to_string(),
            ),
            ev_completed("resp-3"),
        ]),
    )
    .await;
    let second_completion = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-2", "done"),
            ev_completed("resp-4"),
        ]),
    )
    .await;

    test.submit_turn("wait for the rest of the cell").await?;

    let second_items = function_tool_output_items(&second_completion.single_request(), "call-2");
    assert_eq!(
        second_items.len(),
        2,
        "wait should return a header and one text item"
    );
    let wait_output = text_item(&second_items, /*index*/ 1).to_string();
    Ok(LargeCell {
        exec_description,
        exec_output,
        wait_output,
    })
}

fn assert_exec_default(cell: &LargeCell, tokens: usize) {
    let sentence = format!(
        "`max_output_tokens` sets the token budget for direct `exec` results. Defaults to {tokens} tokens."
    );
    assert!(
        cell.exec_description.contains(&sentence),
        "exec should say: {sentence}\n{}",
        cell.exec_description
    );
}

/// R1.1, R1.4: `tool_output_token_limit = 400000` keeps a 20 000-token `exec` and `wait` output
/// whole, for a token-policy and a byte-policy model.
#[test_case(TOKEN_POLICY_TEST_MODEL; "token_policy_model")]
#[test_case(BYTE_POLICY_TEST_MODEL; "byte_policy_model")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_cell_output_follows_raised_tool_output_token_limit(
    model: &'static str,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let cell = run_large_cell(model, |config| {
        config.tool_output_token_limit = Some(400_000);
    })
    .await?;

    assert_exec_default(&cell, 400_000);
    for (tool, output, fill) in [
        ("exec", &cell.exec_output, "x"),
        ("wait", &cell.wait_output, "y"),
    ] {
        assert!(
            *output == fill.repeat(LARGE_OUTPUT_BYTES),
            "{tool} output was truncated: {} bytes, {output:.300}",
            output.len()
        );
    }

    Ok(())
}

/// R1.3: without configuration the bundled 10 000-token policy cuts the same cell as before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_cell_output_keeps_bundled_budget_by_default() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let cell = run_large_cell(TOKEN_POLICY_TEST_MODEL, |_| {}).await?;

    assert_exec_default(&cell, 10_000);
    for (tool, output, fill) in [
        ("exec", &cell.exec_output, "x"),
        ("wait", &cell.wait_output, "y"),
    ] {
        assert!(
            output.starts_with(
                "Warning: truncated output (original token count: 20000)\nTotal output lines: 1\n\n"
            ),
            "{tool} output was not truncated by the bundled budget: {} bytes, {output:.300}",
            output.len()
        );
        assert_regex_match(
            &format!(
                r"(?s)\AWarning: truncated output \(original token count: 20000\)\nTotal output lines: 1\n\n{fill}+…10000 tokens truncated…{fill}+\z"
            ),
            output,
        );
    }

    Ok(())
}

/// R1.3: without configuration a byte-policy model (like the fallback metadata of an unknown
/// model) keeps the upstream 10 000-token cell budget, not its 10 000-byte tool output policy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_byte_policy_model_keeps_upstream_cell_budget_by_default() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let cell = run_large_cell(BYTE_POLICY_TEST_MODEL, |_| {}).await?;

    assert_exec_default(&cell, 10_000);
    for (tool, output) in [("exec", &cell.exec_output), ("wait", &cell.wait_output)] {
        // The cell keeps 10 000 tokens (about 40 000 bytes); history then applies the model's
        // byte policy. A 10 000-byte cell budget would hand history the whole 80 000 bytes' cut.
        let captures = assert_regex_match(
            r"(?s)\AWarning: truncated output \(original token count: 20000\)\nTotal output lines: 1\n\n.*…(\d+) chars truncated….*\z",
            output,
        );
        let removed_by_history: usize = captures[1].parse()?;
        let cell_bytes = output.len() + removed_by_history;
        assert!(
            (39_000..=42_000).contains(&cell_bytes),
            "{tool} cell should keep about 40 000 bytes before history, kept {cell_bytes}"
        );
    }

    Ok(())
}
