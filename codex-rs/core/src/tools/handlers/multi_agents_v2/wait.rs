use super::*;
use crate::agent::status::is_final;
use crate::session::InputQueueActivity;
use crate::tools::handlers::multi_agents_spec::WaitAgentTimeoutOptions;
use crate::tools::handlers::multi_agents_spec::create_wait_agent_tool_v2;
use codex_protocol::AgentPath;
use codex_tools::ToolSpec;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;
use tokio::time::timeout_at;

#[derive(Default)]
pub(crate) struct Handler {
    options: WaitAgentTimeoutOptions,
    /// Fork: describe and wait for background terminal exit notifications.
    exit_notifications: bool,
}

impl Handler {
    pub(crate) fn new(options: WaitAgentTimeoutOptions, exit_notifications: bool) -> Self {
        Self {
            options,
            exit_notifications,
        }
    }
}

impl ToolExecutor<ToolInvocation> for Handler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("wait_agent")
    }

    fn spec(&self) -> ToolSpec {
        create_wait_agent_tool_v2(self.options, self.exit_notifications)
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(self.handle_call(invocation))
    }
}

impl Handler {
    async fn handle_call(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn crate::tools::context::ToolOutput>, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            call_id,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: WaitArgs = parse_arguments(&arguments)?;
        let min_timeout_ms = turn.config.multi_agent_v2.min_wait_timeout_ms;
        let max_timeout_ms = turn.config.multi_agent_v2.max_wait_timeout_ms;
        let default_timeout_ms = turn.config.multi_agent_v2.default_wait_timeout_ms;
        let requested_timeout_ms = args.timeout_ms;
        let timeout_ms = match requested_timeout_ms {
            Some(0) => 0,
            Some(ms) if ms > max_timeout_ms => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "timeout_ms must be at most {max_timeout_ms}"
                )));
            }
            Some(ms) => ms.max(min_timeout_ms),
            None => default_timeout_ms,
        };
        let wait_started = Instant::now();
        let deadline = wait_started + Duration::from_millis(timeout_ms as u64);

        let turn_state = session
            .input_queue
            .turn_state_for_sub_id(&session.active_turn, &turn.sub_id)
            .await;
        let (mut activity_rx, pending_activity) = session
            .input_queue
            .subscribe_activity(turn_state.as_deref())
            .await;

        session
            .emit_turn_item_started(
                &turn,
                &TurnItem::CollabAgentToolCall(CollabAgentToolCallItem {
                    id: call_id.clone(),
                    tool: CollabAgentTool::Wait,
                    status: CollabAgentToolCallStatus::InProgress,
                    sender_thread_id: session.thread_id,
                    receiver_thread_ids: Vec::new(),
                    receiver_agents: Vec::new(),
                    prompt: None,
                    model: None,
                    reasoning_effort: None,
                    agents_states: Default::default(),
                }),
            )
            .await;

        let outcome = if let Some(outcome) = ready_activity(&mut activity_rx, pending_activity) {
            outcome
        } else {
            let snapshot = active_descendant_snapshot(session.as_ref(), turn.as_ref()).await?;
            let no_agents = snapshot == ActiveAgentSnapshot::default()
                && !board_posts_may_arrive(session.as_ref(), turn.as_ref()).await?;
            // Fork: an expected background terminal notification is worth waiting for.
            let terminals_only = no_agents && session.has_pending_background_terminal_work().await;
            if no_agents && !terminals_only {
                recheck_activity(session.as_ref(), turn_state.as_deref())
                    .await
                    .unwrap_or(WaitOutcome::NoActiveAgents)
            } else {
                // Fork: a hung terminal must not hold a wait without `timeout_ms` for the
                // full default (R4.5).
                let deadline = if terminals_only {
                    wait_started
                        + Duration::from_millis(effective_timeout_ms(
                            requested_timeout_ms,
                            timeout_ms,
                            /*terminals_only*/ true,
                        ) as u64)
                } else {
                    deadline
                };
                wait_for_activity(
                    session.as_ref(),
                    turn.as_ref(),
                    turn_state.as_deref(),
                    &mut activity_rx,
                    deadline,
                )
                .await?
            }
        };
        // Dropped waits do not have an observed outcome and are not included.
        turn.session_telemetry.record_duration(
            "codex.multi_agent.wait.duration_ms",
            wait_started.elapsed(),
            &[(
                "outcome",
                match outcome {
                    WaitOutcome::MailboxActivity => "mailbox",
                    WaitOutcome::BackgroundTerminal => "background_terminal",
                    WaitOutcome::Workflow => "workflow",
                    WaitOutcome::Steered => "steered",
                    WaitOutcome::NoActiveAgents => "no_active_agents",
                    WaitOutcome::TimedOut(..) => "timed_out",
                },
            )],
        );
        let result = WaitAgentResult::from_outcome(outcome, requested_timeout_ms, timeout_ms);

        session
            .emit_turn_item_completed(
                &turn,
                TurnItem::CollabAgentToolCall(CollabAgentToolCallItem {
                    id: call_id,
                    tool: CollabAgentTool::Wait,
                    status: CollabAgentToolCallStatus::Completed,
                    sender_thread_id: session.thread_id,
                    receiver_thread_ids: Vec::new(),
                    receiver_agents: Vec::new(),
                    prompt: None,
                    model: None,
                    reasoning_effort: None,
                    agents_states: HashMap::new(),
                }),
            )
            .await;

        Ok(boxed_tool_output(result))
    }
}

impl CoreToolRuntime for Handler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitArgs {
    timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct WaitAgentResult {
    pub(crate) message: String,
    pub(crate) timed_out: bool,
}

impl WaitAgentResult {
    fn from_outcome(
        outcome: WaitOutcome,
        requested_timeout_ms: Option<i64>,
        timeout_ms: i64,
    ) -> Self {
        let (message, timed_out) = match outcome {
            WaitOutcome::MailboxActivity => ("Wait completed.".to_string(), false),
            WaitOutcome::BackgroundTerminal => (
                "Wait completed: a background terminal notification arrived.".to_string(),
                false,
            ),
            WaitOutcome::Workflow => (
                "Wait completed: a workflow run finished; its notification follows.".to_string(),
                false,
            ),
            WaitOutcome::Steered => ("Wait interrupted by new input.".to_string(), false),
            WaitOutcome::NoActiveAgents => ("No active agents.".to_string(), false),
            WaitOutcome::TimedOut(snapshot, background_terminals) => {
                let mut message = format!(
                    "Wait timed out. Active agents: pending_init={}, running={}, interrupted={}.",
                    snapshot.pending_init, snapshot.running, snapshot.interrupted
                );
                // Fork: say what the wait is still for when terminals are pending (R4.5).
                if !background_terminals.is_empty() {
                    message.push(' ');
                    message.push_str(&describe_background_terminals(&background_terminals));
                }
                (message, true)
            }
        };
        let message = match requested_timeout_ms {
            Some(requested_timeout_ms) if requested_timeout_ms < timeout_ms => format!(
                "{message}\n\nRequested timeout of {requested_timeout_ms}ms was clamped to the minimum of {timeout_ms}ms."
            ),
            Some(_) | None => message,
        };
        Self { message, timed_out }
    }
}

impl ToolOutput for WaitAgentResult {
    fn log_output(&self) -> String {
        tool_output_json_text(self, "wait_agent")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, /*success*/ None, "wait_agent")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "wait_agent")
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ActiveAgentSnapshot {
    pending_init: usize,
    running: usize,
    interrupted: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WaitOutcome {
    MailboxActivity,
    /// Fork: a background terminal exit notification is waiting for the model.
    BackgroundTerminal,
    /// Fork: a workflow run finished and its notification is waiting for the model.
    Workflow,
    Steered,
    NoActiveAgents,
    /// Agent snapshot and the terminals whose exit will still be reported.
    TimedOut(ActiveAgentSnapshot, Vec<WaitingTerminal>),
}

/// Fork: the longest wait without `timeout_ms` that only background terminals hold (R4.5).
pub(crate) const BACKGROUND_TERMINAL_WAIT_CAP_MS: i64 = 300_000;

/// Fork: a wait that the model did not bound and that only terminals hold ends early.
fn effective_timeout_ms(
    requested_timeout_ms: Option<i64>,
    timeout_ms: i64,
    terminals_only: bool,
) -> i64 {
    if requested_timeout_ms.is_none() && terminals_only {
        timeout_ms.min(BACKGROUND_TERMINAL_WAIT_CAP_MS)
    } else {
        timeout_ms
    }
}

/// Fork: one terminal that will still report its exit, as a timeout snapshot shows it.
#[derive(Clone, Debug, Eq, PartialEq)]
struct WaitingTerminal {
    session_id: i32,
    running_for: Duration,
    /// `None` while the process printed nothing.
    quiet_for: Option<Duration>,
}

fn describe_background_terminals(terminals: &[WaitingTerminal]) -> String {
    let details = terminals
        .iter()
        .map(|terminal| {
            let output = match terminal.quiet_for {
                Some(quiet_for) => format!("last output {:.1}s ago", quiet_for.as_secs_f64()),
                None => "no output yet".to_string(),
            };
            format!(
                "session {}: running for {:.1}s, {output}",
                terminal.session_id,
                terminal.running_for.as_secs_f64()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Background terminals that will report their exit: {} ({details}). Their exits are reported even after this turn ends, so you can end the turn instead of waiting again.",
        terminals.len()
    )
}

/// A bound board delivers posts from agents outside the caller's subtree. Remote and
/// host-connected boards have authors outside this runtime; a tree-local board can only
/// receive posts from the other agents of this tree.
async fn board_posts_may_arrive(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
) -> Result<bool, FunctionCallError> {
    use crate::agent_message_board::BoardAudience;
    match crate::agent_message_board::board_audience(&session.services.thread_extension_data) {
        BoardAudience::None => Ok(false),
        BoardAudience::External => Ok(true),
        BoardAudience::Tree => Ok(
            active_agent_snapshot(session, turn, &AgentPath::root()).await?
                != ActiveAgentSnapshot::default(),
        ),
    }
}

async fn active_descendant_snapshot(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
) -> Result<ActiveAgentSnapshot, FunctionCallError> {
    let current_agent_path = turn
        .session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root);
    active_agent_snapshot(session, turn, &current_agent_path).await
}

/// Counts active agents under `prefix`, excluding the caller.
async fn active_agent_snapshot(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
    prefix: &AgentPath,
) -> Result<ActiveAgentSnapshot, FunctionCallError> {
    let current_agent_path = turn
        .session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root);
    let prefix = prefix.to_string();
    let agents = session
        .services
        .agent_control
        .list(
            session.thread_id,
            turn.parent_thread_id,
            &turn.session_source,
            Some(&prefix),
        )
        .await
        .map_err(collab_spawn_error)?;
    let mut snapshot = ActiveAgentSnapshot::default();
    let workflow_children = session.services.local_agent_runtime.workflow_children();
    for agent in agents {
        if agent
            .metadata
            .agent_path
            .as_deref()
            .is_some_and(|path| *path == *current_agent_path.as_str())
        {
            continue;
        }
        // Fork: workflow agents report to their run, not to this wait (dynamic workflow, R3.3).
        if agent
            .metadata
            .agent_path
            .as_ref()
            .is_some_and(|path| workflow_children.owner(path.as_str()).is_some())
        {
            continue;
        }
        match &agent.status {
            AgentStatus::PendingInit => snapshot.pending_init += 1,
            AgentStatus::Running => snapshot.running += 1,
            AgentStatus::Interrupted => snapshot.interrupted += 1,
            status @ (AgentStatus::Completed(_)
            | AgentStatus::Errored(_)
            | AgentStatus::Shutdown
            | AgentStatus::NotFound) => debug_assert!(is_final(status)),
        }
    }
    Ok(snapshot)
}

fn ready_activity(
    activity_rx: &mut tokio::sync::watch::Receiver<InputQueueActivity>,
    pending_activity: Option<InputQueueActivity>,
) -> Option<WaitOutcome> {
    if let Some(activity) = pending_activity {
        return Some(wait_outcome_for_activity(activity));
    }
    match activity_rx.has_changed() {
        Ok(true) => Some(wait_outcome_for_activity(*activity_rx.borrow_and_update())),
        Ok(false) | Err(_) => None,
    }
}

async fn recheck_activity(
    session: &crate::session::session::Session,
    turn_state: Option<&tokio::sync::Mutex<crate::state::TurnState>>,
) -> Option<WaitOutcome> {
    let (mut activity_rx, pending_activity) =
        session.input_queue.subscribe_activity(turn_state).await;
    ready_activity(&mut activity_rx, pending_activity)
}

async fn wait_for_activity(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
    turn_state: Option<&tokio::sync::Mutex<crate::state::TurnState>>,
    activity_rx: &mut tokio::sync::watch::Receiver<InputQueueActivity>,
    deadline: Instant,
) -> Result<WaitOutcome, FunctionCallError> {
    if let Some(outcome) = ready_activity(activity_rx, /*pending_activity*/ None) {
        return Ok(outcome);
    }
    match timeout_at(deadline, activity_rx.changed()).await {
        Ok(Ok(())) => Ok(wait_outcome_for_activity(*activity_rx.borrow_and_update())),
        Ok(Err(_)) | Err(_) => {
            if let Some(outcome) = ready_activity(activity_rx, /*pending_activity*/ None) {
                return Ok(outcome);
            }
            let snapshot = active_descendant_snapshot(session, turn).await?;
            if let Some(outcome) = recheck_activity(session, turn_state).await {
                return Ok(outcome);
            }
            let now = Instant::now();
            let background_terminals = session
                .services
                .unified_exec_manager
                .pending_exit_notifications()
                .await
                .into_iter()
                .map(|terminal| WaitingTerminal {
                    session_id: terminal.process_id,
                    running_for: now.saturating_duration_since(terminal.started_at),
                    quiet_for: terminal
                        .last_output_at
                        .map(|last_output_at| now.saturating_duration_since(last_output_at)),
                })
                .collect();
            Ok(WaitOutcome::TimedOut(snapshot, background_terminals))
        }
    }
}

fn wait_outcome_for_activity(activity: InputQueueActivity) -> WaitOutcome {
    match activity {
        InputQueueActivity::Mailbox => WaitOutcome::MailboxActivity,
        InputQueueActivity::BackgroundTerminal => WaitOutcome::BackgroundTerminal,
        InputQueueActivity::Workflow => WaitOutcome::Workflow,
        InputQueueActivity::Steer => WaitOutcome::Steered,
    }
}

#[cfg(test)]
mod background_terminal_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Fork (R4.5): only a wait without `timeout_ms` that only terminals hold is capped.
    #[test]
    fn caps_only_unbounded_waits_for_terminals() {
        let default = 1_800_000;
        assert_eq!(
            effective_timeout_ms(None, default, /*terminals_only*/ true),
            BACKGROUND_TERMINAL_WAIT_CAP_MS
        );
        assert_eq!(
            effective_timeout_ms(None, default, /*terminals_only*/ false),
            default
        );
        assert_eq!(
            effective_timeout_ms(Some(default), default, /*terminals_only*/ true),
            default
        );
        assert_eq!(
            effective_timeout_ms(None, 60_000, /*terminals_only*/ true),
            60_000
        );
    }

    /// Fork (R4.5): a timeout names each terminal, how long it ran and when it last spoke.
    #[test]
    fn timeout_snapshot_describes_each_terminal() {
        let result = WaitAgentResult::from_outcome(
            WaitOutcome::TimedOut(
                ActiveAgentSnapshot::default(),
                vec![
                    WaitingTerminal {
                        session_id: 1000,
                        running_for: Duration::from_millis(300_000),
                        quiet_for: Some(Duration::from_millis(298_000)),
                    },
                    WaitingTerminal {
                        session_id: 1001,
                        running_for: Duration::from_millis(5_000),
                        quiet_for: None,
                    },
                ],
            ),
            /*requested_timeout_ms*/ None,
            /*timeout_ms*/ BACKGROUND_TERMINAL_WAIT_CAP_MS,
        );
        assert_eq!(
            result,
            WaitAgentResult {
                message: "Wait timed out. Active agents: pending_init=0, running=0, interrupted=0. Background terminals that will report their exit: 2 (session 1000: running for 300.0s, last output 298.0s ago; session 1001: running for 5.0s, no output yet). Their exits are reported even after this turn ends, so you can end the turn instead of waiting again.".to_string(),
                timed_out: true,
            }
        );
        assert_eq!(
            WaitAgentResult::from_outcome(
                WaitOutcome::TimedOut(ActiveAgentSnapshot::default(), Vec::new()),
                /*requested_timeout_ms*/ None,
                /*timeout_ms*/ 1_000,
            )
            .message,
            "Wait timed out. Active agents: pending_init=0, running=0, interrupted=0."
        );
    }
}
