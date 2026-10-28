//! Fork: the output budget of a Code Mode cell without the `max_output_tokens` pragma follows the
//! configured output truncation (specs/code-mode-cell-output-budget-2026-10-29.md, R1.1, R1.3).

use super::*;
use pretty_assertions::assert_eq;

/// About 20 000 tokens per output batch: more than the bundled 10 000-token budget.
const LARGE_OUTPUT_BYTES: usize = 80_000;

/// The texts that `exec` and the following `wait` give the model for a cell that prints
/// [`LARGE_OUTPUT_BYTES`] of `x`, yields, then prints as many `y`.
async fn large_exec_and_wait_outputs(
    configure: impl FnOnce(&mut Config) + Send + 'static,
) -> Result<(String, String)> {
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
        TOKEN_POLICY_TEST_MODEL,
        configure,
    )
    .await?;

    let first_items = custom_tool_output_items(&first_completion.single_request(), "call-1");
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
    Ok((exec_output, wait_output))
}

/// R1.1: `tool_output_token_limit = 400000` keeps a 20 000-token `exec` and `wait` output whole.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_cell_output_follows_raised_tool_output_token_limit() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let (exec_output, wait_output) = large_exec_and_wait_outputs(|config| {
        config.tool_output_token_limit = Some(400_000);
    })
    .await?;

    for (tool, output, fill) in [("exec", &exec_output, "x"), ("wait", &wait_output, "y")] {
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

    let (exec_output, wait_output) = large_exec_and_wait_outputs(|_| {}).await?;

    for (tool, output, fill) in [("exec", &exec_output, "x"), ("wait", &wait_output, "y")] {
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
