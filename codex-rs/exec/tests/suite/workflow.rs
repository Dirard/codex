//! Fork: `codex exec` waits for workflow runs (`specs/dynamic-workflow-2026-10-25.md`, R7.5).
#![cfg(not(target_os = "windows"))]

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

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

const AGENT_MARKER: &str = "You are a workflow agent started by an orchestration script";
const NOTIFICATION_MARKER: &str = "<workflow_notification>";

/// Answers the parent and the workflow agent by request content.
#[derive(Clone, Default)]
struct Routed {
    served: Arc<Mutex<Vec<&'static str>>>,
    script: String,
    agent_delay: Duration,
}

fn message(id: &str, text: &str) -> String {
    responses::sse(vec![
        responses::ev_response_created(id),
        responses::ev_assistant_message(&format!("msg-{id}"), text),
        responses::ev_completed(id),
    ])
}

fn has_call_output(body: &str) -> bool {
    serde_json::from_str::<Value>(body).is_ok_and(|body| {
        body["input"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == "wf-1")
        })
    })
}

impl Respond for Routed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body).to_string();
        let (name, sse, delay) = if body.contains(AGENT_MARKER) {
            (
                "agent",
                message("resp-agent", "the agent answer"),
                self.agent_delay,
            )
        } else if body.contains(NOTIFICATION_MARKER) {
            (
                "notified",
                message("resp-notified", "FINAL: the workflow finished"),
                Duration::ZERO,
            )
        } else if has_call_output(&body) {
            (
                "after_tool",
                message("resp-after-tool", "Started; waiting for the notification."),
                Duration::ZERO,
            )
        } else {
            let arguments = json!({ "script": self.script }).to_string();
            (
                "start",
                responses::sse(vec![
                    responses::ev_response_created("resp-start"),
                    responses::ev_function_call("wf-1", "workflow", &arguments),
                    responses::ev_completed("resp-start"),
                ]),
                Duration::ZERO,
            )
        };
        self.served.lock().expect("served").push(name);
        responses::sse_response(sse).set_delay(delay)
    }
}

async fn run_exec(
    agent_delay: Duration,
    extra: &[&str],
) -> anyhow::Result<(String, Vec<&'static str>)> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    let routed = Routed {
        served: Arc::default(),
        script: "export const meta = { name: 'exec-run', description: 'd' };\nreturn await agent('TASK-EXEC');\n".to_string(),
        agent_delay,
    };
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(routed.clone())
        .mount(&server)
        .await;

    let mut cmd = test.cmd_with_server(&server);
    cmd.arg("--skip-git-repo-check")
        .arg("-C")
        .arg(test.cwd_path())
        .arg("-c")
        .arg("features.collab=true")
        .arg("-c")
        .arg("features.multi_agent_v2=true")
        .arg("-c")
        .arg("features.workflows=true");
    for arg in extra {
        cmd.arg("-c").arg(arg);
    }
    let output = cmd
        .arg("run the workflow")
        .timeout(Duration::from_secs(60))
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "stdout: {stdout}\nstderr: {stderr}"
    );
    let served = routed.served.lock().expect("served").clone();
    Ok((stdout, served))
}

/// R7.5: exec stays alive after its first turn, processes the turn started by the run's
/// notification and prints that turn's final message.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_waits_for_workflow_runs_and_prints_the_notification_turn() -> anyhow::Result<()> {
    let (stdout, served) = run_exec(Duration::from_millis(1500), &[]).await?;
    assert!(stdout.contains("FINAL: the workflow finished"), "{stdout}");
    assert_eq!(served, vec!["start", "after_tool", "agent", "notified"]);
    Ok(())
}

/// R7.5: after `workflows.exec_wait_timeout_ms` exec cancels the run and processes the turn
/// of its `cancelled` notification before exiting.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_cancels_runs_after_the_wait_timeout() -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let (stdout, served) = run_exec(
        Duration::from_secs(120),
        &["workflows.exec_wait_timeout_ms=1000"],
    )
    .await?;
    assert!(started.elapsed() < Duration::from_secs(60));
    assert!(stdout.contains("FINAL: the workflow finished"), "{stdout}");
    // The agent may be cancelled before its first request reaches the model.
    assert_eq!(&served[..2], &["start", "after_tool"]);
    assert_eq!(served.last(), Some(&"notified"));
    Ok(())
}
