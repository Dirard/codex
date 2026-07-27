use super::*;
use crate::agent::status::is_final;
use crate::session::InputQueueActivity;
use crate::tools::handlers::multi_agents_spec::WaitAgentTimeoutOptions;
use crate::tools::handlers::multi_agents_spec::create_wait_agent_tool_v2;
use codex_tools::ToolSpec;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;
use tokio::time::timeout_at;

#[derive(Default)]
pub(crate) struct Handler {
    options: WaitAgentTimeoutOptions,
}

impl Handler {
    pub(crate) fn new(options: WaitAgentTimeoutOptions) -> Self {
        Self { options }
    }
}

impl ToolExecutor<ToolInvocation> for Handler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("wait_agent")
    }

    fn spec(&self) -> ToolSpec {
        create_wait_agent_tool_v2(self.options)
    }

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
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
        let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);

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
            if snapshot == ActiveAgentSnapshot::default() {
                recheck_activity(session.as_ref(), turn_state.as_deref())
                    .await
                    .unwrap_or(WaitOutcome::NoActiveAgents)
            } else {
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
            WaitOutcome::Steered => ("Wait interrupted by new input.".to_string(), false),
            WaitOutcome::NoActiveAgents => ("No active agents.".to_string(), false),
            WaitOutcome::TimedOut(snapshot) => (
                format!(
                    "Wait timed out. Active agents: pending_init={}, running={}, interrupted={}.",
                    snapshot.pending_init, snapshot.running, snapshot.interrupted
                ),
                true,
            ),
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
    fn log_preview(&self) -> String {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaitOutcome {
    MailboxActivity,
    Steered,
    NoActiveAgents,
    TimedOut(ActiveAgentSnapshot),
}

async fn active_descendant_snapshot(
    session: &crate::session::session::Session,
    turn: &crate::session::turn_context::TurnContext,
) -> Result<ActiveAgentSnapshot, FunctionCallError> {
    session
        .services
        .agent_control
        .register_session_root(session.thread_id, turn.parent_thread_id);
    let current_agent_path = turn
        .session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root);
    let current_agent_name = current_agent_path.to_string();
    let agents = session
        .services
        .agent_control
        .list_agents(&turn.session_source, Some(&current_agent_name))
        .await
        .map_err(collab_spawn_error)?;
    let mut snapshot = ActiveAgentSnapshot::default();
    for agent in agents {
        if agent.agent_name == current_agent_name {
            continue;
        }
        match &agent.agent_status {
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
            Ok(WaitOutcome::TimedOut(snapshot))
        }
    }
}

fn wait_outcome_for_activity(activity: InputQueueActivity) -> WaitOutcome {
    match activity {
        InputQueueActivity::Mailbox => WaitOutcome::MailboxActivity,
        InputQueueActivity::Steer => WaitOutcome::Steered,
    }
}
