//! One `agent()` call of a workflow script (UC3, UC4, UC5, UC10).

use std::sync::Arc;
use std::time::Duration;

use codex_features::AgentIsolation;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::error::AgentErrorContext;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::CodexErrorInfo;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::turn_input::TurnStartOptions;
use codex_workflow::JournalEvent;
use codex_workflow::TokenCharge;
use codex_workflow::WORKFLOW_AGENT_INSTRUCTIONS;
use codex_workflow::call_key;
use codex_workflow::check_strict_compatible;
use codex_workflow::child_agent_name;
use codex_workflow::null_reason;
use codex_workflow::parse_final_message;
use codex_workflow::validate_value;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::WORKFLOW_TURN_TRIGGER;
use super::run::WorkflowRun;
use crate::agent::api::AgentInput;
use crate::agent::api::AgentTarget;
use crate::agent::api::SendRequest;
use crate::agent::child_config::build_agent_resume_config;
use crate::agent::control::workflow_children::WorkflowChildOutcome;
use crate::agent::types::AgentMessage;
use crate::agent::types::MessageDeliveryMode;
use crate::codex_thread::CodexThread;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::thread_spawn::ThreadSpawnError;
use crate::thread_spawn::ThreadSpawnRequest;
use crate::thread_spawn::WorkflowSpawnOptions;
use crate::thread_spawn::spawn_workflow_child;

/// Pause between attempts to find agent capacity (UC4, R4.3).
const CAPACITY_RETRY: Duration = Duration::from_millis(500);
/// How often a running agent is checked for a pending approval (UC10, R10.2).
const APPROVAL_POLL: Duration = Duration::from_secs(1);

const ALLOWED_OPTIONS: &str =
    "schema, model, effort, agent_type, label, phase, isolation, timeout_ms";

#[derive(Deserialize)]
struct AgentCallInput {
    seq: u64,
    prompt: String,
    #[serde(default)]
    opts: serde_json::Map<String, JsonValue>,
    #[serde(default)]
    phase: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentOptions {
    schema: Option<JsonValue>,
    model: Option<String>,
    effort: Option<ReasoningEffort>,
    agent_type: Option<String>,
    label: Option<String>,
    phase: Option<String>,
    isolation: Option<AgentIsolation>,
    timeout_ms: Option<u64>,
}

/// How a call ended, before it is recorded.
enum CallOutcome {
    Value(JsonValue),
    Null {
        reason: &'static str,
        detail: Option<String>,
    },
}

fn null(reason: &'static str, detail: Option<String>) -> CallOutcome {
    CallOutcome::Null { reason, detail }
}

fn script_error(kind: &str, message: String) -> JsonValue {
    json!({ "error": kind, "message": message })
}

/// UC3: run one `agent()` call and resolve it to the agent's answer or to null.
///
/// Script errors (unknown options, a non-strict schema) are returned as errors the prelude
/// throws, before any spawn; every runtime failure resolves to null with a recorded reason.
/// The result is journaled before it is returned to the script (R6.1).
pub(super) async fn run_agent_call(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    run: &Arc<WorkflowRun>,
    input: Option<JsonValue>,
    cell_cancel: CancellationToken,
) -> JsonValue {
    let input: AgentCallInput = match serde_json::from_value(input.unwrap_or(JsonValue::Null)) {
        Ok(input) => input,
        Err(error) => return script_error("type", format!("agent(): {error}")),
    };
    let seq = input.seq;
    let options: AgentOptions = match serde_json::from_value(JsonValue::Object(input.opts.clone()))
    {
        Ok(options) => options,
        Err(error) => {
            run.gate.ticket(seq).skip();
            return script_error(
                "type",
                format!("agent() options: {error}; allowed options: {ALLOWED_OPTIONS}"),
            );
        }
    };
    if input.prompt.trim().is_empty() {
        run.gate.ticket(seq).skip();
        return script_error("type", "agent(): the prompt must not be empty".to_string());
    }
    // R3.5: a schema strict structured output would refuse fails before any spawn.
    if let Some(schema) = &options.schema
        && let Err(error) = check_strict_compatible(schema)
    {
        run.gate.ticket(seq).skip();
        return script_error("schema", format!("agent() schema: {error}"));
    }
    let phase = options.phase.clone().or(input.phase);
    let key = call_key(&input.prompt, &input.opts);
    let ordinal = run.next_ordinal(&key);
    run.update(|tracker| tracker.agent_queued(seq, options.label.clone(), phase.clone()));
    let call = AgentCall {
        session,
        step_context,
        run,
        seq,
        prompt: input.prompt,
        options,
        phase,
        key,
        ordinal,
    };
    let (outcome, tokens) = call.execute(&cell_cancel).await;
    call.record(outcome, tokens)
}

struct AgentCall<'a> {
    session: &'a Arc<Session>,
    step_context: &'a Arc<StepContext>,
    run: &'a Arc<WorkflowRun>,
    seq: u64,
    prompt: String,
    options: AgentOptions,
    phase: Option<String>,
    key: String,
    ordinal: u64,
}

struct SpawnedChild {
    thread_id: ThreadId,
    path: AgentPath,
    outcomes: mpsc::UnboundedReceiver<WorkflowChildOutcome>,
}

enum Waited {
    Outcome(WorkflowChildOutcome),
    Interrupted,
    Gone,
    Timeout,
    Cancelled,
}

impl AgentCall<'_> {
    /// UC4: queue in script order, take a slot, then spawn and supervise the agent.
    async fn execute(&self, cell_cancel: &CancellationToken) -> (CallOutcome, TokenCharge) {
        let ticket = self.run.gate.ticket(self.seq);
        // R3.8: a worktree is available only with agent worktree isolation enabled; refuse
        // early, so the call neither waits for a slot nor spends an agent start.
        if let Err(error) = crate::agent::worktree_isolation::resolve_isolation(
            Some(self.isolation()),
            &self.step_context.turn.config,
        ) {
            ticket.skip();
            return (
                null(null_reason::ISOLATION_UNAVAILABLE, Some(error)),
                TokenCharge::default(),
            );
        }
        if self.run.is_cancelled() {
            ticket.skip();
            return (null(self.cancel_reason(), None), TokenCharge::default());
        }
        // R5.2: calls that have not started yet resolve to null once the budget is spent.
        if self.run.budget_exhausted() {
            ticket.skip();
            return (
                null(null_reason::BUDGET_EXHAUSTED, None),
                TokenCharge::default(),
            );
        }
        let permit = tokio::select! {
            biased;
            _ = self.run.cancellation().cancelled() => {
                return (null(self.cancel_reason(), None), TokenCharge::default());
            }
            _ = cell_cancel.cancelled() => {
                return (null(null_reason::CANCELLED, None), TokenCharge::default());
            }
            permit = ticket.acquire() => permit,
        };
        if self.run.is_cancelled() {
            return (null(self.cancel_reason(), None), TokenCharge::default());
        }
        if self.run.budget_exhausted() {
            return (
                null(null_reason::BUDGET_EXHAUSTED, None),
                TokenCharge::default(),
            );
        }
        // R4.4: the run's agent limit.
        if !self.run.reserve_agent_start() {
            return (
                null(
                    null_reason::AGENT_LIMIT,
                    Some(format!(
                        "the run already started {} agents",
                        self.run.limits.max_agents_per_run
                    )),
                ),
                TokenCharge::default(),
            );
        }
        let spawned = match self.spawn(cell_cancel).await {
            Ok(spawned) => spawned,
            Err(outcome) => {
                self.run.release_agent_start();
                return (outcome, TokenCharge::default());
            }
        };
        let result = self.supervise(spawned, cell_cancel).await;
        drop(permit);
        result
    }

    /// Workflow agents share the parent's checkout unless the call asks for a worktree; the
    /// configured default isolation of `spawn_agent` does not apply to them.
    fn isolation(&self) -> AgentIsolation {
        self.options.isolation.unwrap_or(AgentIsolation::None)
    }

    fn cancel_reason(&self) -> &'static str {
        if self.run.cancel_status() == Some(codex_workflow::RunStatus::Interrupted) {
            null_reason::INTERRUPTED
        } else {
            null_reason::CANCELLED
        }
    }

    /// UC3, R3.1: spawn a fresh child; wait for residency capacity when it is full (R4.3).
    async fn spawn(&self, cell_cancel: &CancellationToken) -> Result<SpawnedChild, CallOutcome> {
        let name = child_agent_name(&self.run.id, self.seq, self.options.label.as_deref());
        let parent = self
            .step_context
            .turn
            .session_source
            .get_agent_path()
            .unwrap_or_else(AgentPath::root);
        let path = parent
            .join(&name)
            .map_err(|error| null(null_reason::SPAWN_FAILED, Some(error)))?;
        let children = self
            .session
            .services
            .local_agent_runtime
            .workflow_children();
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(self.run.limits.capacity_wait_ms);
        loop {
            let outcomes = children.register(&path, &self.run.id);
            let request = ThreadSpawnRequest {
                task_name: name.clone(),
                message: AgentMessage::Plaintext(self.prompt.clone()),
                fork_mode: None,
                agent_type: self.options.agent_type.clone(),
                model: self.options.model.clone(),
                reasoning_effort: self.options.effort.clone(),
                isolation: Some(self.isolation()),
            };
            let workflow = WorkflowSpawnOptions {
                run_spawn_budget: self.run.spawn_budget.clone(),
                final_output_json_schema: self.options.schema.clone(),
                developer_instructions: WORKFLOW_AGENT_INSTRUCTIONS.to_string(),
            };
            let call_id = format!("{}-{}", self.run.id, self.seq);
            match spawn_workflow_child(self.session, self.step_context, &call_id, request, workflow)
                .await
            {
                Ok(result) => {
                    let thread_id = result.agent.thread_id;
                    children.bind_thread(&path, thread_id);
                    return Ok(SpawnedChild {
                        thread_id,
                        path,
                        outcomes,
                    });
                }
                Err(ThreadSpawnError::Backend(error)) if is_capacity_error(&error) => {
                    children.forget(&path);
                    if tokio::time::Instant::now() >= deadline {
                        return Err(null(null_reason::CAPACITY, Some(error.to_string())));
                    }
                    tokio::select! {
                        _ = self.run.capacity_freed.notified() => {}
                        _ = tokio::time::sleep(CAPACITY_RETRY) => {}
                        _ = self.run.cancellation().cancelled() => {
                            return Err(null(self.cancel_reason(), None));
                        }
                        _ = cell_cancel.cancelled() => {
                            return Err(null(null_reason::CANCELLED, None));
                        }
                    }
                }
                Err(ThreadSpawnError::IsolationUnavailable(error)) => {
                    children.forget(&path);
                    return Err(null(null_reason::ISOLATION_UNAVAILABLE, Some(error)));
                }
                Err(ThreadSpawnError::Backend(error))
                    if matches!(error.details(), CodexErrorDetails::AgentLimitReached { .. }) =>
                {
                    children.forget(&path);
                    return Err(null(null_reason::AGENT_LIMIT, Some(error.to_string())));
                }
                Err(error) => {
                    children.forget(&path);
                    return Err(null(null_reason::SPAWN_FAILED, Some(error.to_string())));
                }
            }
        }
    }

    /// UC3: wait for the child's answer, validate it and charge its tokens (UC5).
    async fn supervise(
        &self,
        spawned: SpawnedChild,
        cell_cancel: &CancellationToken,
    ) -> (CallOutcome, TokenCharge) {
        let SpawnedChild {
            thread_id,
            path,
            mut outcomes,
        } = spawned;
        let runtime = &self.session.services.local_agent_runtime;
        self.run.register_child(self.seq, thread_id);
        self.run.journal(JournalEvent::AgentStarted {
            key: self.key.clone(),
            ordinal: self.ordinal,
            label: self.options.label.clone(),
            phase: self.phase.clone(),
            thread_id: thread_id.to_string(),
            agent_path: path.to_string(),
        });
        self.run.update(|tracker| {
            tracker.agent_started(self.seq, path.to_string(), thread_id.to_string());
        });
        let thread = runtime.workflow_child_thread(thread_id).await.ok();
        let outcome = self
            .await_answer(thread_id, thread.as_deref(), &mut outcomes, cell_cancel)
            .await;
        let tokens = match &thread {
            Some(thread) => charged_tokens(thread).await,
            None => TokenCharge::default(),
        };
        self.run.charge(tokens);
        runtime.workflow_children().finish_call(&path);
        self.run.remove_child(self.seq);
        (outcome, tokens)
    }

    async fn await_answer(
        &self,
        thread_id: ThreadId,
        thread: Option<&CodexThread>,
        outcomes: &mut mpsc::UnboundedReceiver<WorkflowChildOutcome>,
        cell_cancel: &CancellationToken,
    ) -> CallOutcome {
        let deadline = self
            .options
            .timeout_ms
            .map(|timeout| tokio::time::Instant::now() + Duration::from_millis(timeout));
        let mut status = thread.map(CodexThread::subscribe_status);
        let mut corrected = false;
        loop {
            let waited = self
                .wait_terminal(thread, outcomes, &mut status, deadline, cell_cancel)
                .await;
            let outcome = match waited {
                Waited::Outcome(outcome) => outcome,
                Waited::Interrupted => return null(null_reason::INTERRUPTED, None),
                Waited::Gone => return null(null_reason::SHUTDOWN, None),
                Waited::Timeout => {
                    self.interrupt(thread_id).await;
                    return null(
                        null_reason::TIMEOUT,
                        self.options
                            .timeout_ms
                            .map(|timeout| format!("no answer within {timeout} ms")),
                    );
                }
                Waited::Cancelled => {
                    self.interrupt(thread_id).await;
                    return null(self.cancel_reason(), None);
                }
            };
            match outcome.status {
                AgentStatus::Completed(text) => {
                    let text = text.unwrap_or_default();
                    self.report_worktree(thread_id).await;
                    let Some(schema) = &self.options.schema else {
                        // An explorer's text answer keeps its result cap; a schema answer
                        // is bounded by its schema and must stay valid JSON.
                        let text = match outcome.result_max_tokens {
                            Some(max_tokens) => {
                                crate::agent::explorer_profile::cap_result(&text, max_tokens)
                            }
                            None => text,
                        };
                        return CallOutcome::Value(JsonValue::String(text));
                    };
                    let checked = parse_final_message(&text).and_then(|value| {
                        validate_value(schema, &value)
                            .map(|()| value)
                            .map_err(|errors| errors.join("; "))
                    });
                    match checked {
                        Ok(value) => return CallOutcome::Value(value),
                        // R3.6: one corrective turn with the errors and the same schema.
                        Err(errors) if !corrected => {
                            corrected = true;
                            if let Err(error) =
                                self.send_correction(thread_id, schema, &errors).await
                            {
                                return null(
                                    null_reason::SCHEMA_INVALID,
                                    Some(format!("{errors}; the corrective turn failed: {error}")),
                                );
                            }
                        }
                        Err(errors) => return null(null_reason::SCHEMA_INVALID, Some(errors)),
                    }
                }
                AgentStatus::Errored(message) => {
                    // R10.3: Guardian's circuit breaker stopped the agent.
                    let reason = if outcome.error_info == Some(CodexErrorInfo::TooManyDenials) {
                        null_reason::GUARDIAN_DENIED
                    } else {
                        null_reason::ERRORED
                    };
                    return null(reason, Some(message));
                }
                AgentStatus::Interrupted => return null(null_reason::INTERRUPTED, None),
                AgentStatus::Shutdown | AgentStatus::NotFound => {
                    return null(null_reason::SHUTDOWN, None);
                }
                AgentStatus::PendingInit | AgentStatus::Running => {}
            }
        }
    }

    async fn wait_terminal(
        &self,
        thread: Option<&CodexThread>,
        outcomes: &mut mpsc::UnboundedReceiver<WorkflowChildOutcome>,
        status: &mut Option<watch::Receiver<AgentStatus>>,
        deadline: Option<tokio::time::Instant>,
        cell_cancel: &CancellationToken,
    ) -> Waited {
        let mut approval_poll = tokio::time::interval(APPROVAL_POLL);
        approval_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut awaiting_approval = false;
        let result = loop {
            tokio::select! {
                biased;
                outcome = outcomes.recv() => break match outcome {
                    Some(outcome) => Waited::Outcome(outcome),
                    // The run settled this call, for example when its cell ended.
                    None => Waited::Cancelled,
                },
                _ = self.run.cancellation().cancelled() => break Waited::Cancelled,
                _ = cell_cancel.cancelled() => break Waited::Cancelled,
                changed = next_status(status) => match changed {
                    AgentStatus::Interrupted => break Waited::Interrupted,
                    AgentStatus::Shutdown | AgentStatus::NotFound => break Waited::Gone,
                    _ => {}
                },
                _ = sleep_until(deadline) => break Waited::Timeout,
                _ = approval_poll.tick() => {
                    let waiting = match thread {
                        Some(thread) => awaits_approval(thread).await,
                        None => false,
                    };
                    if waiting != awaiting_approval {
                        awaiting_approval = waiting;
                        self.run.update(|tracker| {
                            tracker.agent_awaiting_approval(self.seq, waiting);
                        });
                    }
                }
            }
        };
        if awaiting_approval {
            self.run
                .update(|tracker| tracker.agent_awaiting_approval(self.seq, false));
        }
        result
    }

    /// R3.8: record an isolated agent's changes on its branch and log where they are.
    async fn report_worktree(&self, thread_id: ThreadId) {
        let Some(report) = self
            .session
            .services
            .local_agent_runtime
            .workflow_child_worktree_report(thread_id)
            .await
        else {
            return;
        };
        let message = self
            .run
            .update(|tracker| tracker.log(&format!("agent {} worktree:\n{report}", self.seq)));
        self.run.journal(JournalEvent::Log { message });
    }

    async fn interrupt(&self, thread_id: ThreadId) {
        if let Err(error) = self
            .session
            .services
            .agent_control
            .interrupt(
                self.session.thread_id,
                AgentTarget::Id(thread_id),
                MultiAgentVersion::V2,
            )
            .await
        {
            warn!(run_id = %self.run.id, %thread_id, %error, "failed to interrupt a workflow agent");
        }
    }

    /// R3.6: ask the child once more for a JSON answer matching `schema`.
    async fn send_correction(
        &self,
        thread_id: ThreadId,
        schema: &JsonValue,
        errors: &str,
    ) -> Result<(), String> {
        let turn = &self.step_context.turn;
        let resume_config = build_agent_resume_config(turn)?;
        let message = format!(
            "Your final message did not match the required JSON schema: {errors}\nReply again with only a JSON object that matches this schema:\n{schema}"
        );
        self.session
            .services
            .agent_control
            .send(SendRequest {
                caller: self.session.thread_id,
                target: AgentTarget::Id(thread_id),
                resume_config,
                input: AgentInput::Message {
                    message: AgentMessage::Plaintext(message),
                    mode: MessageDeliveryMode::TriggerTurn,
                },
                start_options: TurnStartOptions {
                    final_output_json_schema: Some(schema.clone()),
                    parent_turn_id: Some(self.run.parent_turn_id.clone()),
                    root_turn_id: turn.turn_metadata_state.root_turn_id(),
                    turn_trigger: Some(WORKFLOW_TURN_TRIGGER.to_string()),
                    ..Default::default()
                },
                turn_spawn_budget: Some(self.run.spawn_budget.clone()),
            })
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Journal the call, update the snapshot and build the script's view of the result.
    fn record(&self, outcome: CallOutcome, tokens: TokenCharge) -> JsonValue {
        // R8.5: agents by outcome and null reason.
        if let Some(metrics) = codex_otel::global() {
            let reason = match &outcome {
                CallOutcome::Value(_) => "ok",
                CallOutcome::Null { reason, .. } => reason,
            };
            let _ = metrics.counter("codex.workflow.agents", /*inc*/ 1, &[("outcome", reason)]);
        }
        match outcome {
            CallOutcome::Value(value) => {
                self.run.journal(JournalEvent::AgentFinished {
                    key: self.key.clone(),
                    ordinal: self.ordinal,
                    ok: true,
                    value: Some(value.clone()),
                    reason: None,
                    tokens,
                });
                self.run
                    .update(|tracker| tracker.agent_finished(self.seq, true, None, tokens));
                json!({ "ok": true, "value": value })
            }
            CallOutcome::Null { reason, detail } => {
                if let Some(detail) = detail {
                    let label = self
                        .options
                        .label
                        .as_deref()
                        .map(|label| format!(" ({label})"))
                        .unwrap_or_default();
                    let message = self.run.update(|tracker| {
                        tracker.log(&format!(
                            "agent {}{label} resolved to null ({reason}): {detail}",
                            self.seq
                        ))
                    });
                    self.run.journal(JournalEvent::Log { message });
                }
                self.run.journal(JournalEvent::AgentFinished {
                    key: self.key.clone(),
                    ordinal: self.ordinal,
                    ok: false,
                    value: None,
                    reason: Some(reason.to_string()),
                    tokens,
                });
                self.run.update(|tracker| {
                    tracker.agent_finished(self.seq, false, Some(reason), tokens);
                });
                json!({ "ok": false, "reason": reason })
            }
        }
    }
}

fn is_capacity_error(error: &CodexErr) -> bool {
    matches!(error.details(), CodexErrorDetails::AgentLimitReached { .. })
        && matches!(
            error.agent_context(),
            Some(AgentErrorContext::ResidencyCapacity | AgentErrorContext::ExecutionCapacity)
        )
}

async fn next_status(status: &mut Option<watch::Receiver<AgentStatus>>) -> AgentStatus {
    let Some(receiver) = status else {
        return std::future::pending().await;
    };
    match receiver.changed().await {
        Ok(()) => receiver.borrow_and_update().clone(),
        Err(_) => {
            *status = None;
            AgentStatus::Shutdown
        }
    }
}

async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// R10.2: the child's active turn waits for an approval.
async fn awaits_approval(thread: &CodexThread) -> bool {
    let turn_state = {
        let active = thread.session.active_turn.lock().await;
        match active.as_ref() {
            Some(turn) => Arc::clone(&turn.turn_state),
            None => return false,
        }
    };
    turn_state.lock().await.has_pending_approvals()
}

/// UC5, R5.1: a fresh child's usage, as non-cached input plus output and cached input.
async fn charged_tokens(thread: &CodexThread) -> TokenCharge {
    let Some(info) = thread.token_usage_info().await else {
        return TokenCharge::default();
    };
    let usage = info.total_token_usage;
    let non_cached_input = usage.input_tokens.saturating_sub(usage.cached_input_tokens);
    TokenCharge {
        charged: u64::try_from(non_cached_input.saturating_add(usage.output_tokens)).unwrap_or(0),
        cached: u64::try_from(usage.cached_input_tokens).unwrap_or(0),
    }
}
