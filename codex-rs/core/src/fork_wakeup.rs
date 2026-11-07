//! Fork: what core knows about a thread's scheduled wakeup
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC1, UC6).
//!
//! The wakeup extension (`codex-rs/ext/wakeup`) owns wakeups and keeps a [`WakeupBeacon`] in
//! the thread store. Core only reads it: residency keeps a MultiAgentV2 child with a pending
//! wakeup loaded (R6.3), the completion mail to its parent says when it resumes (R6.1) and
//! `wait_agent` counts it as active (R6.2). Without the extension there is no beacon and core
//! behaves as before.

use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use codex_extension_api::ExtensionData;

use crate::CodexThread;

/// The thread's pending wakeup as core sees it, kept current by the wakeup extension.
#[derive(Debug, Default)]
pub struct WakeupBeacon {
    state: std::sync::Mutex<Option<PendingWakeupView>>,
}

/// When a pending wakeup is due and, for a `/loop`, which iteration it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingWakeupView {
    pub due_at: DateTime<Utc>,
    /// The loop iteration the wakeup runs and the loop's maximum.
    pub loop_iteration: Option<(u32, u32)>,
}

impl WakeupBeacon {
    pub fn set(&self, view: Option<PendingWakeupView>) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = view;
    }

    pub fn get(&self) -> Option<PendingWakeupView> {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The pending wakeup of the thread whose store is `store`, if the extension published one.
pub(crate) fn pending_wakeup(store: &ExtensionData) -> Option<PendingWakeupView> {
    store.get::<WakeupBeacon>()?.get()
}

/// Whether `thread` is an agent of a workflow run, which never gets wakeup tools (R1.3, R6.5).
/// The thread must be registered: workflow children are known by their agent path.
pub fn is_workflow_child(thread: &CodexThread) -> bool {
    let workflow_children = thread
        .session
        .services
        .local_agent_runtime
        .workflow_children();
    workflow_children.is_workflow_thread(thread.session.thread_id)
        || workflow_children.is_workflow_source(thread.session_source())
}

/// The line a child's completion mail adds while the child has a pending wakeup (R6.1).
pub(crate) fn completion_note(view: &PendingWakeupView) -> String {
    const RESUMES: &str =
        "it resumes by itself and reports again; wait_agent waits for it, close_agent stops it.";
    match view.loop_iteration {
        Some((iteration, max)) => format!(
            "Its /loop runs iteration {iteration} of at most {max} at {}; {RESUMES}",
            clock_time(view.due_at)
        ),
        None => format!(
            "It scheduled a wakeup for {}: {RESUMES}",
            view.due_at
                .to_rfc3339_opts(SecondsFormat::Secs, /*use_z*/ true)
        ),
    }
}

/// The sentence a `wait_agent` timeout adds for descendants with pending wakeups (R6.2).
pub(crate) fn wait_timeout_note(agents: &[(String, PendingWakeupView)]) -> String {
    if agents.is_empty() {
        return String::new();
    }
    let list = agents
        .iter()
        .map(|(path, view)| format!("{path} (next at {})", clock_time(view.due_at)))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        " Agents with scheduled wakeups: {list}. Each reports again after its wakeup; close_agent stops it."
    )
}

/// `12:05:00Z`: a wakeup is at most a day away, so the time of day names it.
fn clock_time(at: DateTime<Utc>) -> String {
    at.format("%H:%M:%SZ").to_string()
}

#[cfg(test)]
#[path = "fork_wakeup_tests.rs"]
mod tests;
