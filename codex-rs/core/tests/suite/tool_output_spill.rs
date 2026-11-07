//! Fork: the full output of truncated tools (`specs/tool-output-spill-2026-11-07.md`,
//! feature `tool_output_spill`): what the model receives and what stays on disk.
#![cfg(not(target_os = "windows"))]

use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use codex_features::Feature;
use codex_protocol::models::PermissionProfile;
use core_test_support::responses::ResponseMock;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::skip_if_remote;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use regex_lite::Regex;
use serde_json::json;
use wiremock::MockServer;

const NOTICE_PREFIX: &str = "Full output saved to: ";

/// A builder with `tool_output_spill` on or off and the given output limits.
fn builder(
    enabled: bool,
    token_limit: Option<usize>,
    max_lines: Option<usize>,
) -> TestCodexBuilder {
    test_codex()
        .with_model("gpt-5.5")
        .with_config(move |config| {
            if enabled {
                config
                    .features
                    .enable(Feature::ToolOutputSpill)
                    .expect("enable tool_output_spill");
            }
            config.tool_output_token_limit = token_limit;
            config.output_truncation.max_lines = max_lines;
        })
}

/// Mounts one model step that calls `exec_command` with `cmd`, and the final answer after it;
/// returns the mock of the answer step, which receives the tool output.
async fn mount_exec(server: &MockServer, call_id: &str, cmd: &str, step: usize) -> ResponseMock {
    let args = json!({ "cmd": cmd, "yield_time_ms": 10_000 });
    mount_sse_once(
        server,
        sse(vec![
            ev_response_created(&format!("resp-{step}-call")),
            ev_function_call(call_id, "exec_command", &args.to_string()),
            ev_completed(&format!("resp-{step}-call")),
        ]),
    )
    .await;
    mount_sse_once(
        server,
        sse(vec![
            ev_assistant_message(&format!("msg-{step}"), "done"),
            ev_completed(&format!("resp-{step}-done")),
        ]),
    )
    .await
}

/// Runs one turn in which the model calls `exec_command` with `cmd`; returns the text of the
/// tool output the next request carried.
async fn run_exec(
    fixture: &TestCodex,
    server: &MockServer,
    call_id: &str,
    cmd: &str,
) -> Result<String> {
    let answer = mount_exec(server, call_id, cmd, 1).await;
    fixture
        .submit_turn_with_permission_profile("run the command", PermissionProfile::Disabled)
        .await?;
    answer
        .single_request()
        .function_call_output_text(call_id)
        .context("function_call_output for the exec call")
}

/// The path the notice names, if the output has one.
fn notice_path(output: &str) -> Option<PathBuf> {
    let notice = &output[output.rfind(NOTICE_PREFIX)? + NOTICE_PREFIX.len()..];
    let end = notice.find(" (")?;
    Some(PathBuf::from(&notice[..end]))
}

fn spill_dir(fixture: &TestCodex) -> PathBuf {
    fixture.codex_home_path().join("tool_outputs")
}

fn seq_output(count: usize) -> String {
    (1..=count).map(|line| format!("{line}\n")).collect()
}

/// R1.1, R1.3, R3.1: a long `exec_command` output reaches the model as the usual preview plus the
/// notice, and the file it names holds the whole output.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exec_command_long_output_is_saved_and_named_in_model_input() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill file is written on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(/*enabled*/ true, Some(1_000), /*max_lines*/ None)
        .build(&server)
        .await?;
    let output = run_exec(&fixture, &server, "spill-exec", "seq 1 5000").await?;

    let path = notice_path(&output).with_context(|| format!("no notice in {output}"))?;
    let thread_dir = spill_dir(&fixture)
        .canonicalize()?
        .join(fixture.session_configured.thread_id.to_string());
    assert_eq!(path.parent(), Some(thread_dir.as_path()));
    assert_eq!(std::fs::read_to_string(&path)?, seq_output(5000));
    let notice = &output[output.rfind(NOTICE_PREFIX).unwrap_or_default()..];
    assert!(
        notice.contains("(5000 lines, 23893 bytes, ~5974 tokens). Read the omitted part with exec_command, e.g. sed -n 'A,Bp' or rg -n 'PATTERN' on that file."),
        "{notice}"
    );
    assert!(output.contains("tokens truncated"), "{output}");
    assert!(output.ends_with(notice));
    assert!(output.contains("\n\nFull output saved to: "));
    Ok(())
}

/// R1.1: an output that fits the token budget but not `[output_truncation] max_lines` is cut by
/// history alone, and is saved too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn line_limit_cut_is_saved() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill file is written on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(/*enabled*/ true, /*token_limit*/ None, Some(50))
        .build(&server)
        .await?;
    let output = run_exec(&fixture, &server, "spill-lines", "seq 1 200").await?;
    assert!(output.contains("lines truncated"), "{output}");
    let path = notice_path(&output).with_context(|| format!("no notice in {output}"))?;
    assert_eq!(std::fs::read_to_string(path)?, seq_output(200));
    Ok(())
}

/// Replaces the varying header values of an exec output.
fn normalized(output: &str) -> String {
    let wall_time = Regex::new(r"Wall time: [0-9.]+ seconds").expect("regex");
    let chunk = Regex::new(r"Chunk ID: [^\n]+").expect("regex");
    let output = wall_time.replace_all(output, "Wall time: N seconds");
    chunk.replace_all(&output, "Chunk ID: N").into_owned()
}

/// R1.2: with the feature off, the model input is what it is without the table, nothing is
/// written, and a short output is never touched while the feature is on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn feature_off_keeps_requests_identical() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let off = builder(/*enabled*/ false, Some(1_000), /*max_lines*/ None)
        .build(&server)
        .await?;
    let off_output = run_exec(&off, &server, "spill-off", "seq 1 5000").await?;
    assert!(!off_output.contains(NOTICE_PREFIX), "{off_output}");
    assert!(!spill_dir(&off).exists());

    let server = start_mock_server().await;
    let without_table = test_codex()
        .with_model("gpt-5.5")
        .with_config(|config| config.tool_output_token_limit = Some(1_000))
        .build(&server)
        .await?;
    let plain_output = run_exec(&without_table, &server, "spill-off", "seq 1 5000").await?;
    assert_eq!(normalized(&off_output), normalized(&plain_output));

    let server = start_mock_server().await;
    let on = builder(/*enabled*/ true, Some(1_000), /*max_lines*/ None)
        .build(&server)
        .await?;
    let short = run_exec(&on, &server, "spill-short", "seq 1 20").await?;
    assert!(!short.contains(NOTICE_PREFIX), "{short}");
    assert!(!spill_dir(&on).exists());
    Ok(())
}

/// R3.1, R3.4: the model reads the omitted middle with `sed -n` on the path from the notice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_reads_the_omitted_part_with_sed() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill file is written on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(/*enabled*/ true, Some(1_000), /*max_lines*/ None)
        .build(&server)
        .await?;
    let output = run_exec(&fixture, &server, "spill-long", "seq 1 5000").await?;
    assert!(!output.contains("\n2500\n"), "the middle was cut");
    let path = notice_path(&output).with_context(|| format!("no notice in {output}"))?;

    let read = run_exec(
        &fixture,
        &server,
        "spill-read",
        &format!("sed -n '2500,2501p' '{}'", path.display()),
    )
    .await?;
    assert!(read.ends_with("Output:\n2500\n2501\n"), "{read}");
    Ok(())
}

/// The rollout lines of `fixture`'s thread.
async fn rollout_text(fixture: &TestCodex) -> Result<String> {
    fixture.codex.ensure_rollout_materialized().await;
    fixture.codex.flush_rollout().await?;
    let path = fixture
        .session_configured
        .rollout_path
        .clone()
        .context("rollout path")?;
    Ok(tokio::fs::read_to_string(path).await?)
}

/// R3.7, R4.5: the rollout keeps the preview with the notice and a no-op policy, never the saved
/// middle of the output.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollout_stores_the_preview_without_the_saved_text() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill file is written on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(/*enabled*/ true, Some(1_000), /*max_lines*/ None)
        .build(&server)
        .await?;
    let output = run_exec(&fixture, &server, "spill-rollout", "seq 1 5000").await?;
    let rollout = rollout_text(&fixture).await?;
    let line = rollout
        .lines()
        .find(|line| line.contains("\"function_call_output\"") && line.contains("spill-rollout"))
        .context("rollout line of the tool output")?;
    let item: serde_json::Value = serde_json::from_str(line)?;
    assert_eq!(item["payload"]["output"], json!(output));
    assert!(!line.contains("\\n2500\\n"), "{line}");
    assert_eq!(
        item["metadata"]["history_truncation_max_lines"],
        json!(null)
    );
    assert!(
        item["metadata"]["history_truncation_policy"].is_object(),
        "{line}"
    );
    Ok(())
}

/// R4.3, R5.6: a resumed thread sends the same preview and notice under another output limit,
/// and a notice whose file was removed stays as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_replays_the_notice_unchanged() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill file is written on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(/*enabled*/ true, Some(1_000), /*max_lines*/ None)
        .build(&server)
        .await?;
    let output = run_exec(&fixture, &server, "spill-resume", "seq 1 5000").await?;
    let path = notice_path(&output).context("notice")?;
    rollout_text(&fixture).await?;
    std::fs::remove_file(&path)?;

    let resumed_answer = mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-resumed", "resumed"),
            ev_completed("resp-resumed"),
        ]),
    )
    .await;
    let mut resume_builder = builder(/*enabled*/ false, Some(50), Some(3));
    let resumed = resume_builder.restart(&server, &fixture).await?;
    resumed.submit_turn("continue").await?;
    assert_eq!(
        resumed_answer
            .single_request()
            .function_call_output_text("spill-resume")
            .context("resumed tool output")?,
        output
    );
    Ok(())
}

/// R5.5: an ephemeral thread removes its saved outputs when it shuts down.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ephemeral_thread_removes_its_outputs_on_shutdown() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill file is written on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(/*enabled*/ true, Some(1_000), /*max_lines*/ None)
        .with_config(|config| config.ephemeral = true)
        .build(&server)
        .await?;
    let output = run_exec(&fixture, &server, "spill-ephemeral", "seq 1 5000").await?;
    let path = notice_path(&output).context("notice")?;
    assert!(path.exists());
    let thread_dir = path.parent().context("thread dir")?.to_path_buf();
    fixture.codex.shutdown_and_wait().await?;
    assert!(!thread_dir.exists(), "{}", thread_dir.display());
    Ok(())
}

/// S-3 (review 2026-11-07): a starting session sweeps saved outputs older than `max_age_days`
/// with the feature off and before any spill; younger files stay.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_start_sweeps_old_outputs_with_the_feature_off() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_remote!(Ok(()), "the spill files live on the Codex host");
    let server = start_mock_server().await;
    let fixture = builder(
        /*enabled*/ false, /*token_limit*/ None, /*max_lines*/ None,
    )
    .with_pre_build_hook(|home| {
        let thread = home.join("tool_outputs").join("old-thread");
        std::fs::create_dir_all(&thread).expect("create a thread directory");
        std::fs::write(thread.join("expired.txt"), "secret").expect("write expired");
        std::fs::write(thread.join("fresh.txt"), "recent").expect("write fresh");
        std::fs::File::options()
            .write(true)
            .open(thread.join("expired.txt"))
            .and_then(|file| {
                file.set_modified(
                    std::time::SystemTime::now() - std::time::Duration::from_secs(8 * 24 * 60 * 60),
                )
            })
            .expect("age the expired file");
    })
    .build(&server)
    .await?;
    let thread = spill_dir(&fixture).join("old-thread");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while thread.join("expired.txt").exists() && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(
        !thread.join("expired.txt").exists(),
        "older than max_age_days"
    );
    assert!(thread.join("fresh.txt").exists());
    Ok(())
}
