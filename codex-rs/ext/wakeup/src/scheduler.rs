//! Timers and turn starts of scheduled wakeups (`specs/scheduled-wakeups-2026-11-07.md`, UC3).
//!
//! A timer only marks its wakeup due; a due wakeup starts a turn through
//! `CodexThread::start_turn_if_idle` when the thread is idle and an automatic turn is
//! allowed, otherwise it stays due and the next `on_thread_idle` after a completed or failed
//! turn starts it (D2). A wakeup never joins a running turn (R3.2).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Weak;

use chrono::DateTime;
use chrono::Utc;
use codex_core::CodexThread;
use codex_core::NotSubmittedReason;
use codex_core::StartIfIdleSubmission;
use codex_core::ThreadManager;
use codex_core::TurnInput;
use codex_core::TurnInputRequest;
use codex_core::TurnStartOptions;
use codex_core::context::ContextualUserFragment;
use codex_core::context::InternalContextSource;
use codex_core::context::InternalModelContextFragment;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ModeKind;
use codex_protocol::models::ContentItemMetadata;
use codex_protocol::models::ContentItemNamespace;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::user_input::UserInput;
use tokio::time::Instant;

use crate::state::TurnKind;
use crate::state::WALL_CLOCK_RECHECK;
use crate::state::WakeupThreadState;

/// A turn the extension asks the host to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StartRequest {
    /// A model wakeup: the rendered wake input, hidden context with tool provenance (R3.7).
    Wakeup { kind: TurnKind, body: String },
    /// A loop iteration: the user's loop prompt as user input (R5.2).
    LoopIteration { kind: TurnKind, prompt: String },
}

/// What the host did with a start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StartOutcome {
    Started {
        turn_id: String,
    },
    /// Not now: a turn runs or is reserved, Plan mode, an interrupted thread or mail that
    /// starts a turn first (R3.2, R3.3).
    Held,
    /// The host is draining or the runtime is gone (R3.4).
    Closed,
}

pub(crate) type HostFuture<'a> = Pin<Box<dyn Future<Output = StartOutcome> + Send + 'a>>;

/// Starts turns for wakeups; a trait so tests can use a fake host.
pub(crate) trait WakeupHost: Send + Sync {
    fn start_turn<'a>(
        &'a self,
        state: &'a WakeupThreadState,
        request: StartRequest,
    ) -> HostFuture<'a>;
}

/// Waits until the wakeup `id` is due: its monotonic deadline passed or, checked at least
/// every [`WALL_CLOCK_RECHECK`], its wall-clock due time, which also passes while the host
/// sleeps (R3.6). Then marks it due and tries to fire it in a task of its own, so dropping
/// the timer with the wakeup cannot cancel the start.
pub(crate) async fn run_timer(
    state: Weak<WakeupThreadState>,
    id: String,
    due_at: DateTime<Utc>,
    deadline: Instant,
) {
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        {
            let Some(state) = state.upgrade() else {
                return;
            };
            if state.now() >= due_at {
                break;
            }
        }
        tokio::time::sleep((deadline - now).min(WALL_CLOCK_RECHECK)).await;
    }
    let Some(state) = state.upgrade() else {
        return;
    };
    if state.mark_due(&id) {
        tokio::spawn(fire_if_due(state));
    }
}

/// Starts the due wakeup when the thread is idle and an automatic turn is allowed (UC3):
/// one start at a time per thread, so the timer and `on_thread_idle` never start two turns
/// for one wakeup (R3.5).
#[expect(
    clippy::await_holding_invalid_type,
    reason = "a start and its bookkeeping must not interleave with another start or a cancel"
)]
pub(crate) async fn fire_if_due(state: Arc<WakeupThreadState>) {
    let _fire = state.fire_lock.lock().await;
    let Some(request) = state.prepare_start() else {
        return;
    };
    let kind = request.kind().clone();
    let outcome = state.host.start_turn(&state, request).await;
    state.finish_start(&kind, outcome);
}

/// The host of app-server, TUI and `codex exec` threads: the live thread of the
/// `ThreadManager`.
pub(crate) struct ThreadManagerHost {
    pub(crate) thread_manager: Weak<ThreadManager>,
}

impl ThreadManagerHost {
    /// The live thread of this state's runtime; a later runtime of the same thread id has
    /// another state (R4.4).
    pub(crate) async fn live_thread(
        thread_manager: &Weak<ThreadManager>,
        state: &WakeupThreadState,
    ) -> Option<Arc<CodexThread>> {
        let thread_manager = thread_manager.upgrade()?;
        let thread_id = ThreadId::from_string(&state.thread_id).ok()?;
        let thread = thread_manager.get_thread(thread_id).await.ok()?;
        let same_runtime = thread
            .thread_extension_data()
            .get::<WakeupThreadState>()
            .is_some_and(|found| std::ptr::eq(Arc::as_ptr(&found), state));
        same_runtime.then_some(thread)
    }
}

impl WakeupHost for ThreadManagerHost {
    fn start_turn<'a>(
        &'a self,
        state: &'a WakeupThreadState,
        request: StartRequest,
    ) -> HostFuture<'a> {
        Box::pin(async move {
            let Some(thread) = Self::live_thread(&self.thread_manager, state).await else {
                return StartOutcome::Closed;
            };
            // `start_if_idle` does not look at the interrupt mark, and user input starts
            // even in Plan mode: check both here (R3.3).
            if matches!(thread.agent_status().await, AgentStatus::Interrupted) {
                return StartOutcome::Held;
            }
            let previous = thread
                .thread_extension_data()
                .get::<TurnStartOptions>()
                .map(|options| options.as_ref().clone())
                .unwrap_or_default();
            let request = match request {
                StartRequest::Wakeup { body, .. } => {
                    let item = ContextualUserFragment::into(
                        InternalModelContextFragment::new(
                            InternalContextSource::from_static("scheduled_wakeup"),
                            body,
                        )
                        .with_metadata(ContentItemMetadata::tool(Some(
                            ContentItemNamespace::Functions,
                        ))),
                    );
                    TurnInputRequest::new(TurnInput::ResponseItem(item)).on_start(
                        TurnStartOptions {
                            turn_trigger: Some("wakeup".to_string()),
                            ..previous
                        },
                    )
                }
                StartRequest::LoopIteration { prompt, .. } => {
                    if thread
                        .thread_settings_snapshot()
                        .await
                        .collaboration_mode
                        .mode
                        == ModeKind::Plan
                    {
                        return StartOutcome::Held;
                    }
                    TurnInputRequest::user_input(vec![UserInput::Text {
                        text: prompt,
                        text_elements: Vec::new(),
                    }])
                    .on_start(TurnStartOptions {
                        turn_trigger: Some("loop".to_string()),
                        ..previous
                    })
                }
            };
            match thread.start_turn_if_idle(request).await {
                Ok(StartIfIdleSubmission::Started { turn_id, .. }) => {
                    StartOutcome::Started { turn_id }
                }
                Ok(StartIfIdleSubmission::NotSubmitted {
                    reason: NotSubmittedReason::ServerDraining,
                }) => StartOutcome::Closed,
                Ok(StartIfIdleSubmission::NotSubmitted { reason }) => {
                    tracing::debug!(?reason, "scheduled wakeup waits for an idle thread");
                    StartOutcome::Held
                }
                Err(error) => {
                    tracing::warn!(%error, "scheduled wakeup could not start a turn");
                    StartOutcome::Held
                }
            }
        })
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
