//! Fork: use cases 1, 2 and 4 of `specs/tool-output-spill-2026-11-07.md`.

use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use super::store::SpillRecord;
use super::*;
use crate::context_manager::ContextManager;
use crate::function_tool::FunctionCallError;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::tools::context::ExecCommandToolOutput;
use crate::tools::context::McpToolOutput;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use crate::tools::registry::ToolRegistry;
use crate::tools::router::ToolRouter;
use codex_protocol::mcp::CallToolResult;
use codex_protocol::models::ImageReference;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ToolMode;
use core_test_support::TempDirExt;
use pretty_assertions::assert_eq;

const CALL_ID: &str = "call-spill";

fn function_payload() -> ToolPayload {
    ToolPayload::Function {
        arguments: "{}".to_string(),
    }
}

fn numbered_lines(count: usize) -> String {
    (1..=count)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn exec_output(text: &str) -> ExecCommandToolOutput {
    ExecCommandToolOutput {
        event_call_id: CALL_ID.to_string(),
        chunk_id: "abc123".to_string(),
        wall_time: std::time::Duration::from_millis(1250),
        raw_output: text.into(),
        truncation: OutputTruncation::new(
            TruncationPolicy::Tokens(10_000),
            /*max_lines*/ None,
        ),
        max_output_tokens: None,
        process_id: None,
        exit_code: Some(0),
        original_token_count: None,
        hook_command: None,
        tool_cwd: None,
        armed_notifications: Default::default(),
    }
}

fn mcp_output(text: &str, truncation: OutputTruncation) -> McpToolOutput {
    McpToolOutput {
        result: CallToolResult {
            content: vec![serde_json::json!({ "type": "text", "text": text })],
            structured_content: None,
            is_error: Some(false),
            meta: Some(serde_json::json!({ "trace": "kept" })),
        },
        tool_input: serde_json::json!({ "query": "all" }),
        result_metadata_capture_allowed: true,
        wall_time: std::time::Duration::from_millis(250),
        original_image_detail_supported: false,
        truncation,
    }
}

fn result(output: Box<dyn ToolOutput>) -> AnyToolResult {
    AnyToolResult {
        call_id: CALL_ID.to_string(),
        payload: function_payload(),
        result: output,
        post_tool_use_payload: None,
    }
}

/// What history records for `result` on a step whose output truncation is `step_truncation`,
/// and, when `replay` is set, that replay under another configuration records the same.
fn recorded(
    result: AnyToolResult,
    step_truncation: OutputTruncation,
    replay: bool,
) -> FunctionCallOutputPayload {
    let mut response = result.into_response();
    let mut history = ContextManager::default();
    history.record_annotated_items(std::slice::from_mut(&mut response), step_truncation);
    let items = history.raw_items().cloned().collect::<Vec<_>>();
    if replay {
        let mut replayed = ContextManager::default();
        replayed.replay_annotated_item(
            &response,
            OutputTruncation::new(TruncationPolicy::Bytes(1), /*max_lines*/ Some(0)),
        );
        assert_eq!(replayed.raw_items().cloned().collect::<Vec<_>>(), items);
    }
    match items.as_slice() {
        [ResponseItem::FunctionCallOutput { output, .. }]
        | [ResponseItem::CustomToolCallOutput { output, .. }] => output.clone(),
        other => panic!("expected one tool output, got {other:?}"),
    }
}

fn text_of(output: &FunctionCallOutputPayload) -> String {
    visible_text(output).into_owned()
}

/// The model-visible text of a spilled output, split at the notice.
fn split_notice(text: &str) -> (&str, &str) {
    let start = text
        .rfind("Full output saved to: ")
        .unwrap_or_else(|| panic!("no spill notice in {text}"));
    (&text[..start], &text[start..])
}

fn test_record(path: &str) -> SpillRecord {
    SpillRecord {
        path: PathBuf::from(path),
        lines: 5000,
        bytes: 38_893,
        tokens: 9_724,
        capped: None,
    }
}

/// R4.2, R7.4: the notice text, with `tools.exec_command` for Code Mode, as its own item after
/// media, and with a capped file.
#[test]
fn the_notice_follows_the_template() {
    let record = test_record("/home/user/.codex/tool_outputs/t/0123456789abcdef.txt");
    let notice = notice::spill_notice(&record, "exec_command");
    assert_eq!(
        notice,
        "Full output saved to: /home/user/.codex/tool_outputs/t/0123456789abcdef.txt (5000 lines, 38893 bytes, ~9724 tokens). Read only the part you need with exec_command: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again."
    );
    assert_eq!(
        notice::spill_notice(&record, "tools.exec_command"),
        "Full output saved to: /home/user/.codex/tool_outputs/t/0123456789abcdef.txt (5000 lines, 38893 bytes, ~9724 tokens). Read only the part you need with tools.exec_command: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again."
    );
    let capped = SpillRecord {
        capped: Some(store::CappedSpill {
            kept_half: 4_194_304,
            original_bytes: 9_000_000,
        }),
        ..record
    };
    assert_eq!(
        notice::spill_notice(&capped, "exec_command"),
        "Full output saved to: /home/user/.codex/tool_outputs/t/0123456789abcdef.txt (5000 lines, 38893 bytes, ~9724 tokens; the file keeps the first and last 4194304 bytes of 9000000). Read only the part you need with exec_command: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again."
    );

    let mut text = FunctionCallOutputPayload::from_text("head".to_string());
    append_notice(&mut text, "NOTICE");
    assert_eq!(
        text.body,
        FunctionCallOutputBody::Text("head\n\nNOTICE".to_string())
    );
    let image = FunctionCallOutputContentItem::InputImage {
        image: ImageReference::Inline {
            image_url: "data:image/png;base64,AAAA".to_string(),
        },
        detail: None,
    };
    let mut items = FunctionCallOutputPayload {
        body: FunctionCallOutputBody::ContentItems(vec![
            FunctionCallOutputContentItem::InputText {
                text: "head".to_string(),
            },
            image.clone(),
        ]),
        success: Some(true),
    };
    append_notice(&mut items, "NOTICE");
    assert_eq!(
        items.body,
        FunctionCallOutputBody::ContentItems(vec![
            FunctionCallOutputContentItem::InputText {
                text: "head".to_string(),
            },
            image,
            FunctionCallOutputContentItem::InputText {
                text: "NOTICE".to_string(),
            },
        ])
    );
}

/// `specs/fork-tool-guidance-2026-11-08.md` UC4, R4.1: a file of very long lines, such as the
/// compact JSON of an MCP `structuredContent`, gets byte- and match-oriented reads instead of
/// line ranges, which would return the whole line again; up to 2048 bytes per line on average
/// the notice keeps line ranges.
#[test]
fn a_file_of_long_lines_gets_reads_that_do_not_return_whole_lines() {
    let path = "/home/user/.codex/tool_outputs/t/0123456789abcdef.txt";
    let one_line = SpillRecord {
        lines: 1,
        bytes: 200_000,
        tokens: 50_000,
        ..test_record(path)
    };
    assert_eq!(
        notice::spill_notice(&one_line, "exec_command"),
        "Full output saved to: /home/user/.codex/tool_outputs/t/0123456789abcdef.txt (1 lines, 200000 bytes, ~50000 tokens). The lines are very long: read only the part you need with exec_command, e.g. jq for JSON, rg -o '.{0,200}PATTERN.{0,200}' or head -c N; a larger read is cut again."
    );
    let at_the_limit = SpillRecord {
        lines: 10,
        bytes: 20_480,
        ..test_record(path)
    };
    assert!(
        notice::spill_notice(&at_the_limit, "tools.exec_command").ends_with(
            "Read only the part you need with tools.exec_command: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again."
        )
    );
    let above_the_limit = SpillRecord {
        bytes: 20_491,
        ..at_the_limit
    };
    assert!(
        notice::spill_notice(&above_the_limit, "tools.exec_command").contains(
            "The lines are very long: read only the part you need with tools.exec_command,"
        )
    );
}

/// R4.1, R1.3: for exec, MCP and dynamic tools the text before the notice is exactly what
/// history records of the original output without the feature.
#[test]
fn the_preview_matches_history_without_the_feature() {
    let step_truncation = OutputTruncation::new_with_mcp_max_lines(
        TruncationPolicy::Tokens(10_000),
        Some(100),
        Some(40),
    );
    let long = numbered_lines(5000);
    let original = |name: &str| -> Box<dyn ToolOutput> {
        match name {
            "exec" => Box::new(exec_output(&long)),
            "mcp" => Box::new(mcp_output(&long, step_truncation)),
            _ => Box::new(ItemTextSpill(FunctionToolOutput::from_text(
                long.clone(),
                Some(true),
            ))),
        }
    };
    for name in ["exec", "mcp", "dynamic"] {
        let without = text_of(&recorded(
            result(original(name)),
            step_truncation,
            /*replay*/ false,
        ));
        let spilled =
            SpilledToolOutput::for_test(original(name), step_truncation, function_payload());
        let with = text_of(&recorded(
            result(Box::new(spilled)),
            step_truncation,
            /*replay*/ true,
        ));
        let (preview, notice) = split_notice(&with);
        assert_eq!(preview, format!("{without}\n\n"), "{name}");
        assert!(
            notice.starts_with("Full output saved to: /home/user/.codex/"),
            "{name}"
        );
        assert!(!without.contains("line 2500"), "{name}: the middle was cut");
    }
}

/// R4.3: recording and replay under any configuration keep a spilled item byte for byte, for
/// text and content items with media, under token and byte policies.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recording_leaves_the_spilled_item_unchanged() {
    let long = numbered_lines(3000);
    let image = FunctionCallOutputContentItem::InputImage {
        image: ImageReference::Inline {
            image_url: "data:image/png;base64,AAAA".to_string(),
        },
        detail: None,
    };
    let audio = FunctionCallOutputContentItem::InputAudio {
        audio_url: "data:audio/wav;base64,UklGRg==".to_string(),
    };
    let bodies = vec![
        vec![FunctionCallOutputContentItem::InputText { text: long.clone() }],
        vec![
            FunctionCallOutputContentItem::InputText { text: long.clone() },
            image.clone(),
            audio.clone(),
        ],
        vec![
            image,
            FunctionCallOutputContentItem::InputText { text: long.clone() },
            audio,
            FunctionCallOutputContentItem::InputText { text: long },
        ],
    ];
    for body in bodies {
        for step_truncation in [
            OutputTruncation::new(TruncationPolicy::Tokens(200), /*max_lines*/ None),
            OutputTruncation::new(TruncationPolicy::Tokens(10_000), Some(50)),
            OutputTruncation::new(TruncationPolicy::Bytes(700), /*max_lines*/ None),
            OutputTruncation::new(TruncationPolicy::Bytes(10_000), Some(7)),
        ] {
            let spilled = SpilledToolOutput::for_test(
                Box::new(ItemTextSpill(FunctionToolOutput::from_content(
                    body.clone(),
                    Some(true),
                ))),
                step_truncation,
                function_payload(),
            );
            let expected = match spilled.to_response_item(CALL_ID, &function_payload()) {
                ResponseInputItem::FunctionCallOutput { output, .. } => output,
                other => panic!("unexpected item {other:?}"),
            };
            assert!(text_of(&expected).contains("Full output saved to: "));
            let recorded = recorded(
                result(Box::new(spilled)),
                step_truncation,
                /*replay*/ true,
            );
            assert_eq!(recorded, expected, "{step_truncation:?}");
        }
    }
}

/// R4.4: the wrapper reports the original output to logs, hooks, Code Mode and recording.
#[test]
fn the_wrapper_forwards_the_original() {
    let truncation = OutputTruncation::new(TruncationPolicy::Tokens(100), /*max_lines*/ None);
    let original = mcp_output(&numbered_lines(2000), truncation);
    let payload = function_payload();
    let spilled =
        SpilledToolOutput::for_test(Box::new(original.clone()), truncation, payload.clone());
    assert_eq!(spilled.log_output(), original.log_output());
    assert_eq!(
        spilled.success_for_logging(),
        original.success_for_logging()
    );
    assert_eq!(
        spilled.code_mode_result(&payload),
        original.code_mode_result(&payload)
    );
    assert_eq!(
        spilled.tool_result_metadata(),
        original.tool_result_metadata()
    );
    assert_eq!(
        spilled.post_tool_use_response(CALL_ID, &payload),
        original.post_tool_use_response(CALL_ID, &payload)
    );
    assert_eq!(
        spilled.post_tool_use_input(&payload),
        original.post_tool_use_input(&payload)
    );
    assert_eq!(spilled.spill_text(), None, "the hook runs once per call");
}

/// R2.4: a dynamic tool's output is eligible with the text of its items and otherwise behaves
/// as the `FunctionToolOutput` it wraps.
#[test]
fn dynamic_tool_output_is_eligible_and_unchanged() {
    let items = || {
        vec![
            FunctionCallOutputContentItem::InputText {
                text: "first".to_string(),
            },
            FunctionCallOutputContentItem::InputImage {
                image: ImageReference::Inline {
                    image_url: "data:image/png;base64,AAAA".to_string(),
                },
                detail: None,
            },
            FunctionCallOutputContentItem::InputText {
                text: String::new(),
            },
            FunctionCallOutputContentItem::InputText {
                text: "second".to_string(),
            },
        ]
    };
    let payload = function_payload();
    let plain = FunctionToolOutput::from_content(items(), Some(false));
    let wrapped = ItemTextSpill(FunctionToolOutput::from_content(items(), Some(false)));
    assert_eq!(wrapped.spill_text().as_deref(), Some("first\nsecond"));
    assert_eq!(
        plain.spill_text(),
        None,
        "apply_patch and agents stay ineligible"
    );
    assert_eq!(
        wrapped.to_response_item(CALL_ID, &payload),
        plain.to_response_item(CALL_ID, &payload)
    );
    assert_eq!(wrapped.log_output(), plain.log_output());
    assert_eq!(wrapped.success_for_logging(), plain.success_for_logging());
    assert_eq!(
        wrapped.post_tool_use_response(CALL_ID, &payload),
        plain.post_tool_use_response(CALL_ID, &payload)
    );
    assert_eq!(
        wrapped.code_mode_result(&payload),
        plain.code_mode_result(&payload)
    );
}

/// A session whose turn has the feature on, `CODEX_HOME` at `codex_home`, a 100-line history
/// limit and a Code Mode step that offers `exec_command` as a tool and inside Code Mode.
async fn spill_session(
    codex_home: &tempfile::TempDir,
    enabled: bool,
) -> (Arc<Session>, Arc<TurnContext>, Arc<StepContext>) {
    let (session, mut turn) = crate::session::tests::make_session_and_context().await;
    let mut config = (*turn.config).clone();
    config.codex_home = codex_home.abs();
    if enabled {
        config
            .features
            .enable(Feature::ToolOutputSpill)
            .expect("enable tool_output_spill");
    }
    config.output_truncation.max_lines = Some(100);
    turn.config = Arc::new(config);
    let turn = Arc::new(turn);
    let step = step_with_exec_command(&turn, /*direct*/ true, /*nested*/ true);
    (Arc::new(session), turn, step)
}

/// A step of `turn` that offers `exec_command` as a model tool (`direct`) and inside Code Mode
/// (`nested`).
fn step_with_exec_command(turn: &Arc<TurnContext>, direct: bool, nested: bool) -> Arc<StepContext> {
    let spec = ToolSpec::Function(codex_tools::ResponsesApiTool {
        name: "exec_command".to_string(),
        description: "Test tool.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: codex_tools::JsonSchema::default(),
        output_schema: None,
    });
    let tool_mode = match (direct, nested) {
        (_, false) => ToolMode::Direct,
        (true, true) => ToolMode::CodeMode,
        (false, true) => ToolMode::CodeModeOnly,
    };
    let code_mode_tool_names = if nested {
        BTreeMap::from([("exec_command".to_string(), ToolName::plain("exec_command"))])
    } else {
        BTreeMap::new()
    };
    let router = ToolRouter::from_parts(
        ToolRegistry::empty_for_test(),
        if direct { vec![spec] } else { Vec::new() },
        tool_mode,
        code_mode_tool_names,
        /*tool_namespaces_info*/ None,
        &[],
    );
    StepContext::for_test(Arc::clone(turn)).with_tool_router_for_test(Arc::new(router))
}

fn invocation(
    session: &Arc<Session>,
    turn: &Arc<TurnContext>,
    step: &Arc<StepContext>,
    source: ToolCallSource,
) -> ToolInvocation {
    ToolInvocation {
        session: Arc::clone(session),
        turn: Arc::clone(turn),
        step_context: Arc::clone(step),
        cancellation_token: tokio_util::sync::CancellationToken::new(),
        tracker: Arc::new(tokio::sync::Mutex::new(
            crate::turn_diff_tracker::TurnDiffTracker::new(),
        )),
        call_id: CALL_ID.to_string(),
        tool_name: ToolName::plain("exec_command"),
        source,
        payload: function_payload(),
        native_call: None,
        environments: Default::default(),
    }
}

fn spill_files(codex_home: &Path) -> Vec<PathBuf> {
    let root = codex_home.join(store::SPILL_DIR);
    let Ok(threads) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    threads
        .filter_map(Result::ok)
        .flat_map(|thread| std::fs::read_dir(thread.path()).into_iter().flatten())
        .filter_map(Result::ok)
        .map(|file| file.path())
        .collect()
}

/// R1.1, R1.3: a 5 000-line exec output under a 100-line limit is saved whole, and the text
/// before the notice is what history records without the feature.
#[tokio::test]
async fn truncated_output_is_spilled_and_the_preview_is_unchanged() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let full = numbered_lines(5000);
    let mut spilled = result(Box::new(exec_output(&full)));
    spill_truncated_output(
        &invocation(&session, &turn, &step, ToolCallSource::Direct),
        &mut spilled,
    )
    .await;

    let files = spill_files(home.path());
    let [file] = files.as_slice() else {
        panic!("expected one spill file, got {files:?}");
    };
    assert_eq!(std::fs::read_to_string(file)?, full);
    let step_truncation = step.output_truncation();
    assert_eq!(step_truncation.max_lines, Some(100));
    let with = text_of(&recorded(spilled, step_truncation, /*replay*/ true));
    let without = text_of(&recorded(
        result(Box::new(exec_output(&full))),
        step_truncation,
        /*replay*/ false,
    ));
    let (preview, notice) = split_notice(&with);
    assert_eq!(preview, format!("{without}\n\n"));
    let canonical = file.canonicalize()?;
    assert_eq!(
        notice,
        format!(
            "Full output saved to: {} (5000 lines, {} bytes, ~{} tokens). Read only the part you need with exec_command: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again.",
            canonical.display(),
            full.len(),
            approx_token_count(&full)
        )
    );
    assert!(canonical.starts_with(home.path().canonicalize()?.join("tool_outputs")));
    Ok(())
}

/// R1.2: an output that fits is left alone and nothing is written; so is any output while the
/// feature is off.
#[tokio::test]
async fn output_that_fits_is_not_wrapped() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let short = numbered_lines(20);
    let mut fits = result(Box::new(exec_output(&short)));
    spill_truncated_output(
        &invocation(&session, &turn, &step, ToolCallSource::Direct),
        &mut fits,
    )
    .await;
    assert_eq!(
        fits.result.to_response_item(CALL_ID, &function_payload()),
        exec_output(&short).to_response_item(CALL_ID, &function_payload())
    );
    assert_eq!(fits.result.history_truncation_override(), None);

    let off_home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&off_home, /*enabled*/ false).await;
    let long = numbered_lines(5000);
    let mut off = result(Box::new(exec_output(&long)));
    spill_truncated_output(
        &invocation(&session, &turn, &step, ToolCallSource::Direct),
        &mut off,
    )
    .await;
    assert_eq!(off.result.history_truncation_override(), None);
    assert_eq!(spill_files(home.path()), Vec::<PathBuf>::new());
    assert!(!off_home.path().join("tool_outputs").exists());
    Ok(())
}

fn nested_source() -> ToolCallSource {
    ToolCallSource::CodeMode {
        cell_id: "cell-1".to_string(),
        runtime_tool_call_id: "tool-1".to_string(),
    }
}

/// An exec output of `text` for a call with `max_output_tokens`.
fn exec_output_with_budget(text: &str, max_output_tokens: Option<usize>) -> ExecCommandToolOutput {
    ExecCommandToolOutput {
        max_output_tokens,
        ..exec_output(text)
    }
}

/// R7.1, R7.2, R7.4, R7.5: a nested `exec_command` whose own `max_output_tokens` cut the output is
/// saved whole; the script gets the same result with the notice after its output, and the rest
/// of the result, its history item and its logs stay the original's.
#[tokio::test]
async fn a_cut_nested_exec_result_is_spilled_for_the_script() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let full = numbered_lines(5000);
    let original = || exec_output_with_budget(&full, Some(100));
    let mut nested = result(Box::new(original()));
    spill_truncated_output(
        &invocation(&session, &turn, &step, nested_source()),
        &mut nested,
    )
    .await;

    let files = spill_files(home.path());
    let [file] = files.as_slice() else {
        panic!("expected one spill file, got {files:?}");
    };
    assert_eq!(std::fs::read_to_string(file)?, full);
    let payload = function_payload();
    let mut expected = original().code_mode_result(&payload);
    let cut = expected[SCRIPT_TEXT_FIELD]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("no output in {expected}"))?;
    assert!(cut.contains("truncated"), "{cut}");
    expected[SCRIPT_TEXT_FIELD] = JsonValue::String(format!(
        "{cut}\n\nFull output saved to: {} (5000 lines, {} bytes, ~{} tokens). Read only the part you need with tools.exec_command: rg -n 'PATTERN' to find it, then sed -n 'A,Bp' for a range no larger than the preview above; a larger read is cut again.",
        file.canonicalize()?.display(),
        full.len(),
        approx_token_count(&full)
    ));
    assert_eq!(nested.result.code_mode_result(&payload), expected);
    assert_eq!(
        nested.result.to_response_item(CALL_ID, &payload),
        original().to_response_item(CALL_ID, &payload)
    );
    assert_eq!(
        nested.result.history_truncation_override(),
        original().history_truncation_override()
    );
    assert_eq!(nested.result.log_output(), original().log_output());
    assert_eq!(
        nested.result.post_tool_use_response(CALL_ID, &payload),
        original().post_tool_use_response(CALL_ID, &payload)
    );
    assert_eq!(
        nested.result.spill_text(),
        None,
        "the hook runs once per call"
    );
    Ok(())
}

/// Builds a tool output of a text.
type OutputOf = fn(&str) -> Box<dyn ToolOutput>;

/// R7.2, R7.3, R7.6: nested results that reach the script whole are left alone and nothing is
/// written, however long: `exec_command` without `max_output_tokens` or within it, an MCP result
/// and a dynamic tool result; so is a cut `exec_command` result while the feature is off.
#[tokio::test]
async fn nested_results_the_script_gets_whole_are_not_spilled() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let long = numbered_lines(5000);
    let payload = function_payload();
    let outputs: [(&str, OutputOf); 4] = [
        ("exec without max_output_tokens", |text| {
            Box::new(exec_output_with_budget(
                text, /*max_output_tokens*/ None,
            ))
        }),
        ("exec within max_output_tokens", |text| {
            Box::new(exec_output_with_budget(text, Some(1_000_000)))
        }),
        ("mcp", |text| {
            Box::new(mcp_output(
                text,
                OutputTruncation::new(TruncationPolicy::Tokens(100), /*max_lines*/ None),
            ))
        }),
        ("dynamic tool", |text| {
            fork_dynamic_tool_output(FunctionToolOutput::from_text(text.to_string(), Some(true)))
        }),
    ];
    for (name, output) in outputs {
        assert!(output(&long).spill_text().is_some(), "{name} is eligible");
        let mut nested = result(output(&long));
        spill_truncated_output(
            &invocation(&session, &turn, &step, nested_source()),
            &mut nested,
        )
        .await;
        assert_eq!(
            nested.result.code_mode_result(&payload),
            output(&long).code_mode_result(&payload),
            "{name}"
        );
    }
    assert!(!home.path().join("tool_outputs").exists());

    let off_home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&off_home, /*enabled*/ false).await;
    let mut off = result(Box::new(exec_output_with_budget(&long, Some(100))));
    spill_truncated_output(
        &invocation(&session, &turn, &step, nested_source()),
        &mut off,
    )
    .await;
    assert_eq!(
        off.result.code_mode_result(&payload),
        exec_output_with_budget(&long, Some(100)).code_mode_result(&payload)
    );
    assert!(!off_home.path().join("tool_outputs").exists());
    Ok(())
}

/// R7.4: a notice that reaches a script names `tools.exec_command`; a direct notice names
/// `exec_command` when the model has it as a tool or there is no Code Mode, and
/// `tools.exec_command` when the step offers it only inside Code Mode.
#[tokio::test]
async fn the_notice_names_how_the_model_reaches_exec_command() {
    let (_, turn) = crate::session::tests::make_session_and_context().await;
    let turn = Arc::new(turn);
    for (direct, nested, direct_call, nested_call) in [
        (true, true, "exec_command", "tools.exec_command"),
        (false, true, "tools.exec_command", "tools.exec_command"),
        (true, false, "exec_command", "exec_command"),
    ] {
        let step = step_with_exec_command(&turn, direct, nested);
        assert_eq!(
            exec_command_reference(&step, &ToolCallSource::Direct),
            direct_call,
            "direct: {direct}, nested: {nested}"
        );
        assert_eq!(
            exec_command_reference(&step, &nested_source()),
            nested_call,
            "direct: {direct}, nested: {nested}"
        );
    }
}

/// R1.5: when the file cannot be written the output is delivered as without the feature.
#[tokio::test]
async fn a_failed_save_keeps_the_output() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    // A file where the spill directory should be.
    std::fs::write(home.path().join("tool_outputs"), "not a directory")?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let long = numbered_lines(5000);
    let mut kept = result(Box::new(exec_output(&long)));
    spill_truncated_output(
        &invocation(&session, &turn, &step, ToolCallSource::Direct),
        &mut kept,
    )
    .await;
    assert_eq!(kept.result.history_truncation_override(), None);
    assert_eq!(
        kept.result.to_response_item(CALL_ID, &function_payload()),
        exec_output(&long).to_response_item(CALL_ID, &function_payload())
    );
    Ok(())
}

/// R2.6: outputs that do not declare their full text stay as they are, however long.
#[tokio::test]
async fn ineligible_outputs_are_not_wrapped() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let long = numbered_lines(5000);
    let mut patch = result(Box::new(FunctionToolOutput::from_text(
        long.clone(),
        Some(true),
    )));
    spill_truncated_output(
        &invocation(&session, &turn, &step, ToolCallSource::Direct),
        &mut patch,
    )
    .await;
    assert_eq!(patch.result.history_truncation_override(), None);
    assert!(!home.path().join("tool_outputs").exists());
    Ok(())
}

/// A tool that returns a long exec output or fails with a long message.
struct LongOutputTool {
    fail: bool,
}

impl ToolExecutor<ToolInvocation> for LongOutputTool {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("long_output")
    }

    fn spec(&self) -> codex_tools::ToolSpec {
        codex_tools::ToolSpec::Function(codex_tools::ResponsesApiTool {
            name: "long_output".to_string(),
            description: "Test tool.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: codex_tools::JsonSchema::default(),
            output_schema: None,
        })
    }

    fn handle<'a>(&'a self, _invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        let fail = self.fail;
        Box::pin(async move {
            if fail {
                Err(FunctionCallError::RespondToModel(numbered_lines(5000)))
            } else {
                Ok(Box::new(exec_output(&numbered_lines(5000))) as Box<dyn ToolOutput>)
            }
        })
    }
}

impl CoreToolRuntime for LongOutputTool {}

/// R1.1 through dispatch, R1.6: the registry spills a long direct result, never a failure
/// response.
#[tokio::test]
async fn failure_responses_are_not_spilled() -> anyhow::Result<()> {
    for fail in [true, false] {
        let home = tempfile::tempdir()?;
        let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
        let registry = ToolRegistry::from_tools([
            Arc::new(LongOutputTool { fail }) as Arc<dyn CoreToolRuntime>
        ]);
        let mut call = invocation(&session, &turn, &step, ToolCallSource::Direct);
        call.tool_name = ToolName::plain("long_output");
        let dispatched = registry
            .dispatch_any_with_state(call, /*call_state*/ None)
            .await;
        if fail {
            assert!(matches!(
                dispatched,
                Err(FunctionCallError::RespondToModel(_))
            ));
            assert!(!home.path().join("tool_outputs").exists());
        } else {
            let dispatched = dispatched.map_err(|err| anyhow::anyhow!("{err}"))?;
            let text = match dispatched
                .result
                .to_response_item(CALL_ID, &function_payload())
            {
                ResponseInputItem::FunctionCallOutput { output, .. } => text_of(&output),
                other => panic!("unexpected item {other:?}"),
            };
            assert!(text.contains("Full output saved to: "), "{text}");
            assert_eq!(spill_files(home.path()).len(), 1);
        }
    }
    Ok(())
}

/// S-2 (review 2026-11-07): two long outputs of one call id in a thread are saved to two files,
/// and each notice names the file with its own output.
#[cfg(unix)]
#[tokio::test]
async fn a_repeated_call_id_gets_its_own_file() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (session, turn, step) = spill_session(&home, /*enabled*/ true).await;
    let mut notices = Vec::new();
    for marker in ["first", "second"] {
        let full = format!("{marker}\n{}", numbered_lines(5000));
        let mut spilled = result(Box::new(exec_output(&full)));
        spill_truncated_output(
            &invocation(&session, &turn, &step, ToolCallSource::Direct),
            &mut spilled,
        )
        .await;
        let text = text_of(&recorded(
            spilled,
            step.output_truncation(),
            /*replay*/ true,
        ));
        let (_, notice) = split_notice(&text);
        let path = notice
            .strip_prefix("Full output saved to: ")
            .and_then(|rest| rest.split(" (").next())
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("no path in {notice}"))?;
        notices.push((path, full));
    }
    assert_ne!(notices[0].0, notices[1].0);
    for (path, full) in &notices {
        assert_eq!(&std::fs::read_to_string(path)?, full);
    }
    assert_eq!(spill_files(home.path()).len(), 2);
    Ok(())
}

/// S-3 (review 2026-11-07): a starting session sweeps saved outputs without a spill and with
/// the feature off: expired files go, fresh ones stay.
#[cfg(unix)]
#[tokio::test]
async fn a_starting_session_sweeps_with_the_feature_off() -> anyhow::Result<()> {
    let home = tempfile::tempdir()?;
    let (_session, turn, _step) = spill_session(&home, /*enabled*/ false).await;
    let thread = home.path().join(store::SPILL_DIR).join("old-thread");
    crate::fork_private_fs::create_private_dir(&thread)?;
    let expired = thread.join("expired.txt");
    let fresh = thread.join("fresh.txt");
    std::fs::write(&expired, "secret")?;
    std::fs::write(&fresh, "recent")?;
    let max_age =
        std::time::Duration::from_secs(turn.config.tool_output_spill.max_age_days * 86_400);
    std::fs::File::options()
        .write(true)
        .open(&expired)?
        .set_modified(
            std::time::SystemTime::now() - max_age - std::time::Duration::from_secs(60),
        )?;

    if let Some(sweep) = sweep_on_session_start(&turn.config) {
        sweep.await?;
    }
    assert!(!expired.exists(), "older than max_age_days");
    assert!(fresh.exists());
    Ok(())
}
