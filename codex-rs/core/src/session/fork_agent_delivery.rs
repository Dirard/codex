//! Fork: delivery of a child agent's result to its parent (docs/fork/01-multi-agent-runtime.md
//! F2, B4): queue-only delivery of inter-agent mail, the status a terminal event publishes
//! only after the parent's mail, and the V1 follow-up completion watcher.

use std::sync::Arc;

use codex_protocol::protocol::CodexErrorInfo;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_protocol::turn_input::TurnStartOptions;
use futures::FutureExt;
use futures::future::BoxFuture;

use super::TurnInput;
use super::handlers;
use super::new_submission_id;
use super::session::Session;
use super::turn_context::TurnContext;
use crate::agent::AgentStatus;
use crate::agent::agent_status_from_event;
use crate::agent::control::fork_completion::FollowupCompletionWatcher;
use crate::state::TurnState;

impl Session {
    pub(crate) fn enqueue_inter_agent_communication(
        self: &Arc<Self>,
        communication: crate::PendingAgentMessage,
    ) -> BoxFuture<'_, String> {
        async move {
            let id = new_submission_id();
            handlers::inter_agent_communication(
                self,
                id.clone(),
                communication,
                TurnStartOptions::default(),
            )
            .await;
            id
        }
        .boxed()
    }

    /// Fork: upstream's `send_event_raw_with_persistence`, publishing the status of the event
    /// itself; the body is `send_event_raw_with_persistence_and_status` (B4).
    ///
    /// Not an `async fn`: it returns the body's future, so every event pays no extra future
    /// layer (stack depth matters, see `guardian_ephemeral_retry_preserves_parallel_trunk_and_fork_history`).
    pub(super) fn send_event_raw_with_persistence(
        &self,
        event: Event,
        persist: bool,
    ) -> impl Future<Output = ()> + Send + '_ {
        let status = agent_status_from_event(&event.msg);
        self.send_event_raw_with_persistence_and_status(event, persist, status)
    }

    /// Fork: upstream's `deliver_event_raw`, publishing the status of the event itself; the
    /// body is `deliver_event_raw_with_status` (B4). Returns the body's future, like
    /// `send_event_raw_with_persistence`.
    pub(super) fn deliver_event_raw(&self, event: Event) -> impl Future<Output = ()> + Send + '_ {
        let status = agent_status_from_event(&event.msg);
        self.deliver_event_raw_with_status(event, status)
    }

    /// Fork: a V1 child's turn started by mail that asks for a turn reports its completion to
    /// the parent through the legacy follow-up completion watcher. The wake prepares it before
    /// it checks that its turn reservation still holds (H3).
    pub(crate) async fn fork_followup_watcher(
        &self,
        turn_context: &TurnContext,
        input: &[TurnInput],
    ) -> Option<FollowupCompletionWatcher> {
        let has_trigger_turn = input.iter().any(
            |item| matches!(item, TurnInput::InterAgentCommunication(mail) if mail.trigger_turn),
        );
        if !has_trigger_turn || turn_context.multi_agent_version == MultiAgentVersion::V2 {
            return None;
        }
        let local_agent_control = self.services.local_agent_runtime.control(self.session_id());
        let status_rx = local_agent_control
            .subscribe_status(self.thread_id)
            .await
            .ok()?;
        let child_agent_path = turn_context.session_source.get_agent_path();
        let child_reference = child_agent_path
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| self.thread_id.to_string());
        local_agent_control
            .prepare_followup_completion_watcher(
                self.thread_id,
                turn_context.session_source.clone(),
                child_reference,
                child_agent_path,
                status_rx,
            )
            .await
    }

    /// Fork (H5): a wake whose turn reservation is gone puts the input it took back in the
    /// thread queues instead of dropping it with the reservation's turn state. Mail keeps the
    /// start options the wake gave its turn context.
    pub(crate) async fn fork_requeue_input(
        &self,
        turn_state: &tokio::sync::Mutex<TurnState>,
        turn_context: &TurnContext,
    ) {
        let input = self
            .input_queue
            .take_pending_input_for_turn_state(turn_state)
            .await;
        let metadata = &turn_context.turn_metadata_state;
        let start_options = TurnStartOptions {
            turn_trigger: metadata.current_turn_trigger(),
            final_output_json_schema: turn_context.final_output_json_schema.clone(),
            parent_turn_id: metadata.parent_turn_id(),
            initiating_agent_path: metadata.initiating_agent_path().cloned(),
            root_turn_id: metadata.root_turn_id(),
            cyber_access_program: turn_context.cyber_access_program,
            ..Default::default()
        };
        self.input_queue
            .requeue_lost_wake_input(input, start_options)
            .await;
    }
}

/// Fork (B4): the agent status that `send_event` publishes for `msg` after the parent's mail.
/// `maybe_notify_parent_of_terminal_turn` replaces it with a terminal error it reports. A turn
/// status error of a spawned V2 child publishes nothing: its terminal event publishes the
/// error after the parent's mail, so a parent's `wait_agent` never sees a final child status
/// before the mail.
pub(super) fn event_status(turn_context: &TurnContext, msg: &EventMsg) -> Option<AgentStatus> {
    if turn_context.multi_agent_version == MultiAgentVersion::V2
        && matches!(
            &turn_context.session_source,
            SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                agent_path: Some(_),
                ..
            })
        )
        && matches!(
            msg,
            EventMsg::Error(error)
                if error
                    .codex_error_info
                    .as_ref()
                    .is_some_and(CodexErrorInfo::affects_turn_status)
        )
    {
        return None;
    }
    agent_status_from_event(msg)
}

/// Fork: an explorer's result is capped for its parent (specs/explorer-and-questions-2026-10-25.md).
pub(super) fn result_max_tokens(turn_context: &TurnContext) -> Option<usize> {
    crate::agent::explorer_profile::result_max_tokens(
        &turn_context.config,
        &turn_context.session_source,
    )
}
