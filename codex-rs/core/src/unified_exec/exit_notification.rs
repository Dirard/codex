//! Fork: exit notifications for background unified exec processes.
//!
//! Realizes UC2 of `specs/background-exec-notifications-2026-10-25.md`: the exit of an
//! armed process becomes at most one bounded, honest report for the thread, and command
//! output inside it cannot pass for words of the user or of Codex (R2.8). Output watches
//! (UC6, `notify_on_output`) live in the child module `output_watch`.

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

use crate::session::session::Session;
use crate::unified_exec::MAX_UNIFIED_EXEC_PROCESSES;

pub(crate) const NOTIFICATION_COMMAND_MAX_BYTES: usize = 512;
pub(crate) const NOTIFICATION_TAIL_MAX_BYTES: usize = 2048;
const TRUNCATION_MARKER: &str = "…[truncated]…";
pub(crate) const NOTIFICATION_TAIL_MAX_LINES: usize = 40;
/// Label of the boundary lines around unread output.
const OUTPUT_LABEL: &str = "output";
/// An exit notification after output watch notifications (review 2026-10-27, P3-2).
const WATCH_REPORTED_LINE: &str = "Lines that matched its output watch were already reported; the unread output below may repeat them.";

static NEXT_NOTIFIER_KEY: AtomicU64 = AtomicU64::new(1);

#[path = "output_watch.rs"]
mod output_watch;
pub(crate) use output_watch::*;
#[path = "terminal_journal.rs"]
mod terminal_journal;

/// A key for a queued notification (an exit notifier or an output watch); unlike a process
/// ID it is never reused.
fn next_notification_key() -> u64 {
    NEXT_NOTIFIER_KEY.fetch_add(1, Ordering::Relaxed)
}

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
    /// The output watch of the process, if one is on (UC6).
    output_watch: Option<Arc<OutputWatch>>,
    /// The output watch whose final batch went to the queue right before the exit
    /// notification; a poll that shows the exit withdraws it too (R6.9, review P3-1).
    final_watch_key: Option<u64>,
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
    /// The starting call asked for a report; otherwise only a later subscription arms it.
    requested: bool,
    /// The starting call set `notify_on_exit`: the model waits for the exit itself, not only
    /// for matching output (review 2026-10-27, P2-3).
    exit_requested: bool,
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
    /// A notifier whose starting call asked for the exit report.
    #[cfg(test)]
    pub(crate) fn new(process_id: i32, command: String, started_at: Instant) -> Arc<Self> {
        Self::with_request(
            process_id, command, started_at, /*requested*/ true, /*exit_requested*/ true,
        )
    }

    fn with_request(
        process_id: i32,
        command: String,
        started_at: Instant,
        requested: bool,
        exit_requested: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            key: next_notification_key(),
            process_id,
            command,
            started_at,
            requested,
            exit_requested,
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

    pub(crate) fn command(&self) -> &str {
        &self.command
    }

    pub(crate) fn is_armed(&self) -> bool {
        self.lock().armed
    }

    /// Whether the exit watcher must build the exit report: the notification is armed, or
    /// the starting call asked for it and may arm it once it returns (R6.3).
    pub(crate) fn wants_report(&self) -> bool {
        self.requested || self.lock().armed
    }

    /// The exit report waits for `arm`: the process exited before its `exec_command` call
    /// returned the session ID.
    pub(crate) fn awaits_arming(&self) -> bool {
        let state = self.lock();
        !state.armed && !state.settled && state.report.is_some()
    }

    /// The output watch of the process while it is on (UC6).
    pub(crate) fn output_watch(&self) -> Option<Arc<OutputWatch>> {
        self.lock()
            .output_watch
            .as_ref()
            .filter(|watch| !watch.is_stopped())
            .cloned()
    }

    /// Replace the output watch of the process and stop the previous one.
    pub(crate) fn set_output_watch(&self, watch: Option<Arc<OutputWatch>>) {
        let previous = std::mem::replace(&mut self.lock().output_watch, watch);
        if let Some(previous) = previous {
            previous.stop();
        }
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
    /// Returns the keys of what was already handed off, so the caller can withdraw it from the
    /// thread queue: the exit notification (R2.10) and the final batch of the output watch
    /// that went right before it (R6.9).
    pub(crate) fn observe(&self) -> Vec<u64> {
        let mut state = self.lock();
        state.settled = true;
        if !state.delivered {
            return Vec::new();
        }
        std::iter::once(self.key)
            .chain(state.final_watch_key)
            .collect()
    }

    /// The final batch of `watch_key` was queued right before the exit notification.
    fn note_final_watch_batch(&self, watch_key: u64) {
        self.lock().final_watch_key = Some(watch_key);
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
        if let Some(watch) = state.output_watch.take() {
            watch.stop();
        }
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
            watch_reported: false,
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
    /// that is on its way into the thread queue (P3-3); such a process is in the terminal
    /// journal (D6).
    pub(crate) fn is_pending(&self) -> bool {
        let state = self.lock();
        state.armed && (!state.settled || state.delivering)
    }

    /// Whether the thread waits for this process (`codex exec`, `wait_agent`, residency): a
    /// notification on its way into the queue, an armed `notify_on_exit`, or an output watch
    /// that saw a matching line, or started, less than its quiet period ago. A
    /// quiet watch still reports its lines and the exit while the session runs, but nothing
    /// waits for it any longer (review 2026-10-27, P2-3).
    pub(crate) fn is_awaited(&self, now: Instant) -> bool {
        let state = self.lock();
        if !state.armed {
            return false;
        }
        if state.delivering {
            return true;
        }
        !state.settled
            && (self.exit_requested
                || state
                    .output_watch
                    .as_ref()
                    .is_some_and(|watch| watch.is_fresh(now)))
    }
}

/// A notification that is ready to enter the thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingExitNotification {
    pub(crate) key: u64,
    pub(crate) process_id: i32,
    pub(crate) command: String,
    pub(crate) report: ExitReport,
    /// The output watch of the process reported matching lines, which the unread output may
    /// repeat (review 2026-10-27, P3-2).
    pub(crate) watch_reported: bool,
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
        if self.watch_reported {
            lines.push(WATCH_REPORTED_LINE.to_string());
        }
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

// Hooks called from the upstream unified exec files (`mod.rs`, `process.rs`,
// `async_watcher.rs`, `process_manager.rs`). They live here so those files keep only the
// calls; see `specs/fork-rebase-sustainability-2026-10-26.md` UC2.

/// What a shutdown owes the model about armed processes (R2.5).
#[derive(Debug, Default)]
pub(crate) struct TerminatedProcesses {
    /// Armed processes the shutdown stopped before their exit was reported.
    pub(crate) shutdown_notices: Vec<ShutdownNotice>,
    /// Notifications on their way into the thread queue when the shutdown began.
    pub(crate) deliveries_in_flight: Vec<Arc<ExitNotifier>>,
}

impl TerminatedProcesses {
    /// Notices in session ID order, as `terminate_all_processes` returns them.
    pub(crate) fn sorted(mut self) -> Self {
        self.shutdown_notices
            .sort_by_key(|notice| notice.process_id);
        self
    }
}

/// A live terminal whose exit will still be reported (R4.5, R7.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingTerminal {
    pub(crate) process_id: i32,
    pub(crate) command: String,
    pub(crate) started_at: tokio::time::Instant,
    pub(crate) last_output_at: Option<tokio::time::Instant>,
    /// The pattern of the output watch, if one is on.
    pub(crate) output_watch: Option<String>,
}

/// What `exec_command` asks of the notifications of its process (R1.2, R6.3).
#[derive(Clone, Debug, Default)]
pub(crate) struct NotifyRequest {
    /// Exit notifications are enabled for this interactive call, so the process gets a
    /// notifier that `write_stdin` can arm later.
    pub(crate) enabled: bool,
    /// `notify_on_exit`.
    pub(crate) exit: bool,
    /// `notify_on_output`, validated.
    pub(crate) output: Option<Arc<OutputPattern>>,
}

impl NotifyRequest {
    /// Validates the notification arguments of an `exec_command` call before it runs (R6.2);
    /// without the feature or for a one-shot call they are ignored (R1.4, R6.1).
    pub(crate) fn for_exec_command(
        enabled: bool,
        notify_on_exit: bool,
        notify_on_output: Option<&str>,
    ) -> Result<Self, String> {
        if !enabled {
            return Ok(Self::default());
        }
        let output = match notify_on_output {
            None | Some("") => None,
            Some(source) => Some(OutputPattern::parse(source)?),
        };
        Ok(Self {
            enabled,
            exit: notify_on_exit,
            output,
        })
    }

    fn requested(&self) -> bool {
        self.exit || self.output.is_some()
    }
}

impl ExitNotifier {
    /// The promise of an `exec_command` call: it exists only while the process is stored,
    /// and it is armed once the call returns a session ID. With the feature enabled every
    /// stored process gets one, so `write_stdin` can subscribe to it later (R6.3).
    pub(crate) fn for_request(
        process_stored: bool,
        request: &super::ExecCommandRequest,
        started_at: Instant,
    ) -> Option<Arc<Self>> {
        (process_stored && request.notify.enabled).then(|| {
            Self::with_request(
                request.process_id,
                request.hook_command.clone(),
                started_at,
                request.notify.requested(),
                request.notify.exit,
            )
        })
    }
}

impl super::ProcessEntry {
    /// Tell a pending exit notification that Codex is stopping this process.
    ///
    /// A process that already exited on its own keeps its real exit status. A shutdown
    /// settles notifiers through `ExitNotifier::settle_for_shutdown` instead.
    pub(super) fn set_termination_cause(&self, cause: TerminationCause) {
        if let Some(exit_notifier) = &self.exit_notifier
            && !self.process.has_exited()
        {
            exit_notifier.set_termination_cause(cause);
        }
    }

    /// Before `terminate_all_processes` stops this process: a shutdown records what the model
    /// is still owed, any other cause tells the exit notification who stopped the process.
    pub(super) fn settle_termination(
        &self,
        cause: TerminationCause,
        terminated: &mut TerminatedProcesses,
    ) {
        match (cause, self.exit_notifier.as_ref()) {
            (TerminationCause::Shutdown, Some(exit_notifier)) => {
                match exit_notifier.settle_for_shutdown() {
                    ShutdownSettle::Notice(notice) => terminated.shutdown_notices.push(notice),
                    ShutdownSettle::DeliveryInFlight => terminated
                        .deliveries_in_flight
                        .push(Arc::clone(exit_notifier)),
                    ShutdownSettle::Nothing => {}
                }
            }
            _ => self.set_termination_cause(cause),
        }
    }

    /// A client asked to terminate this process. Returns the notifier, so a stop that fails
    /// can forget the cause again with [`clear_client_termination`].
    pub(super) fn mark_client_termination(&self) -> Option<Arc<ExitNotifier>> {
        self.set_termination_cause(TerminationCause::ClientRequest);
        self.exit_notifier.clone()
    }
}

/// The process keeps running, so its later exit keeps its real outcome (R2.11).
pub(super) fn clear_client_termination(exit_notifier: Option<Arc<ExitNotifier>>) {
    if let Some(exit_notifier) = exit_notifier {
        exit_notifier.clear_termination_cause(TerminationCause::ClientRequest);
    }
}

impl<const MAX_BYTES: usize> super::process::OutputBuffers<MAX_BYTES> {
    /// Remember when the process last produced output, for `wait_agent` snapshots (R4.5),
    /// and match new lines against the output watch (R6.5).
    pub(super) fn note_output(&mut self, chunk: &[u8]) {
        self.fork.note_output(chunk);
    }
}

/// The most recent output and what precedes it, see [`last_bytes`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LastBytes {
    pub(crate) bytes: Vec<u8>,
    pub(crate) readable_earlier: usize,
    pub(crate) lost: usize,
}

/// Returns up to `max_bytes` of the most recent contiguous pending output without draining
/// it, with the retained bytes before it (a later drain still returns them) and the bytes
/// the cap already dropped (nothing returns them).
///
/// Only the public API of the head/tail buffer is used. While nothing was omitted, head and
/// tail are contiguous. After an omission only the tail is contiguous with the end of the
/// output: an omission happens only once the head holds its full half of the budget, so the
/// retained bytes past `MAX_BYTES / 2` are the tail.
pub(crate) fn last_bytes<const MAX_BYTES: usize>(
    output: &super::process::OutputBuffers<MAX_BYTES>,
    max_bytes: usize,
) -> LastBytes {
    let retained = output.pending.to_bytes();
    let lost = output.pending.omitted_bytes();
    let contiguous_start = if lost == 0 {
        0
    } else {
        (MAX_BYTES / 2).min(retained.len())
    };
    let start = retained
        .len()
        .saturating_sub(max_bytes)
        .max(contiguous_start);
    LastBytes {
        bytes: retained[start..].to_vec(),
        readable_earlier: start,
        lost,
    }
}

type SharedOutputBuffers = Arc<tokio::sync::Mutex<super::process::OutputBuffers>>;

/// What the exit watcher needs to report the exit of an armed process. It is captured
/// before the session and the output buffer move into the end event.
pub(crate) struct ExitWatch(Option<(Arc<ExitNotifier>, Arc<Session>, SharedOutputBuffers)>);

impl ExitWatch {
    pub(crate) fn new(
        exit_notifier: Option<Arc<ExitNotifier>>,
        session: &Arc<Session>,
        output_buffer: &SharedOutputBuffers,
    ) -> Self {
        Self(
            exit_notifier
                .map(|notifier| (notifier, Arc::clone(session), Arc::clone(output_buffer))),
        )
    }

    /// Settle the notification of the stopped process. The exit watcher calls this under the
    /// interaction lock, so a concurrent `write_stdin` either observed this exit already or
    /// runs after the notification is queued and can withdraw it (R2.10). Only the wake runs
    /// as a separate task.
    ///
    /// The output watch of the process ends here: its undelivered events go to the thread
    /// right before the exit notification, or are dropped when no exit notification
    /// follows (R6.9).
    pub(crate) async fn report(self, process: &super::UnifiedExecProcess, duration: Duration) {
        let Some((exit_notifier, session, output_buffer)) = self.0 else {
            return;
        };
        if exit_notifier.wants_report() {
            let report = exit_report(process, &output_buffer, duration).await;
            if let Some(pending) = exit_notifier.on_exit(report) {
                let watch = output_buffer.lock().await.fork.watch.take();
                deliver_exit(&session, &exit_notifier, pending, watch.as_deref()).await;
                return;
            }
            if exit_notifier.awaits_arming() {
                // `exec_command` arms it right after; that delivers the watch's final batch.
                return;
            }
        }
        if let Some(watch) = output_buffer.lock().await.fork.watch.take() {
            watch.stop();
        }
    }
}

/// The final batch of the output watch, then the exit notification (R6.9).
async fn deliver_exit(
    session: &Arc<Session>,
    exit_notifier: &Arc<ExitNotifier>,
    mut pending: PendingExitNotification,
    watch: Option<&OutputWatch>,
) {
    if let Some(watch) = watch {
        // Every line the process printed is in by now; match the rest, the unterminated
        // line too, off the thread queue lock (P2-1).
        watch.match_waiting_lines(/*final_batch*/ true).await;
        if session
            .deliver_output_watch_notification(watch, /*exited*/ true)
            .await
        {
            exit_notifier.note_final_watch_batch(watch.key());
        }
        pending.watch_reported = watch.has_reported();
        watch.stop();
    }
    session
        .deliver_background_terminal_notification(exit_notifier, pending)
        .await;
}

/// Describe the stopped process for its exit notification without draining output.
async fn exit_report(
    process: &super::UnifiedExecProcess,
    output_buffer: &tokio::sync::Mutex<super::process::OutputBuffers>,
    duration: Duration,
) -> ExitReport {
    let LastBytes {
        bytes: unread_tail,
        readable_earlier: unread_earlier_bytes,
        lost: unread_lost_bytes,
    } = last_bytes(&*output_buffer.lock().await, NOTIFICATION_TAIL_MAX_BYTES);
    let failure_message = process.failure_message();
    let sandbox_denied = failure_message.is_none()
        && !process.termination_requested.load(Ordering::Acquire)
        && process
            .check_for_sandbox_denial_with_text(&String::from_utf8_lossy(&unread_tail))
            .await
            .is_err();
    let outcome = match failure_message {
        Some(message) => ExitOutcome::Failed { message },
        None => ExitOutcome::Exited {
            exit_code: process.exit_code(),
        },
    };
    ExitReport {
        outcome,
        duration,
        unread_tail,
        unread_earlier_bytes,
        unread_lost_bytes,
        sandbox_denied,
    }
}

impl super::UnifiedExecContext {
    /// `exec_command` returned a session ID, so the exit is reported later and, with
    /// `notify_on_output`, the output watch starts now. A process that already exited is
    /// queued before this call returns (R2.1, R6.3, R6.9).
    pub(crate) async fn arm_exit_notification(
        &self,
        response_process_id: Option<i32>,
        exit_notifier: Option<&Arc<ExitNotifier>>,
        process: &super::UnifiedExecProcess,
        notify: &NotifyRequest,
    ) -> ArmedNotifications {
        let (Some(_), Some(exit_notifier)) = (response_process_id, exit_notifier) else {
            return ArmedNotifications::default();
        };
        let output_watch = match &notify.output {
            Some(pattern) => {
                self.start_output_watch(exit_notifier, process, pattern)
                    .await
            }
            None => OutputWatchState::Unchanged,
        };
        let watching = matches!(output_watch, OutputWatchState::On(_));
        if notify.exit || watching {
            self.arm(exit_notifier, process).await;
            self.session.fork_sync_terminal_journal().await;
        }
        ArmedNotifications {
            exit: notify.exit || watching,
            output_watch,
        }
    }

    /// Arm the exit notification. A process that already exited is reported now, after the
    /// final batch of its output watch.
    async fn arm(&self, exit_notifier: &Arc<ExitNotifier>, process: &super::UnifiedExecProcess) {
        if let Some(pending) = exit_notifier.arm() {
            let watch = process
                .output_handles()
                .output_buffer
                .lock()
                .await
                .fork
                .watch
                .take();
            deliver_exit(&self.session, exit_notifier, pending, watch.as_deref()).await;
        }
    }

    /// After every `write_stdin` to a live process: the response drained output, so matching
    /// lines it showed whole are no news (P3-2); then `notify_on_output` starts, replaces or
    /// stops the output watch, and a started watch arms the exit notification (R6.3, R6.4).
    /// The caller holds the interaction lock, so the exit watcher reports only after this. A
    /// watch that cannot start says why (P3-6).
    pub(crate) async fn change_output_watch(
        &self,
        response_process_id: Option<i32>,
        process: &super::UnifiedExecProcess,
        exit_notifier: Option<&Arc<ExitNotifier>>,
        change: Option<OutputWatchChange>,
        budget: PollBudget,
    ) -> ArmedNotifications {
        if response_process_id.is_none() {
            return ArmedNotifications::default();
        }
        self.mark_polled(process, budget).await;
        let Some(change) = change else {
            return ArmedNotifications::default();
        };
        let output_watch = match (change, exit_notifier) {
            (OutputWatchChange::Stop, exit_notifier) => {
                let watch = process
                    .output_handles()
                    .output_buffer
                    .lock()
                    .await
                    .fork
                    .watch
                    .take();
                if let Some(watch) = watch {
                    watch.stop();
                }
                if let Some(exit_notifier) = exit_notifier {
                    exit_notifier.set_output_watch(/*watch*/ None);
                }
                OutputWatchState::Off
            }
            (OutputWatchChange::Watch(_), None) => {
                OutputWatchState::NotStarted(OutputWatchRefusal::NoNotifications)
            }
            (OutputWatchChange::Watch(_), Some(_)) if process.has_exited() => {
                OutputWatchState::NotStarted(OutputWatchRefusal::Exited)
            }
            (OutputWatchChange::Watch(pattern), Some(exit_notifier)) => {
                let output_watch = self
                    .start_output_watch(exit_notifier, process, &pattern)
                    .await;
                if matches!(output_watch, OutputWatchState::On(_)) {
                    self.arm(exit_notifier, process).await;
                    self.session.fork_sync_terminal_journal().await;
                }
                output_watch
            }
        };
        ArmedNotifications {
            exit: exit_notifier.is_some_and(|notifier| notifier.is_armed()),
            output_watch,
        }
    }

    /// The response of a poll drained the output it shows; tell the output watch, and withdraw
    /// its queued notification when the response showed all of its lines (P3-2).
    async fn mark_polled(&self, process: &super::UnifiedExecProcess, budget: PollBudget) {
        let watch = {
            let output = process.output_handles().output_buffer.lock().await;
            let Some(watch) = output.fork.watch.clone() else {
                return;
            };
            watch.mark_read(output.pending.total_bytes(), budget);
            watch
        };
        self.session
            .input_queue
            .withdraw_background_notification_if(watch.key(), || watch.forget_read_queued())
            .await;
    }

    /// Start a watch unless the thread already runs the most watches (R6.8). Unread output
    /// that arrived after the call collected its response is matched right away: the
    /// response already showed everything before it.
    async fn start_output_watch(
        &self,
        exit_notifier: &Arc<ExitNotifier>,
        process: &super::UnifiedExecProcess,
        pattern: &Arc<OutputPattern>,
    ) -> OutputWatchState {
        let others = self
            .session
            .services
            .unified_exec_manager
            .active_output_watches(exit_notifier.process_id())
            .await;
        if others >= OUTPUT_WATCH_MAX_ACTIVE {
            return OutputWatchState::NotStarted(OutputWatchRefusal::Limit);
        }
        let watch = OutputWatch::with_windows(
            exit_notifier.process_id(),
            exit_notifier.command().to_string(),
            Arc::clone(pattern),
            OutputWatchWindows::from_config(&self.step_context.turn.config.exec_exit_notifications),
        );
        {
            let mut output = process.output_handles().output_buffer.lock().await;
            if let Some(previous) = output.fork.watch.replace(Arc::clone(&watch)) {
                previous.stop();
            }
            // Only lines are split here; the backlog keeps the newest of them for matching.
            watch.skip_bytes(output.pending.omitted_bytes());
            let unread = output.pending.to_bytes();
            watch.observe_chunk(&unread);
        }
        exit_notifier.set_output_watch(Some(Arc::clone(&watch)));
        spawn_delivery(watch, &self.session, process.cancellation_token());
        OutputWatchState::On(pattern.source().to_string())
    }

    /// A `write_stdin` response shows the exit or failure of this process, so its exit
    /// notification is settled and, when it was already queued, withdrawn with the final
    /// batch of its output watch (R2.2, R2.10, R6.9).
    pub(crate) async fn observe_exit(&self, exit_notifier: Option<&ExitNotifier>) {
        let Some(exit_notifier) = exit_notifier else {
            return;
        };
        for key in exit_notifier.observe() {
            self.session
                .input_queue
                .withdraw_background_notification(key)
                .await;
        }
        self.session.fork_sync_terminal_journal().await;
    }
}

impl super::UnifiedExecProcessManager {
    /// A turn of the thread ended: output watches that wait for their first matching line
    /// count that wait from now (`OutputWatch::note_turn_finished`, tails 2026-10-27).
    pub(crate) async fn note_turn_finished(&self) {
        let now = Instant::now();
        let watches: Vec<Arc<OutputWatch>> = self
            .process_store
            .lock()
            .await
            .processes
            .values()
            .filter_map(|entry| entry.exit_notifier.as_ref()?.output_watch())
            .collect();
        for watch in watches {
            watch.note_turn_finished(now);
        }
    }

    /// Whether the thread waits for a live process or for a notification on its way into the
    /// thread queue (`ExitNotifier::is_awaited`).
    pub(crate) async fn has_pending_exit_notifications(&self) -> bool {
        let now = Instant::now();
        self.process_store
            .lock()
            .await
            .processes
            .values()
            .any(|entry| {
                entry
                    .exit_notifier
                    .as_ref()
                    .is_some_and(|notifier| notifier.is_awaited(now))
            })
    }

    /// The terminals the thread waits for, by session ID (R4.5, R7.4, P2-3).
    pub(crate) async fn pending_exit_notifications(&self) -> Vec<PendingTerminal> {
        let now = Instant::now();
        let pending: Vec<(Arc<ExitNotifier>, SharedOutputBuffers)> = self
            .process_store
            .lock()
            .await
            .processes
            .values()
            .filter_map(|entry| {
                let notifier = entry.exit_notifier.as_ref()?;
                notifier.is_awaited(now).then(|| {
                    (
                        Arc::clone(notifier),
                        Arc::clone(&entry.process.output_handles().output_buffer),
                    )
                })
            })
            .collect();
        let mut terminals = Vec::with_capacity(pending.len());
        for (notifier, output_buffer) in pending {
            terminals.push(PendingTerminal {
                process_id: notifier.process_id(),
                command: notifier.command().to_string(),
                started_at: notifier.started_at(),
                last_output_at: output_buffer.lock().await.fork.last_output_at,
                output_watch: notifier
                    .output_watch()
                    .map(|watch| watch.pattern().to_string()),
            });
        }
        terminals.sort_by_key(|terminal| terminal.process_id);
        terminals
    }

    /// Output watches that are on, except the one of `process_id` (R6.8).
    pub(crate) async fn active_output_watches(&self, process_id: i32) -> usize {
        self.process_store
            .lock()
            .await
            .processes
            .values()
            .filter(|entry| {
                entry.process_id != process_id
                    && entry
                        .exit_notifier
                        .as_ref()
                        .is_some_and(|notifier| notifier.output_watch().is_some())
            })
            .count()
    }

    /// Free a process whose notification left nothing for `write_stdin` to read (R2.12).
    /// Only the entry that `exit_notifier` belongs to is removed, and only once its process
    /// exited and the initial `exec_command` returned.
    pub(crate) async fn release_after_notification(&self, exit_notifier: &ExitNotifier) {
        let removed = {
            let mut store = self.process_store.lock().await;
            let process_id = exit_notifier.process_id();
            let releasable = store.processes.get(&process_id).is_some_and(|entry| {
                entry
                    .exit_notifier
                    .as_ref()
                    .is_some_and(|notifier| std::ptr::eq(notifier.as_ref(), exit_notifier))
                    && entry.process.has_exited()
                    && !entry.initial_exec_command_active.load(Ordering::Acquire)
            });
            if !releasable {
                return;
            }
            store.remove(process_id)
        };
        if let Some(entry) = removed {
            entry.process.cancellation_token().cancel();
            super::process_manager::unregister_network_approval_for_entry(&entry).await;
        }
    }
}

#[cfg(test)]
#[path = "exit_notification_tests.rs"]
mod tests;
