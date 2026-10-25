//! Fork: background `exec_command` processes notify the thread when they exit.
//!
//! Scenarios for `specs/background-exec-notifications-2026-10-25.md`.

use std::time::Duration;

use anyhow::Result;
use codex_features::Feature;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use core_test_support::process::wait_for_pid_file;
use core_test_support::process::wait_for_process_exit;
use core_test_support::responses::ResponseMock;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::skip_if_sandbox;
use core_test_support::skip_if_target_windows;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event_with_timeout;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

const NOTIFICATION_MARKER: &str = "<background_terminal_notification>";
const ARMED_LINE: &str = "Exit notification: on";
const EVENT_TIMEOUT: Duration = Duration::from_secs(30);

async fn build(server: &wiremock::MockServer, exit_notifications: bool) -> Result<TestCodex> {
    test_codex()
        .with_config(move |config| {
            if exit_notifications {
                config
                    .features
                    .enable(Feature::ExecExitNotifications)
                    .expect("enable exit notifications");
            }
        })
        .build(server)
        .await
}

fn exec_args(cmd: &str, yield_time_ms: u64, notify_on_exit: bool) -> String {
    let mut args = json!({
        "cmd": cmd,
        "yield_time_ms": yield_time_ms,
        "login": false,
    });
    if notify_on_exit {
        args["notify_on_exit"] = json!(true);
    }
    args.to_string()
}

fn tool_call(response_id: &str, call_id: &str, name: &str, arguments: &str) -> String {
    sse(vec![
        ev_response_created(response_id),
        ev_function_call(call_id, name, arguments),
        ev_completed(response_id),
    ])
}

fn message(response_id: &str, text: &str) -> String {
    sse(vec![
        ev_response_created(response_id),
        ev_assistant_message(&format!("msg-{response_id}"), text),
        ev_completed(response_id),
    ])
}

fn notifications(request: &ResponsesRequest) -> Vec<String> {
    request
        .message_input_texts("user")
        .into_iter()
        .filter(|text| text.starts_with(NOTIFICATION_MARKER))
        .collect()
}

fn plain_tool(request: &ResponsesRequest, name: &str) -> Value {
    request.body_json()["tools"]
        .as_array()
        .and_then(|tools| {
            tools
                .iter()
                .find(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
        })
        .cloned()
        .unwrap_or_else(|| panic!("tool {name} missing"))
}

/// Counts turns that start and complete until `completions` turns completed.
async fn count_turns_until_completed(test: &TestCodex, completions: usize) -> usize {
    let mut started = 0;
    let mut completed = 0;
    while completed < completions {
        match wait_for_event_with_timeout(&test.codex, |_| true, EVENT_TIMEOUT).await {
            EventMsg::TurnStarted(_) => started += 1,
            EventMsg::TurnComplete(_) => completed += 1,
            _ => {}
        }
    }
    started
}

/// No further model request arrives within `quiet`.
async fn assert_requests_settle(mock: &ResponseMock, expected: usize, quiet: Duration) {
    tokio::time::sleep(quiet).await;
    assert_eq!(mock.requests().len(), expected);
}

/// R5.1: the background command outlives its turn and its exit starts the next turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_notification_starts_turn_after_background_command_outlives_its_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 2; echo exit-notify-done",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            message("resp-2", "Started; I will continue when it exits."),
            message("resp-3", "The command finished."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ true).await?;

    test.submit_turn("run the long command in the background")
        .await?;
    // The turn ended while the process was still running.
    assert_eq!(mock.requests().len(), 2);
    assert_eq!(count_turns_until_completed(&test, 1).await, 1);

    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let exec_command = plain_tool(&requests[0], "exec_command");
    assert!(
        exec_command["parameters"]["properties"]
            .get("notify_on_exit")
            .is_some(),
        "{exec_command}"
    );
    let write_stdin = plain_tool(&requests[0], "write_stdin");
    assert!(
        write_stdin["description"]
            .as_str()
            .is_some_and(|text| text.contains("notify_on_exit")),
        "{write_stdin}"
    );
    let output = requests[1]
        .function_call_output_text("bg-call")
        .expect("exec_command output");
    assert!(
        output.contains("Process running with session ID 1000"),
        "{output}"
    );
    assert!(output.contains(ARMED_LINE), "{output}");
    assert_eq!(notifications(&requests[1]), Vec::<String>::new());
    let delivered = notifications(&requests[2]);
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert!(
        delivered[0].contains("Background terminal session 1000 exited with code 0 after "),
        "{}",
        delivered[0]
    );
    assert!(
        delivered[0].contains("Command: sleep 2; echo exit-notify-done"),
        "{}",
        delivered[0]
    );
    assert!(
        delivered[0].contains("exit-notify-done"),
        "{}",
        delivered[0]
    );
    // Nobody polled: the notification replaced every write_stdin call.
    let called_tools = requests[2]
        .inputs_of_type("function_call")
        .into_iter()
        .filter_map(|call| call.get("name").and_then(Value::as_str).map(str::to_string))
        .collect::<Vec<_>>();
    assert_eq!(called_tools, vec!["exec_command".to_string()]);
    assert_requests_settle(&mock, 3, Duration::from_millis(500)).await;
    Ok(())
}

/// R3.2: an exit during a running turn joins that turn's next model request.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn exit_during_running_turn_joins_next_model_request() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 1; echo early-exit",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            tool_call(
                "resp-2",
                "fg-call",
                "exec_command",
                &exec_args(
                    "sleep 3; echo foreground-done",
                    /*yield_time_ms*/ 10_000,
                    /*notify_on_exit*/ false,
                ),
            ),
            message("resp-3", "Both commands are done."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ true).await?;

    test.submit_turn("run two commands").await?;

    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let delivered = notifications(&requests[2]);
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert!(delivered[0].contains("session 1000 exited with code 0"));
    assert!(delivered[0].contains("early-exit"));
    assert!(
        requests[2]
            .function_call_output_text("fg-call")
            .is_some_and(|output| output.contains("foreground-done"))
    );
    // The notification was consumed by the running turn, so no new turn starts.
    assert_requests_settle(&mock, 3, Duration::from_secs(1)).await;
    Ok(())
}

/// R2.2: a poll that reports the exit settles the notification.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn poll_that_observes_exit_suppresses_notification() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let poll = json!({
        "session_id": 1000,
        "chars": "",
        "yield_time_ms": 10_000,
    })
    .to_string();
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 1; echo seen-by-poll",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            tool_call("resp-2", "poll-call", "write_stdin", &poll),
            message("resp-3", "Seen."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ true).await?;

    test.submit_turn("run and poll").await?;

    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let output = requests[2]
        .function_call_output_text("poll-call")
        .expect("write_stdin output");
    assert!(output.contains("Process exited with code 0"), "{output}");
    assert!(output.contains("seen-by-poll"), "{output}");
    assert!(
        requests
            .iter()
            .all(|request| notifications(request).is_empty())
    );
    assert_requests_settle(&mock, 3, Duration::from_secs(2)).await;
    Ok(())
}

/// R3.3: after a user interrupt the notification waits for the next user turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_thread_receives_notification_with_next_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 2; echo after-interrupt",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            tool_call(
                "resp-2",
                "blocking-call",
                "exec_command",
                &exec_args(
                    "sleep 60", /*yield_time_ms*/ 30_000, /*notify_on_exit*/ false,
                ),
            ),
            message("resp-3", "Continuing with the result."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ true).await?;

    test.codex
        .start_or_steer_turn(codex_core::TurnInputRequest::user_input(vec![
            codex_protocol::user_input::UserInput::Text {
                text: "start and get interrupted".to_string(),
                text_elements: Vec::new(),
            },
        ]))
        .await?;
    wait_for_event_with_timeout(
        &test.codex,
        |event| matches!(event, EventMsg::ExecCommandBegin(begin) if begin.call_id == "blocking-call"),
        EVENT_TIMEOUT,
    )
    .await;
    test.codex.submit(Op::Interrupt).await?;
    wait_for_event_with_timeout(
        &test.codex,
        |event| matches!(event, EventMsg::TurnAborted(_)),
        EVENT_TIMEOUT,
    )
    .await;
    // The background command exits about two seconds after it started.
    assert_requests_settle(&mock, 2, Duration::from_secs(3)).await;

    test.submit_turn("what happened?").await?;
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let delivered = notifications(&requests[2]);
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert!(delivered[0].contains("after-interrupt"), "{}", delivered[0]);

    test.codex.submit(Op::CleanBackgroundTerminals).await?;
    Ok(())
}

/// R2.4: a termination requested by the user is reported without waking the thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_termination_is_reported_without_waking() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 60", /*yield_time_ms*/ 250, /*notify_on_exit*/ true,
                ),
            ),
            message("resp-2", "Started."),
            message("resp-3", "It was stopped."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ true).await?;

    test.submit_turn("start a long command").await?;
    assert!(test.codex.terminate_background_terminal(1000).await);
    assert_requests_settle(&mock, 2, Duration::from_secs(1)).await;

    test.submit_turn("status?").await?;
    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let delivered = notifications(&requests[2]);
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert!(
        delivered[0].contains("session 1000 was terminated at the user's request"),
        "{}",
        delivered[0]
    );
    Ok(())
}

/// R2.5: shutdown kills the process and reports nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_terminates_without_notification_or_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let pid_dir = tempfile::tempdir()?;
    let pid_path = pid_dir.path().join("pid");
    let command = format!(
        "printf '%s' $$ > '{}' && exec sleep 60",
        pid_path.to_string_lossy()
    );
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    &command, /*yield_time_ms*/ 250, /*notify_on_exit*/ true,
                ),
            ),
            message("resp-2", "Started."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ true).await?;

    test.submit_turn("start and quit").await?;
    let pid = wait_for_pid_file(&pid_path).await?;
    test.codex.submit(Op::Shutdown).await?;
    wait_for_event_with_timeout(
        &test.codex,
        |event| matches!(event, EventMsg::ShutdownComplete),
        EVENT_TIMEOUT,
    )
    .await;
    wait_for_process_exit(&pid).await?;
    assert_requests_settle(&mock, 2, Duration::from_millis(500)).await;
    Ok(())
}

/// R1.1, R1.4: without the feature nothing is advertised and nothing is delivered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_feature_keeps_tools_and_ignores_notify_on_exit() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 1; echo flag-off",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            message("resp-2", "Started."),
        ],
    )
    .await;
    let test = build(&server, /*exit_notifications*/ false).await?;

    test.submit_turn("start without the feature").await?;
    let requests = mock.requests();
    let exec_command = plain_tool(&requests[0], "exec_command");
    assert!(
        exec_command["parameters"]["properties"]
            .get("notify_on_exit")
            .is_none(),
        "{exec_command}"
    );
    let write_stdin = plain_tool(&requests[0], "write_stdin");
    assert!(
        !write_stdin["description"]
            .as_str()
            .unwrap_or_default()
            .contains("notify_on_exit")
    );
    let output = requests[1]
        .function_call_output_text("bg-call")
        .expect("exec_command output");
    assert!(output.contains("Process running with session ID 1000"));
    assert!(!output.contains(ARMED_LINE), "{output}");
    assert_requests_settle(&mock, 2, Duration::from_secs(2)).await;
    Ok(())
}

/// R4.1: V2 `wait_agent` without agents waits for the background terminal.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_agent_returns_on_background_terminal_exit() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");
    const NAMESPACE: &str = "collaboration";

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 1; echo waited-for",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            sse(vec![
                ev_response_created("resp-2"),
                ev_function_call_with_namespace("wait-call", NAMESPACE, "wait_agent", "{}"),
                ev_completed("resp-2"),
            ]),
            message("resp-3", "Done waiting."),
        ],
    )
    .await;
    let test = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config
                .features
                .enable(Feature::MultiAgentV2)
                .expect("enable multi-agent v2");
            config
                .features
                .enable(Feature::ExecExitNotifications)
                .expect("enable exit notifications");
        })
        .build(&server)
        .await?;

    test.submit_turn("start and wait").await?;

    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    let wait_agent = requests[0]
        .tool_by_name(NAMESPACE, "wait_agent")
        .expect("wait_agent tool");
    assert!(
        wait_agent["description"]
            .as_str()
            .is_some_and(|text| text.contains("notify_on_exit")),
        "{wait_agent}"
    );
    let wait_output = requests[2]
        .function_call_output_text("wait-call")
        .expect("wait_agent output");
    assert!(
        wait_output.contains("Wait completed: a background terminal notification arrived."),
        "{wait_output}"
    );
    let delivered = notifications(&requests[2]);
    assert_eq!(delivered.len(), 1, "{delivered:?}");
    assert!(delivered[0].contains("waited-for"));
    Ok(())
}

/// R4.1: a `wait_agent` timeout names the terminals that will still report their exit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_agent_timeout_reports_pending_background_terminals() -> Result<()> {
    skip_if_no_network!(Ok(()));
    skip_if_sandbox!(Ok(()));
    skip_if_target_windows!(Ok(()), "uses POSIX shell commands");
    const NAMESPACE: &str = "collaboration";

    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![
            tool_call(
                "resp-1",
                "bg-call",
                "exec_command",
                &exec_args(
                    "sleep 3; echo still-running-at-timeout",
                    /*yield_time_ms*/ 250,
                    /*notify_on_exit*/ true,
                ),
            ),
            sse(vec![
                ev_response_created("resp-2"),
                ev_function_call_with_namespace(
                    "wait-call",
                    NAMESPACE,
                    "wait_agent",
                    r#"{"timeout_ms":200}"#,
                ),
                ev_completed("resp-2"),
            ]),
            message("resp-3", "It is still running."),
            message("resp-4", "Now it has finished."),
        ],
    )
    .await;
    let test = test_codex()
        .with_model("gpt-5.4")
        .with_config(|config| {
            config
                .features
                .enable(Feature::MultiAgentV2)
                .expect("enable multi-agent v2");
            config
                .features
                .enable(Feature::ExecExitNotifications)
                .expect("enable exit notifications");
            config.multi_agent_v2.min_wait_timeout_ms = 100;
        })
        .build(&server)
        .await?;

    test.submit_turn("start and wait briefly").await?;
    let wait_output = mock.requests()[2]
        .function_call_output_text("wait-call")
        .expect("wait_agent output");
    assert!(
        wait_output.contains(
            "Wait timed out. Active agents: pending_init=0, running=0, interrupted=0. Background terminals that will report their exit: 1."
        ),
        "{wait_output}"
    );

    // The exit still arrives afterwards and starts its own turn.
    assert_eq!(count_turns_until_completed(&test, 1).await, 1);
    let requests = mock.requests();
    assert_eq!(requests.len(), 4);
    assert!(notifications(&requests[3])[0].contains("still-running-at-timeout"));
    Ok(())
}
