//! One `agent()` call of a workflow script (UC3, UC4, UC5, UC6, UC10).

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
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
use codex_protocol::protocol::TokenUsage;
use codex_protocol::turn_input::TurnStartOptions;
use codex_workflow::CachedAnswer;
use codex_workflow::JournalEvent;
use codex_workflow::TokenCharge;
use codex_workflow::WORKFLOW_AGENT_INSTRUCTIONS;
use codex_workflow::call_key;
use codex_workflow::check_strict_compatible;
use codex_workflow::child_agent_name;
use codex_workflow::is_schema_format_rejection;
use codex_workflow::native_rejected_follow_up;
use codex_workflow::null_reason;
use codex_workflow::parse_final_message;
use codex_workflow::prompt_schema_suffix;
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
use super::usage::token_charge;
use crate::agent::api::AgentInput;
use crate::agent::api::AgentTarget;
use crate::agent::api::SendRequest;
use crate::agent::child_config::build_agent_resume_config;
use crate::agent::control::workflow_children::WorkflowChildOutcome;
use crate::agent::control::workflow_children::WorkflowUsageSink;
use crate::agent::control::workflow_children::WorkflowWorktreeReport;
use crate::agent::types::AgentMessage;
use crate::agent::types::MessageDeliveryMode;
use crate::codex_thread::CodexThread;
use crate::config::WorkflowStructuredOutput;
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
    "schema, model, effort, agent_type, label, phase, isolation, timeout_ms, details";

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
    /// R3.13: resolve to `{answer, worktree}` instead of the bare answer.
    details: Option<bool>,
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
/// A call the resumed run already answered resolves from its journal without an agent (UC6,
/// R6.3). The result is journaled before it is returned to the script (R6.1).
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
    // R6.3: the resumed run's answer needs neither a slot nor an agent start.
    if let Some((answer, from)) = run.take_cached(&key, ordinal) {
        run.gate.ticket(seq).skip();
        return record_cached(
            run,
            CachedCall {
                seq,
                key,
                ordinal,
                label: options.label,
                phase,
                isolated: options.isolation == Some(AgentIsolation::Worktree),
                details: options.details == Some(true),
            },
            answer,
            &from,
        );
    }
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
        schema_in_prompt: AtomicBool::new(false),
        worktree_report: std::sync::Mutex::new(None),
    };
    let (outcome, tokens) = call.execute(&cell_cancel).await;
    call.record(outcome, tokens)
}

/// A call answered from the resumed run's journal.
struct CachedCall {
    seq: u64,
    key: String,
    ordinal: u64,
    label: Option<String>,
    phase: Option<String>,
    /// The call asked for `isolation: "worktree"`.
    isolated: bool,
    /// The call asked for `details: true` (R3.13).
    details: bool,
}

/// R6.3, R6.5: settle a call from the journal of run `from`. An isolated agent's answer
/// carries the report of where its changes are (R3.8): the resumed run logs it and keeps it
/// in its own journal, and says explicitly when the source journal does not record it. A
/// call with `details: true` gets the journaled worktree details with the answer (R3.13).
fn record_cached(
    run: &WorkflowRun,
    call: CachedCall,
    answer: CachedAnswer,
    from: &str,
) -> JsonValue {
    let CachedCall {
        seq,
        key,
        ordinal,
        label,
        phase,
        isolated,
        details,
    } = call;
    if let Some(metrics) = codex_otel::global() {
        let _ = metrics.counter(
            "codex.workflow.agents",
            /*inc*/ 1,
            &[("outcome", "cached")],
        );
    }
    if isolated {
        match &answer.worktree {
            Some(report) => run.log_line(&format!(
                "agent {seq} answered from the journal of {from}; its worktree{}:\n{report}",
                apply_hint(answer.worktree_details.as_ref())
            )),
            None => run.log_line(&format!(
                "agent {seq} answered from the journal of {from}, which does not record that isolated agent's worktree; look for its branch in the log of {from}"
            )),
        }
    }
    let CachedAnswer {
        value,
        worktree,
        worktree_details,
    } = answer;
    let script_value = script_value(details, &value, worktree_details.as_ref());
    run.journal(JournalEvent::AgentFinished {
        call: Some(seq),
        key,
        ordinal,
        label: label.clone(),
        phase: phase.clone(),
        ok: true,
        value: Some(value),
        reason: None,
        tokens: TokenCharge::default(),
        cached: true,
        worktree,
        worktree_details,
    });
    run.update(|tracker| tracker.agent_cached(seq, label, phase));
    json!({ "ok": true, "value": script_value })
}

/// R3.8 (D1 of the combined live run): how the parent brings an isolated agent's changes
/// into its checkout, ` (apply it with `apply_agent_worktree` target `<agent>`)`, for the run
/// log line in front of the `<agent_worktree>` block; empty without the agent's name.
fn apply_hint(details: Option<&JsonValue>) -> String {
    details
        .and_then(|details| details["agent"].as_str())
        .map(|agent| format!(" (apply it with `apply_agent_worktree` target `{agent}`)"))
        .unwrap_or_default()
}

/// R3.13: what a successful call resolves to in the script: the bare answer, or with
/// `details: true` the answer and the worktree details of an isolated agent (null for a call
/// without isolation or a journal that does not record them).
fn script_value(details: bool, answer: &JsonValue, worktree: Option<&JsonValue>) -> JsonValue {
    if details {
        json!({ "answer": answer, "worktree": worktree })
    } else {
        answer.clone()
    }
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
    /// R3.12: this call carries its schema in the task instead of `final_output_json_schema`.
    schema_in_prompt: AtomicBool,
    /// R3.8: the last report of where the isolated agent left its changes; journaled with
    /// the answer, so a resume that reuses it can report them (R6.3), and returned to a call
    /// with `details: true` (R3.13).
    worktree_report: std::sync::Mutex<Option<WorkflowWorktreeReport>>,
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
    HardBudget,
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
        // R4.2, R4.4: the run's own count of agent starts.
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

    /// R5.1: charge this call's agent for each model response as it is recorded.
    fn usage_sink(&self) -> WorkflowUsageSink {
        let run = Arc::downgrade(self.run);
        let seq = self.seq;
        Arc::new(move |usage: &TokenUsage| {
            if let Some(run) = run.upgrade() {
                run.charge_live(seq, token_charge(usage));
            }
        })
    }

    /// R3.12: the task text and the native schema of the spawn, in the run's mode.
    fn task_and_schema(&self) -> (String, Option<JsonValue>) {
        let Some(schema) = &self.options.schema else {
            return (self.prompt.clone(), None);
        };
        if self.run.schema_in_prompt() {
            self.schema_in_prompt.store(true, Ordering::Release);
            (
                format!("{}\n\n{}", self.prompt, prompt_schema_suffix(schema)),
                None,
            )
        } else {
            (self.prompt.clone(), Some(schema.clone()))
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
        let (task, final_output_json_schema) = self.task_and_schema();
        loop {
            if self.run.budget_exhausted() {
                return Err(null(null_reason::BUDGET_EXHAUSTED, None));
            }
            let outcomes = children.register(&path, &self.run.id, Some(self.usage_sink()));
            let request = ThreadSpawnRequest {
                task_name: name.clone(),
                message: AgentMessage::Plaintext(task.clone()),
                fork_mode: None,
                agent_type: self.options.agent_type.clone(),
                model: self.options.model.clone(),
                reasoning_effort: self.options.effort.clone(),
                isolation: Some(self.isolation()),
            };
            let workflow = WorkflowSpawnOptions {
                final_output_json_schema: final_output_json_schema.clone(),
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

    /// UC3: wait for the child's answer, validate it and settle its tokens (UC5).
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
            call: Some(self.seq),
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
        let total = match &thread {
            Some(thread) => charged_tokens(thread).await,
            None => TokenCharge::default(),
        };
        // R5.1: live charging already counted most of it; only the rest is added.
        let tokens = self.run.settle_call_tokens(self.seq, total);
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
        let mut fell_back = false;
        let mut waiting_for_terminal = false;
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
                Waited::HardBudget => {
                    // R5.4: a hard-mode run spent above its threshold.
                    self.interrupt(thread_id).await;
                    return null(
                        null_reason::BUDGET_EXHAUSTED,
                        Some("interrupted by the run's hard token budget".to_string()),
                    );
                }
            };
            match outcome.status {
                AgentStatus::Completed(text) => {
                    // R3.10: an agent that ended its turn while a background terminal will
                    // still report its exit answers in the turn that notification starts.
                    if let Some(thread) = thread
                        && thread.session.background_terminals_block_eviction().await
                    {
                        if !waiting_for_terminal {
                            waiting_for_terminal = true;
                            self.run.log_line(&format!(
                                "agent {} waits for its background terminal before answering",
                                self.seq
                            ));
                        }
                        continue;
                    }
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
                            let message = format!(
                                "Your final message did not match the required JSON schema: {errors}\nReply again with only a JSON object that matches this schema:\n{schema}"
                            );
                            if let Err(error) = self.send_follow_up(thread_id, message).await {
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
                    // R3.12: the provider refused native structured output; ask again with
                    // the schema in the task, without spending the corrective turn.
                    if let Some(schema) = &self.options.schema
                        && !fell_back
                        && self.native_fallback_applies(outcome.error_info.as_ref(), &message)
                    {
                        fell_back = true;
                        self.run.reject_native_schema();
                        self.schema_in_prompt.store(true, Ordering::Release);
                        if let Err(error) = self
                            .send_follow_up(thread_id, native_rejected_follow_up(schema))
                            .await
                        {
                            return null(
                                null_reason::ERRORED,
                                Some(format!("{message}; the prompt-schema turn failed: {error}")),
                            );
                        }
                        continue;
                    }
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

    /// R3.12: an `auto` run's native schema request was refused by the provider.
    fn native_fallback_applies(&self, error_info: Option<&CodexErrorInfo>, message: &str) -> bool {
        if self.run.limits.structured_output != WorkflowStructuredOutput::Auto
            || self.schema_in_prompt.load(Ordering::Acquire)
        {
            return false;
        }
        let (http_status, bad_request) = match error_info {
            Some(
                CodexErrorInfo::HttpConnectionFailed { http_status_code }
                | CodexErrorInfo::ResponseStreamConnectionFailed { http_status_code }
                | CodexErrorInfo::ResponseTooManyFailedAttempts { http_status_code },
            ) => (*http_status_code, false),
            Some(CodexErrorInfo::BadRequest | CodexErrorInfo::InvalidPrompt) => (None, true),
            _ => (None, false),
        };
        is_schema_format_rejection(http_status, bad_request, message)
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
                _ = self.run.hard_budget().cancelled() => break Waited::HardBudget,
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
        self.run.log_line(&format!(
            "agent {} worktree{}:\n{}",
            self.seq,
            apply_hint(Some(&report.details)),
            report.text
        ));
        *self
            .worktree_report
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(report);
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

    /// R3.6, R3.12: send the child one more turn asking for a JSON answer; the native schema
    /// goes along unless this call carries it in the task.
    async fn send_follow_up(&self, thread_id: ThreadId, message: String) -> Result<(), String> {
        let turn = &self.step_context.turn;
        let resume_config = build_agent_resume_config(turn)?;
        let final_output_json_schema = if self.schema_in_prompt.load(Ordering::Acquire) {
            None
        } else {
            self.options.schema.clone()
        };
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
                    final_output_json_schema,
                    parent_turn_id: Some(self.run.parent_turn_id.clone()),
                    root_turn_id: turn.turn_metadata_state.root_turn_id(),
                    turn_trigger: Some(WORKFLOW_TURN_TRIGGER.to_string()),
                    ..Default::default()
                },
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
            let _ = metrics.counter(
                "codex.workflow.agents",
                /*inc*/ 1,
                &[("outcome", reason)],
            );
        }
        match outcome {
            CallOutcome::Value(value) => {
                let report = self
                    .worktree_report
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take();
                let (worktree, worktree_details) = match report {
                    Some(report) => (Some(report.text), Some(report.details)),
                    None => (None, None),
                };
                let script_value = script_value(
                    self.options.details == Some(true),
                    &value,
                    worktree_details.as_ref(),
                );
                self.run.journal(JournalEvent::AgentFinished {
                    call: Some(self.seq),
                    key: self.key.clone(),
                    ordinal: self.ordinal,
                    label: self.options.label.clone(),
                    phase: self.phase.clone(),
                    ok: true,
                    value: Some(value),
                    reason: None,
                    tokens,
                    cached: false,
                    worktree,
                    worktree_details,
                });
                self.run
                    .update(|tracker| tracker.agent_finished(self.seq, true, None, tokens));
                json!({ "ok": true, "value": script_value })
            }
            CallOutcome::Null { reason, detail } => {
                if let Some(detail) = detail {
                    let label = self
                        .options
                        .label
                        .as_deref()
                        .map(|label| format!(" ({label})"))
                        .unwrap_or_default();
                    self.run.log_line(&format!(
                        "agent {}{label} resolved to null ({reason}): {detail}",
                        self.seq
                    ));
                }
                self.run.journal(JournalEvent::AgentFinished {
                    call: Some(self.seq),
                    key: self.key.clone(),
                    ordinal: self.ordinal,
                    label: self.options.label.clone(),
                    phase: self.phase.clone(),
                    ok: false,
                    value: None,
                    reason: Some(reason.to_string()),
                    tokens,
                    cached: false,
                    worktree: None,
                    worktree_details: None,
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

/// UC5, R5.1: a fresh child's total usage, as non-cached input plus output and cached input.
async fn charged_tokens(thread: &CodexThread) -> TokenCharge {
    match thread.token_usage_info().await {
        Some(info) => token_charge(&info.total_token_usage),
        None => TokenCharge::default(),
    }
}
