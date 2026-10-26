//! Fork: `thread/workflow/updated` for clients subscribed to a thread
//! (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.2).
//!
//! Each thread listener runs a forwarder that follows the thread's workflow changes and
//! hands snapshots to the listener, so they are ordered with the thread's other
//! notifications. A run is sent at most once per [`MIN_UPDATE_INTERVAL`]; its terminal
//! snapshot is sent at once, exactly one time.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use codex_app_server_protocol::WorkflowRun;
use codex_core::CodexThread;
use codex_workflow::RunSnapshot;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tokio_util::task::AbortOnDropHandle;

use crate::thread_state::ThreadListenerCommand;

/// Shortest time between two updates of one active run.
pub(crate) const MIN_UPDATE_INTERVAL: Duration = Duration::from_millis(250);

/// Starts the forwarder of `thread`; it stops when the handle is dropped, the listener's
/// command channel closes or the thread goes away.
pub(crate) fn spawn_workflow_update_forwarder(
    thread: &Arc<CodexThread>,
    commands: mpsc::UnboundedSender<ThreadListenerCommand>,
) -> AbortOnDropHandle<()> {
    // Subscribe before reading snapshots so no change is missed.
    let mut updates = thread.subscribe_workflow_updates();
    let thread = Arc::downgrade(thread);
    AbortOnDropHandle::new(tokio::spawn(async move {
        let mut pacer = UpdatePacer::default();
        loop {
            let deadline = pacer.next_deadline();
            let changed = tokio::select! {
                received = updates.recv() => match received {
                    Ok(run_id) => Changed::Run(run_id),
                    // Missed changes: every run may be stale.
                    Err(RecvError::Lagged(_)) => Changed::All,
                    Err(RecvError::Closed) => return,
                },
                () = tokio::time::sleep_until(deadline.unwrap_or_else(Instant::now)),
                    if deadline.is_some() => Changed::Runs(pacer.take_due_runs(Instant::now())),
            };
            let Some(thread) = thread.upgrade() else {
                return;
            };
            let snapshots = thread.workflow_runs();
            drop(thread);
            let now = Instant::now();
            for snapshot in snapshots {
                if !changed.includes(&snapshot.run_id) {
                    continue;
                }
                let Some(snapshot) = pacer.offer(snapshot, now) else {
                    continue;
                };
                let run = WorkflowRun::from_snapshot(snapshot, /*include_agents*/ false);
                let command =
                    ThreadListenerCommand::EmitThreadWorkflowUpdated { run: Box::new(run) };
                if commands.send(command).is_err() {
                    return;
                }
            }
        }
    }))
}

/// Runs whose snapshots may have changed.
enum Changed {
    Run(String),
    Runs(Vec<String>),
    All,
}

impl Changed {
    fn includes(&self, run_id: &str) -> bool {
        match self {
            Self::Run(id) => id == run_id,
            Self::Runs(ids) => ids.iter().any(|id| id == run_id),
            Self::All => true,
        }
    }
}

#[derive(Debug, Default)]
struct RunPacing {
    last_sent_at: Option<Instant>,
    last_sent_revision: u64,
    /// A newer snapshot waits for the interval to pass.
    pending: bool,
    /// The terminal snapshot was sent.
    finished: bool,
}

/// Decides when each run's snapshot is sent.
#[derive(Debug, Default)]
struct UpdatePacer {
    runs: HashMap<String, RunPacing>,
}

impl UpdatePacer {
    /// The snapshot to send now for a run whose latest snapshot is `snapshot`, if any.
    fn offer(&mut self, snapshot: RunSnapshot, now: Instant) -> Option<RunSnapshot> {
        let pacing = self.runs.entry(snapshot.run_id.clone()).or_default();
        if pacing.finished || snapshot.revision <= pacing.last_sent_revision {
            pacing.pending = false;
            return None;
        }
        let terminal = snapshot.status.is_terminal();
        let due = terminal
            || pacing
                .last_sent_at
                .is_none_or(|sent| now.duration_since(sent) >= MIN_UPDATE_INTERVAL);
        if !due {
            pacing.pending = true;
            return None;
        }
        *pacing = RunPacing {
            last_sent_at: Some(now),
            last_sent_revision: snapshot.revision,
            pending: false,
            finished: terminal,
        };
        Some(snapshot)
    }

    /// When the earliest pending run may be sent.
    fn next_deadline(&self) -> Option<Instant> {
        self.runs
            .values()
            .filter(|pacing| pacing.pending)
            .filter_map(|pacing| pacing.last_sent_at)
            .map(|sent| sent + MIN_UPDATE_INTERVAL)
            .min()
    }

    /// Pending runs that may be sent at `now`; they stop pending until offered again, so
    /// a run whose snapshot is gone never keeps the deadline in the past.
    fn take_due_runs(&mut self, now: Instant) -> Vec<String> {
        self.runs
            .iter_mut()
            .filter(|(_, pacing)| {
                pacing.pending
                    && pacing
                        .last_sent_at
                        .is_none_or(|sent| now.duration_since(sent) >= MIN_UPDATE_INTERVAL)
            })
            .map(|(run_id, pacing)| {
                pacing.pending = false;
                run_id.clone()
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "workflow_updates_tests.rs"]
mod tests;
