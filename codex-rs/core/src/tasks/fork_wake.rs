//! Fork (H5 in `docs/fork/01-multi-agent-runtime.md`, review 2026-10-27 P2-2): a wake starts
//! its task only in the turn reservation it checked.
//!
//! `maybe_start_turn_for_pending_work_with_sub_id` checks that its reservation still holds and
//! calls `start_task`, which awaits plugins, token usage, the thread queue and the
//! `BeforeTaskRegistration` hooks before it registers the task. An interrupt in that window
//! takes the reservation; upstream `start_task` then creates a new active turn, so the wake
//! still runs and calls the model after the interrupt, and the mail it took stays in the
//! reservation's orphaned turn state. Here `start_task` registers a wake only in its own
//! reservation; otherwise the wake's input goes back to the thread queues.

use std::sync::Arc;
use std::sync::PoisonError;

use codex_protocol::protocol::TurnAbortReason;
use tokio::sync::Mutex;

use crate::agent::control::fork_completion::FollowupCompletionWatcher;
use crate::session::TurnInput;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::state::ActiveTurn;
use crate::state::TurnState;

/// The reservation a wake's turn context belongs to, kept in its turn store until the task
/// is registered.
struct WakeStart {
    reservation: Arc<Mutex<TurnState>>,
    /// The V1 follow-up watcher starts with the task, not before (H3).
    watcher: std::sync::Mutex<Option<FollowupCompletionWatcher>>,
}

impl Session {
    /// The wake is about to call `start_task` for `turn_context` in `reservation`.
    pub(super) fn fork_bind_wake(
        &self,
        turn_context: &TurnContext,
        reservation: &Arc<Mutex<TurnState>>,
        watcher: Option<FollowupCompletionWatcher>,
    ) {
        turn_context.extension_data.insert(WakeStart {
            reservation: Arc::clone(reservation),
            watcher: std::sync::Mutex::new(watcher),
        });
    }

    /// A wake lost its reservation inside `start_task`: its input and the queue input that
    /// `start_task` drained go back to the thread queues, and extensions that saw the turn
    /// start see it aborted.
    pub(super) async fn fork_lost_wake_start(
        self: &Arc<Self>,
        turn_context: &TurnContext,
        drained: Vec<TurnInput>,
        lifecycle_started: bool,
    ) {
        let Some(wake) = turn_context.extension_data.remove::<WakeStart>() else {
            return;
        };
        // K4: the turn never started, so drop any holder record its start hook wrote, or a
        // close would wait the full grace period for a turn that is not coming.
        self.fork_drop_holder_turn(turn_context).await;
        self.input_queue
            .extend_pending_input_for_turn_state(wake.reservation.as_ref(), drained)
            .await;
        self.fork_requeue_input(&wake.reservation, turn_context)
            .await;
        if lifecycle_started {
            self.emit_turn_abort_lifecycle(
                TurnAbortReason::Interrupted,
                turn_context.extension_data.as_ref(),
            )
            .await;
        }
    }
}

/// The active turn `start_task` uses: any turn for an explicit task, only the reservation it
/// checked for a wake (P2-2).
pub(super) fn fork_task_turn<'a>(
    active: &'a mut Option<ActiveTurn>,
    turn_context: &TurnContext,
) -> Option<&'a mut ActiveTurn> {
    match turn_context.extension_data.get::<WakeStart>() {
        None => Some(active.get_or_insert_with(ActiveTurn::default)),
        Some(wake) => active
            .as_mut()
            .filter(|turn| Arc::ptr_eq(&turn.turn_state, &wake.reservation)),
    }
}

/// `fork_task_turn` at task registration: a wake that still holds its reservation starts its
/// follow-up watcher and forgets the reservation.
pub(super) fn fork_register_turn<'a>(
    active: &'a mut Option<ActiveTurn>,
    turn_context: &TurnContext,
) -> Option<&'a mut ActiveTurn> {
    let turn = fork_task_turn(active, turn_context)?;
    if let Some(wake) = turn_context.extension_data.remove::<WakeStart>()
        && let Some(watcher) = wake
            .watcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    {
        watcher.start();
    }
    Some(turn)
}
