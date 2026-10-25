//! Fork: exit notifications for background unified exec processes.
//!
//! Realizes UC2 of `specs/background-exec-notifications-2026-10-25.md`: the exit of an
//! armed process becomes at most one bounded, honest report for the thread.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::time::Duration;

use codex_utils_string::truncate_middle_with_marker;

use crate::unified_exec::MAX_UNIFIED_EXEC_PROCESSES;

pub(crate) const NOTIFICATION_COMMAND_MAX_BYTES: usize = 512;
pub(crate) const NOTIFICATION_TAIL_MAX_BYTES: usize = 2048;
const TRUNCATION_MARKER: &str = "…[truncated]…";
pub(crate) const NOTIFICATION_TAIL_MAX_LINES: usize = 40;

/// Why Codex itself stopped a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminationCause {
    /// Pruned to keep at most `MAX_UNIFIED_EXEC_PROCESSES` open terminals.
    ProcessLimit,
    /// A client or the user asked to terminate background terminals.
    ClientRequest,
    /// The session shut down; nothing is reported.
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ExitOutcome {
    Exited { exit_code: Option<i32> },
    Failed { message: String },
    Terminated(TerminationCause),
}

/// What the exit watcher observed when the process stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExitReport {
    pub(crate) outcome: ExitOutcome,
    pub(crate) duration: Duration,
    /// The most recent output the model has not read, and how many unread bytes precede it.
    pub(crate) unread_tail: Vec<u8>,
    pub(crate) unread_earlier_bytes: usize,
    pub(crate) sandbox_denied: bool,
}

#[derive(Default)]
struct NotifierState {
    armed: bool,
    settled: bool,
    report: Option<ExitReport>,
    cause: Option<TerminationCause>,
}

/// One exit notification promise for one background process.
///
/// Delivery happens exactly once, when the promise is armed (the model was told a
/// notification follows) and the exit report exists, in whichever order they occur.
/// Observing the exit in a tool response or a shutdown settles it without delivery.
pub(crate) struct ExitNotifier {
    process_id: i32,
    command: String,
    state: Mutex<NotifierState>,
}

impl std::fmt::Debug for ExitNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExitNotifier")
            .field("process_id", &self.process_id)
            .finish_non_exhaustive()
    }
}

impl ExitNotifier {
    pub(crate) fn new(process_id: i32, command: String) -> Arc<Self> {
        Arc::new(Self {
            process_id,
            command,
            state: Mutex::new(NotifierState::default()),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, NotifierState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Arm the notification once `exec_command` has returned a session ID.
    pub(crate) fn arm(&self) -> Option<PendingExitNotification> {
        let mut state = self.lock();
        if state.armed || state.settled {
            return None;
        }
        state.armed = true;
        let report = state.report.take()?;
        state.settled = true;
        Some(self.pending(report))
    }

    /// The model saw the exit in a tool response; no notification is needed.
    pub(crate) fn observe(&self) {
        self.lock().settled = true;
    }

    /// Record that Codex is stopping the process. The first cause wins.
    pub(crate) fn set_termination_cause(&self, cause: TerminationCause) {
        let mut state = self.lock();
        if state.cause.is_none() {
            state.cause = Some(cause);
        }
    }

    /// Collect the exit report from the exit watcher.
    pub(crate) fn on_exit(&self, mut report: ExitReport) -> Option<PendingExitNotification> {
        let mut state = self.lock();
        if state.settled || state.report.is_some() {
            return None;
        }
        match (state.cause, &report.outcome) {
            (Some(TerminationCause::Shutdown), _) => {
                state.settled = true;
                return None;
            }
            // A failure explains the stop better than the termination that followed it.
            (Some(_), ExitOutcome::Failed { .. }) | (None, _) => {}
            (Some(cause), _) => report.outcome = ExitOutcome::Terminated(cause),
        }
        if !state.armed {
            state.report = Some(report);
            return None;
        }
        state.settled = true;
        Some(self.pending(report))
    }

    /// Whether the thread still expects a notification from this process.
    pub(crate) fn is_pending(&self) -> bool {
        let state = self.lock();
        state.armed && !state.settled
    }

    fn pending(&self, report: ExitReport) -> PendingExitNotification {
        PendingExitNotification {
            process_id: self.process_id,
            command: self.command.clone(),
            report,
        }
    }
}

/// A notification that is ready to enter the thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingExitNotification {
    pub(crate) process_id: i32,
    pub(crate) command: String,
    pub(crate) report: ExitReport,
}

impl PendingExitNotification {
    /// Whether this notification should start a turn for an idle thread.
    pub(crate) fn wakes(&self) -> bool {
        !matches!(
            self.report.outcome,
            ExitOutcome::Terminated(TerminationCause::ClientRequest | TerminationCause::Shutdown)
        )
    }

    /// Render the notification body. `with_output` is false once many are pending.
    pub(crate) fn render(&self, with_output: bool) -> String {
        let id = self.process_id;
        let duration = format!("{:.1}s", self.report.duration.as_secs_f64());
        let mut lines = vec![match &self.report.outcome {
            ExitOutcome::Exited {
                exit_code: Some(code),
            } => format!(
                "Background terminal session {id} exited with code {code}{} after {duration}.",
                signal_suffix(*code)
            ),
            ExitOutcome::Exited { exit_code: None } => format!(
                "Background terminal session {id} exited after {duration} (exit code unavailable)."
            ),
            ExitOutcome::Failed { message } => format!(
                "Background terminal session {id} failed after {duration}: {}",
                truncate_middle_with_marker(
                    message.trim(),
                    NOTIFICATION_COMMAND_MAX_BYTES,
                    TRUNCATION_MARKER
                )
            ),
            ExitOutcome::Terminated(TerminationCause::ProcessLimit) => format!(
                "Background terminal session {id} was terminated by Codex after {duration} because the limit of {MAX_UNIFIED_EXEC_PROCESSES} open terminals was reached."
            ),
            ExitOutcome::Terminated(
                TerminationCause::ClientRequest | TerminationCause::Shutdown,
            ) => format!(
                "Background terminal session {id} was terminated at the user's request after {duration}."
            ),
        }];
        lines.push(format!(
            "Command: {}",
            truncate_middle_with_marker(
                &self.command,
                NOTIFICATION_COMMAND_MAX_BYTES,
                TRUNCATION_MARKER
            )
        ));
        if self.report.sandbox_denied {
            lines.push("The Codex sandbox likely blocked this command.".to_string());
        }
        // Only a process that exited on its own keeps its unread output pollable.
        let readable = matches!(self.report.outcome, ExitOutcome::Exited { .. });
        let read_hint = |what: &str| {
            if readable {
                format!("{what}; call write_stdin with session_id {id} to read it.")
            } else {
                format!("{what}.")
            }
        };
        if !with_output {
            lines.push(read_hint(
                "Output omitted because many background terminal notifications are pending",
            ));
            return lines.join("\n");
        }
        let (tail, earlier_bytes) = bounded_tail(
            &self.report.unread_tail,
            self.report.unread_earlier_bytes,
            NOTIFICATION_TAIL_MAX_LINES,
        );
        if tail.is_empty() && earlier_bytes == 0 {
            lines.push("No unread output.".to_string());
            return lines.join("\n");
        }
        if earlier_bytes > 0 {
            lines.push(read_hint(&format!(
                "Earlier unread output omitted ({earlier_bytes} bytes)"
            )));
        }
        lines.push(format!(
            "Unread output (last {} lines):",
            tail.lines().count()
        ));
        lines.push(tail);
        lines.join("\n")
    }
}

fn signal_suffix(exit_code: i32) -> String {
    if cfg!(unix) && (129..=192).contains(&exit_code) {
        format!(" (signal {})", exit_code - 128)
    } else {
        String::new()
    }
}

/// Keep the last `max_lines` lines of `bytes`, starting on a UTF-8 boundary.
/// Returns the text and the number of unread bytes that precede it.
fn bounded_tail(bytes: &[u8], earlier_bytes: usize, max_lines: usize) -> (String, usize) {
    // Skip continuation bytes cut off from a scalar that began before the tail.
    let start = bytes
        .iter()
        .take(char::MAX.len_utf8() - 1)
        .take_while(|byte| (*byte & 0b1100_0000) == 0b1000_0000)
        .count();
    let mut bytes = &bytes[start..];
    let mut earlier_bytes = earlier_bytes.saturating_add(start);
    let trimmed_end = bytes
        .iter()
        .rposition(|byte| *byte != b'\n')
        .map_or(0, |index| index + 1);
    let content = &bytes[..trimmed_end];
    let mut newlines = 0usize;
    if let Some(index) = content.iter().rposition(|byte| {
        if *byte == b'\n' {
            newlines += 1;
        }
        newlines == max_lines
    }) {
        earlier_bytes = earlier_bytes.saturating_add(index + 1);
        bytes = &content[index + 1..];
    } else {
        bytes = content;
    }
    (String::from_utf8_lossy(bytes).into_owned(), earlier_bytes)
}

#[cfg(test)]
#[path = "exit_notification_tests.rs"]
mod tests;
