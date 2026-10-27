//! One workflow run: its state, its Code Mode cell and its lifecycle (UC2, UC5, UC7, UC9).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicBool;
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
use codex_workflow::CachedAnswer;
use codex_workflow::HOST_TOOLS;
use codex_workflow::Journal;
use codex_workflow::JournalCache;
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
use crate::config::WorkflowBudgetMode;
use crate::config::WorkflowStructuredOutput;
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
    pub(crate) budget_mode: WorkflowBudgetMode,
    /// R5.4: charged tokens above which running agents are interrupted; only in hard mode
    /// with a budget.
    pub(crate) hard_budget_threshold: Option<u64>,
    pub(crate) structured_output: WorkflowStructuredOutput,
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
    /// Agents this run started: the run's own start limit (UC4, R4.2).
    started_agents: AtomicUsize,
    /// Cancels queued calls, running agents and the cell.
    cancel: CancellationToken,
    cancel_status: Mutex<Option<RunStatus>>,
    children: Mutex<HashMap<u64, ThreadId>>,
    key_ordinals: Mutex<HashMap<String, u64>>,
    /// Signalled when an agent of this run finishes, so capacity waits retry.
    pub(crate) capacity_freed: Notify,
    finished: watch::Sender<bool>,
    /// The script and args of a run whose journal lives in memory (R6.6), for `resume_from`
    /// without them (R6.3); a persistent run reads them from `script.js` and `args.json`.
    ephemeral_files: Option<(String, String)>,
    /// Answers of the run this one resumes (R6.3); dropped when the run finishes.
    resume_cache: Mutex<Option<JournalCache>>,
    /// R5.4: cancelled once a hard-mode run spends above its threshold.
    hard_budget: CancellationToken,
    /// R5.4: the threshold was crossed and logged; set once.
    hard_budget_hit: AtomicBool,
    /// R3.12: the provider refused native structured output during this run.
    native_schema_rejected: AtomicBool,
}

/// Input of [`start_run`] after static validation.
pub(crate) struct StartRunRequest {
    pub(crate) meta: WorkflowMeta,
    pub(crate) script: String,
    pub(crate) body: String,
    pub(crate) args_json: String,
    pub(crate) budget_tokens: Option<u64>,
    /// The journal of the run that `resume_from` names (R6.3).
    pub(crate) resume: Option<JournalCache>,
}

/// A run of an earlier process rebuilt from its directory (R6.4, R6.7).
pub(crate) struct RestoredRun {
    pub(crate) id: String,
    pub(crate) meta: WorkflowMeta,
    pub(crate) tracker: RunTracker,
    pub(crate) journal: Journal,
    pub(crate) result_json: Option<String>,
}

/// Everything a [`WorkflowRun`] is built from.
struct RunParts {
    id: String,
    meta: WorkflowMeta,
    limits: RunLimits,
    parent_turn_id: String,
    tracker: RunTracker,
    updates: broadcast::Sender<String>,
    journal: Journal,
    ephemeral_files: Option<(String, String)>,
    resume: Option<JournalCache>,
    result_json: Option<String>,
    /// A terminal run of an earlier process: nothing to cancel or wait for.
    restored: bool,
}

impl WorkflowRun {
    fn from_parts(parts: RunParts) -> Self {
        let RunParts {
            id,
            meta,
            limits,
            parent_turn_id,
            tracker,
            updates,
            journal,
            ephemeral_files,
            resume,
            result_json,
            restored,
        } = parts;
        let cancel = CancellationToken::new();
        if restored {
            cancel.cancel();
        }
        Self {
            id,
            meta,
            limits,
            parent_turn_id,
            revision: watch::channel(tracker.revision()).0,
            tracker: Mutex::new(tracker),
            updates,
            result_json: Mutex::new(result_json),
            script_error: Mutex::new(None),
            journal,
            gate: OrderedGate::new(limits.max_concurrent_agents),
            started_agents: AtomicUsize::new(0),
            cancel,
            cancel_status: Mutex::new(None),
            children: Mutex::new(HashMap::new()),
            key_ordinals: Mutex::new(HashMap::new()),
            capacity_freed: Notify::new(),
            finished: watch::channel(restored).0,
            ephemeral_files,
            resume_cache: Mutex::new(resume),
            hard_budget: CancellationToken::new(),
            hard_budget_hit: AtomicBool::new(false),
            native_schema_rejected: AtomicBool::new(false),
        }
    }

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

    /// Apply a change to the snapshot and publish the new revision, if the change made one.
    pub(crate) fn update<T>(&self, change: impl FnOnce(&mut RunTracker) -> T) -> T {
        let (value, before, after) = {
            let mut tracker = self.tracker();
            let before = tracker.revision();
            let value = change(&mut tracker);
            (value, before, tracker.revision())
        };
        if after != before {
            self.revision.send_replace(after);
            let _ = self.updates.send(self.id.clone());
        }
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

    /// UC5, R5.1: tokens charged to the run. The snapshot is the only count, so the budget,
    /// the hard threshold and every client see the same number.
    fn charged_tokens(&self) -> u64 {
        self.tracker().charged_tokens()
    }

    /// UC5: whether charged tokens reached the run's limit.
    pub(crate) fn budget_exhausted(&self) -> bool {
        self.limits
            .budget_tokens
            .is_some_and(|limit| self.charged_tokens() >= limit)
    }

    /// UC5, R5.1: charge one model response of the agent of call `seq` as it is recorded. A
    /// call that already settled its tokens takes no more: its agent's total holds the
    /// response (R5.1).
    pub(crate) fn charge_live(&self, seq: u64, tokens: TokenCharge) {
        if self.update(|tracker| tracker.agent_charged(seq, tokens)) {
            self.check_hard_budget();
        }
    }

    /// R5.1: settle the tokens of call `seq` from its agent's total usage, once; only what
    /// live charging did not count yet is added. Returns the call's tokens.
    pub(crate) fn settle_call_tokens(&self, seq: u64, total: TokenCharge) -> TokenCharge {
        let tokens = self.update(|tracker| tracker.settle_agent_tokens(seq, total));
        self.check_hard_budget();
        tokens
    }

    /// R5.4: in hard mode, stop running agents once the run spent above its threshold; the
    /// first call that sees it cancels and logs.
    fn check_hard_budget(&self) {
        let Some(threshold) = self.limits.hard_budget_threshold else {
            return;
        };
        let spent = self.charged_tokens();
        if spent > threshold && !self.hard_budget_hit.swap(true, Ordering::AcqRel) {
            self.hard_budget.cancel();
            self.log_line(&format!(
                "the run spent {spent} tokens, above its hard budget of {threshold}; running agents are interrupted"
            ));
        }
    }

    /// R5.4: cancelled once a hard-mode run exceeded its threshold.
    pub(crate) fn hard_budget(&self) -> &CancellationToken {
        &self.hard_budget
    }

    /// Record a line in the run log and the journal.
    pub(crate) fn log_line(&self, line: &str) {
        let message = self.update(|tracker| tracker.log(line));
        self.journal(JournalEvent::Log { message });
    }

    /// R6.3: the source answer of the call with `key` and `ordinal` and the source run's id,
    /// if this run resumes a run that answered it; the snapshot's resume counts follow.
    pub(crate) fn take_cached(&self, key: &str, ordinal: u64) -> Option<(CachedAnswer, String)> {
        let mut cache = self
            .resume_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let cache = cache.as_mut()?;
        let answer = cache.take(key, ordinal)?;
        let stats = cache.stats();
        let from = stats.from.clone();
        self.update(|tracker| tracker.set_resume(stats));
        Some((answer, from))
    }

    /// R6.3, R6.6: a finished run keeps only what clients and `resume_from` read: its
    /// snapshot, result and journal. The resume cache and the call-key counts are dropped.
    fn release_finished_state(&self) {
        self.resume_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        std::mem::take(
            &mut *self
                .key_ordinals
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
    }

    /// R3.12: whether schema calls of this run put the schema in the task.
    pub(crate) fn schema_in_prompt(&self) -> bool {
        match self.limits.structured_output {
            WorkflowStructuredOutput::Prompt => true,
            WorkflowStructuredOutput::Native => false,
            WorkflowStructuredOutput::Auto => self.native_schema_rejected.load(Ordering::Acquire),
        }
    }

    /// R3.12: the provider refused native structured output; returns whether this is the
    /// first refusal of the run, which is then logged.
    pub(crate) fn reject_native_schema(&self) -> bool {
        let first = !self.native_schema_rejected.swap(true, Ordering::AcqRel);
        if first {
            self.log_line(
                "the model provider rejected native structured output; schema calls of this run now put the schema in the task",
            );
        }
        first
    }

    /// The script and args the run started with (R6.3): from the run directory of a
    /// persistent run, from memory for an ephemeral one.
    pub(crate) async fn script_and_args(&self) -> Result<(String, String), String> {
        if let Some((script, args_json)) = &self.ephemeral_files {
            return Ok((script.clone(), args_json.clone()));
        }
        let Some(dir) = self.journal_dir() else {
            return Err(format!(
                "workflow run `{}` kept no script to resume",
                self.id
            ));
        };
        tokio::task::spawn_blocking(move || codex_workflow::read_run_files(&dir))
            .await
            .map_err(|error| error.to_string())
            .and_then(|files| files.map_err(|error| error.to_string()))
            .map_err(|error| {
                format!(
                    "failed to read the script and args of workflow run `{}`: {error}",
                    self.id
                )
            })
    }

    /// Records of the run's journal (R6.3).
    pub(crate) fn journal_records(&self) -> std::io::Result<Vec<codex_workflow::JournalRecord>> {
        self.journal.records()
    }

    /// R6.4, R6.7: a terminal run of an earlier process, rebuilt from its journal. It has no
    /// cell; cancelling it returns its snapshot (R9.4).
    pub(crate) fn restored(run: RestoredRun, updates: broadcast::Sender<String>) -> Self {
        let limits = RunLimits {
            max_concurrent_agents: 1,
            max_agents_per_run: 0,
            capacity_wait_ms: 0,
            budget_tokens: run.tracker.snapshot(false).budget_tokens,
            notification_max_tokens: 0,
            max_heap_bytes: 0,
            budget_mode: WorkflowBudgetMode::Soft,
            hard_budget_threshold: None,
            structured_output: WorkflowStructuredOutput::Auto,
        };
        Self::from_parts(RunParts {
            id: run.id,
            meta: run.meta,
            limits,
            parent_turn_id: String::new(),
            tracker: run.tracker,
            updates,
            journal: run.journal,
            ephemeral_files: None,
            resume: None,
            result_json: run.result_json,
            restored: true,
        })
    }

    /// UC5, R5.1: `budget.used()` and `budget.remaining()` of the script.
    pub(crate) fn budget_json(&self) -> JsonValue {
        let used = self.charged_tokens();
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
    let runs_dir =
        (!config.ephemeral).then(|| super::resume::thread_runs_dir(config, session.thread_id));
    // After a restart the registry is empty; an id must not reuse an earlier run's journal.
    let run_id =
        registry.allocate_id(|id| runs_dir.as_ref().is_some_and(|dir| dir.join(id).exists()));
    let journal = match &runs_dir {
        None => Journal::in_memory(),
        Some(dir) => Journal::create(
            dir.join(&run_id).as_path(),
            &request.script,
            &request.args_json,
        )
        .map_err(|error| format!("failed to create the workflow journal: {error}"))?,
    };
    let budget_tokens = request.budget_tokens.or(workflows.default_budget_tokens);
    let limits = RunLimits {
        max_concurrent_agents: workflows.max_concurrent_agents,
        max_agents_per_run: workflows.max_agents_per_run,
        capacity_wait_ms: workflows.capacity_wait_ms,
        budget_tokens,
        notification_max_tokens: workflows.notification_max_tokens,
        max_heap_bytes: workflows.max_heap_bytes,
        budget_mode: workflows.budget_mode,
        hard_budget_threshold: budget_tokens
            .filter(|_| workflows.budget_mode == WorkflowBudgetMode::Hard)
            .map(|budget| workflows.hard_budget_threshold(budget)),
        structured_output: workflows.structured_output,
    };
    let started_at_ms = now_ms();
    let mut tracker = RunTracker::new(
        run_id.clone(),
        request.meta.name.clone(),
        request.meta.description.clone(),
        &request.meta.phases,
        limits.budget_tokens,
        started_at_ms,
    );
    let resume_stats = request.resume.as_ref().map(JournalCache::stats);
    if let Some(stats) = &resume_stats {
        tracker.set_resume(stats.clone());
    }
    let ephemeral_files = journal
        .dir()
        .is_none()
        .then(|| (request.script.clone(), request.args_json.clone()));
    let run = Arc::new(WorkflowRun::from_parts(RunParts {
        id: run_id.clone(),
        meta: request.meta,
        limits,
        parent_turn_id: step_context.turn.sub_id.clone(),
        tracker,
        updates: registry.update_sender(),
        journal,
        ephemeral_files,
        resume: request.resume,
        result_json: None,
        restored: false,
    }));
    run.journal(JournalEvent::RunStarted {
        run_id: run_id.clone(),
        name: run.meta.name.clone(),
        script_sha256: codex_workflow::sha256_hex(request.script.as_bytes()),
        args_sha256: codex_workflow::sha256_hex(request.args_json.as_bytes()),
        parent_turn_id: run.parent_turn_id.clone(),
        resume_from: resume_stats.as_ref().map(|stats| stats.from.clone()),
        started_at_ms: Some(started_at_ms),
        budget_tokens: limits.budget_tokens,
        resume_source_results: resume_stats.as_ref().map(|stats| stats.source_results),
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

/// How long the syntax check of a script may take.
const SYNTAX_CHECK_YIELD_MS: u64 = 30_000;

/// N5 (live recheck 2026-10-27): compiles a validated script body in a Code Mode session of
/// its own before a run is registered, so a syntax error answers the `workflow` call instead
/// of a failed run's notification. The body sits in `async function __wf_main()` on the
/// first line, as in the run's cell, so V8 compiles it the same way and reports the same
/// lines; it is wrapped in a function that is never called, so nothing of the script runs.
/// Only a `SyntaxError` is returned: a check that cannot run leaves the run to report it.
pub(crate) async fn check_script_syntax(session: &Session, body: &str) -> Result<(), String> {
    let provider = session.services.code_mode_service.session_provider();
    let code_mode = match provider.create_session().await {
        Ok(code_mode) => code_mode,
        Err(error) => {
            warn!(%error, "cannot start the workflow syntax check");
            return Ok(());
        }
    };
    let request = ExecuteRequest {
        tool_call_id: "workflow-syntax-check".to_string(),
        enabled_tools: Vec::new(),
        source: format!(
            "async function __wf_check() {{ async function __wf_main() {{{body}\n}} }}\n"
        ),
        yield_time_ms: Some(SYNTAX_CHECK_YIELD_MS),
        max_output_tokens: Some(1_000),
    };
    let delegate: Arc<dyn CodeModeSessionDelegate> =
        Arc::new(codex_code_mode::NoopCodeModeSessionDelegate);
    let response = match code_mode.execute(request, delegate, /*preempt*/ None).await {
        Ok(started) => started.initial_response().await,
        Err(error) => Err(error),
    };
    if let Err(error) = code_mode.shutdown().await {
        warn!(%error, "failed to shut down the workflow syntax check");
    }
    match response {
        Ok(RuntimeResponse::Result {
            error_text: Some(error),
            ..
        }) if error.contains("SyntaxError") => Err(display_error(error.trim())),
        Ok(_) => Ok(()),
        Err(error) => {
            warn!(%error, "the workflow syntax check did not run");
            Ok(())
        }
    }
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
        finished_at_ms: Some(finished_at),
    });
    // R6.1, R6.7: keep the result next to the journal for runs rebuilt from disk. It is
    // written after `run_finished` and replaces the file whole, so a crash leaves either no
    // result or a whole one, never a result of a run the next load marks interrupted.
    if let Some(result) = &result_json
        && let Err(error) = run.journal.write_result(result)
    {
        warn!(run_id = %run.id, %error, "failed to persist the workflow result");
    }
    // R6.8: index the finished run, so a later process lists it without reading its journal.
    if let Some(thread_dir) = run
        .journal_dir()
        .as_deref()
        .and_then(std::path::Path::parent)
        && let Err(error) = codex_workflow::append_index(thread_dir, &snapshot)
    {
        warn!(run_id = %run.id, %error, "failed to index the finished workflow run");
    }
    run.release_finished_state();
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

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
