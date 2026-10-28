//! Fork: thread spawn outcomes and inherited dynamic tools of `ThreadManagerState`.
//!
//! A resume can find the thread already running (another resume published it first). The
//! fork reports that as [`ThreadSpawnOutcome::AlreadyRunning`], so agent reloads do not
//! count a second spawn. Upstream callers keep `spawn_thread`, which returns the thread
//! either way.

use codex_protocol::ThreadId;
use codex_protocol::dynamic_tools::DynamicToolSpec;
use codex_protocol::error::Result as CodexResult;

use super::NewThread;
use super::StartThreadOptions;
use super::ThreadManagerState;
use super::ThreadSpawnRequest;

/// What `spawn_thread_outcome` did with a spawn request.
pub(crate) enum ThreadSpawnOutcome {
    /// The thread was started and published by this request.
    Spawned(NewThread),
    /// A resume found the thread already running; nothing was started.
    AlreadyRunning(NewThread),
}

impl ThreadSpawnOutcome {
    pub(crate) fn into_new_thread(self) -> NewThread {
        match self {
            Self::Spawned(thread) | Self::AlreadyRunning(thread) => thread,
        }
    }

    /// The thread, and whether it was already running.
    pub(crate) fn into_parts(self) -> (NewThread, bool) {
        match self {
            Self::Spawned(thread) => (thread, false),
            Self::AlreadyRunning(thread) => (thread, true),
        }
    }
}

impl ThreadManagerState {
    /// Every child inherits its parent's dynamic tools, not only V2 children with
    /// `MultiAgentV2DynamicTools`.
    pub(crate) const CHILDREN_INHERIT_DYNAMIC_TOOLS: bool = true;

    /// Spawn a new thread with optional history and register it with the manager; a resume
    /// of a running thread returns that thread.
    ///
    /// A combinator, not an `async fn`: a debug build would otherwise put another frame the
    /// size of the spawn future on every thread start, resume and fork stack, and with Rust
    /// 1.95 the app-server request path no longer fits the 8 MiB test thread stack.
    pub(super) fn spawn_thread(
        &self,
        request: ThreadSpawnRequest,
    ) -> impl std::future::Future<Output = CodexResult<NewThread>> + Send + '_ {
        futures::FutureExt::map(self.spawn_thread_outcome(request), |outcome| {
            outcome.map(ThreadSpawnOutcome::into_new_thread)
        })
    }

    /// A child without its own dynamic tools inherits its parent's
    /// (`specs/fork-resume-dynamic-tools-2026-10-24.md`).
    pub(super) async fn inherit_dynamic_tools(
        &self,
        options: &mut StartThreadOptions,
        parent_thread_id: Option<ThreadId>,
    ) {
        if options.dynamic_tools.is_empty() {
            options.dynamic_tools = self.dynamic_tools_for_child(parent_thread_id).await;
        }
    }

    pub(super) async fn dynamic_tools_for_child(
        &self,
        parent_thread_id: Option<ThreadId>,
    ) -> Vec<DynamicToolSpec> {
        let Some(parent_thread_id) = parent_thread_id else {
            return Vec::new();
        };
        let Ok(parent) = self.get_thread(parent_thread_id).await else {
            return Vec::new();
        };
        parent.session.dynamic_tools().await.to_vec()
    }
}
