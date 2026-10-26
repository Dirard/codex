//! Fork: `codex exec` waits for background terminal notifications
//! (`specs/background-exec-notifications-2026-10-25.md`, UC7).
#![cfg(not(target_os = "windows"))]

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use core_test_support::responses;
use core_test_support::test_codex_exec::test_codex_exec;
use serde_json::Value;
use serde_json::json;
use wiremock::Mock;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

const NOTIFICATION_MARKER: &str = "<background_terminal_notification>";

/// Answers by request content: start a background command, end the first turn, answer
/// the turn its exit notification starts.
#[derive(Clone)]
struct Routed {
    served: Arc<Mutex<Vec<&'static str>>>,
    bodies: Arc<Mutex<Vec<Value>>>,
    command: String,
}

fn message(id: &str, text: &str) -> String {
    responses::sse(vec![
        responses::ev_response_created(id),
        responses::ev_assistant_message(&format!("msg-{id}"), text),
        responses::ev_completed(id),
    ])
}

fn has_call_output(body: &Value) -> bool {
    body["input"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["type"] == "function_call_output" && item["call_id"] == "bg-1")
    })
}

impl Respond for Routed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let text = String::from_utf8_lossy(&request.body).to_string();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let (name, sse) = if text.contains(NOTIFICATION_MARKER) {
            (
                "notified",
                message("resp-notified", "FINAL: the build printed built-ok"),
            )
        } else if has_call_output(&body) {
            (
                "after_tool",
                message("resp-after-tool", "Started; I will report when it exits."),
            )
        } else {
            let arguments = json!({
                "cmd": self.command,
                "yield_time_ms": 250,
                "login": false,
                "notify_on_exit": true,
            })
            .to_string();
            (
                "start",
                responses::sse(vec![
                    responses::ev_response_created("resp-start"),
                    responses::ev_function_call("bg-1", "exec_command", &arguments),
                    responses::ev_completed("resp-start"),
                ]),
            )
        };
        self.served.lock().expect("served").push(name);
        self.bodies.lock().expect("bodies").push(body);
        responses::sse_response(sse)
    }
}

struct ExecRun {
    stdout: String,
    stderr: String,
    success: bool,
    code: Option<i32>,
    elapsed: Duration,
    served: Vec<&'static str>,
    bodies: Vec<Value>,
}

async fn run_exec(command: &str, overrides: &[&str]) -> anyhow::Result<ExecRun> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    let routed = Routed {
        served: Arc::default(),
        bodies: Arc::default(),
        command: command.to_string(),
    };
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(routed.clone())
        .mount(&server)
        .await;

    let mut cmd = test.cmd_with_server(&server);
    cmd.arg("--skip-git-repo-check")
        .arg("-C")
        .arg(test.cwd_path());
    for override_arg in overrides {
        cmd.arg("-c").arg(override_arg);
    }
    let started = Instant::now();
    let output = cmd
        .arg("build in the background")
        .timeout(Duration::from_secs(60))
        .output()?;
    let served = routed.served.lock().expect("served").clone();
    let bodies = routed.bodies.lock().expect("bodies").clone();
    Ok(ExecRun {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        success: output.status.success(),
        code: output.status.code(),
        elapsed: started.elapsed(),
        served,
        bodies,
    })
}

/// The first request as text. It carries no history yet, so a notification parameter in it
/// can only come from the tool definitions, wherever the request puts them.
fn first_request_text(bodies: &[Value]) -> String {
    bodies.first().map(Value::to_string).unwrap_or_default()
}

/// R7.1, R7.4: exec stays alive after its first turn, processes the turn the exit
/// notification starts and prints that turn's final message.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_waits_for_a_background_terminal_and_prints_the_notification_turn()
-> anyhow::Result<()> {
    let run = run_exec(
        "sleep 2; echo built-ok",
        &["features.exec_exit_notifications=true"],
    )
    .await?;
    assert!(
        run.success,
        "stdout: {}\nstderr: {}",
        run.stdout, run.stderr
    );
    assert!(
        run.stdout.contains("FINAL: the build printed built-ok"),
        "stdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
    assert_eq!(run.served, vec!["start", "after_tool", "notified"]);
    let notified = run.bodies.last().expect("notified body").to_string();
    assert!(notified.contains("exited with code 0"), "{notified}");
    assert!(notified.contains("built-ok"), "{notified}");
    // Exec now offers the notification parameters.
    let first = first_request_text(&run.bodies);
    assert!(first.contains("notify_on_exit"), "{first}");
    assert!(first.contains("notify_on_output"), "{first}");
    Ok(())
}

/// R7.3: after `exec_wait_timeout_ms` exec names the terminal it did not wait for, stops
/// it and exits with code 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_gives_up_on_background_terminals_after_the_timeout() -> anyhow::Result<()> {
    let run = run_exec(
        "sleep 60; echo never",
        &[
            "features.exec_exit_notifications.enabled=true",
            "features.exec_exit_notifications.exec_wait_timeout_ms=1500",
        ],
    )
    .await?;
    assert_eq!(
        run.code,
        Some(1),
        "stdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
    assert!(run.elapsed < Duration::from_secs(30), "{:?}", run.elapsed);
    // Exec runs without deterministic session IDs.
    assert!(
        run.stderr
            .contains("codex exec stopped waiting for background terminals after ")
            && run.stderr.contains("still pending: session ")
            && run.stderr.contains(" (sleep 60; echo never, running for ")
            && run
                .stderr
                .contains("They are terminated now and their exits are not reported."),
        "stderr: {}",
        run.stderr
    );
    assert_eq!(run.served, vec!["start", "after_tool"]);
    Ok(())
}

/// R7.5, R1.4: without the feature exec offers no notifications and ends after the turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_without_the_feature_ends_after_the_first_turn() -> anyhow::Result<()> {
    let run = run_exec("sleep 60; echo never", &[]).await?;
    assert!(
        run.success,
        "stdout: {}\nstderr: {}",
        run.stdout, run.stderr
    );
    assert!(run.elapsed < Duration::from_secs(30), "{:?}", run.elapsed);
    assert_eq!(run.served, vec!["start", "after_tool"]);
    let first = first_request_text(&run.bodies);
    assert!(first.contains("exec_command"), "{first}");
    assert!(!first.contains("notify_on_exit"), "{first}");
    assert!(!first.contains("notify_on_output"), "{first}");
    assert!(!run.stderr.contains("stopped waiting"), "{}", run.stderr);
    Ok(())
}
