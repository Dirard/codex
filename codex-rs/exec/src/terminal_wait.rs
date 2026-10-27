//! Fork: `codex exec` stays alive while its thread still owes the model a background terminal
//! notification (`specs/background-exec-notifications-2026-10-25.md`, UC7).
//!
//! With `exec_exit_notifications` enabled, when a turn completes and live terminals still
//! have armed exit notifications (`notify_on_exit`, `notify_on_output`), a queued notification
//! will start a turn, or another turn runs, exec keeps processing the turns those
//! notifications start. After `features.exec_exit_notifications.exec_wait_timeout_ms` it
//! warns, names the terminals it did not wait for and exits with code 1. An interrupt (Ctrl-C)
//! that ends the wait does the same, and terminals that nothing waits for any longer, such as
//! a server whose output watch went quiet, are named when exec exits (review 2026-10-27,
//! P2-3, P3-5).

use std::time::Duration;

use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadBackgroundTerminal;
use codex_app_server_protocol::ThreadBackgroundTerminalsListParams;
use codex_app_server_protocol::ThreadBackgroundTerminalsListResponse;
use codex_app_server_protocol::ThreadBackgroundTerminalsPendingParams;
use codex_app_server_protocol::ThreadBackgroundTerminalsPendingResponse;
use codex_app_server_protocol::TurnStatus;
use tokio::time::Instant;
use tracing::info;
use tracing::warn;

use crate::RequestIdSequencer;
use crate::event_processor::EventProcessor;
use crate::send_request_with_response;

/// How often exec checks the thread again while it waits without a turn (R7.2).
pub(crate) const EXEC_TERMINAL_RECHECK: Duration = Duration::from_secs(1);
/// How long a turn that is running when the wait expires may still finish (R7.3).
pub(crate) const EXEC_EXPIRED_TURN_GRACE: Duration = Duration::from_secs(120);
/// Longest command a timeout warning quotes, in characters.
const WARNING_COMMAND_MAX_CHARS: usize = 120;

/// What a timer of the terminal wait decided.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TerminalTick {
    /// Exec keeps waiting for terminals.
    Wait,
    /// Exec no longer waits for terminals.
    Done,
}

struct Waiting {
    started_at: Instant,
    deadline: Instant,
    next_check: Instant,
    /// The wait timed out; only a turn already in progress may finish, until this.
    expired_until: Option<Instant>,
}

/// The wait of `codex exec` for the background terminals of its primary thread.
pub(crate) struct TerminalWait {
    enabled: bool,
    timeout: Duration,
    thread_id: String,
    waiting: Option<Waiting>,
    expired: bool,
    /// A turn started on the thread while exec waits, and it has not completed yet.
    turn_in_progress: bool,
    /// The last turn of the thread that completed, and whether the user interrupted it.
    last_completed_turn: Option<String>,
    last_turn_interrupted: bool,
}

impl TerminalWait {
    pub(crate) fn new(config: &codex_core::config::Config, thread_id: &str) -> Self {
        Self {
            enabled: config
                .features
                .enabled(codex_features::Feature::ExecExitNotifications),
            timeout: Duration::from_millis(config.exec_exit_notifications.exec_wait_timeout_ms),
            thread_id: thread_id.to_string(),
            waiting: None,
            expired: false,
            turn_in_progress: false,
            last_completed_turn: None,
            last_turn_interrupted: false,
        }
    }

    pub(crate) fn is_waiting(&self) -> bool {
        self.waiting.is_some()
    }

    pub(crate) fn turn_in_progress(&self) -> bool {
        self.waiting.is_some() && self.turn_in_progress
    }

    /// When a timer of the wait fires next: the recheck while no turn runs, the timeout,
    /// or the end of the grace of an expired wait.
    pub(crate) fn next_wakeup(&self) -> Option<Instant> {
        let waiting = self.waiting.as_ref()?;
        if let Some(expired_until) = waiting.expired_until {
            return Some(expired_until);
        }
        Some(if self.turn_in_progress {
            waiting.deadline
        } else {
            waiting.deadline.min(waiting.next_check)
        })
    }

    /// Follows the turns of the thread: a turn started while exec waits becomes its task,
    /// and the last completed turn says whether the user interrupted it.
    pub(crate) fn observe(&mut self, notification: &ServerNotification, task_id: &mut String) {
        match notification {
            ServerNotification::TurnStarted(payload)
                if payload.thread_id == self.thread_id && self.waiting.is_some() =>
            {
                *task_id = payload.turn.id.clone();
                self.turn_in_progress = true;
            }
            ServerNotification::TurnCompleted(payload) if payload.thread_id == self.thread_id => {
                self.last_completed_turn = Some(payload.turn.id.clone());
                self.last_turn_interrupted = matches!(payload.turn.status, TurnStatus::Interrupted);
            }
            _ => {}
        }
    }

    /// After a turn completed: keep waiting while the thread still owes the model a
    /// background terminal notification (R7.1). Returns whether exec waits. When exec stops,
    /// it names the terminals it leaves: after an interrupted turn those still owed (P3-5),
    /// otherwise those still running that nothing waits for (P2-3).
    pub(crate) async fn keep_waiting(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
        event_processor: &mut dyn EventProcessor,
    ) -> bool {
        self.turn_in_progress = false;
        if !self.enabled || self.expired {
            self.waiting = None;
            return false;
        }
        if self.last_turn_interrupted {
            // A turn that exec ran while it waited (one an exit notification started) ends
            // that wait; the first turn ran before exec waited for anything (live run D5).
            let was_waiting = self.waiting.take().is_some();
            if let Some(pending) = self.pending(client, request_ids).await
                && !pending.terminals.is_empty()
            {
                event_processor.process_warning(if was_waiting {
                    interrupt_warning(Some(&pending))
                } else {
                    interrupted_turn_warning(&pending)
                });
            }
            return false;
        }
        let pending = self.pending(client, request_ids).await;
        let owed = pending.as_ref().is_some_and(|pending| self.owes(pending));
        if !owed {
            self.waiting = None;
            self.name_running_terminals(client, request_ids, event_processor)
                .await;
            return false;
        }
        let now = Instant::now();
        let timeout = self.timeout;
        let waiting = self.waiting.get_or_insert_with(|| Waiting {
            started_at: now,
            deadline: now + timeout,
            next_check: now,
            expired_until: None,
        });
        waiting.next_check = now + EXEC_TERMINAL_RECHECK;
        // A silent wait of up to half an hour looked like a hang (live recheck 2026-10-27,
        // N1): after every turn that leaves exec waiting, stderr says for what and how long.
        // The JSON output has no event for this; it stays on stderr there too.
        if let Some(message) = pending.as_ref().and_then(|pending| {
            waiting_message(pending, waiting.deadline.saturating_duration_since(now))
        }) {
            eprintln!("{message}");
        }
        true
    }

    /// A timer of the wait fired (R7.2, R7.3).
    pub(crate) async fn on_tick(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
        event_processor: &mut dyn EventProcessor,
        error_seen: &mut bool,
    ) -> TerminalTick {
        let now = Instant::now();
        let Some(waiting) = self.waiting.as_ref() else {
            return TerminalTick::Done;
        };
        if let Some(expired_until) = waiting.expired_until {
            if now >= expired_until {
                warn!("the turn after the background terminal wait expired did not finish in time");
                self.waiting = None;
                return TerminalTick::Done;
            }
            return TerminalTick::Wait;
        }
        if now >= waiting.deadline {
            let waited = now.saturating_duration_since(waiting.started_at);
            let pending = self.pending(client, request_ids).await;
            event_processor.process_warning(timeout_warning(waited, pending.as_ref()));
            *error_seen = true;
            self.expired = true;
            if self.turn_in_progress {
                if let Some(waiting) = self.waiting.as_mut() {
                    waiting.expired_until = Some(now + EXEC_EXPIRED_TURN_GRACE);
                }
                return TerminalTick::Wait;
            }
            self.waiting = None;
            return TerminalTick::Done;
        }
        if self.turn_in_progress || now < waiting.next_check {
            return TerminalTick::Wait;
        }
        let owed = match self.pending(client, request_ids).await {
            Some(pending) => self.owes(&pending),
            None => false,
        };
        if !owed {
            info!("background terminals settled; codex exec stops waiting");
            self.waiting = None;
            self.name_running_terminals(client, request_ids, event_processor)
                .await;
            return TerminalTick::Done;
        }
        if let Some(waiting) = self.waiting.as_mut() {
            waiting.next_check = now + EXEC_TERMINAL_RECHECK;
        }
        TerminalTick::Wait
    }

    /// Whether the thread still owes the model something exec should wait for: a live
    /// terminal with an armed exit notification, a queued notification that will start a
    /// turn, or a turn other than the one that just completed.
    fn owes(&self, pending: &ThreadBackgroundTerminalsPendingResponse) -> bool {
        let other_turn = pending
            .active_turn_id
            .as_ref()
            .is_some_and(|turn| self.last_completed_turn.as_ref() != Some(turn));
        let owed = !pending.terminals.is_empty()
            || pending.notification_queued
            || pending.turn_starting
            || other_turn;
        if owed {
            info!(
                terminals = ?pending
                    .terminals
                    .iter()
                    .map(|terminal| terminal.process_id.as_str())
                    .collect::<Vec<_>>(),
                notification_queued = pending.notification_queued,
                "waiting for background terminals before exiting"
            );
        }
        owed
    }

    /// An interrupt (Ctrl-C) ends the wait between turns: the terminals it leaves are named,
    /// and exec exits with an error, as after the timeout (P3-5).
    pub(crate) async fn on_interrupt(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
        event_processor: &mut dyn EventProcessor,
        error_seen: &mut bool,
    ) {
        if self.waiting.take().is_none() {
            return;
        }
        let pending = self.pending(client, request_ids).await;
        event_processor.process_warning(interrupt_warning(pending.as_ref()));
        *error_seen = true;
    }

    /// Exec ends without waiting for terminals that still run, for example a server whose
    /// output watch went quiet; the shutdown terminates them (P2-3).
    async fn name_running_terminals(
        &self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
        event_processor: &mut dyn EventProcessor,
    ) {
        match send_request_with_response::<ThreadBackgroundTerminalsListResponse>(
            client,
            ClientRequest::ThreadBackgroundTerminalsList {
                request_id: request_ids.next(),
                params: ThreadBackgroundTerminalsListParams {
                    thread_id: self.thread_id.clone(),
                    cursor: None,
                    limit: None,
                },
            },
            "thread/backgroundTerminals/list",
        )
        .await
        {
            Ok(list) => {
                if let Some(warning) = running_terminals_warning(&list.data) {
                    event_processor.process_warning(warning);
                }
            }
            Err(err) => warn!("thread/backgroundTerminals/list failed: {err}"),
        }
    }

    async fn pending(
        &self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> Option<ThreadBackgroundTerminalsPendingResponse> {
        match send_request_with_response::<ThreadBackgroundTerminalsPendingResponse>(
            client,
            ClientRequest::ThreadBackgroundTerminalsPending {
                request_id: request_ids.next(),
                params: ThreadBackgroundTerminalsPendingParams {
                    thread_id: self.thread_id.clone(),
                },
            },
            "thread/backgroundTerminals/pending",
        )
        .await
        {
            Ok(pending) => Some(pending),
            Err(err) => {
                warn!("thread/backgroundTerminals/pending failed: {err}");
                None
            }
        }
    }
}

/// The pending terminals as warnings name them.
fn pending_terminals(pending: Option<&ThreadBackgroundTerminalsPendingResponse>) -> Vec<String> {
    pending
        .map(|pending| {
            pending
                .terminals
                .iter()
                .map(|terminal| {
                    format!(
                        "session {} ({}, running for {:.1}s)",
                        terminal.process_id,
                        shorten(&terminal.command),
                        Duration::from_millis(terminal.running_for_ms).as_secs_f64()
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

/// What exec waits for after a turn, and for how long at most (live recheck 2026-10-27, N1):
/// each terminal with its exit notification or output watch. `None` while only a notification
/// or a turn is on its way, which takes moments.
pub(crate) fn waiting_message(
    pending: &ThreadBackgroundTerminalsPendingResponse,
    remaining: Duration,
) -> Option<String> {
    if pending.terminals.is_empty() {
        return None;
    }
    let terminals = pending
        .terminals
        .iter()
        .map(|terminal| {
            let mut reasons = Vec::new();
            if terminal.notify_on_exit {
                reasons.push("exit notification armed".to_string());
            }
            if let Some(pattern) = &terminal.output_watch {
                reasons.push(format!("output watch \"{}\"", shorten(pattern)));
            }
            format!(
                "session {} ({}): {}",
                terminal.process_id,
                shorten(&terminal.command),
                reasons.join(", ")
            )
        })
        .collect::<Vec<_>>();
    Some(format!(
        "codex exec is waiting for {}; up to {}; Ctrl-C to stop",
        terminals.join("; "),
        short_duration(remaining)
    ))
}

/// "30m", "29m 40s", "45s", "1.5s".
fn short_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds >= 60 {
        let rest = seconds % 60;
        if rest == 0 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}m {rest}s", seconds / 60)
        }
    } else if duration.subsec_millis() == 0 {
        format!("{seconds}s")
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

/// An interrupt ended the wait: what exec did not wait for (P3-5).
pub(crate) fn interrupt_warning(
    pending: Option<&ThreadBackgroundTerminalsPendingResponse>,
) -> String {
    let terminals = pending_terminals(pending);
    if terminals.is_empty() {
        "codex exec was interrupted while it waited for background terminal notifications; they are not reported.".to_string()
    } else {
        format!(
            "codex exec was interrupted while it waited for background terminals; still pending: {}. They are terminated now and their exits are not reported.",
            terminals.join(", ")
        )
    }
}

/// An interrupt ended a turn before exec waited for terminals (combined live run D5).
pub(crate) fn interrupted_turn_warning(
    pending: &ThreadBackgroundTerminalsPendingResponse,
) -> String {
    format!(
        "codex exec was interrupted during a turn; background terminals still pending: {}. They are terminated now and their exits are not reported.",
        pending_terminals(Some(pending)).join(", ")
    )
}

/// Exec exits while terminals still run that nothing waits for (P2-3).
pub(crate) fn running_terminals_warning(terminals: &[ThreadBackgroundTerminal]) -> Option<String> {
    if terminals.is_empty() {
        return None;
    }
    let names = terminals
        .iter()
        .map(|terminal| {
            format!(
                "session {} ({})",
                terminal.process_id,
                shorten(&terminal.command)
            )
        })
        .collect::<Vec<_>>();
    Some(format!(
        "codex exec is exiting while background terminals still run; they are terminated now: {}.",
        names.join(", ")
    ))
}

/// The warning of an expired wait names what exec did not wait for (R7.3).
pub(crate) fn timeout_warning(
    waited: Duration,
    pending: Option<&ThreadBackgroundTerminalsPendingResponse>,
) -> String {
    let waited = format!("{:.1}s", waited.as_secs_f64());
    let terminals = pending_terminals(pending);
    if terminals.is_empty() {
        format!(
            "codex exec stopped waiting for background terminal notifications after {waited}; the remaining ones are not reported."
        )
    } else {
        format!(
            "codex exec stopped waiting for background terminals after {waited}; still pending: {}. They are terminated now and their exits are not reported.",
            terminals.join(", ")
        )
    }
}

fn shorten(command: &str) -> String {
    let command = command.split_whitespace().collect::<Vec<_>>().join(" ");
    if command.chars().count() <= WARNING_COMMAND_MAX_CHARS {
        return command;
    }
    let mut short: String = command
        .chars()
        .take(WARNING_COMMAND_MAX_CHARS - 1)
        .collect();
    short.push('…');
    short
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::PendingBackgroundTerminal;
    use pretty_assertions::assert_eq;

    fn pending(
        terminals: Vec<PendingBackgroundTerminal>,
    ) -> ThreadBackgroundTerminalsPendingResponse {
        ThreadBackgroundTerminalsPendingResponse {
            terminals,
            notification_queued: false,
            active_turn_id: None,
            turn_starting: false,
        }
    }

    #[test]
    fn timeout_warning_names_the_pending_terminals() {
        let warning = timeout_warning(
            Duration::from_millis(1_800_250),
            Some(&pending(vec![PendingBackgroundTerminal {
                process_id: "1000".to_string(),
                command: "make   all\n".to_string(),
                running_for_ms: 1_805_200,
                output_watch: None,
                notify_on_exit: true,
            }])),
        );
        assert_eq!(
            warning,
            "codex exec stopped waiting for background terminals after 1800.2s; still pending: session 1000 (make all, running for 1805.2s). They are terminated now and their exits are not reported."
        );
        assert_eq!(
            timeout_warning(Duration::from_secs(2), Some(&pending(Vec::new()))),
            "codex exec stopped waiting for background terminal notifications after 2.0s; the remaining ones are not reported."
        );
        let long = shorten(&"x".repeat(500));
        assert_eq!(long.chars().count(), WARNING_COMMAND_MAX_CHARS);
        assert!(long.ends_with('…'));
    }

    /// Live recheck 2026-10-27, N1: after a turn exec says what it waits for, why, and how
    /// long at most, instead of waiting in silence.
    #[test]
    fn waiting_message_names_each_terminal_and_the_longest_wait() {
        let server = PendingBackgroundTerminal {
            process_id: "7572".to_string(),
            command: "./dev.sh".to_string(),
            running_for_ms: 90_000,
            output_watch: Some("(?i)ready|error".to_string()),
            notify_on_exit: true,
        };
        let watch = PendingBackgroundTerminal {
            process_id: "7573".to_string(),
            command: "npm run   watch".to_string(),
            running_for_ms: 5_000,
            output_watch: Some("compiled".to_string()),
            notify_on_exit: false,
        };
        assert_eq!(
            waiting_message(&pending(vec![server.clone()]), Duration::from_secs(1800)),
            Some(
                "codex exec is waiting for session 7572 (./dev.sh): exit notification armed, output watch \"(?i)ready|error\"; up to 30m; Ctrl-C to stop"
                    .to_string()
            )
        );
        assert_eq!(
            waiting_message(
                &pending(vec![server, watch]),
                Duration::from_millis(1_777_400)
            ),
            Some(
                "codex exec is waiting for session 7572 (./dev.sh): exit notification armed, output watch \"(?i)ready|error\"; session 7573 (npm run watch): output watch \"compiled\"; up to 29m 37s; Ctrl-C to stop"
                    .to_string()
            )
        );
        assert_eq!(
            waiting_message(&pending(Vec::new()), Duration::from_secs(4)),
            None
        );
        assert_eq!(short_duration(Duration::from_millis(4_000)), "4s");
        assert_eq!(short_duration(Duration::from_millis(1_500)), "1.5s");
    }

    /// Review 2026-10-27 P3-5 and P2-3: an interrupt and an exit that leaves running
    /// terminals say what happens to them.
    #[test]
    fn interrupt_and_exit_warnings_name_the_terminals() {
        let terminal = PendingBackgroundTerminal {
            process_id: "1000".to_string(),
            command: "make all".to_string(),
            running_for_ms: 12_000,
            output_watch: None,
            notify_on_exit: true,
        };
        assert_eq!(
            interrupt_warning(Some(&pending(vec![terminal]))),
            "codex exec was interrupted while it waited for background terminals; still pending: session 1000 (make all, running for 12.0s). They are terminated now and their exits are not reported."
        );
        assert_eq!(
            interrupt_warning(None),
            "codex exec was interrupted while it waited for background terminal notifications; they are not reported."
        );
        let terminal = PendingBackgroundTerminal {
            process_id: "7961".to_string(),
            command: "./build.sh".to_string(),
            running_for_ms: 30_700,
            output_watch: None,
            notify_on_exit: true,
        };
        assert_eq!(
            interrupted_turn_warning(&pending(vec![terminal])),
            "codex exec was interrupted during a turn; background terminals still pending: session 7961 (./build.sh, running for 30.7s). They are terminated now and their exits are not reported."
        );
        assert_eq!(running_terminals_warning(&[]), None);
        let running = ThreadBackgroundTerminal {
            item_id: "item".to_string(),
            process_id: "1001".to_string(),
            command: "./dev.sh".to_string(),
            cwd: serde_json::from_value(serde_json::json!("/tmp")).expect("cwd"),
            os_pid: None,
            cpu_percent: None,
            rss_kb: None,
        };
        assert_eq!(
            running_terminals_warning(&[running]),
            Some(
                "codex exec is exiting while background terminals still run; they are terminated now: session 1001 (./dev.sh)."
                    .to_string()
            )
        );
    }
}
