//! One workflow run: its state, its Code Mode cell and its lifecycle (UC2, UC5, UC7, UC9).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_code_mode::CellId;
use codex_code_mode::CodeModeSession;
use codex_code_mode::CodeModeSessionCellExecutionLimits;
use codex_code_mode::CodeModeSessionDelegate;
use codex_code_mode::CodeModeToolKind;
use codex_code_mode::ExecuteRequest;
use codex_code_mode::RuntimeResponse;
use codex_code_mode::ToolDefinition;
use codex_code_mode::WaitRequest;
use codex_protocol::ThreadId;
use codex_protocol::protocol::MultiAgentVersion;
use codex_tools::ToolName;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_text;
use codex_workflow::HOST_TOOLS;
use codex_workflow::Journal;
use codex_workflow::JournalEvent;
use codex_workflow::MAX_RESULT_BYTES;
use codex_workflow::RunSnapshot;
use codex_workflow::RunStatus;
use codex_workflow::RunTracker;
use codex_workflow::TokenCharge;
use codex_workflow::WorkflowMeta;
use codex_workflow::display_error;
use codex_workflow::render_notification;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;
use tokio::sync::Notify;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::delegate::WorkflowCellDelegate;
use super::gate::OrderedGate;
use super::now_ms;
use super::registry::WorkflowRegistry;
use crate::agent::api::AgentTarget;
use crate::session::session::Session;
use crate::session::step_context::StepContext;

/// How long one `execute` or `wait` observation of the cell lasts; the run loops on it.
const CELL_YIELD_TIME_MS: u64 = 30_000;
/// How long cancellation waits for in-flight calls to interrupt their agents.
const CANCEL_GRACE: Duration = Duration::from_secs(5);

/// Limits of one run, resolved from `[workflows]` and the tool call (UC4, UC5).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunLimits {
    pub(crate) max_concurrent_agents: usize,
    pub(crate) max_agents_per_run: usize,
    pub(crate) capacity_wait_ms: u64,
    pub(crate) budget_tokens: Option<u64>,
    pub(crate) notification_max_tokens: usize,
    pub(crate) max_heap_bytes: usize,
}

/// A run's state shared by the run task, its cell delegate, tools and clients.
pub(crate) struct WorkflowRun {
    pub(crate) id: String,
    pub(crate) meta: WorkflowMeta,
    pub(crate) limits: RunLimits,
    pub(crate) parent_turn_id: String,
    tracker: Mutex<RunTracker>,
    revision: watch::Sender<u64>,
    updates: broadcast::Sender<String>,
    result_json: Mutex<Option<String>>,
    script_error: Mutex<Option<String>>,
    journal: Journal,
    pub(crate) gate: OrderedGate,
    /// Agents this run started: its only start limit (UC4, R4.2), not a `TurnSpawnBudget`.
    started_agents: AtomicUsize,
    charged_tokens: AtomicU64,
    /// Cancels queued calls, running agents and the cell.
    cancel: CancellationToken,
    cancel_status: Mutex<Option<RunStatus>>,
    children: Mutex<HashMap<u64, ThreadId>>,
    key_ordinals: Mutex<HashMap<String, u64>>,
    /// Signalled when an agent of this run finishes, so capacity waits retry.
    pub(crate) capacity_freed: Notify,
    finished: watch::Sender<bool>,
}

/// Input of [`start_run`] after static validation.
pub(crate) struct StartRunRequest {
    pub(crate) meta: WorkflowMeta,
    pub(crate) script: String,
    pub(crate) body: String,
    pub(crate) args_json: String,
    pub(crate) budget_tokens: Option<u64>,
}

impl WorkflowRun {
    pub(crate) fn status(&self) -> RunStatus {
        self.tracker().status()
    }

    fn tracker(&self) -> std::sync::MutexGuard<'_, RunTracker> {
        self.tracker.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// UC8: the run snapshot; the per-call list only with `include_agents`.
    pub(crate) fn snapshot(&self, include_agents: bool) -> RunSnapshot {
        self.tracker().snapshot(include_agents)
    }

    pub(crate) fn subscribe_revision(&self) -> watch::Receiver<u64> {
        self.revision.subscribe()
    }

    /// Apply a change to the snapshot and publish the new revision.
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut RunTracker) -> T) -> T {
        let (value, revision) = {
            let mut tracker = self.tracker();
            let value = change(&mut tracker);
            (value, tracker.revision())
        };
        self.revision.send_replace(revision);
        let _ = self.updates.send(self.id.clone());
        value
    }

    /// UC6: append to the journal; failures are logged, the run goes on.
    pub(crate) fn journal(&self, event: JournalEvent) {
        if let Err(error) = self.journal.append(event) {
            warn!(run_id = %self.id, %error, "failed to append to the workflow journal");
        }
    }

    pub(crate) fn journal_dir(&self) -> Option<std::path::PathBuf> {
        self.journal.dir().map(std::path::Path::to_path_buf)
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }

    /// UC9: stop the run; the first requested status wins.
    pub(crate) fn cancel(&self, status: RunStatus) {
        {
            let mut cancel_status = self
                .cancel_status
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if cancel_status.is_none() && !self.status().is_terminal() {
                *cancel_status = Some(status);
            }
        }
        self.cancel.cancel();
    }

    /// The status a cancellation asked for, before the run settles.
    pub(crate) fn cancel_status(&self) -> Option<RunStatus> {
        *self
            .cancel_status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The full result as JSON text.
    pub(crate) fn result_json(&self) -> Option<String> {
        self.result_json
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// UC5: whether charged tokens reached the run's limit.
    pub(crate) fn budget_exhausted(&self) -> bool {
        self.limits
            .budget_tokens
            .is_some_and(|limit| self.charged_tokens.load(Ordering::Acquire) >= limit)
    }

    pub(crate) fn charge(&self, tokens: TokenCharge) {
        self.charged_tokens
            .fetch_add(tokens.charged, Ordering::AcqRel);
    }

    /// UC5, R5.1: `budget.used()` and `budget.remaining()` of the script.
    pub(crate) fn budget_json(&self) -> JsonValue {
        let used = self.charged_tokens.load(Ordering::Acquire);
        json!({
            "limit": self.limits.budget_tokens,
            "used": used,
            "remaining": self.limits.budget_tokens.map(|limit| limit.saturating_sub(used)),
        })
    }

    /// UC4, R4.2, R4.4: reserve one of the run's agent starts before a spawn; a spawned agent
    /// keeps it, a failed spawn returns it with [`Self::release_agent_start`].
    pub(crate) fn reserve_agent_start(&self) -> bool {
        self.started_agents
            .try_update(Ordering::AcqRel, Ordering::Acquire, |started| {
                (started < self.limits.max_agents_per_run).then_some(started + 1)
            })
            .is_ok()
    }

    /// Return a start reserved by a call that did not spawn.
    pub(crate) fn release_agent_start(&self) {
        let _ = self
            .started_agents
            .try_update(Ordering::AcqRel, Ordering::Acquire, |started| {
                Some(started.saturating_sub(1))
            });
    }

    /// UC6, R6.2: ordinal of a call among the run's calls with the same key.
    pub(crate) fn next_ordinal(&self, key: &str) -> u64 {
        let mut ordinals = self
            .key_ordinals
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let ordinal = ordinals.entry(key.to_string()).or_default();
        let current = *ordinal;
        *ordinal += 1;
        current
    }

    pub(crate) fn register_child(&self, seq: u64, thread_id: ThreadId) {
        self.children
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(seq, thread_id);
    }

    pub(crate) fn remove_child(&self, seq: u64) {
        self.children
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&seq);
        self.capacity_freed.notify_waiters();
    }

    fn live_children(&self) -> Vec<(u64, ThreadId)> {
        self.children
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(seq, thread_id)| (*seq, *thread_id))
            .collect()
    }

    /// Wait until the run task finished and delivered its notification.
    pub(crate) async fn wait_finished(&self) {
        let mut finished = self.finished.subscribe();
        let _ = finished.wait_for(|finished| *finished).await;
    }

    /// UC2: record a `phase`, `log`, result or script error from the cell.
    pub(crate) fn record_event(&self, input: Option<JsonValue>) -> Result<JsonValue, String> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case")]
        enum CellEvent {
            Phase { seq: u64, title: String },
            Log { message: String },
            Result { value_json: String },
            Error { message: String },
        }
        let event: CellEvent = serde_json::from_value(input.unwrap_or(JsonValue::Null))
            .map_err(|error| format!("invalid workflow event: {error}"))?;
        match event {
            CellEvent::Phase { seq, title } => {
                let title = codex_workflow::truncate_chars(&title, 120);
                self.update(|tracker| tracker.set_phase(seq, &title));
                self.journal(JournalEvent::Phase { title });
            }
            CellEvent::Log { message } => {
                let message = self.update(|tracker| tracker.log(&message));
                self.journal(JournalEvent::Log { message });
            }
            CellEvent::Result { value_json } => {
                if value_json.len() > MAX_RESULT_BYTES {
                    *self
                        .script_error
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(format!(
                        "the workflow result has {} bytes of JSON; the limit is {MAX_RESULT_BYTES}",
                        value_json.len()
                    ));
                } else {
                    *self
                        .result_json
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(value_json);
                }
            }
            CellEvent::Error { message } => {
                *self
                    .script_error
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(display_error(&message));
            }
        }
        Ok(json!({}))
    }
}

/// Host tools of a workflow cell.
fn host_tool_definitions() -> Vec<ToolDefinition> {
    HOST_TOOLS
        .iter()
        .map(|name| ToolDefinition {
            name: (*name).to_string(),
            tool_name: ToolName::plain(*name),
            description: "Workflow host callback.".to_string(),
            kind: CodeModeToolKind::Function,
            input_schema: None,
            input_schema_max_bytes: None,
            output_schema: None,
        })
        .collect()
}

/// UC1: register a run, persist its script and args and start it in the background.
pub(crate) fn start_run(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    request: StartRunRequest,
) -> Result<Arc<WorkflowRun>, String> {
    let registry = WorkflowRegistry::for_session(session);
    let config = &step_context.turn.config;
    let workflows = config.workflows;
    let active = registry.active_run_ids();
    if active.len() >= workflows.max_active_runs {
        return Err(format!(
            "this thread already has {} active workflow runs ({}); wait for their notifications or cancel one with workflow_cancel",
            active.len(),
            active.join(", ")
        ));
    }
    let run_id = registry.allocate_id();
    let journal = if config.ephemeral {
        Journal::in_memory()
    } else {
        let dir = config
            .codex_home
            .join("workflows")
            .join(session.thread_id.to_string())
            .join(&run_id);
        Journal::create(dir.as_path(), &request.script, &request.args_json)
            .map_err(|error| format!("failed to create the workflow journal: {error}"))?
    };
    let limits = RunLimits {
        max_concurrent_agents: workflows.max_concurrent_agents,
        max_agents_per_run: workflows.max_agents_per_run,
        capacity_wait_ms: workflows.capacity_wait_ms,
        budget_tokens: request.budget_tokens.or(workflows.default_budget_tokens),
        notification_max_tokens: workflows.notification_max_tokens,
        max_heap_bytes: workflows.max_heap_bytes,
    };
    let tracker = RunTracker::new(
        run_id.clone(),
        request.meta.name.clone(),
        request.meta.description.clone(),
        &request.meta.phases,
        limits.budget_tokens,
        now_ms(),
    );
    let revision = tracker.revision();
    let run = Arc::new(WorkflowRun {
        id: run_id.clone(),
        meta: request.meta,
        limits,
        parent_turn_id: step_context.turn.sub_id.clone(),
        tracker: Mutex::new(tracker),
        revision: watch::channel(revision).0,
        updates: registry.update_sender(),
        result_json: Mutex::new(None),
        script_error: Mutex::new(None),
        journal,
        gate: OrderedGate::new(limits.max_concurrent_agents),
        started_agents: AtomicUsize::new(0),
        charged_tokens: AtomicU64::new(0),
        cancel: CancellationToken::new(),
        cancel_status: Mutex::new(None),
        children: Mutex::new(HashMap::new()),
        key_ordinals: Mutex::new(HashMap::new()),
        capacity_freed: Notify::new(),
        finished: watch::channel(false).0,
    });
    run.journal(JournalEvent::RunStarted {
        run_id: run_id.clone(),
        name: run.meta.name.clone(),
        script_sha256: codex_workflow::sha256_hex(request.script.as_bytes()),
        args_sha256: codex_workflow::sha256_hex(request.args_json.as_bytes()),
        parent_turn_id: run.parent_turn_id.clone(),
        resume_from: None,
    });
    registry.insert(Arc::clone(&run));
    let _ = run.updates.send(run_id);
    let source =
        codex_workflow::build_cell_source(&request.body, &request.args_json, limits.budget_tokens);
    tokio::spawn(run_main(
        Arc::clone(session),
        Arc::clone(step_context),
        Arc::clone(&run),
        source,
    ));
    Ok(run)
}

enum CellEnd {
    Completed { error_text: Option<String> },
    Terminated,
}

/// UC2: run the cell on the run's own delegate until it ends, then settle the run and
/// notify the parent (UC7).
async fn run_main(
    session: Arc<Session>,
    step_context: Arc<StepContext>,
    run: Arc<WorkflowRun>,
    source: String,
) {
    let outcome = execute_cell(&session, &step_context, &run, source).await;
    // Calls the script left running end with its cell.
    run.cancel.cancel();
    interrupt_children(&session, &run).await;
    let script_error = run
        .script_error
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let (status, error) = match (run.cancel_status(), outcome) {
        (Some(status), _) => (status, None),
        (None, Err(error)) => (RunStatus::Failed, Some(error)),
        (None, Ok(_)) if script_error.is_some() => (RunStatus::Failed, script_error),
        (
            None,
            Ok(CellEnd::Completed {
                error_text: Some(error),
            }),
        ) => (RunStatus::Failed, Some(display_error(&error))),
        (None, Ok(CellEnd::Completed { error_text: None })) if run.result_json().is_some() => {
            (RunStatus::Completed, None)
        }
        (None, Ok(_)) => (
            RunStatus::Failed,
            Some("the workflow cell ended without a result".to_string()),
        ),
    };
    finish_run(&session, &run, status, error).await;
}

async fn execute_cell(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    run: &Arc<WorkflowRun>,
    source: String,
) -> Result<CellEnd, String> {
    let provider = session.services.code_mode_service.session_provider();
    let code_mode = provider
        .create_session_with_limits(CodeModeSessionCellExecutionLimits {
            max_yield_time_ms: None,
            max_heap_size_bytes: Some(run.limits.max_heap_bytes),
        })
        .await
        .map_err(|error| format!("failed to start the workflow cell: {error}"))?;
    let result = drive_cell(session, step_context, run, code_mode.as_ref(), source).await;
    if let Err(error) = code_mode.shutdown().await {
        warn!(run_id = %run.id, %error, "failed to shut down the workflow cell session");
    }
    result
}

async fn drive_cell(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    run: &Arc<WorkflowRun>,
    code_mode: &dyn CodeModeSession,
    source: String,
) -> Result<CellEnd, String> {
    let delegate: Arc<dyn CodeModeSessionDelegate> = Arc::new(WorkflowCellDelegate::new(
        Arc::clone(session),
        Arc::clone(step_context),
        Arc::clone(run),
    ));
    let request = ExecuteRequest {
        tool_call_id: format!("workflow-{}", run.id),
        enabled_tools: host_tool_definitions(),
        source,
        yield_time_ms: Some(CELL_YIELD_TIME_MS),
        max_output_tokens: Some(1_000),
    };
    let started = code_mode
        .execute(request, delegate, /*preempt*/ None)
        .await
        .map_err(|error| format!("failed to start the workflow cell: {error}"))?;
    let cell_id = started.cell_id.clone();
    let mut response = tokio::select! {
        response = started.initial_response() => response?,
        _ = run.cancel.cancelled() => return terminate_cell(session, run, code_mode, cell_id).await,
    };
    loop {
        match response {
            RuntimeResponse::Yielded { .. } => {
                let wait = code_mode.wait(
                    WaitRequest {
                        cell_id: cell_id.clone(),
                        yield_time_ms: CELL_YIELD_TIME_MS,
                    },
                    /*preempt*/ None,
                );
                response = tokio::select! {
                    outcome = wait => RuntimeResponse::from(outcome?),
                    _ = run.cancel.cancelled() => {
                        return terminate_cell(session, run, code_mode, cell_id).await;
                    }
                };
            }
            RuntimeResponse::Terminated { .. } => return Ok(CellEnd::Terminated),
            RuntimeResponse::Result { error_text, .. } => {
                return Ok(CellEnd::Completed { error_text });
            }
        }
    }
}

/// UC9: let in-flight calls interrupt their agents, then stop the cell.
async fn terminate_cell(
    session: &Arc<Session>,
    run: &Arc<WorkflowRun>,
    code_mode: &dyn CodeModeSession,
    cell_id: CellId,
) -> Result<CellEnd, String> {
    let deadline = tokio::time::Instant::now() + CANCEL_GRACE;
    while !run.live_children().is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    interrupt_children(session, run).await;
    if let Err(error) = code_mode.terminate(cell_id).await {
        warn!(run_id = %run.id, %error, "failed to terminate the workflow cell");
    }
    Ok(CellEnd::Terminated)
}

/// Interrupt agents whose calls did not settle, and record those calls as null.
async fn interrupt_children(session: &Arc<Session>, run: &Arc<WorkflowRun>) {
    for (seq, thread_id) in run.live_children() {
        if let Err(error) = session
            .services
            .agent_control
            .interrupt(
                session.thread_id,
                AgentTarget::Id(thread_id),
                MultiAgentVersion::V2,
            )
            .await
        {
            warn!(run_id = %run.id, %thread_id, %error, "failed to interrupt a workflow agent");
        }
        session
            .services
            .local_agent_runtime
            .workflow_children()
            .finish_call_for_thread(thread_id);
        run.remove_child(seq);
    }
    let reason = match run.cancel_status() {
        Some(RunStatus::Interrupted) => codex_workflow::null_reason::INTERRUPTED,
        _ => codex_workflow::null_reason::CANCELLED,
    };
    run.update(|tracker| tracker.settle_open_calls(reason));
}

/// UC7: settle the snapshot and journal, then deliver one notification to the parent.
async fn finish_run(
    session: &Arc<Session>,
    run: &Arc<WorkflowRun>,
    status: RunStatus,
    error: Option<String>,
) {
    let result_json = run.result_json();
    let preview = result_json.as_ref().map(|json| {
        truncate_text(
            json,
            TruncationPolicy::Tokens(run.limits.notification_max_tokens),
        )
    });
    let error = error.map(|error| truncate_text(&error, TruncationPolicy::Tokens(1_000)));
    let finished_at = now_ms();
    run.update(|tracker| tracker.finish(status, finished_at, preview.clone(), error.clone()));
    let snapshot = run.snapshot(/*include_agents*/ false);
    run.journal(JournalEvent::RunFinished {
        status: status.as_str().to_string(),
        result_bytes: result_json.as_ref().map_or(0, String::len),
        tokens: TokenCharge {
            charged: snapshot.charged_tokens,
            cached: snapshot.cached_tokens,
        },
        error,
    });
    // R8.5: runs by status, duration and charged tokens.
    if let Some(metrics) = codex_otel::global() {
        let tags = [("status", status.as_str())];
        let _ = metrics.counter("codex.workflow.runs", /*inc*/ 1, &tags);
        let _ = metrics.histogram(
            "codex.workflow.run_duration_ms",
            i64::try_from(snapshot.duration_ms(finished_at)).unwrap_or(i64::MAX),
            &tags,
        );
        let _ = metrics.histogram(
            "codex.workflow.charged_tokens",
            i64::try_from(snapshot.charged_tokens).unwrap_or(i64::MAX),
            &tags,
        );
    }
    let text = render_notification(&snapshot, preview.as_deref(), finished_at);
    session.deliver_workflow_notification(text).await;
    run.finished.send_replace(true);
}
