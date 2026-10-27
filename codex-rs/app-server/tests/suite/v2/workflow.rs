//! Fork: `thread/workflow/*` and `thread/workflow/updated` over dynamic workflow runs
//! (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.2, R8.3; UC9 R9.1, R9.4).
//!
//! One scripted Responses mock serves the parent thread and every workflow agent; requests
//! are routed by their content, so agents may answer in any order.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ItemCompletedNotification;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadItemPayload;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadWorkflowCancelParams;
use codex_app_server_protocol::ThreadWorkflowCancelResponse;
use codex_app_server_protocol::ThreadWorkflowListParams;
use codex_app_server_protocol::ThreadWorkflowListResponse;
use codex_app_server_protocol::ThreadWorkflowReadParams;
use codex_app_server_protocol::ThreadWorkflowReadResponse;
use codex_app_server_protocol::ThreadWorkflowUpdatedNotification;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_app_server_protocol::WorkflowAgentState;
use codex_app_server_protocol::WorkflowItem;
use codex_app_server_protocol::WorkflowRun;
use codex_app_server_protocol::WorkflowRunStatus;
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
const AGENT_MARKER: &str = "You are a workflow agent started by an orchestration script";
const NOTIFICATION_MARKER: &str = "<workflow_notification>";
const WORKFLOW_CALL_ID: &str = "wf-call-1";

type Predicate = Box<dyn Fn(&str) -> bool + Send + Sync>;

struct Rule {
    name: &'static str,
    matches: Predicate,
    body: String,
    remaining: usize,
    delay: Duration,
}

/// Serves each request with the first matching rule that has uses left.
#[derive(Clone, Default)]
struct ScriptedResponses {
    rules: Arc<Mutex<Vec<Rule>>>,
    served: Arc<Mutex<Vec<&'static str>>>,
}

impl Respond for ScriptedResponses {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body = String::from_utf8_lossy(&request.body);
        let mut rules = self.rules.lock().expect("rules");
        let Some(rule) = rules
            .iter_mut()
            .find(|rule| rule.remaining > 0 && (rule.matches)(&body))
        else {
            self.served.lock().expect("served").push("unmatched");
            return ResponseTemplate::new(500).set_body_string("no scripted response");
        };
        rule.remaining -= 1;
        self.served.lock().expect("served").push(rule.name);
        responses::sse_response(rule.body.clone()).set_delay(rule.delay)
    }
}

impl ScriptedResponses {
    async fn mount(server: &MockServer) -> Self {
        let responses = Self::default();
        Mock::given(method("POST"))
            .and(path_regex(".*/responses$"))
            .respond_with(responses.clone())
            .mount(server)
            .await;
        responses
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

    /// The parent starts `script`, then answers the tool result and the notification.
    fn parent_runs_workflow(&self, script: &str) {
        let arguments = json!({ "script": script }).to_string();
        self.rule(
            "parent_start",
            1,
            Duration::ZERO,
            |body| {
                is_parent(body)
                    && !body.contains("function_call_output")
                    && !body.contains(NOTIFICATION_MARKER)
            },
            responses::sse(vec![
                responses::ev_response_created("resp-parent-start"),
                responses::ev_function_call(WORKFLOW_CALL_ID, "workflow", &arguments),
                responses::ev_completed("resp-parent-start"),
            ]),
        );
        self.rule(
            "parent_after_tool",
            1,
            Duration::ZERO,
            |body| {
                is_parent(body)
                    && has_call_output(body, WORKFLOW_CALL_ID)
                    && !body.contains(NOTIFICATION_MARKER)
            },
            message("resp-parent-after-tool", "Started the workflow."),
        );
        self.rule(
            "parent_notified",
            usize::MAX,
            Duration::ZERO,
            |body| is_parent(body) && body.contains(NOTIFICATION_MARKER),
            message("resp-parent-notified", "The workflow finished."),
        );
    }

    /// A workflow agent whose task contains `marker` answers `text` after `delay`.
    fn agent(&self, marker: &'static str, text: &str, delay: Duration) {
        self.rule(
            marker,
            1,
            delay,
            move |body| body.contains(AGENT_MARKER) && body.contains(marker),
            message(&format!("resp-{marker}"), text),
        );
    }

    fn served(&self, name: &str) -> usize {
        self.served
            .lock()
            .expect("served")
            .iter()
            .filter(|served| **served == name)
            .count()
    }
}

fn is_parent(body: &str) -> bool {
    !body.contains(AGENT_MARKER)
}

fn has_call_output(body: &str, call_id: &str) -> bool {
    serde_json::from_str::<Value>(body).is_ok_and(|body| {
        body["input"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        })
    })
}

fn message(response_id: &str, text: &str) -> String {
    let mut answer = responses::ev_assistant_message(&format!("msg-{response_id}"), text);
    answer["item"]["phase"] = json!("final_answer");
    responses::sse(vec![
        responses::ev_response_created(response_id),
        answer,
        responses::ev_completed(response_id),
    ])
}

async fn start_app_server(server: &MockServer, codex_home: &TempDir) -> Result<TestAppServer> {
    start_app_server_with_args(server, codex_home, &[]).await
}

async fn start_app_server_with_args(
    server: &MockServer,
    codex_home: &TempDir,
    args: &[&str],
) -> Result<TestAppServer> {
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::MultiAgentV2)
        .enable_feature(Feature::Workflows)
        .with_provider_config("supports_websockets = false")
        .with_extra_config("[workflows]\nrequire_approval = \"never\"")
        .write(codex_home.path())?;
    TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .with_args(args)
        .build_initialized()
        .await
}

/// A Code Mode host listening for gRPC on a local TCP port, and its URL.
async fn start_remote_code_mode_host() -> Result<(tokio::process::Child, String)> {
    let mut host =
        tokio::process::Command::new(codex_utils_cargo_bin::cargo_bin("codex-code-mode-host")?)
            .args(["--listen", "grpc://127.0.0.1:0"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(/*kill_on_drop*/ true)
            .spawn()?;
    let stdout = host.stdout.take().expect("host stdout");
    let url = timeout(
        READ_TIMEOUT,
        tokio::io::AsyncBufReadExt::lines(tokio::io::BufReader::new(stdout)).next_line(),
    )
    .await??
    .expect("the host publishes its URL");
    Ok((host, url))
}

/// Starts a turn that launches the scripted workflow and returns the thread id, the turn
/// id and the start item of the run.
async fn launch_workflow(app: &mut TestAppServer) -> Result<(String, String, WorkflowItem)> {
    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let TurnStartResponse { turn } = app
        .request(|request_id| ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "run the workflow".to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await?;
    let started = loop {
        let completed: ItemCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("item/completed")).await??;
        if let ThreadItemPayload::Workflow(item) = completed.item.payload {
            assert_eq!(completed.turn_id, turn.id);
            break item;
        }
    };
    Ok((thread.id, turn.id, started))
}

/// Reads `thread/workflow/updated` for `run_id` until its terminal snapshot.
async fn updates_until_terminal(app: &mut TestAppServer, run_id: &str) -> Result<Vec<WorkflowRun>> {
    let mut updates = Vec::new();
    loop {
        let update: ThreadWorkflowUpdatedNotification = timeout(
            READ_TIMEOUT,
            app.read_notification("thread/workflow/updated"),
        )
        .await??;
        if update.run.run_id != run_id {
            continue;
        }
        let terminal = update.run.status.is_terminal();
        updates.push(update.run);
        if terminal {
            return Ok(updates);
        }
    }
}

/// Waits for `count` completed turns of `thread_id`; workflow agents' turns are skipped.
async fn wait_for_turns(app: &mut TestAppServer, thread_id: &str, count: usize) -> Result<()> {
    let mut completed_turns = 0;
    while completed_turns < count {
        let completed: TurnCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("turn/completed")).await??;
        if completed.thread_id == thread_id {
            completed_turns += 1;
        }
    }
    Ok(())
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

const TWO_AGENT_SCRIPT: &str = r#"export const meta = { name: 'two-agents', description: 'Two agents in sequence', phases: ['Work'] };
phase('Work');
const first = await agent('TASK-FIRST', { label: 'first' });
log('first done');
const second = await agent('TASK-SECOND', { label: 'second' });
return { first, second };
"#;

/// R8.2, R8.3: a run streams `thread/workflow/updated` with growing revisions and one
/// terminal snapshot; `list` and `read` show it, and `thread/read` replays the start item
/// in the launching turn and the notification in the turn that received it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workflow_run_streams_updates_and_replays_its_items() -> Result<()> {
    let server = responses::start_mock_server().await;
    let scripted = ScriptedResponses::mount(&server).await;
    scripted.parent_runs_workflow(TWO_AGENT_SCRIPT);
    // The first agent answers late enough for the run to be updated more than once.
    scripted.agent("TASK-FIRST", "first answer", Duration::from_millis(600));
    scripted.agent("TASK-SECOND", "second answer", Duration::ZERO);
    let codex_home = TempDir::new()?;
    let mut app = start_app_server(&server, &codex_home).await?;

    let (thread_id, launch_turn_id, started) = launch_workflow(&mut app).await?;
    assert_eq!(started.id, WORKFLOW_CALL_ID);
    assert_eq!(started.name, "two-agents");
    assert_eq!(started.status, WorkflowRunStatus::Running);
    assert_eq!(started.notification, None);
    let run_id = started.run_id.clone();

    let updates = updates_until_terminal(&mut app, &run_id).await?;
    assert!(updates.len() >= 2, "{updates:#?}");
    assert!(
        updates
            .windows(2)
            .all(|pair| pair[0].revision < pair[1].revision),
        "revisions must grow: {updates:#?}"
    );
    let terminal = updates.last().expect("terminal update");
    assert_eq!(
        terminal.status,
        WorkflowRunStatus::Completed,
        "{terminal:#?}"
    );
    assert_eq!(terminal.agent_counts.completed, 2);
    assert_eq!(terminal.logs_tail, vec!["first done".to_string()]);
    assert!(updates.iter().all(|run| run.agents.is_none()));
    assert_eq!(
        updates
            .iter()
            .filter(|run| run.status.is_terminal())
            .count(),
        1
    );

    // The launching turn and the turn started by the completion notification, which
    // shows the run's terminal item live.
    wait_for_turns(&mut app, &thread_id, 2).await?;
    assert_eq!(scripted.served("parent_notified"), 1);
    let finished = loop {
        let completed: ItemCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("item/completed")).await??;
        if let ThreadItemPayload::Workflow(item) = completed.item.payload
            && item.status.is_terminal()
        {
            assert_eq!(completed.thread_id, thread_id);
            assert_ne!(completed.turn_id, launch_turn_id);
            break item;
        }
    };

    let ThreadWorkflowListResponse { runs } = app
        .request(|request_id| ClientRequest::ThreadWorkflowList {
            request_id,
            params: ThreadWorkflowListParams {
                thread_id: thread_id.clone(),
            },
        })
        .await?;
    assert_eq!(runs, vec![terminal.clone()]);

    let ThreadWorkflowReadResponse { run, result } = app
        .request(|request_id| ClientRequest::ThreadWorkflowRead {
            request_id,
            params: ThreadWorkflowReadParams {
                thread_id: thread_id.clone(),
                run_id: run_id.clone(),
                include_result: true,
            },
        })
        .await?;
    let agents = run.agents.clone().expect("read returns the agent calls");
    assert_eq!(
        agents
            .iter()
            .map(|agent| (agent.seq, agent.label.as_deref(), agent.state))
            .collect::<Vec<_>>(),
        vec![
            (1, Some("first"), WorkflowAgentState::Completed),
            (2, Some("second"), WorkflowAgentState::Completed),
        ]
    );
    assert!(agents.iter().all(|agent| agent.thread_id.is_some()));
    assert_eq!(
        WorkflowRun {
            agents: None,
            ..run
        },
        terminal.clone()
    );
    assert_eq!(
        serde_json::from_str::<Value>(&result.expect("read returns the result"))?,
        json!({ "first": "first answer", "second": "second answer" })
    );

    // R9.4: cancelling a finished run returns it unchanged.
    let ThreadWorkflowCancelResponse { run } = app
        .request(|request_id| ClientRequest::ThreadWorkflowCancel {
            request_id,
            params: ThreadWorkflowCancelParams {
                thread_id: thread_id.clone(),
                run_id: run_id.clone(),
            },
        })
        .await?;
    assert_eq!(run, terminal.clone());

    assert_eq!(
        request_error(
            &mut app,
            "thread/workflow/read",
            json!({ "threadId": thread_id, "runId": "wf-missing" }),
        )
        .await?,
        "workflow run not found: wf-missing"
    );
    let unknown_thread = codex_protocol::ThreadId::new().to_string();
    assert_eq!(
        request_error(
            &mut app,
            "thread/workflow/list",
            json!({ "threadId": unknown_thread }),
        )
        .await?,
        format!("thread not loaded: {unknown_thread}")
    );

    // R8.3: the start item in the launching turn, the notification in the next turn.
    let ThreadReadResponse { thread, .. } = app
        .request(|request_id| ClientRequest::ThreadRead {
            request_id,
            params: ThreadReadParams {
                thread_id: thread_id.clone(),
                include_turns: true,
            },
        })
        .await?;
    let workflow_items = |turn_id: &str| -> Vec<WorkflowItem> {
        thread
            .turns
            .iter()
            .filter(|turn| turn.id == turn_id)
            .flat_map(|turn| turn.items.iter())
            .filter_map(|item| match &item.payload {
                ThreadItemPayload::Workflow(item) => Some(item.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(workflow_items(&launch_turn_id), vec![started]);
    let notified_turn = thread
        .turns
        .iter()
        .find(|turn| turn.id != launch_turn_id)
        .expect("the notification started a turn");
    let notified = workflow_items(&notified_turn.id);
    assert_eq!(notified, vec![finished], "{:#?}", thread.turns);
    assert_eq!(notified[0].id, format!("{run_id}-notification"));
    assert_eq!(notified[0].run_id, run_id);
    assert_eq!(notified[0].name, "two-agents");
    assert_eq!(notified[0].status, WorkflowRunStatus::Completed);
    let text = notified[0]
        .notification
        .as_deref()
        .expect("notification text");
    assert!(
        text.starts_with(&format!("Workflow \"two-agents\" ({run_id}) completed in ")),
        "{text}"
    );
    assert!(text.contains("2 agents (2 ok, 0 null)"), "{text}");
    Ok(())
}

const SLOW_AGENT_SCRIPT: &str = r#"export const meta = { name: 'slow-agent', description: 'One agent that never answers in time' };
return await agent('TASK-SLOW', { label: 'slow' });
"#;

/// R9.1: `thread/workflow/cancel` interrupts a running agent and settles the run as
/// `cancelled`, which reaches subscribers as the terminal snapshot.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workflow_cancel_settles_a_running_run() -> Result<()> {
    let server = responses::start_mock_server().await;
    let scripted = ScriptedResponses::mount(&server).await;
    scripted.parent_runs_workflow(SLOW_AGENT_SCRIPT);
    scripted.agent("TASK-SLOW", "too late", Duration::from_secs(120));
    let codex_home = TempDir::new()?;
    let mut app = start_app_server(&server, &codex_home).await?;

    let (thread_id, _, started) = launch_workflow(&mut app).await?;
    let run_id = started.run_id;
    // Cancel once the agent is working.
    loop {
        let update: ThreadWorkflowUpdatedNotification = timeout(
            READ_TIMEOUT,
            app.read_notification("thread/workflow/updated"),
        )
        .await??;
        if update.run.run_id == run_id && update.run.agent_counts.running == 1 {
            break;
        }
    }

    let ThreadWorkflowCancelResponse { run: cancelled } = app
        .request(|request_id| ClientRequest::ThreadWorkflowCancel {
            request_id,
            params: ThreadWorkflowCancelParams {
                thread_id: thread_id.clone(),
                run_id: run_id.clone(),
            },
        })
        .await?;
    assert_eq!(
        cancelled.status,
        WorkflowRunStatus::Cancelled,
        "{cancelled:#?}"
    );
    assert_eq!(cancelled.agent_counts.null_results, 1);
    assert_eq!(cancelled.agent_counts.running, 0);
    assert_eq!(
        cancelled.null_reasons.get("cancelled").copied(),
        Some(1),
        "{cancelled:#?}"
    );
    assert!(cancelled.finished_at_ms.is_some());

    let updates = updates_until_terminal(&mut app, &run_id).await?;
    assert_eq!(updates.last(), Some(&cancelled));

    let ThreadWorkflowReadResponse { run, .. } = app
        .request(|request_id| ClientRequest::ThreadWorkflowRead {
            request_id,
            params: ThreadWorkflowReadParams {
                thread_id: thread_id.clone(),
                run_id: run_id.clone(),
                include_result: false,
            },
        })
        .await?;
    let agents = run.agents.expect("agent calls");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].state, WorkflowAgentState::Null);
    assert_eq!(agents[0].reason.as_deref(), Some("cancelled"));
    assert_eq!(scripted.served("TASK-SLOW"), 1);
    Ok(())
}

/// R2.1, R2.2 (stage 2): with `--code-mode-host`, runs execute on a remote gRPC Code Mode
/// host: the script's host calls arrive as gRPC tool calls, agents answer and the run
/// completes with its snapshots streamed to the client.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workflow_runs_on_a_remote_code_mode_host() -> Result<()> {
    let (_host, host_url) = start_remote_code_mode_host().await?;
    let server = responses::start_mock_server().await;
    let scripted = ScriptedResponses::mount(&server).await;
    scripted.parent_runs_workflow(TWO_AGENT_SCRIPT);
    scripted.agent("TASK-FIRST", "first answer", Duration::from_millis(300));
    scripted.agent("TASK-SECOND", "second answer", Duration::ZERO);
    let codex_home = TempDir::new()?;
    let mut app =
        start_app_server_with_args(&server, &codex_home, &["--code-mode-host", &host_url]).await?;

    let (thread_id, _, started) = launch_workflow(&mut app).await?;
    let updates = updates_until_terminal(&mut app, &started.run_id).await?;
    let terminal = updates.last().expect("terminal update");
    assert_eq!(
        terminal.status,
        WorkflowRunStatus::Completed,
        "{terminal:#?}"
    );
    assert_eq!(terminal.agent_counts.completed, 2);
    assert_eq!(terminal.logs_tail, vec!["first done".to_string()]);
    let ThreadWorkflowReadResponse { result, .. } = app
        .request(|request_id| ClientRequest::ThreadWorkflowRead {
            request_id,
            params: ThreadWorkflowReadParams {
                thread_id: thread_id.clone(),
                run_id: started.run_id.clone(),
                include_result: true,
            },
        })
        .await?;
    assert_eq!(
        serde_json::from_str::<Value>(&result.expect("result"))?,
        json!({ "first": "first answer", "second": "second answer" })
    );
    wait_for_turns(&mut app, &thread_id, 2).await?;
    assert_eq!(scripted.served("parent_notified"), 1);
    Ok(())
}

/// R2.9 (stage 2): when the remote Code Mode host goes away during a run, the run fails
/// with the session error and its running agent is stopped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn workflow_fails_when_the_remote_host_goes_away() -> Result<()> {
    let (mut host, host_url) = start_remote_code_mode_host().await?;
    let server = responses::start_mock_server().await;
    let scripted = ScriptedResponses::mount(&server).await;
    scripted.parent_runs_workflow(SLOW_AGENT_SCRIPT);
    scripted.agent("TASK-SLOW", "too late", Duration::from_secs(120));
    let codex_home = TempDir::new()?;
    let mut app =
        start_app_server_with_args(&server, &codex_home, &["--code-mode-host", &host_url]).await?;

    let (_, _, started) = launch_workflow(&mut app).await?;
    loop {
        let update: ThreadWorkflowUpdatedNotification = timeout(
            READ_TIMEOUT,
            app.read_notification("thread/workflow/updated"),
        )
        .await??;
        if update.run.run_id == started.run_id && update.run.agent_counts.running == 1 {
            break;
        }
    }
    host.kill().await?;

    let updates = updates_until_terminal(&mut app, &started.run_id).await?;
    let terminal = updates.last().expect("terminal update");
    assert_eq!(terminal.status, WorkflowRunStatus::Failed, "{terminal:#?}");
    assert!(
        terminal
            .error
            .as_deref()
            .is_some_and(|error| error.contains("code-mode")),
        "{terminal:#?}"
    );
    assert_eq!(terminal.agent_counts.running, 0, "{terminal:#?}");
    assert_eq!(terminal.agent_counts.null_results, 1, "{terminal:#?}");
    Ok(())
}
