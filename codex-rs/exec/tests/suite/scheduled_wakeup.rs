//! Fork: `codex exec` waits for scheduled wakeups (`specs/scheduled-wakeups-2026-11-07.md`,
//! UC7).
#![cfg(not(target_os = "windows"))]

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use core_test_support::responses;
use core_test_support::test_codex_exec::test_codex_exec;
use serde_json::json;
use wiremock::Mock;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

/// Answers by request content: schedule a wakeup, end the first turn, answer the wakeup turn.
#[derive(Clone)]
struct Routed {
    served: Arc<Mutex<Vec<&'static str>>>,
    delay_seconds: u64,
}

fn message(id: &str, text: &str) -> String {
    responses::sse(vec![
        responses::ev_response_created(id),
        responses::ev_assistant_message(&format!("msg-{id}"), text),
        responses::ev_completed(id),
    ])
}

impl Respond for Routed {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let text = String::from_utf8_lossy(&request.body).to_string();
        let (name, sse) = if text.contains("source=\\\"scheduled_wakeup\\\"") {
            ("woken", message("resp-woken", "FINAL: CI is green"))
        } else if text.contains("\"call_id\":\"wake-1\"") {
            (
                "after_tool",
                message("resp-after-tool", "I will check later."),
            )
        } else {
            let arguments = json!({
                "delay_seconds": self.delay_seconds,
                "prompt": "check CI",
                "reason": "CI still running",
            })
            .to_string();
            (
                "schedule",
                responses::sse(vec![
                    responses::ev_response_created("resp-schedule"),
                    responses::ev_function_call_with_namespace(
                        "wake-1",
                        "clock",
                        "schedule_wakeup",
                        &arguments,
                    ),
                    responses::ev_completed("resp-schedule"),
                ]),
            )
        };
        self.served.lock().expect("served").push(name);
        responses::sse_response(sse)
    }
}

struct ExecRun {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    served: Vec<&'static str>,
}

async fn run_exec(delay_seconds: u64, overrides: &[&str]) -> anyhow::Result<ExecRun> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    let routed = Routed {
        served: Arc::default(),
        delay_seconds,
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
    let output = cmd
        .arg("watch CI")
        .timeout(Duration::from_secs(60))
        .output()?;
    let served = routed.served.lock().expect("served").clone();
    Ok(ExecRun {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code(),
        served,
    })
}

/// R7.1, R7.2: exec prints the waiting line, processes the wakeup turn, prints its final
/// message and exits with 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_waits_for_the_wakeup_turn() -> anyhow::Result<()> {
    let run = run_exec(
        1,
        &[
            "features.scheduled_wakeups.enabled=true",
            "features.scheduled_wakeups.min_delay_seconds=1",
        ],
    )
    .await?;
    assert_eq!(
        run.code,
        Some(0),
        "stdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
    assert_eq!(run.served, vec!["schedule", "after_tool", "woken"]);
    assert!(
        run.stderr
            .contains("codex exec is waiting for a scheduled wakeup at"),
        "{}",
        run.stderr
    );
    assert!(run.stderr.contains("CI still running"), "{}", run.stderr);
    assert!(run.stdout.contains("FINAL: CI is green"), "{}", run.stdout);
    Ok(())
}

/// R7.3: after `exec_wait_timeout_ms` exec cancels the wakeup, warns and exits with 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_cancels_a_wakeup_after_its_wait_timeout() -> anyhow::Result<()> {
    let run = run_exec(
        30,
        &[
            "features.scheduled_wakeups.enabled=true",
            "features.scheduled_wakeups.min_delay_seconds=1",
            "features.scheduled_wakeups.exec_wait_timeout_ms=1000",
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
    assert_eq!(run.served, vec!["schedule", "after_tool"]);
    let output = format!("{}{}", run.stdout, run.stderr);
    assert!(
        output.contains("codex exec stopped waiting for the scheduled wakeup after"),
        "{output}"
    );
    Ok(())
}

/// R1.2, R7.5: without the feature the model has no wakeup tool and exec exits after the
/// first turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_without_the_feature_does_not_wait() -> anyhow::Result<()> {
    let run = run_exec(1, &[]).await?;
    assert!(
        !run.stderr.contains("waiting for a scheduled wakeup"),
        "{}",
        run.stderr
    );
    assert!(!run.served.contains(&"woken"), "{:?}", run.served);
    Ok(())
}
