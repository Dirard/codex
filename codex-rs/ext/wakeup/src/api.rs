//! What app-server asks of a thread's wakeups: `thread/wakeup/read`, `thread/wakeup/cancel`,
//! `thread/loop/start` and `thread/wakeup/updated` (`specs/scheduled-wakeups-2026-11-07.md`,
//! UC8).

use std::sync::Arc;

use codex_core::CodexThread;

use crate::state::CancelReason;
use crate::state::LoopStartRequest;
use crate::state::UpdateSink;
use crate::state::WakeupSnapshot;
use crate::state::WakeupThreadState;

/// The wakeup state of a thread runtime, when the extension is installed.
pub fn thread_state(thread: &CodexThread) -> Option<Arc<WakeupThreadState>> {
    thread.thread_extension_data().get::<WakeupThreadState>()
}

/// Whether the thread may use wakeups, and its state (R8.1).
pub fn read(thread: &CodexThread) -> (bool, WakeupSnapshot) {
    match thread_state(thread) {
        Some(state) => (state.available(), state.snapshot()),
        None => (false, WakeupSnapshot::default()),
    }
}

/// Cancels the thread's wakeup and loop for the user (R4.2, R8.2): whether anything was
/// cancelled, and the state after it. A cancel after a wakeup's turn started leaves the turn
/// running (R4.6).
pub async fn cancel(thread: &CodexThread) -> (bool, WakeupSnapshot) {
    let Some(state) = thread_state(thread) else {
        return (false, WakeupSnapshot::default());
    };
    let _fire = state.fire_lock.lock().await;
    let cancelled = state.cancel(CancelReason::User);
    (cancelled.anything(), state.snapshot())
}

/// Starts a `/loop` (R5.1, R5.7, R8.3): iteration 1 runs at once when the thread is idle,
/// otherwise as soon as it becomes idle. The error names the invalid parameter.
pub async fn start_loop(
    thread: &CodexThread,
    request: LoopStartRequest,
) -> Result<WakeupSnapshot, String> {
    let Some(state) = thread_state(thread) else {
        return Err("scheduled wakeups are not available for this thread".to_string());
    };
    let snapshot = {
        let _fire = state.fire_lock.lock().await;
        state.start_loop(request)?
    };
    tokio::spawn(crate::scheduler::fire_if_due(state));
    Ok(snapshot)
}

/// Sends every later update of the thread's wakeups to `sink`, synchronously with the change,
/// until it returns `false` (R8.4). `false` when the extension is not installed.
pub fn subscribe(thread: &CodexThread, sink: UpdateSink) -> bool {
    match thread_state(thread) {
        Some(state) => {
            state.subscribe(sink);
            true
        }
        None => false,
    }
}
