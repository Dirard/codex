//! Fork: completion watchers of legacy (non-V2) children.
//!
//! A follow-up turn of a finished child gets its own watcher, which replaces the watcher of
//! the previous turn (`tasks/mod.rs` starts it). Explorer results are capped like the
//! completion mail of V2 children (`specs/explorer-and-questions-2026-10-25.md`).

use super::*;
use crate::codex_thread::CodexThread;
use futures::future::AbortHandle;
use futures::future::Abortable;
use std::future::Future;
use tokio::task::JoinHandle;

/// Where a completion watcher reads the child's status from.
pub(crate) enum CompletionWatcherStart {
    /// Subscribe to the child's status now.
    CurrentStatus,
    /// Continue an existing subscription after the child's current turn.
    AfterCurrentTurn(StatusSubscription),
}

enum PreviousWatcherAction {
    Wait,
    Abort,
}

impl CompletionWatcherStart {
    /// The status updates a watcher waits on. A failed subscription yields no updates, so the
    /// watcher reads the child's status once.
    pub(super) async fn subscribe(
        self,
        control: &LocalAgentControl,
        child_thread_id: ThreadId,
    ) -> CodexResult<StatusSubscription> {
        Ok(match self {
            Self::CurrentStatus => control
                .subscribe_status(child_thread_id)
                .await
                .unwrap_or_else(|_| futures::stream::empty().boxed()),
            Self::AfterCurrentTurn(status_updates) => status_updates,
        })
    }
}

/// The parent receives at most the explorer profile's result limit.
pub(super) async fn capped_status(
    status: AgentStatus,
    child_thread: Option<&CodexThread>,
) -> AgentStatus {
    match (status, child_thread) {
        (AgentStatus::Completed(Some(message)), Some(child_thread)) => {
            let child_config = child_thread.config().await;
            let child_source = child_thread.config_snapshot().await.session_source;
            match crate::agent::explorer_profile::result_max_tokens(&child_config, &child_source) {
                Some(max_tokens) => AgentStatus::Completed(Some(
                    crate::agent::explorer_profile::cap_result(&message, max_tokens),
                )),
                None => AgentStatus::Completed(Some(message)),
            }
        }
        (status, _) => status,
    }
}

/// A watcher that a later follow-up watcher can abort.
pub(super) fn abortable(
    watcher: impl Future<Output = ()> + Send + 'static,
) -> (AbortHandle, impl Future<Output = ()> + Send + 'static) {
    let (abort, registration) = AbortHandle::new_pair();
    (abort, async move {
        let _ = Abortable::new(watcher, registration).await;
    })
}

impl LocalAgentControl {
    /// Starts a detached watcher for sub-agents spawned from another thread.
    pub(super) fn maybe_start_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        session_source: Option<SessionSource>,
        child_reference: String,
        child_agent_path: Option<AgentPath>,
    ) {
        self.start_completion_watcher(
            child_thread_id,
            session_source,
            child_reference,
            child_agent_path,
            CompletionWatcherStart::CurrentStatus,
        );
    }

    /// Records the newest watcher of `child_thread_id`; a watcher it replaces is aborted.
    pub(super) fn register_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        abort: AbortHandle,
        completion: JoinHandle<()>,
    ) {
        if let Some(previous_watcher) = self
            .runtime
            .completion_watchers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(child_thread_id, (abort, completion))
        {
            previous_watcher.0.abort();
        }
    }

    pub(crate) async fn start_followup_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        session_source: SessionSource,
        child_reference: String,
        child_agent_path: Option<AgentPath>,
        mut status_updates: StatusSubscription,
    ) {
        let Some(Ok(initial_snapshot)) = status_updates.next().await else {
            return;
        };
        let Some(initial_status) = initial_snapshot.status() else {
            return;
        };
        let previous_watcher_action = match initial_status {
            AgentStatus::Completed(_) | AgentStatus::Errored(_) => PreviousWatcherAction::Wait,
            AgentStatus::Interrupted => PreviousWatcherAction::Abort,
            AgentStatus::PendingInit | AgentStatus::Running => {
                // The first or still-active turn already owns its completion watcher.
                return;
            }
            AgentStatus::Shutdown | AgentStatus::NotFound => return,
        };
        let previous_watcher = self
            .runtime
            .completion_watchers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&child_thread_id);
        if let Some((abort, completion)) = previous_watcher {
            match previous_watcher_action {
                PreviousWatcherAction::Wait => {}
                PreviousWatcherAction::Abort => abort.abort(),
            }
            let _ = completion.await;
        }
        self.start_completion_watcher(
            child_thread_id,
            Some(session_source),
            child_reference,
            child_agent_path,
            CompletionWatcherStart::AfterCurrentTurn(status_updates),
        );
    }
}
