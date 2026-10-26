//! Fork: what a MultiAgentV2 `wait_agent` reports besides mailbox activity.
//!
//! A wait without active descendants returns `No active agents`; a timeout shows an
//! active-agent snapshot and the background terminals whose exit will still be reported
//! (`specs/background-exec-notifications-2026-10-25.md`, R4.5); workflow agents report to
//! their run (`specs/dynamic-workflow-2026-10-25.md`, R3.3).

use super::*;
use crate::agent::status::is_final;
use codex_protocol::AgentPath;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ActiveAgentSnapshot {
    pub(super) pending_init: usize,
    pub(super) running: usize,
    pub(super) interrupted: usize,
}

/// The longest wait without `timeout_ms` that only background terminals hold (R4.5).
pub(crate) const BACKGROUND_TERMINAL_WAIT_CAP_MS: i64 = 300_000;

/// A wait that the model did not bound and that only terminals hold ends early.
pub(super) fn effective_timeout_ms(
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

/// One terminal that will still report its exit, as a timeout snapshot shows it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WaitingTerminal {
    pub(super) session_id: i32,
    pub(super) running_for: Duration,
    /// `None` while the process printed nothing.
    pub(super) quiet_for: Option<Duration>,
}

pub(super) fn describe_background_terminals(terminals: &[WaitingTerminal]) -> String {
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
pub(super) async fn board_posts_may_arrive(
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

pub(super) async fn active_descendant_snapshot(
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
pub(super) async fn active_agent_snapshot(
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
        .map_err(wait_list_error)?;
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

/// A failed agent listing fails `wait_agent` as a wait, not as a spawn
/// (B12 in `docs/fork/01-multi-agent-runtime.md`).
fn wait_list_error(err: CodexErr) -> FunctionCallError {
    match err.details() {
        CodexErrorDetails::UnsupportedOperation(message) if message == "thread manager dropped" => {
            FunctionCallError::RespondToModel("collab manager unavailable".to_string())
        }
        CodexErrorDetails::UnsupportedOperation(message) => {
            FunctionCallError::RespondToModel(message.clone())
        }
        _ => FunctionCallError::RespondToModel(format!("collab wait failed: {err}")),
    }
}

pub(super) fn ready_activity(
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

pub(super) async fn recheck_activity(
    session: &crate::session::session::Session,
    turn_state: Option<&tokio::sync::Mutex<crate::state::TurnState>>,
) -> Option<WaitOutcome> {
    let (mut activity_rx, pending_activity) =
        session.input_queue.subscribe_activity(turn_state).await;
    ready_activity(&mut activity_rx, pending_activity)
}

pub(super) fn wait_outcome_for_activity(activity: InputQueueActivity) -> WaitOutcome {
    match activity {
        InputQueueActivity::Mailbox => WaitOutcome::MailboxActivity,
        InputQueueActivity::BackgroundTerminal => WaitOutcome::BackgroundTerminal,
        InputQueueActivity::Workflow => WaitOutcome::Workflow,
        InputQueueActivity::Steer => WaitOutcome::Steered,
    }
}

/// The terminals whose exit will still be reported, as a timeout snapshot shows them.
pub(super) async fn waiting_terminals(
    session: &crate::session::session::Session,
) -> Vec<WaitingTerminal> {
    let now = Instant::now();
    session
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
        .collect()
}
