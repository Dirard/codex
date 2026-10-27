//! Fork: delivery of a child's terminal result to its parent, loaded or not.
//!
//! Results are queue-only mail. A loaded parent receives it before the child publishes its
//! terminal status; an unloaded parent keeps it in the runtime mailbox. Workflow children
//! report to their run instead (`specs/dynamic-workflow-2026-10-25.md`, UC3).

use super::*;
use crate::agent::api::AgentTurnOutcome;

impl LocalAgentControl {
    /// Delivers a terminal result to a loaded or unloaded parent.
    ///
    /// Loaded recipients receive the mail synchronously, before the sender publishes its
    /// terminal status, while their residency is pinned. Queue-only mail for an unloaded
    /// recipient is retained in the runtime mailbox, as `submit_inter_agent_communication` does.
    /// Results are queue-only; turn-triggering mail belongs to `send_inter_agent_communication`,
    /// which also carries turn start options.
    pub(crate) async fn enqueue_inter_agent_communication(
        &self,
        agent_id: ThreadId,
        mut communication: InterAgentCommunication,
        context: AgentCommunicationContext,
    ) -> CodexResult<String> {
        let state = self.runtime.upgrade()?;
        let observers = state.extensions.message_lifecycle_contributors();
        let delivery = (!observers.is_empty()).then(|| {
            let message_id = communication
                .id
                .take()
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| ResponseItemId::new("amsg"));
            communication.id = Some(message_id.clone());
            MessageDelivery::publish(context.sender_thread_id, message_id, observers)
        });
        let log_content = crate::agent_communication::send_log_content(&communication);
        // A second attempt observes the registry after an eviction that raced the pin.
        let mut attempts_left = 2;
        let communication_id = loop {
            attempts_left -= 1;
            let thread = {
                let threads = state.threads.read().await;
                match threads.get(&agent_id) {
                    Some(thread) if !thread.session_source.is_internal() => Arc::clone(thread),
                    None if !communication.trigger_turn => {
                        let _membership = self.runtime.admit_start()?;
                        self.runtime.ensure_agent_known(agent_id)?;
                        let submission_id = uuid::Uuid::now_v7().to_string();
                        self.runtime.mailboxes.enqueue(
                            agent_id,
                            Some(submission_id.clone()),
                            vec![crate::PendingAgentMessage::new(communication, delivery)],
                        )?;
                        break submission_id;
                    }
                    Some(_) | None => return Err(CodexErr::ThreadNotFound(agent_id)),
                }
            };
            if communication.trigger_turn {
                thread
                    .ensure_execution_capacity_for_turn_start(self)
                    .await?;
            }
            let _residency_guard = match thread
                .session
                .services
                .local_agent_runtime
                .pin_v2_residency(&state, &thread)
                .await
            {
                Ok(guard) => guard,
                Err(err)
                    if attempts_left > 0
                        && matches!(err.details(), CodexErrorDetails::ThreadNotFound(_)) =>
                {
                    continue;
                }
                Err(err) => return Err(err),
            };
            break thread
                .session
                .enqueue_inter_agent_communication(crate::PendingAgentMessage::new(
                    communication,
                    delivery,
                ))
                .await;
        };
        crate::agent_communication::emit_agent_communication_send(
            &communication_id,
            &context,
            log_content.as_deref(),
            agent_id,
        );
        Ok(communication_id)
    }

    /// `send_inter_agent_communication` for a child's terminal result. Results are queue-only
    /// and never start a turn, so `start_options` is not used.
    pub(super) async fn deliver_result_communication(
        &self,
        agent_id: ThreadId,
        communication: InterAgentCommunication,
        context: AgentCommunicationContext,
        _start_options: TurnStartOptions,
    ) -> CodexResult<String> {
        self.enqueue_inter_agent_communication(agent_id, communication, context)
            .await
    }

    /// A workflow child reports to its run, never to the parent (dynamic workflow, UC3).
    pub(super) fn deliver_to_workflow(
        &self,
        child_agent_path: &AgentPath,
        outcome: &AgentTurnOutcome,
    ) -> bool {
        self.runtime
            .workflow_children
            .deliver(child_agent_path, outcome)
    }

    /// Spawns a child whose first input is mail, as `spawn_agent` does for V2 children.
    #[cfg(test)]
    pub(crate) async fn spawn_agent_with_communication(
        &self,
        config: Config,
        communication: InterAgentCommunication,
        context: AgentCommunicationContext,
        session_source: Option<SessionSource>,
        options: crate::agent::types::SpawnAgentOptions,
    ) -> CodexResult<LiveAgent> {
        let (spawned_agent, _) = Box::pin(self.spawn_agent_internal(
            config,
            super::spawn::SpawnInitialInput::InterAgentCommunication(communication, context),
            session_source,
            options,
        ))
        .await?;
        Ok(spawned_agent)
    }
}
