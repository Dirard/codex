use super::*;
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
            WaitOutcome::TimedOut(snapshot, pending) => {
                let mut message = format!(
                    "Wait timed out. Active agents: pending_init={}, running={}, interrupted={}.",
                    snapshot.pending_init, snapshot.running, snapshot.interrupted
                );
                // Fork: say what the wait is still for: terminals and workflow runs (R4.5).
                message.push_str(&describe_pending_work(&pending));
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum WaitOutcome {
    MailboxActivity,
    /// Fork: a background terminal exit notification is waiting for the model.
    BackgroundTerminal,
    /// Fork: a workflow run finished and its notification is waiting for the model.
    Workflow,
    Steered,
    NoActiveAgents,
    /// Agent snapshot and the terminals and workflow runs still pending.
    TimedOut(ActiveAgentSnapshot, PendingWork),
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
            Ok(WaitOutcome::TimedOut(snapshot, pending_work(session).await))
        }
    }
}

// Fork: active-agent snapshots, background terminals, workflow runs and listing errors.
#[path = "wait_fork.rs"]
pub(super) mod fork;
use fork::*;

#[cfg(test)]
#[path = "wait_fork_tests.rs"]
mod background_terminal_tests;
