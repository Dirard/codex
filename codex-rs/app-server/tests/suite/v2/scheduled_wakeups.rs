//! Fork: scheduled wakeups and `/loop` through app-server
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC1–UC5, UC8).

use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadLoopStartParams;
use codex_app_server_protocol::ThreadLoopStartResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadWakeupCancelParams;
use codex_app_server_protocol::ThreadWakeupCancelReason;
use codex_app_server_protocol::ThreadWakeupCancelResponse;
use codex_app_server_protocol::ThreadWakeupChange;
use codex_app_server_protocol::ThreadWakeupReadParams;
use codex_app_server_protocol::ThreadWakeupReadResponse;
use codex_app_server_protocol::ThreadWakeupUpdatedNotification;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::MockServer;

const READ_TIMEOUT: Duration = Duration::from_secs(30);

async fn start(server: &MockServer, codex_home: &TempDir, enabled: bool) -> Result<TestAppServer> {
    let mut config =
        MockResponsesConfig::new(&server.uri()).with_provider_config("supports_websockets = false");
    if enabled {
        config = config.with_extra_config(
            "[features.scheduled_wakeups]\nenabled = true\nmin_delay_seconds = 1\n",
        );
    }
    config.write(codex_home.path())?;
    TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await
}

fn schedule_call(call_id: &str, delay_seconds: u64, prompt: &str) -> String {
    responses::sse(vec![
        responses::ev_response_created(call_id),
        responses::ev_function_call_with_namespace(
            call_id,
            "clock",
            "schedule_wakeup",
            &json!({ "delay_seconds": delay_seconds, "prompt": prompt, "reason": "poll CI" })
                .to_string(),
        ),
        responses::ev_completed(call_id),
    ])
}

fn answer(id: &str, text: &str) -> String {
    responses::sse(vec![
        responses::ev_response_created(id),
        responses::ev_assistant_message(&format!("msg-{id}"), text),
        responses::ev_completed(id),
    ])
}

async fn start_turn(app: &mut TestAppServer, thread_id: &str, text: &str) -> Result<String> {
    let TurnStartResponse { turn } = app
        .request(|request_id| ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: thread_id.to_string(),
                input: vec![UserInput::Text {
                    text: text.to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await?;
    Ok(turn.id)
}

async fn wait_turns(app: &mut TestAppServer, thread_id: &str, count: usize) -> Result<Vec<String>> {
    let mut turns = Vec::new();
    while turns.len() < count {
        let completed: TurnCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("turn/completed")).await??;
        if completed.thread_id == thread_id {
            turns.push(completed.turn.id);
        }
    }
    Ok(turns)
}

async fn read_state(app: &mut TestAppServer, thread_id: &str) -> Result<ThreadWakeupReadResponse> {
    app.request(|request_id| ClientRequest::ThreadWakeupRead {
        request_id,
        params: ThreadWakeupReadParams {
            thread_id: thread_id.to_string(),
        },
    })
    .await
}

async fn request_error(app: &mut TestAppServer, method: &str, params: Value) -> Result<String> {
    let request_id = app.send_request(method, Some(params)).await?;
    let error: JSONRPCError = timeout(
        READ_TIMEOUT,
        app.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    Ok(error.error.message)
}

fn tool_names(body: &Value) -> String {
    body.get("tools").map(Value::to_string).unwrap_or_default()
}

/// R1.2, R8.1: without the flag the model sees no wakeup tool and `thread/wakeup/read`
/// reports it unavailable; an unknown thread is an invalid request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn read_reports_availability_and_the_flag_hides_the_tools() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_sse_sequence(&server, vec![answer("r1", "Done.")]).await;
    let codex_home = TempDir::new()?;
    let mut app = start(&server, &codex_home, /*enabled*/ false).await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let read = read_state(&mut app, &thread.id).await?;
    assert!(!read.available);
    assert_eq!(read.state.wakeup, None);
    start_turn(&mut app, &thread.id, "hello").await?;
    wait_turns(&mut app, &thread.id, 1).await?;
    let body = mock.single_request().body_json();
    assert!(!tool_names(&body).contains("schedule_wakeup"), "{body}");

    let message = request_error(
        &mut app,
        "thread/wakeup/read",
        json!({ "threadId": "00000000-0000-0000-0000-000000000000" }),
    )
    .await?;
    assert!(message.contains("thread not loaded"), "{message}");
    Ok(())
}

/// R1.3, R2.7, R3.1, R3.5, R8.4: with the flag a root thread gets the tools; scheduling
/// publishes `scheduled` before the call's item completes; after the turn ends the wakeup
/// starts one turn whose input is the hidden `scheduled_wakeup` context with the note.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn wakeup_starts_a_turn_after_the_turn_ends() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_sse_sequence(
        &server,
        vec![
            schedule_call("call-1", 1, "check the CI job"),
            answer("r2", "I will check later."),
            answer("r3", "CI is green."),
        ],
    )
    .await;
    let codex_home = TempDir::new()?;
    let mut app = start(&server, &codex_home, /*enabled*/ true).await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    assert!(read_state(&mut app, &thread.id).await?.available);
    let first = start_turn(&mut app, &thread.id, "watch CI").await?;

    // `scheduled` reaches the client before the turn that scheduled it completes (R2.7); the
    // extension tool call emits no item of its own. Then the wakeup starts one more turn.
    let mut saw_scheduled = false;
    let mut completed = Vec::new();
    while completed.len() < 2 {
        let message = timeout(READ_TIMEOUT, app.read_next_message()).await??;
        let JSONRPCMessage::Notification(notification) = message else {
            continue;
        };
        match notification.method.as_str() {
            "thread/wakeup/updated" => {
                let update: ThreadWakeupUpdatedNotification =
                    serde_json::from_value(notification.params.unwrap_or_default())?;
                if update.change == ThreadWakeupChange::Scheduled && completed.is_empty() {
                    saw_scheduled = true;
                }
            }
            "turn/completed" => {
                let turn: TurnCompletedNotification =
                    serde_json::from_value(notification.params.unwrap_or_default())?;
                if completed.is_empty() {
                    assert!(
                        saw_scheduled,
                        "scheduled must precede the turn's completion"
                    );
                    assert_eq!(turn.turn.id, first);
                }
                completed.push(turn.turn.id);
            }
            _ => {}
        }
    }
    assert_ne!(completed[1], first);

    let requests = mock.requests();
    assert_eq!(requests.len(), 3);
    assert!(tool_names(&requests[0].body_json()).contains("schedule_wakeup"));
    let wake = requests[2].body_json().to_string();
    // The wake input carries the note but not the reason (the history still has the call).
    let start = wake
        .find("Scheduled wakeup wk-")
        .expect("the wake input is in the request");
    let end = start
        + wake[start..]
            .find("otherwise finish and report.")
            .expect("the wake input ends");
    let wake_input = &wake[start..end];
    assert!(wake_input.contains("check the CI job"), "{wake_input}");
    assert!(
        !wake_input.contains("poll CI"),
        "the reason stays out: {wake_input}"
    );
    let read = read_state(&mut app, &thread.id).await?;
    assert_eq!(read.state.wakeup, None);
    assert_eq!(read.state.chain.fired, 1);
    Ok(())
}

/// R4.2, R4.5, R8.2: `thread/wakeup/cancel` stops a pending wakeup without a turn, and the
/// next user turn carries the notice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_request_stops_the_wakeup() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_sse_sequence(
        &server,
        vec![
            schedule_call("call-1", 600, "check later"),
            answer("r2", "Scheduled."),
            answer("r3", "Okay."),
        ],
    )
    .await;
    let codex_home = TempDir::new()?;
    let mut app = start(&server, &codex_home, /*enabled*/ true).await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    start_turn(&mut app, &thread.id, "watch CI").await?;
    wait_turns(&mut app, &thread.id, 1).await?;
    assert!(
        read_state(&mut app, &thread.id)
            .await?
            .state
            .wakeup
            .is_some()
    );

    let cancelled: ThreadWakeupCancelResponse = app
        .request(|request_id| ClientRequest::ThreadWakeupCancel {
            request_id,
            params: ThreadWakeupCancelParams {
                thread_id: thread.id.clone(),
            },
        })
        .await?;
    assert!(cancelled.cancelled);
    assert_eq!(cancelled.state.wakeup, None);
    let update: ThreadWakeupUpdatedNotification =
        timeout(READ_TIMEOUT, app.read_notification("thread/wakeup/updated")).await??;
    let _ = update;

    start_turn(&mut app, &thread.id, "anything new?").await?;
    wait_turns(&mut app, &thread.id, 1).await?;
    let last = mock
        .requests()
        .pop()
        .expect("a request")
        .body_json()
        .to_string();
    assert!(last.contains("the user cancelled it"), "{last}");
    Ok(())
}

/// R5.1, R5.2, R5.3, R5.5, R8.3: a fixed loop runs its prompt as user turns with the loop
/// context and ends at `maxIterations` with `limit`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fixed_loop_runs_its_prompt_as_user_turns() -> Result<()> {
    let server = responses::start_mock_server().await;
    let mock = responses::mount_sse_sequence(
        &server,
        vec![
            answer("r1", "Deploy pending."),
            answer("r2", "Deploy done."),
        ],
    )
    .await;
    let codex_home = TempDir::new()?;
    let mut app = start(&server, &codex_home, /*enabled*/ true).await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let started: ThreadLoopStartResponse = app
        .request(|request_id| ClientRequest::ThreadLoopStart {
            request_id,
            params: ThreadLoopStartParams {
                thread_id: thread.id.clone(),
                prompt: "check the deploy".to_string(),
                interval_seconds: Some(1),
                max_iterations: Some(2),
            },
        })
        .await?;
    assert!(started.state.loop_state.is_some());
    wait_turns(&mut app, &thread.id, 2).await?;
    let ended = loop {
        let update: ThreadWakeupUpdatedNotification =
            timeout(READ_TIMEOUT, app.read_notification("thread/wakeup/updated")).await??;
        if update.change == ThreadWakeupChange::LoopEnded {
            break update;
        }
    };
    assert_eq!(ended.cancel_reason, Some(ThreadWakeupCancelReason::Limit));
    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        let body = request.body_json().to_string();
        assert!(body.contains("check the deploy"), "{body}");
        assert!(body.contains("source=\\\"loop\\\""), "{body}");
    }
    Ok(())
}

/// R5.7, R8.3: invalid loop parameters and a thread without the feature are invalid
/// requests.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn loop_start_rejects_bad_params_and_unavailable_threads() -> Result<()> {
    let server = responses::start_mock_server().await;
    let codex_home = TempDir::new()?;
    let mut app = start(&server, &codex_home, /*enabled*/ true).await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    for (params, expected) in [
        (json!({ "prompt": " " }), "prompt must not be empty"),
        (
            json!({ "prompt": "x", "intervalSeconds": 7200 }),
            "intervalSeconds must be from 1 to 3600",
        ),
        (
            json!({ "prompt": "x", "maxIterations": 0 }),
            "maxIterations must be from 1 to 50",
        ),
    ] {
        let mut params = params;
        params["threadId"] = json!(thread.id);
        let message = request_error(&mut app, "thread/loop/start", params).await?;
        assert!(message.contains(expected), "{message}");
    }

    let other_home = TempDir::new()?;
    let mut disabled = start(&server, &other_home, /*enabled*/ false).await?;
    let thread = disabled
        .start_thread(ThreadStartParams::default())
        .await?
        .thread;
    let message = request_error(
        &mut disabled,
        "thread/loop/start",
        json!({ "threadId": thread.id, "prompt": "x" }),
    )
    .await?;
    assert!(message.contains("not available"), "{message}");
    Ok(())
}
