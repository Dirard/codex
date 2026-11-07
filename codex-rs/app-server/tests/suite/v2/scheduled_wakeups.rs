//! Fork: scheduled wakeups and `/loop` through app-server
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC1–UC6, UC8).

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ApprovalsReviewer;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ReviewStartResponse;
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
use codex_features::Feature;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

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

// Review fixes (W-2, T-1): scripted Responses that serve several threads by content.

const WAKE_MARKER: &str = "Scheduled wakeup wk-";
const COLLAB: &str = "collaboration";
const WORKFLOW_AGENT_MARKER: &str = "You are a workflow agent started by an orchestration script";
const GUARDIAN_MARKER: &str = ">>> TRANSCRIPT START";
const REVIEW_MARKER: &str = "You are acting as a reviewer for a proposed code change";

type Predicate = Box<dyn Fn(&str) -> bool + Send + Sync>;

struct Rule {
    name: &'static str,
    matches: Predicate,
    body: String,
    remaining: usize,
    delay: Duration,
}

/// Serves each Responses request with the first matching rule that has uses left and records
/// which rule served which request body, so threads may ask in any order.
#[derive(Clone, Default)]
struct Scripted {
    rules: Arc<Mutex<Vec<Rule>>>,
    served: Arc<Mutex<Vec<(&'static str, String)>>>,
}

impl Respond for Scripted {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body).into_owned();
        let mut rules = self.rules.lock().expect("rules");
        let Some(rule) = rules
            .iter_mut()
            .find(|rule| rule.remaining > 0 && (rule.matches)(&body))
        else {
            self.served
                .lock()
                .expect("served")
                .push(("unmatched", body));
            return ResponseTemplate::new(500).set_body_string("no scripted response");
        };
        rule.remaining -= 1;
        let response = responses::sse_response(rule.body.clone()).set_delay(rule.delay);
        self.served.lock().expect("served").push((rule.name, body));
        response
    }
}

impl Scripted {
    async fn mount(server: &MockServer) -> Self {
        let scripted = Self::default();
        Mock::given(method("POST"))
            .and(path_regex(".*/responses$"))
            .respond_with(scripted.clone())
            .mount(server)
            .await;
        scripted
    }

    fn rule(
        &self,
        name: &'static str,
        times: usize,
        delay: Duration,
        matches: impl Fn(&str) -> bool + Send + Sync + 'static,
        body: String,
    ) {
        self.rules.lock().expect("rules").push(Rule {
            name,
            matches: Box::new(matches),
            body,
            remaining: times,
            delay,
        });
    }

    fn bodies(&self, name: &str) -> Vec<String> {
        self.served
            .lock()
            .expect("served")
            .iter()
            .filter(|(served, _)| *served == name)
            .map(|(_, body)| body.clone())
            .collect()
    }

    fn names(&self) -> Vec<&'static str> {
        self.served
            .lock()
            .expect("served")
            .iter()
            .map(|(name, _)| *name)
            .collect()
    }

    /// The first request `name` served, once it arrives.
    async fn first(&self, name: &str) -> Result<String> {
        let found = timeout(READ_TIMEOUT, async {
            loop {
                if let Some(body) = self.bodies(name).into_iter().next() {
                    return body;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        found.map_err(|_| anyhow::anyhow!("`{name}` was never served; served: {:?}", self.names()))
    }
}

fn call(call_id: &str, namespace: Option<&str>, name: &str, arguments: Value) -> String {
    let event = match namespace {
        Some(namespace) => responses::ev_function_call_with_namespace(
            call_id,
            namespace,
            name,
            &arguments.to_string(),
        ),
        None => responses::ev_function_call(call_id, name, &arguments.to_string()),
    };
    responses::sse(vec![
        responses::ev_response_created(&format!("resp-{call_id}")),
        event,
        responses::ev_completed(&format!("resp-{call_id}")),
    ])
}

/// The output of `call_id` in a request body.
fn call_output(body: &str, call_id: &str) -> Option<String> {
    let body: Value = serde_json::from_str(body).ok()?;
    body["input"].as_array()?.iter().find_map(|item| {
        (item["type"] == "function_call_output" && item["call_id"] == call_id)
            .then(|| item["output"].to_string())
    })
}

fn has_call_output(body: &str, call_id: &str) -> bool {
    call_output(body, call_id).is_some()
}

/// Whether the request's metadata names the MultiAgentV2 agent at `path`.
fn names_agent(body: &str, path: &str) -> bool {
    body.contains(&format!(r#"\"agent_name\":\"{path}\""#))
}

fn from_root(body: &str) -> bool {
    names_agent(body, "/root")
}

/// A child's request; a root request may name the child too, in tool output or mail.
fn from_child(body: &str, path: &str) -> bool {
    names_agent(body, path) && !from_root(body)
}

fn offers_wakeups(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .is_ok_and(|body| tool_names(&body).contains("schedule_wakeup"))
}

fn config(server: &MockServer, extra: &str) -> MockResponsesConfig {
    MockResponsesConfig::new(&server.uri())
        .with_provider_config("supports_websockets = false")
        .with_extra_config(extra)
}

const WAKEUPS_ON: &str = "[features.scheduled_wakeups]\nenabled = true\nmin_delay_seconds = 1\n";
const WAKEUPS_OFF: &str = "[features.scheduled_wakeups]\nenabled = false\n";

/// W-2, R5.1: after the user interrupts a turn (Esc), `thread/loop/start` runs iteration 1
/// at once; the interrupt holds back only the model's own wakeups (R3.3), not a loop the user
/// just started.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn loop_starts_on_an_interrupted_thread() -> Result<()> {
    let server = responses::start_mock_server().await;
    let scripted = Scripted::mount(&server).await;
    scripted.rule(
        "slow",
        1,
        Duration::from_secs(60),
        |body| body.contains("work slowly"),
        answer("r-slow", "Too late."),
    );
    scripted.rule(
        "iteration",
        1,
        Duration::ZERO,
        |body| body.contains("check the deploy"),
        answer("r-iteration", "Deploy done."),
    );
    let codex_home = TempDir::new()?;
    config(&server, WAKEUPS_ON).write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let turn = start_turn(&mut app, &thread.id, "work slowly").await?;
    scripted.first("slow").await?;
    app.interrupt_turn_and_wait_for_aborted(thread.id.clone(), turn, READ_TIMEOUT)
        .await?;

    let started: ThreadLoopStartResponse = app
        .request(|request_id| ClientRequest::ThreadLoopStart {
            request_id,
            params: ThreadLoopStartParams {
                thread_id: thread.id.clone(),
                prompt: "check the deploy".to_string(),
                interval_seconds: Some(1),
                max_iterations: Some(1),
            },
        })
        .await?;
    assert!(started.state.loop_state.is_some());
    wait_turns(&mut app, &thread.id, 1).await?;
    let ended = loop {
        let update: ThreadWakeupUpdatedNotification =
            timeout(READ_TIMEOUT, app.read_notification("thread/wakeup/updated")).await??;
        if update.change == ThreadWakeupChange::LoopEnded {
            break update;
        }
    };
    assert_eq!(ended.cancel_reason, Some(ThreadWakeupCancelReason::Limit));
    assert_eq!(scripted.bodies("iteration").len(), 1);
    Ok(())
}

/// R3.2: a wakeup that comes due while a turn runs never joins it: the turn's later model
/// request carries no wake input, and the wakeup turn starts only after that turn completed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_due_wakeup_never_joins_a_running_turn() -> Result<()> {
    let server = responses::start_mock_server().await;
    let scripted = Scripted::mount(&server).await;
    scripted.rule(
        "wake",
        1,
        Duration::ZERO,
        |body| body.contains(WAKE_MARKER),
        answer("r-wake", "CI is green."),
    );
    scripted.rule(
        "schedule",
        1,
        Duration::ZERO,
        |body| !has_call_output(body, "call-1"),
        schedule_call("call-1", 1, "check the CI job"),
    );
    // The model works on for longer than the wakeup's delay.
    scripted.rule(
        "slow_step",
        1,
        Duration::from_millis(2_500),
        |body| has_call_output(body, "call-1") && !has_call_output(body, "call-2"),
        call("call-2", None, "noop_probe", json!({})),
    );
    scripted.rule(
        "after_due",
        1,
        Duration::ZERO,
        |body| has_call_output(body, "call-2"),
        answer("r-after", "Still working, then done."),
    );
    let codex_home = TempDir::new()?;
    config(&server, WAKEUPS_ON).write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let first = start_turn(&mut app, &thread.id, "watch CI").await?;

    let mut events = Vec::new();
    while events
        .iter()
        .filter(|(method, _)| method == "turn/completed")
        .count()
        < 2
    {
        let message = timeout(READ_TIMEOUT, app.read_next_message()).await??;
        let JSONRPCMessage::Notification(notification) = message else {
            continue;
        };
        let params = notification.params.unwrap_or_default();
        if params["threadId"].as_str() != Some(thread.id.as_str()) {
            continue;
        }
        if notification.method == "turn/started" || notification.method == "turn/completed" {
            let turn_id = params["turn"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            events.push((notification.method, turn_id));
        }
    }
    let wake_turn = events
        .iter()
        .map(|(_, turn_id)| turn_id.clone())
        .find(|turn_id| *turn_id != first)
        .expect("a wakeup turn");
    assert_eq!(
        events,
        vec![
            ("turn/started".to_string(), first.clone()),
            ("turn/completed".to_string(), first),
            ("turn/started".to_string(), wake_turn.clone()),
            ("turn/completed".to_string(), wake_turn),
        ]
    );
    let after_due = scripted.first("after_due").await?;
    assert!(
        !after_due.contains(WAKE_MARKER),
        "the running turn got the wake input"
    );
    assert_eq!(scripted.bodies("wake").len(), 1);
    Ok(())
}

/// UC6 (R6.1, R6.2), T-1: a MultiAgentV2 child schedules a wakeup and ends its turn; the
/// parent's mail names the wakeup, the parent's `wait_agent` waits for the child instead of
/// answering `No active agents.`, and the parent gets the report of the child's wakeup turn.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn child_wakeup_reports_again_to_a_waiting_parent() -> Result<()> {
    const SPAWN: &str = "call-spawn";
    const WAIT_1: &str = "call-wait-1";
    const WAIT_2: &str = "call-wait-2";
    const SCHEDULE: &str = "call-child-schedule";
    const CHILD: &str = "/root/ci_watch";
    let server = responses::start_mock_server().await;
    let scripted = Scripted::mount(&server).await;
    scripted.rule(
        "child_wake_report",
        1,
        Duration::ZERO,
        |body| from_child(body, CHILD) && body.contains(WAKE_MARKER),
        answer("r-child-wake", "CI is green now."),
    );
    scripted.rule(
        "child_schedule",
        1,
        Duration::ZERO,
        |body| from_child(body, CHILD) && !has_call_output(body, SCHEDULE),
        call(
            SCHEDULE,
            Some("clock"),
            "schedule_wakeup",
            json!({ "delay_seconds": 1, "prompt": "check CI again", "reason": "CI running" }),
        ),
    );
    scripted.rule(
        "child_first_report",
        1,
        Duration::ZERO,
        |body| from_child(body, CHILD) && has_call_output(body, SCHEDULE),
        answer("r-child-first", "CI is still running."),
    );
    scripted.rule(
        "parent_spawn",
        1,
        Duration::ZERO,
        |body| from_root(body) && !has_call_output(body, SPAWN),
        call(
            SPAWN,
            Some(COLLAB),
            "spawn_agent",
            json!({ "message": "Watch CI and report.", "task_name": "ci_watch" }),
        ),
    );
    scripted.rule(
        "parent_wait_1",
        1,
        Duration::ZERO,
        |body| from_root(body) && has_call_output(body, SPAWN) && !has_call_output(body, WAIT_1),
        call(WAIT_1, Some(COLLAB), "wait_agent", json!({})),
    );
    scripted.rule(
        "parent_wait_2",
        1,
        Duration::ZERO,
        |body| from_root(body) && has_call_output(body, WAIT_1) && !has_call_output(body, WAIT_2),
        call(WAIT_2, Some(COLLAB), "wait_agent", json!({})),
    );
    scripted.rule(
        "parent_done",
        usize::MAX,
        Duration::ZERO,
        |body| from_root(body) && has_call_output(body, WAIT_2),
        answer("r-parent-done", "Both reports are in."),
    );
    let codex_home = TempDir::new()?;
    config(&server, WAKEUPS_ON)
        .enable_feature(Feature::MultiAgentV2)
        .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    start_turn(
        &mut app,
        &thread.id,
        "Spawn a CI watcher and wait for its reports.",
    )
    .await?;
    wait_turns(&mut app, &thread.id, 1).await?;

    let child_first = scripted.first("child_schedule").await?;
    assert!(
        offers_wakeups(&child_first),
        "a V2 child gets the wakeup tools"
    );
    let after_first = scripted.first("parent_wait_2").await?;
    assert!(
        after_first.contains("It scheduled a wakeup for")
            && after_first.contains("close_agent stops it"),
        "the completion mail names the wakeup (R6.1): {after_first}"
    );
    let done = scripted.first("parent_done").await?;
    let waited = call_output(&done, WAIT_2).expect("the second wait's output");
    assert!(
        !waited.contains("No active agents"),
        "wait_agent waits for the child's wakeup (R6.2): {waited}"
    );
    assert!(
        done.contains("CI is green now."),
        "the parent gets the wakeup turn's report"
    );
    assert_eq!(scripted.bodies("child_wake_report").len(), 1);
    Ok(())
}

/// What each kind of thread was offered, by the rule that served its first request.
async fn requests_of_every_thread_source(enabled: bool) -> Result<Vec<(&'static str, String)>> {
    const SPAWN: &str = "call-spawn";
    const WORKFLOW: &str = "call-workflow";
    const EXEC: &str = "call-exec";
    const SCRIPT: &str = "export const meta = { name: 'probe', description: 'One agent', phases: ['Work'] };\nphase('Work');\nconst out = await agent('WF-PROBE-TASK', { label: 'probe' });\nreturn { out };\n";
    let server = responses::start_mock_server().await;
    let scripted = Scripted::mount(&server).await;
    scripted.rule(
        "guardian",
        usize::MAX,
        Duration::ZERO,
        |body| body.contains(GUARDIAN_MARKER),
        answer("r-guardian", r#"{"outcome":"deny"}"#),
    );
    scripted.rule(
        "review",
        usize::MAX,
        Duration::ZERO,
        |body| body.contains(REVIEW_MARKER),
        answer(
            "r-review",
            &json!({
                "findings": [],
                "overall_correctness": "good",
                "overall_explanation": "Nothing to report.",
                "overall_confidence_score": 0.5
            })
            .to_string(),
        ),
    );
    scripted.rule(
        "workflow_agent",
        usize::MAX,
        Duration::ZERO,
        |body| body.contains(WORKFLOW_AGENT_MARKER),
        answer("r-workflow-agent", "Probe done."),
    );
    scripted.rule(
        "child",
        usize::MAX,
        Duration::ZERO,
        |body| from_child(body, "/root/helper"),
        answer("r-child", "Helper done."),
    );
    scripted.rule(
        "root",
        1,
        Duration::ZERO,
        |body| body.contains("PHASE-SPAWN") && !has_call_output(body, SPAWN),
        call(
            SPAWN,
            Some(COLLAB),
            "spawn_agent",
            json!({ "message": "Say hello.", "task_name": "helper" }),
        ),
    );
    scripted.rule(
        "root_rest",
        usize::MAX,
        Duration::ZERO,
        |body| body.contains("PHASE-SPAWN"),
        answer("r-root-rest", "Spawned."),
    );
    scripted.rule(
        "workflow_root",
        1,
        Duration::ZERO,
        |body| body.contains("PHASE-WORKFLOW") && !has_call_output(body, WORKFLOW),
        call(WORKFLOW, None, "workflow", json!({ "script": SCRIPT })),
    );
    scripted.rule(
        "workflow_root_rest",
        usize::MAX,
        Duration::ZERO,
        |body| body.contains("PHASE-WORKFLOW"),
        answer("r-workflow-rest", "The workflow runs."),
    );
    scripted.rule(
        "guardian_root",
        1,
        Duration::ZERO,
        |body| body.contains("PHASE-GUARDIAN") && !has_call_output(body, EXEC),
        call(
            EXEC,
            None,
            "exec_command",
            json!({ "cmd": "true", "sandbox_permissions": "require_escalated" }),
        ),
    );
    scripted.rule(
        "guardian_root_rest",
        usize::MAX,
        Duration::ZERO,
        |body| body.contains("PHASE-GUARDIAN"),
        answer("r-guardian-rest", "The command was denied."),
    );
    let codex_home = TempDir::new()?;
    let wakeups = if enabled { WAKEUPS_ON } else { WAKEUPS_OFF };
    config(
        &server,
        &format!("[workflows]\nrequire_approval = \"never\"\n\n{wakeups}"),
    )
    .enable_feature(Feature::MultiAgentV2)
    .enable_feature(Feature::Workflows)
    .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;

    // Root and a MultiAgentV2 child.
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    start_turn(&mut app, &thread.id, "PHASE-SPAWN").await?;
    wait_turns(&mut app, &thread.id, 1).await?;
    scripted.first("child").await?;

    // A workflow agent.
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    start_turn(&mut app, &thread.id, "PHASE-WORKFLOW").await?;
    wait_turns(&mut app, &thread.id, 1).await?;
    scripted.first("workflow_agent").await?;

    // A Guardian reviewer of an escalated command.
    let thread = app
        .start_thread(ThreadStartParams {
            approval_policy: Some(AskForApproval::OnRequest),
            approvals_reviewer: Some(ApprovalsReviewer::AutoReview),
            ..Default::default()
        })
        .await?
        .thread;
    start_turn(&mut app, &thread.id, "PHASE-GUARDIAN").await?;
    wait_turns(&mut app, &thread.id, 1).await?;
    scripted.first("guardian").await?;

    // A review sub-agent.
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let request_id = app
        .send_raw_request(
            "review/start",
            Some(json!({
                "threadId": thread.id,
                "target": { "type": "commit", "sha": "1234567deadbeef", "title": "Probe" },
            })),
        )
        .await?;
    let _: ReviewStartResponse = timeout(READ_TIMEOUT, app.read_response(request_id)).await??;
    scripted.first("review").await?;

    let mut firsts = Vec::new();
    for name in ["root", "child", "workflow_agent", "guardian", "review"] {
        firsts.push((name, scripted.first(name).await?));
    }
    Ok(firsts)
}

/// R1.2–R1.4, R6.5, T-1: with the flag on, a root thread and a MultiAgentV2 child get the
/// wakeup tools, while a workflow agent, a Guardian reviewer and a review sub-agent never do;
/// with the flag off nobody gets them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tools_follow_the_flag_and_the_thread_source() -> Result<()> {
    // Each run has its own mock and app-server; together they keep the test short.
    let (on, off) = tokio::try_join!(
        requests_of_every_thread_source(/*enabled*/ true),
        requests_of_every_thread_source(/*enabled*/ false),
    )?;
    for (enabled, requests) in [(true, on), (false, off)] {
        let offered = requests
            .into_iter()
            .map(|(name, body)| (name, offers_wakeups(&body)))
            .collect::<Vec<_>>();
        assert_eq!(
            offered,
            vec![
                ("root", enabled),
                ("child", enabled),
                ("workflow_agent", false),
                ("guardian", false),
                ("review", false),
            ],
            "flag {enabled}"
        );
    }
    Ok(())
}
