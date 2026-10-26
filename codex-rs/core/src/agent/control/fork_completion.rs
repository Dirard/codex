//! Fork: completion watchers of legacy (non-V2) children.
//!
//! A follow-up turn of a finished child gets its own watcher, which replaces the watcher of
//! the previous turn: `tasks/mod.rs` prepares it before the wake's reservation check and
//! starts it with the turn (H3 in `docs/fork/01-multi-agent-runtime.md`). A watcher drops its
//! record when it ends (B9). Explorer results are capped like the
//! completion mail of V2 children (`specs/explorer-and-questions-2026-10-25.md`).

use super::*;
use crate::codex_thread::CodexThread;
use futures::future::AbortHandle;
use futures::future::Abortable;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;
use std::sync::PoisonError;
use tokio::task::JoinHandle;

/// The newest completion watcher of each child, as `LocalAgentRuntime` records them.
type CompletionWatchers = Arc<Mutex<HashMap<ThreadId, (AbortHandle, JoinHandle<()>)>>>;

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

    /// Spawns `watcher` as the newest watcher of `child_thread_id`; a watcher it replaces is
    /// aborted. The record goes away when the watcher ends, unless a newer watcher of the same
    /// child replaced it.
    pub(super) fn spawn_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        abort: AbortHandle,
        watcher: impl Future<Output = ()> + Send + 'static,
    ) {
        let completion_watchers = Arc::clone(&self.runtime.completion_watchers);
        // The task drops its record under this lock, so it cannot do so before the insert.
        let mut watchers = self
            .runtime
            .completion_watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let completion = tokio::spawn(async move {
            let _record = WatcherRecord {
                completion_watchers,
                child_thread_id,
                task_id: tokio::task::id(),
            };
            watcher.await;
        });
        if let Some((previous_abort, _)) = watchers.insert(child_thread_id, (abort, completion)) {
            previous_abort.abort();
        }
    }

    /// Prepares the watcher of a follow-up turn of a finished child. The child's previous
    /// watcher ends first: it reports the previous turn, or it is aborted after an interrupted
    /// turn, whose final status the follow-up turn will produce.
    pub(crate) async fn prepare_followup_completion_watcher(
        &self,
        child_thread_id: ThreadId,
        session_source: SessionSource,
        child_reference: String,
        child_agent_path: Option<AgentPath>,
        mut status_updates: StatusSubscription,
    ) -> Option<FollowupCompletionWatcher> {
        let Some(Ok(initial_snapshot)) = status_updates.next().await else {
            return None;
        };
        let previous_watcher_action = match initial_snapshot.status()? {
            AgentStatus::Completed(_) | AgentStatus::Errored(_) => PreviousWatcherAction::Wait,
            AgentStatus::Interrupted => PreviousWatcherAction::Abort,
            AgentStatus::PendingInit | AgentStatus::Running => {
                // The first or still-active turn already owns its completion watcher.
                return None;
            }
            AgentStatus::Shutdown | AgentStatus::NotFound => return None,
        };
        let previous_watcher = self
            .runtime
            .completion_watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&child_thread_id);
        if let Some((abort, completion)) = previous_watcher {
            match previous_watcher_action {
                PreviousWatcherAction::Wait => {}
                PreviousWatcherAction::Abort => abort.abort(),
            }
            let _ = completion.await;
        }
        Some(FollowupCompletionWatcher {
            control: self.clone(),
            child_thread_id,
            session_source,
            child_reference,
            child_agent_path,
            status_updates,
        })
    }
}

/// The watcher of a follow-up turn, prepared before the turn starts (H3 in
/// `docs/fork/01-multi-agent-runtime.md`). A wake starts it only while it still owns its
/// turn reservation: a watcher of a wake that lost it would report a later turn the wake did
/// not start and would hold up the watcher of the next follow-up.
pub(crate) struct FollowupCompletionWatcher {
    control: LocalAgentControl,
    child_thread_id: ThreadId,
    session_source: SessionSource,
    child_reference: String,
    child_agent_path: Option<AgentPath>,
    status_updates: StatusSubscription,
}

impl FollowupCompletionWatcher {
    /// Watches for the final status of the turn that starts next. It does not wait, so the
    /// caller's reservation check still holds when the turn starts.
    pub(crate) fn start(self) {
        self.control.start_completion_watcher(
            self.child_thread_id,
            Some(self.session_source),
            self.child_reference,
            self.child_agent_path,
            CompletionWatcherStart::AfterCurrentTurn(self.status_updates),
        );
    }
}

/// Drops a watcher's record when the watcher ends, unless a newer watcher of the same child
/// replaced the record (B9).
struct WatcherRecord {
    completion_watchers: CompletionWatchers,
    child_thread_id: ThreadId,
    task_id: tokio::task::Id,
}

impl Drop for WatcherRecord {
    fn drop(&mut self) {
        let mut watchers = self
            .completion_watchers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if watchers
            .get(&self.child_thread_id)
            .is_some_and(|(_, completion)| completion.id() == self.task_id)
        {
            watchers.remove(&self.child_thread_id);
        }
    }
}
