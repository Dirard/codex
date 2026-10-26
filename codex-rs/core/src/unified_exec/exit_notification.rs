//! Fork: exit notifications for background unified exec processes.
//!
//! Realizes UC2 of `specs/background-exec-notifications-2026-10-25.md`: the exit of an
//! armed process becomes at most one bounded, honest report for the thread, and command
//! output inside it cannot pass for words of the user or of Codex (R2.8).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_utils_string::truncate_middle_with_marker;
use codex_utils_string::untrusted::UntrustedBoundary;
use codex_utils_string::untrusted::escape_tags;
use tokio::sync::Notify;
use tokio::time::Instant;

use crate::unified_exec::MAX_UNIFIED_EXEC_PROCESSES;

pub(crate) const NOTIFICATION_COMMAND_MAX_BYTES: usize = 512;
pub(crate) const NOTIFICATION_TAIL_MAX_BYTES: usize = 2048;
const TRUNCATION_MARKER: &str = "…[truncated]…";
pub(crate) const NOTIFICATION_TAIL_MAX_LINES: usize = 40;
/// Label of the boundary lines around unread output.
const OUTPUT_LABEL: &str = "output";

static NEXT_NOTIFIER_KEY: AtomicU64 = AtomicU64::new(1);

/// Why Codex itself stopped a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminationCause {
    /// Pruned to keep at most `MAX_UNIFIED_EXEC_PROCESSES` open terminals.
    ProcessLimit,
    /// A client or the user asked to terminate background terminals.
    ClientRequest,
    /// The session shut down; the shutdown path records a [`ShutdownNotice`] instead.
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
    /// The most recent output the model has not read.
    pub(crate) unread_tail: Vec<u8>,
    /// Unread bytes before the tail that `write_stdin` can still return.
    pub(crate) unread_earlier_bytes: usize,
    /// Unread bytes the output buffer already dropped; nothing can return them.
    pub(crate) unread_lost_bytes: usize,
    pub(crate) sandbox_denied: bool,
}

#[derive(Default)]
struct NotifierState {
    armed: bool,
    settled: bool,
    /// Handed off for delivery but not yet in the thread queue.
    delivering: bool,
    /// Handed off for delivery; a later observation must withdraw it.
    delivered: bool,
    report: Option<ExitReport>,
    cause: Option<TerminationCause>,
}

/// What a shutdown found for one notifier.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ShutdownSettle {
    /// The model still expects this notification: the shutdown must say what happened.
    Notice(ShutdownNotice),
    /// The notification is on its way into the thread queue.
    DeliveryInFlight,
    /// Nothing is owed to the model.
    Nothing,
}

/// One exit notification promise for one background process.
///
/// Delivery happens exactly once, when the promise is armed (the model was told a
/// notification follows) and the exit report exists, in whichever order they occur.
/// Observing the exit in a tool response or a shutdown settles it without delivery.
pub(crate) struct ExitNotifier {
    /// Identifies this notifier's queued notification, unlike a reusable process ID.
    key: u64,
    process_id: i32,
    command: String,
    started_at: Instant,
    state: Mutex<NotifierState>,
    delivery_done: Notify,
}

impl std::fmt::Debug for ExitNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExitNotifier")
            .field("key", &self.key)
            .field("process_id", &self.process_id)
            .finish_non_exhaustive()
    }
}

impl ExitNotifier {
    pub(crate) fn new(process_id: i32, command: String, started_at: Instant) -> Arc<Self> {
        Arc::new(Self {
            key: NEXT_NOTIFIER_KEY.fetch_add(1, Ordering::Relaxed),
            process_id,
            command,
            started_at,
            state: Mutex::new(NotifierState::default()),
            delivery_done: Notify::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, NotifierState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn process_id(&self) -> i32 {
        self.process_id
    }

    pub(crate) fn started_at(&self) -> Instant {
        self.started_at
    }

    /// Arm the notification once `exec_command` has returned a session ID.
    pub(crate) fn arm(&self) -> Option<PendingExitNotification> {
        let mut state = self.lock();
        if state.armed || state.settled {
            return None;
        }
        state.armed = true;
        let report = state.report.take()?;
        Some(self.hand_off(&mut state, report))
    }

    /// The model saw the exit in a tool response; no notification is needed.
    ///
    /// Returns the key of a notification that was already handed off, so the caller can
    /// withdraw it from the thread queue (R2.10).
    pub(crate) fn observe(&self) -> Option<u64> {
        let mut state = self.lock();
        state.settled = true;
        state.delivered.then_some(self.key)
    }

    /// Record that Codex is stopping the process. The first cause wins.
    pub(crate) fn set_termination_cause(&self, cause: TerminationCause) {
        let mut state = self.lock();
        if state.cause.is_none() {
            state.cause = Some(cause);
        }
    }

    /// Forget `cause` after the stop it described failed (R2.11).
    pub(crate) fn clear_termination_cause(&self, cause: TerminationCause) {
        let mut state = self.lock();
        if state.cause == Some(cause) {
            state.cause = None;
        }
    }

    /// Settle the promise because the session shuts down (R2.5).
    pub(crate) fn settle_for_shutdown(&self) -> ShutdownSettle {
        let mut state = self.lock();
        if state.cause.is_none() {
            state.cause = Some(TerminationCause::Shutdown);
        }
        if state.delivering {
            return ShutdownSettle::DeliveryInFlight;
        }
        if !state.armed || state.settled {
            state.settled = true;
            return ShutdownSettle::Nothing;
        }
        state.settled = true;
        ShutdownSettle::Notice(ShutdownNotice {
            process_id: self.process_id,
            command: self.command.clone(),
            duration: Instant::now().saturating_duration_since(self.started_at),
        })
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
        Some(self.hand_off(&mut state, report))
    }

    fn hand_off(&self, state: &mut NotifierState, report: ExitReport) -> PendingExitNotification {
        state.settled = true;
        state.delivering = true;
        state.delivered = true;
        PendingExitNotification {
            key: self.key,
            process_id: self.process_id,
            command: self.command.clone(),
            report,
        }
    }

    /// The handed-off notification is in the thread queue (or was dropped by it).
    pub(crate) fn finish_delivery(&self) {
        self.lock().delivering = false;
        self.delivery_done.notify_waiters();
    }

    /// Wait until a handed-off notification reached the thread queue, or `deadline`.
    pub(crate) async fn wait_for_delivery(&self, deadline: Instant) {
        loop {
            let notified = self.delivery_done.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !self.lock().delivering {
                return;
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return;
            }
        }
    }

    /// Whether the thread still expects a notification from this process, including one
    /// that is on its way into the thread queue (P3-3).
    pub(crate) fn is_pending(&self) -> bool {
        let state = self.lock();
        state.armed && (!state.settled || state.delivering)
    }
}

/// A notification that is ready to enter the thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingExitNotification {
    pub(crate) key: u64,
    pub(crate) process_id: i32,
    pub(crate) command: String,
    pub(crate) report: ExitReport,
}

/// The model-visible body of a notification and what it left unread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenderedNotification {
    pub(crate) text: String,
    /// `write_stdin` would still return unread output that the text did not show.
    pub(crate) leaves_readable_output: bool,
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
    pub(crate) fn render(&self, with_output: bool) -> RenderedNotification {
        self.render_with_boundary(with_output, &UntrustedBoundary::random())
    }

    fn render_with_boundary(
        &self,
        with_output: bool,
        boundary: &UntrustedBoundary,
    ) -> RenderedNotification {
        let id = self.process_id;
        let report = &self.report;
        let duration = format_duration(report.duration);
        let mut lines = vec![match &report.outcome {
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
                escape_tags(&truncate_middle_with_marker(
                    message.trim(),
                    NOTIFICATION_COMMAND_MAX_BYTES,
                    TRUNCATION_MARKER
                ))
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
        lines.push(command_line(&self.command));
        if report.sandbox_denied {
            lines.push("The Codex sandbox likely blocked this command.".to_string());
        }
        // Only a process that exited on its own keeps its unread output pollable.
        let readable = matches!(report.outcome, ExitOutcome::Exited { .. });
        let lost = report.unread_lost_bytes;
        if !with_output {
            let readable_bytes = report
                .unread_tail
                .len()
                .saturating_add(report.unread_earlier_bytes);
            let leaves_readable_output = readable && readable_bytes > 0;
            lines.push(if leaves_readable_output {
                format!(
                    "Output omitted because many background terminal notifications are pending; call write_stdin with session_id {id} to read it."
                )
            } else if readable_bytes.saturating_add(lost) > 0 {
                "Output omitted because many background terminal notifications are pending."
                    .to_string()
            } else {
                "No unread output.".to_string()
            });
            return RenderedNotification {
                text: lines.join("\n"),
                leaves_readable_output,
            };
        }
        let (tail, earlier) = bounded_tail(
            &report.unread_tail,
            report.unread_earlier_bytes,
            NOTIFICATION_TAIL_MAX_LINES,
        );
        if tail.is_empty() && earlier == 0 && lost == 0 {
            lines.push("No unread output.".to_string());
            return RenderedNotification {
                text: lines.join("\n"),
                leaves_readable_output: false,
            };
        }
        if readable {
            match (earlier > 0, lost > 0) {
                (true, false) => lines.push(format!(
                    "Earlier unread output omitted ({earlier} bytes); call write_stdin with session_id {id} to read it."
                )),
                (true, true) => lines.push(format!(
                    "Earlier unread output omitted ({earlier} bytes; call write_stdin with session_id {id} to read them). Another {lost} bytes exceeded the output buffer and are lost."
                )),
                (false, true) => lines.push(format!(
                    "Earlier output omitted: {lost} bytes exceeded the output buffer and are lost."
                )),
                (false, false) => {}
            }
        } else if earlier.saturating_add(lost) > 0 {
            lines.push(format!(
                "Earlier unread output omitted ({} bytes).",
                earlier.saturating_add(lost)
            ));
        }
        if !tail.is_empty() {
            lines.push(boundary.block(
                OUTPUT_LABEL,
                &format!("Unread output (last {} lines)", tail.lines().count()),
                "untrusted program output",
                &tail,
            ));
        }
        RenderedNotification {
            text: lines.join("\n"),
            leaves_readable_output: readable && earlier > 0,
        }
    }
}

/// What the model learns about an armed process that a shutdown stopped (R2.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShutdownNotice {
    pub(crate) process_id: i32,
    pub(crate) command: String,
    pub(crate) duration: Duration,
}

impl ShutdownNotice {
    pub(crate) fn render(&self) -> String {
        format!(
            "Background terminal session {} was stopped when the Codex session ended, after {}; its exit status and unread output are unknown.\n{}",
            self.process_id,
            format_duration(self.duration),
            command_line(&self.command)
        )
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.1}s", duration.as_secs_f64())
}

fn command_line(command: &str) -> String {
    format!(
        "Command: {}",
        escape_tags(&truncate_middle_with_marker(
            command,
            NOTIFICATION_COMMAND_MAX_BYTES,
            TRUNCATION_MARKER
        ))
    )
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
