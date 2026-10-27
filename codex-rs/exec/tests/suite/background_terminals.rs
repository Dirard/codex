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
    /// Arguments of the first turn's `exec_command`.
    arguments: Value,
    /// How long the answer that ends the first turn takes, so the turn keeps running after
    /// the background command started.
    after_tool_delay: Duration,
}

/// `exec_command` arguments that start `command` with an exit notification.
fn notify_on_exit(command: &str) -> Value {
    json!({
        "cmd": command,
        "yield_time_ms": 250,
        "login": false,
        "notify_on_exit": true,
    })
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
            let arguments = self.arguments.to_string();
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
        let delay = if name == "after_tool" {
            self.after_tool_delay
        } else {
            Duration::ZERO
        };
        responses::sse_response(sse).set_delay(delay)
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
    /// The thread's rollout files under `$CODEX_HOME/sessions`, joined.
    rollout: String,
}

/// The text of every rollout file under `home`.
fn rollouts(home: &std::path::Path) -> String {
    fn collect(dir: &std::path::Path, texts: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, texts);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "jsonl")
            {
                texts.push(std::fs::read_to_string(&path).unwrap_or_default());
            }
        }
    }
    let mut texts = Vec::new();
    collect(&home.join("sessions"), &mut texts);
    texts.join("\n")
}

async fn run_exec(command: &str, overrides: &[&str]) -> anyhow::Result<ExecRun> {
    run_exec_with(notify_on_exit(command), overrides).await
}

async fn run_exec_with(arguments: Value, overrides: &[&str]) -> anyhow::Result<ExecRun> {
    run_exec_routed(arguments, Duration::ZERO, overrides).await
}

async fn run_exec_routed(
    arguments: Value,
    after_tool_delay: Duration,
    overrides: &[&str],
) -> anyhow::Result<ExecRun> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    let routed = Routed {
        served: Arc::default(),
        bodies: Arc::default(),
        arguments,
        after_tool_delay,
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
        rollout: rollouts(test.home_path()),
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

/// Review 2026-10-27 P2-3: a dev server under an output watch does not hold exec until the
/// timeout. Once the watch printed no matching line for `output_watch_quiet_ms`, nothing
/// waits for it: exec names the running terminal, stops it and exits with code 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_does_not_wait_for_a_quiet_output_watch() -> anyhow::Result<()> {
    let command = "echo starting; sleep 1; echo 'server ready'; sleep 60";
    let run = run_exec_with(
        json!({
            "cmd": command,
            "yield_time_ms": 250,
            "login": false,
            "notify_on_output": "ready",
        }),
        &[
            "features.exec_exit_notifications.enabled=true",
            "features.exec_exit_notifications.exec_wait_timeout_ms=20000",
            "features.exec_exit_notifications.output_watch_quiet_ms=1500",
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
    assert!(run.elapsed < Duration::from_secs(15), "{:?}", run.elapsed);
    // The "ready" notification still starts its turn while the watch is fresh.
    assert_eq!(run.served, vec!["start", "after_tool", "notified"]);
    assert!(
        run.stderr.contains(
            "codex exec is exiting while background terminals still run; they are terminated now: session "
        ) && run.stderr.contains(&format!("({command})")),
        "stderr: {}",
        run.stderr
    );
    assert!(!run.stderr.contains("stopped waiting"), "{}", run.stderr);
    Ok(())
}

/// Tails 2026-10-27: a server that prints "ready" later than `output_watch_quiet_ms` is still
/// waited for. Before its first matching line a watch waits `output_watch_first_match_ms`, and
/// from the end of the turn that started it: here the turn goes on for 4 s after the server
/// started, the window is 4 s, and "ready" comes 6 s after the start, so neither a quiet
/// window nor a first-match window counted from the watch start would see it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_waits_for_the_first_match_of_a_slow_server() -> anyhow::Result<()> {
    let command = "echo starting; sleep 6; echo 'server ready'; sleep 60";
    let run = run_exec_routed(
        json!({
            "cmd": command,
            "yield_time_ms": 250,
            "login": false,
            "notify_on_output": "ready",
        }),
        Duration::from_secs(4),
        &[
            "features.exec_exit_notifications.enabled=true",
            "features.exec_exit_notifications.exec_wait_timeout_ms=40000",
            "features.exec_exit_notifications.output_watch_quiet_ms=1000",
            "features.exec_exit_notifications.output_watch_first_match_ms=4000",
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
    assert_eq!(
        run.served,
        vec!["start", "after_tool", "notified"],
        "stderr: {}",
        run.stderr
    );
    assert!(
        run.stdout.contains("FINAL: the build printed built-ok"),
        "stdout: {}",
        run.stdout
    );
    assert!(run.elapsed < Duration::from_secs(30), "{:?}", run.elapsed);
    Ok(())
}

/// Starts `codex exec` with a background terminal, sends Ctrl-C once the request that ends
/// its first turn reached the model plus `pause`, and returns the exit code and stderr. With
/// a long `after_tool_delay` the Ctrl-C lands while that turn still runs.
async fn interrupt_exec(
    after_tool_delay: Duration,
    pause: Duration,
) -> anyhow::Result<(Option<i32>, String)> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    let routed = Routed {
        served: Arc::default(),
        bodies: Arc::default(),
        arguments: notify_on_exit("sleep 60; echo never"),
        after_tool_delay,
    };
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(routed.clone())
        .mount(&server)
        .await;
    let mut child = std::process::Command::new(codex_utils_cargo_bin::cargo_bin("codex-exec")?)
        .current_dir(test.cwd_path())
        .env("CODEX_HOME", test.home_path())
        .env("CODEX_SQLITE_HOME", test.home_path())
        .env("CODEX_API_KEY", "dummy")
        .arg("-c")
        .arg(format!(
            "openai_base_url={}",
            serde_json::to_string(&format!("{}/v1", server.uri()))?
        ))
        .args([
            "-c",
            "features.exec_exit_notifications=true",
            "--skip-git-repo-check",
            "build in the background",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let waited = tokio::time::timeout(Duration::from_secs(30), async {
        while !routed
            .served
            .lock()
            .expect("served")
            .contains(&"after_tool")
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if waited.is_err() {
        let _ = child.kill();
        anyhow::bail!("the first turn did not reach its last step");
    }
    tokio::time::sleep(pause).await;
    let pid = libc::pid_t::try_from(child.id())?;
    // SAFETY: `pid` is our own child process.
    assert_eq!(unsafe { libc::kill(pid, libc::SIGINT) }, 0);
    let output = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || child.wait_with_output()),
    )
    .await???;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    ))
}

/// Review 2026-10-27 P3-5: Ctrl-C while exec waits for a background terminal names the
/// terminal it leaves and exits with an error, as the timeout does.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_interrupted_while_waiting_names_the_terminal_and_fails() -> anyhow::Result<()> {
    // The first turn ended; exec now waits for the terminal.
    let (code, stderr) = interrupt_exec(Duration::ZERO, Duration::from_millis(1500)).await?;
    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains(
            "codex exec was interrupted while it waited for background terminals; still pending: session "
        ) && stderr.contains(" (sleep 60; echo never, running for "),
        "stderr: {stderr}"
    );
    Ok(())
}

/// Combined live run 2026-10-27, D5: Ctrl-C during the first turn interrupts that turn; exec
/// was not waiting for terminals yet, and its warning says so while it names them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_interrupted_during_a_turn_names_the_terminal_and_fails() -> anyhow::Result<()> {
    let (code, stderr) =
        interrupt_exec(Duration::from_secs(60), Duration::from_millis(1000)).await?;
    assert_eq!(code, Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains(
            "codex exec was interrupted during a turn; background terminals still pending: session "
        ) && stderr.contains(" (sleep 60; echo never, running for "),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("while it waited"), "stderr: {stderr}");
    Ok(())
}

/// Live recheck 2026-10-27, N1: a dev server started with an output watch and `notify_on_exit`.
/// After "ready" the model answers the notification and, with `disarm`, turns the exit
/// notification off with `write_stdin` before it says it left the server running.
#[derive(Clone)]
struct DevServer {
    served: Arc<Mutex<Vec<&'static str>>>,
    bodies: Arc<Mutex<Vec<Value>>>,
    /// The server is started with `notify_on_exit`.
    notify_on_exit: bool,
    /// `write_stdin` arguments, besides the session, that the model sends after "ready".
    follow_up: Option<Value>,
}

const DEV_SERVER: &str = "echo starting; sleep 1; echo 'server ready'; sleep 60";

impl DevServer {
    fn new(notify_on_exit: bool, follow_up: Option<Value>) -> Self {
        Self {
            served: Arc::default(),
            bodies: Arc::default(),
            notify_on_exit,
            follow_up,
        }
    }

    /// The model turns the exit notification off after "ready".
    fn disarming() -> Self {
        Self::new(true, Some(json!({ "notify_on_exit": false })))
    }
}

/// The session ID that a background terminal notification in `text` names.
fn notified_session_id(text: &str) -> i64 {
    let after = text
        .split("Background terminal session ")
        .nth(1)
        .expect("a background terminal notification");
    after
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .expect("session ID")
}

fn has_output_of(body: &Value, call_id: &str) -> bool {
    body["input"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
    })
}

impl Respond for DevServer {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let text = String::from_utf8_lossy(&request.body).to_string();
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let (name, sse) = if has_output_of(&body, "disarm-1") {
            (
                "after_disarm",
                message("resp-disarmed", "FINAL: left it running"),
            )
        } else if text.contains(NOTIFICATION_MARKER) {
            if let Some(follow_up) = &self.follow_up {
                let mut arguments = json!({
                    "session_id": notified_session_id(&text),
                    "chars": "",
                    "yield_time_ms": 250,
                });
                for (key, value) in follow_up.as_object().into_iter().flatten() {
                    arguments[key] = value.clone();
                }
                let arguments = arguments.to_string();
                (
                    "notified",
                    responses::sse(vec![
                        responses::ev_response_created("resp-notified"),
                        responses::ev_function_call("disarm-1", "write_stdin", &arguments),
                        responses::ev_completed("resp-notified"),
                    ]),
                )
            } else {
                (
                    "notified",
                    message("resp-notified", "FINAL: ready, left it running"),
                )
            }
        } else if has_output_of(&body, "bg-1") {
            (
                "after_tool",
                message("resp-after-tool", "Started the server."),
            )
        } else {
            let arguments = json!({
                "cmd": DEV_SERVER,
                "yield_time_ms": 250,
                "login": false,
                "notify_on_output": "ready",
                "notify_on_exit": self.notify_on_exit,
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

async fn run_dev_server(responder: DevServer, overrides: &[&str]) -> anyhow::Result<ExecRun> {
    let test = test_codex_exec();
    let server = responses::start_mock_server().await;
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(responder.clone())
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
        .arg("start the dev server and tell me when it is ready")
        .timeout(Duration::from_secs(60))
        .output()?;
    let served = responder.served.lock().expect("served").clone();
    let bodies = responder.bodies.lock().expect("bodies").clone();
    Ok(ExecRun {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        success: output.status.success(),
        code: output.status.code(),
        elapsed: started.elapsed(),
        served,
        bodies,
        rollout: rollouts(test.home_path()),
    })
}

/// Live recheck 2026-10-27, N1 (b), N3: exec says after each turn what it waits for and how
/// long, and the "ready" notification of a server whose exit notification is on says that
/// codex exec waits for the exit, how to turn that off, and how to stop the server.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_says_what_it_waits_for_and_the_match_says_how_to_stop_waiting() -> anyhow::Result<()>
{
    let run = run_dev_server(
        DevServer::new(/*notify_on_exit*/ true, /*follow_up*/ None),
        &[
            "features.exec_exit_notifications.enabled=true",
            "features.exec_exit_notifications.exec_wait_timeout_ms=4000",
            "features.exec_exit_notifications.output_watch_quiet_ms=1000",
        ],
    )
    .await?;
    // Nothing turns the exit notification off, so the wait runs out.
    assert_eq!(
        run.code,
        Some(1),
        "stdout: {}\nstderr: {}",
        run.stdout,
        run.stderr
    );
    assert_eq!(run.served, vec!["start", "after_tool", "notified"]);
    let waiting =
        format!(" ({DEV_SERVER}): exit notification armed, output watch \"ready\"; up to ");
    assert!(
        run.stderr.contains("codex exec is waiting for session ")
            && run.stderr.contains(&format!("{waiting}4s; Ctrl-C to stop"))
            && run.stderr.matches(&waiting).count() == 2,
        "stderr: {}",
        run.stderr
    );
    let notified = input_texts(run.bodies.last().expect("notified body"));
    for expected in [
        "codex exec for up to 4 seconds",
        r#"write_stdin {"session_id": "#,
        r#""notify_on_exit": false}"#,
        r#"write_stdin with chars "\u0003" (Ctrl-C)"#,
        "do not kill it by name",
    ] {
        assert!(notified.contains(expected), "{expected}: {notified}");
    }
    Ok(())
}

/// The texts of the request's input items, one per line.
fn input_texts(body: &Value) -> String {
    body["input"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|item| {
            let mut texts = Vec::new();
            if let Some(output) = item["output"].as_str() {
                texts.push(output.to_string());
            }
            for content in item["content"].as_array().into_iter().flatten() {
                if let Some(text) = content["text"].as_str() {
                    texts.push(text.to_string());
                }
            }
            texts
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Live recheck 2026-10-27, N1 (a): once the model turns the exit notification of a server it
/// leaves running off with `write_stdin` `notify_on_exit: false`, exec no longer waits for the
/// exit: it stops waiting when the output watch goes quiet and exits with code 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_stops_waiting_once_the_model_turns_the_exit_notification_off() -> anyhow::Result<()> {
    let run = run_dev_server(
        DevServer::disarming(),
        &[
            "features.exec_exit_notifications.enabled=true",
            "features.exec_exit_notifications.exec_wait_timeout_ms=40000",
            "features.exec_exit_notifications.output_watch_quiet_ms=1000",
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
    assert_eq!(
        run.served,
        vec!["start", "after_tool", "notified", "after_disarm"]
    );
    assert!(
        run.stdout.contains("FINAL: left it running"),
        "{}",
        run.stdout
    );
    assert!(run.elapsed < Duration::from_secs(30), "{:?}", run.elapsed);
    let disarmed = input_texts(run.bodies.last().expect("body after the disarm"));
    assert!(
        disarmed.contains("Exit notification: off (codex exec and wait_agent no longer wait for this process to exit"),
        "{disarmed}"
    );
    assert!(
        run.stderr.contains(
            "codex exec is exiting while background terminals still run; they are terminated now: session "
        ),
        "stderr: {}",
        run.stderr
    );
    Ok(())
}

/// Live recheck 2 R3, R2: after "ready" the model turns the watch into an error watch. The
/// server is up, so the new watch is awaited only while it would keep matching, as after a
/// match, not for a first match: exec exits after the quiet window, not the 45 s first-match
/// window. Exec's line names the window of a watch-only terminal, not the whole exec wait.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_waits_for_a_rearmed_watch_only_while_it_would_keep_matching() -> anyhow::Result<()> {
    let run = run_dev_server(
        DevServer::new(
            /*notify_on_exit*/ false,
            Some(json!({ "notify_on_output": "(?i)error|fail" })),
        ),
        &[
            "features.exec_exit_notifications.enabled=true",
            "features.exec_exit_notifications.exec_wait_timeout_ms=60000",
            "features.exec_exit_notifications.output_watch_quiet_ms=1000",
            "features.exec_exit_notifications.output_watch_first_match_ms=45000",
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
    assert_eq!(
        run.served,
        vec!["start", "after_tool", "notified", "after_disarm"]
    );
    assert!(
        run.elapsed < Duration::from_secs(35),
        "{:?}\nstderr: {}",
        run.elapsed,
        run.stderr
    );
    assert!(
        run.stderr
            .contains("output watch \"ready\" (first match awaited up to ")
            && !run.stderr.contains("; up to "),
        "stderr: {}",
        run.stderr
    );
    Ok(())
}

/// A server that keeps printing, as a dev server logs its health checks.
const PRINTING_SERVER: &str = "while :; do echo 'GET /api/health 200'; sleep 0.05; done";

/// The shutdown notice of a background terminal that Codex stopped with its session.
const STOPPED_NOTICE: &str = "was stopped when the Codex session ended";

/// Live recheck 3 R10: codex exec stops a background command that still prints when it exits.
/// The end of the command reaches the thread's rollout before the rollout closes, so stderr has
/// no "failed to record rollout items"; with or without exit notifications.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_records_the_end_of_a_printing_terminal_it_stops() -> anyhow::Result<()> {
    for overrides in [
        &[][..],
        &["features.exec_exit_notifications.enabled=true"][..],
    ] {
        let run = run_exec_with(
            json!({
                "cmd": PRINTING_SERVER,
                "yield_time_ms": 250,
                "login": false,
            }),
            overrides,
        )
        .await?;
        assert_eq!(
            run.code,
            Some(0),
            "{overrides:?}\nstdout: {}\nstderr: {}",
            run.stdout,
            run.stderr
        );
        assert_eq!(run.served, vec!["start", "after_tool"], "{overrides:?}");
        assert!(
            !run.stderr.contains("failed to record rollout items"),
            "{overrides:?}\nstderr: {}",
            run.stderr
        );
    }
    Ok(())
}

/// Live recheck 3 R10: a server whose exit the model turned off, with its watch on or
/// stopped, is stopped when codex exec exits, and the rollout notes it, as for a server whose
/// exit is still awaited: a resumed thread knows the server no longer runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn exec_notes_the_stop_of_a_server_whose_exit_was_turned_off() -> anyhow::Result<()> {
    for follow_up in [
        json!({ "notify_on_exit": false }),
        json!({ "notify_on_exit": false, "stop_output_watch": true }),
    ] {
        let run = run_dev_server(
            DevServer::new(/*notify_on_exit*/ true, Some(follow_up.clone())),
            &[
                "features.exec_exit_notifications.enabled=true",
                "features.exec_exit_notifications.exec_wait_timeout_ms=40000",
                "features.exec_exit_notifications.output_watch_quiet_ms=1000",
            ],
        )
        .await?;
        assert_eq!(
            run.code,
            Some(0),
            "{follow_up}\nstdout: {}\nstderr: {}",
            run.stdout,
            run.stderr
        );
        assert_eq!(
            run.served,
            vec!["start", "after_tool", "notified", "after_disarm"],
            "{follow_up}"
        );
        assert_eq!(
            run.rollout.matches(STOPPED_NOTICE).count(),
            1,
            "{follow_up}\nrollout: {}",
            run.rollout
        );
        assert!(
            !run.stderr.contains("failed to record rollout items"),
            "{follow_up}\nstderr: {}",
            run.stderr
        );
    }
    Ok(())
}
