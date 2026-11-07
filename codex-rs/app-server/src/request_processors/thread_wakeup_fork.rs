//! Fork: `thread/wakeup/read`, `thread/wakeup/cancel` and `thread/loop/start` over the
//! scheduled wakeups of a loaded thread (`specs/scheduled-wakeups-2026-11-07.md`, UC8).

use std::sync::Arc;

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ClientResponsePayload;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadLoop;
use codex_app_server_protocol::ThreadLoopStartParams;
use codex_app_server_protocol::ThreadLoopStartResponse;
use codex_app_server_protocol::ThreadWakeup;
use codex_app_server_protocol::ThreadWakeupCancelParams;
use codex_app_server_protocol::ThreadWakeupCancelReason;
use codex_app_server_protocol::ThreadWakeupCancelResponse;
use codex_app_server_protocol::ThreadWakeupChain;
use codex_app_server_protocol::ThreadWakeupChange;
use codex_app_server_protocol::ThreadWakeupOrigin;
use codex_app_server_protocol::ThreadWakeupReadParams;
use codex_app_server_protocol::ThreadWakeupReadResponse;
use codex_app_server_protocol::ThreadWakeupState;
use codex_app_server_protocol::ThreadWakeupUpdatedNotification;
use codex_core::CodexThread;
use codex_core::ThreadManager;
use codex_protocol::ThreadId;
use codex_wakeup_extension::CancelReason;
use codex_wakeup_extension::LoopStartRequest;
use codex_wakeup_extension::WakeupChange;
use codex_wakeup_extension::WakeupOrigin;
use codex_wakeup_extension::WakeupSnapshot;
use codex_wakeup_extension::WakeupUpdate;

use crate::error_code::internal_error;
use crate::error_code::invalid_request;

/// Handles a `thread/wakeup/*` or `thread/loop/start` request routed by `MessageProcessor`.
pub(crate) async fn handle(
    thread_manager: &Arc<ThreadManager>,
    request: ClientRequest,
) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
    match request {
        ClientRequest::ThreadWakeupRead { params, .. } => read(thread_manager, params)
            .await
            .map(|response| Some(response.into())),
        ClientRequest::ThreadWakeupCancel { params, .. } => cancel(thread_manager, params)
            .await
            .map(|response| Some(response.into())),
        ClientRequest::ThreadLoopStart { params, .. } => start_loop(thread_manager, params)
            .await
            .map(|response| Some(response.into())),
        other => Err(internal_error(format!(
            "{} is not a thread/wakeup request",
            other.method_name()
        ))),
    }
}

/// R8.1: whether the thread may use wakeups, and its state.
async fn read(
    thread_manager: &Arc<ThreadManager>,
    params: ThreadWakeupReadParams,
) -> Result<ThreadWakeupReadResponse, JSONRPCErrorError> {
    let thread = loaded_thread(thread_manager, &params.thread_id).await?;
    let (available, snapshot) = codex_wakeup_extension::read(&thread);
    Ok(ThreadWakeupReadResponse {
        available,
        state: wakeup_state(snapshot),
    })
}

/// R8.2, R4.2: cancel for the user.
async fn cancel(
    thread_manager: &Arc<ThreadManager>,
    params: ThreadWakeupCancelParams,
) -> Result<ThreadWakeupCancelResponse, JSONRPCErrorError> {
    let thread = loaded_thread(thread_manager, &params.thread_id).await?;
    let (cancelled, snapshot) = codex_wakeup_extension::cancel(&thread).await;
    Ok(ThreadWakeupCancelResponse {
        cancelled,
        state: wakeup_state(snapshot),
    })
}

/// R8.3, R5.1, R5.7: start a loop or name the invalid parameter.
async fn start_loop(
    thread_manager: &Arc<ThreadManager>,
    params: ThreadLoopStartParams,
) -> Result<ThreadLoopStartResponse, JSONRPCErrorError> {
    let thread = loaded_thread(thread_manager, &params.thread_id).await?;
    let snapshot = codex_wakeup_extension::start_loop(
        &thread,
        LoopStartRequest {
            prompt: params.prompt,
            interval_seconds: params.interval_seconds,
            max_iterations: params.max_iterations,
        },
    )
    .await
    .map_err(invalid_request)?;
    Ok(ThreadLoopStartResponse {
        state: wakeup_state(snapshot),
    })
}

/// Wakeups live with the thread's runtime, so only a loaded thread has them.
async fn loaded_thread(
    thread_manager: &Arc<ThreadManager>,
    raw_thread_id: &str,
) -> Result<Arc<CodexThread>, JSONRPCErrorError> {
    let thread_id = ThreadId::from_string(raw_thread_id)
        .map_err(|error| invalid_request(format!("invalid thread id: {error}")))?;
    thread_manager
        .get_thread(thread_id)
        .await
        .map_err(|_| invalid_request(format!("thread not loaded: {thread_id}")))
}

pub(crate) fn wakeup_state(snapshot: WakeupSnapshot) -> ThreadWakeupState {
    ThreadWakeupState {
        wakeup: snapshot.wakeup.map(|wakeup| ThreadWakeup {
            id: wakeup.id,
            origin: match wakeup.origin {
                WakeupOrigin::Model => ThreadWakeupOrigin::Model,
                WakeupOrigin::Loop => ThreadWakeupOrigin::Loop,
            },
            due_at: wakeup.due_at,
            delay_seconds: wakeup.delay_seconds,
            requested_delay_seconds: wakeup.requested_delay_seconds,
            due: wakeup.due,
            reason: wakeup.reason,
            prompt: wakeup.prompt,
            scheduled_in_turn_id: wakeup.scheduled_in_turn_id,
        }),
        loop_state: snapshot.loop_state.map(|state| ThreadLoop {
            id: state.id,
            prompt: state.prompt,
            interval_seconds: state.interval_seconds,
            iteration: state.iteration,
            max_iterations: state.max_iterations,
            started_at: state.started_at,
            ends_at: state.ends_at,
        }),
        chain: ThreadWakeupChain {
            fired: snapshot.chain.fired,
            max_wakeups: snapshot.chain.max_wakeups,
            ends_at: snapshot.chain.ends_at,
        },
    }
}

/// The `thread/wakeup/updated` notification of an update (R8.4).
pub(crate) fn wakeup_updated_notification(
    thread_id: String,
    update: &WakeupUpdate,
) -> ThreadWakeupUpdatedNotification {
    ThreadWakeupUpdatedNotification {
        thread_id,
        change: match update.change {
            WakeupChange::Scheduled => ThreadWakeupChange::Scheduled,
            WakeupChange::Replaced => ThreadWakeupChange::Replaced,
            WakeupChange::Fired => ThreadWakeupChange::Fired,
            WakeupChange::Cancelled => ThreadWakeupChange::Cancelled,
            WakeupChange::LoopStarted => ThreadWakeupChange::LoopStarted,
            WakeupChange::LoopEnded => ThreadWakeupChange::LoopEnded,
        },
        cancel_reason: update.cancel_reason.map(|reason| match reason {
            CancelReason::Model => ThreadWakeupCancelReason::Model,
            CancelReason::User => ThreadWakeupCancelReason::User,
            CancelReason::Interrupt => ThreadWakeupCancelReason::Interrupt,
            CancelReason::Limit => ThreadWakeupCancelReason::Limit,
            CancelReason::Failures => ThreadWakeupCancelReason::Failures,
            CancelReason::Replaced => ThreadWakeupCancelReason::Replaced,
            CancelReason::Completed => ThreadWakeupCancelReason::Completed,
            CancelReason::ThreadClosed => ThreadWakeupCancelReason::ThreadClosed,
        }),
        turn_id: update.turn_id.clone(),
        state: wakeup_state(update.snapshot.clone()),
    }
}
