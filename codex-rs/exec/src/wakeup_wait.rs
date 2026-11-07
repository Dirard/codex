//! Fork: `codex exec` stays alive while its thread has a scheduled wakeup or an active
//! `/loop` (`specs/scheduled-wakeups-2026-11-07.md`, UC7).
//!
//! With `scheduled_wakeups` enabled, when a turn that the user did not interrupt completes
//! and the thread has a pending wakeup or loop, exec says on stderr what it waits for and
//! processes the turns the wakeups start as its tasks. After
//! `features.scheduled_wakeups.exec_wait_timeout_ms`, or on Ctrl-C between turns, it cancels
//! the wakeup, warns and exits with code 1. Without the feature it sends no wakeup request.

use std::time::Duration;

use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use codex_app_server_client::InProcessAppServerClient;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadBackgroundTerminalsPendingParams;
use codex_app_server_protocol::ThreadBackgroundTerminalsPendingResponse;
use codex_app_server_protocol::ThreadWakeupCancelParams;
use codex_app_server_protocol::ThreadWakeupCancelResponse;
use codex_app_server_protocol::ThreadWakeupReadParams;
use codex_app_server_protocol::ThreadWakeupReadResponse;
use codex_app_server_protocol::ThreadWakeupState;
use codex_app_server_protocol::TurnStatus;
use tokio::time::Instant;
use tracing::warn;

use super::terminal_wait::EXEC_EXPIRED_TURN_GRACE;
use super::terminal_wait::TerminalTick;
use crate::RequestIdSequencer;
use crate::event_processor::EventProcessor;
use crate::send_request_with_response;

/// How often exec reads the wakeup state while it waits without a turn.
pub(crate) const EXEC_WAKEUP_RECHECK: Duration = Duration::from_millis(5_000);

struct Waiting {
    started_at: Instant,
    deadline: Instant,
    next_check: Instant,
    /// The wait timed out; only a turn already in progress may finish, until this.
    expired_until: Option<Instant>,
    /// The due time of the wakeup exec waits for, for the timeout warning.
    due_at: Option<i64>,
}

/// The wait of `codex exec` for the scheduled wakeups of its primary thread.
pub(crate) struct WakeupWait {
    enabled: bool,
    timeout: Duration,
    thread_id: String,
    waiting: Option<Waiting>,
    expired: bool,
    turn_in_progress: bool,
    last_completed_turn: Option<String>,
    last_turn_interrupted: bool,
}

impl WakeupWait {
    pub(crate) fn new(config: &codex_core::config::Config, thread_id: &str) -> Self {
        Self {
            enabled: config
                .features
                .enabled(codex_features::Feature::ScheduledWakeups),
            timeout: Duration::from_millis(config.scheduled_wakeups.exec_wait_timeout_ms),
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

    /// R7.1: a turn started while exec waits becomes its task; a wakeup change makes the next
    /// check come at once.
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
            ServerNotification::ThreadWakeupUpdated(payload)
                if payload.thread_id == self.thread_id =>
            {
                if let Some(waiting) = self.waiting.as_mut() {
                    waiting.next_check = Instant::now();
                }
            }
            _ => {}
        }
    }

    /// R7.1, R7.2, R7.5: after a turn that the user did not interrupt, keep waiting while the
    /// thread has a pending wakeup or an active loop. Returns whether exec waits.
    pub(crate) async fn keep_waiting(
        &mut self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> bool {
        self.turn_in_progress = false;
        if !self.enabled || self.expired || self.last_turn_interrupted {
            self.waiting = None;
            return false;
        }
        let Some(state) = self.read(client, request_ids).await else {
            self.waiting = None;
            return false;
        };
        // A wakeup due at the end of the turn fires at once: it is gone from the state, and
        // its turn already runs or is about to start.
        if state.wakeup.is_none()
            && state.loop_state.is_none()
            && !self.other_turn_active(client, request_ids).await
        {
            self.waiting = None;
            return false;
        }
        let now = Instant::now();
        let timeout = self.timeout;
        let waiting = self.waiting.get_or_insert_with(|| Waiting {
            started_at: now,
            deadline: now + timeout,
            next_check: now,
            expired_until: None,
            due_at: None,
        });
        waiting.next_check = now + EXEC_WAKEUP_RECHECK;
        waiting.due_at = state.wakeup.as_ref().map(|wakeup| wakeup.due_at);
        let left = waiting.deadline.saturating_duration_since(now);
        if let Some(message) = waiting_message(&state, Utc::now(), left) {
            eprintln!("{message}");
        }
        true
    }

    /// A timer of the wait fired (R7.3).
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
                warn!("the turn after the scheduled-wakeup wait expired did not finish in time");
                self.waiting = None;
                return TerminalTick::Done;
            }
            return TerminalTick::Wait;
        }
        if now >= waiting.deadline {
            let waited = now.saturating_duration_since(waiting.started_at);
            let due_at = waiting.due_at;
            self.cancel(client, request_ids).await;
            event_processor.process_warning(timeout_warning(waited, due_at));
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
        let pending = self
            .read(client, request_ids)
            .await
            .is_some_and(|state| state.wakeup.is_some() || state.loop_state.is_some());
        // A wakeup that just fired is gone from the state before its turn reaches exec.
        if !pending && !self.other_turn_active(client, request_ids).await {
            self.waiting = None;
            return TerminalTick::Done;
        }
        if let Some(waiting) = self.waiting.as_mut() {
            waiting.next_check = now + EXEC_WAKEUP_RECHECK;
        }
        TerminalTick::Wait
    }

    /// R7.4: Ctrl-C between turns cancels the wakeup and exits with an error.
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
        self.cancel(client, request_ids).await;
        event_processor.process_warning(INTERRUPT_WARNING.to_string());
        *error_seen = true;
    }

    async fn read(
        &self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> Option<ThreadWakeupState> {
        match send_request_with_response::<ThreadWakeupReadResponse>(
            client,
            ClientRequest::ThreadWakeupRead {
                request_id: request_ids.next(),
                params: ThreadWakeupReadParams {
                    thread_id: self.thread_id.clone(),
                },
            },
            "thread/wakeup/read",
        )
        .await
        {
            Ok(response) => Some(response.state),
            Err(err) => {
                warn!("thread/wakeup/read failed: {err}");
                None
            }
        }
    }

    async fn cancel(
        &self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) {
        if let Err(err) = send_request_with_response::<ThreadWakeupCancelResponse>(
            client,
            ClientRequest::ThreadWakeupCancel {
                request_id: request_ids.next(),
                params: ThreadWakeupCancelParams {
                    thread_id: self.thread_id.clone(),
                },
            },
            "thread/wakeup/cancel",
        )
        .await
        {
            warn!("thread/wakeup/cancel failed: {err}");
        }
    }

    /// Whether a turn other than the last completed one runs or is about to start.
    async fn other_turn_active(
        &self,
        client: &InProcessAppServerClient,
        request_ids: &mut RequestIdSequencer,
    ) -> bool {
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
            Ok(pending) => {
                pending.turn_starting
                    || pending
                        .active_turn_id
                        .is_some_and(|turn| self.last_completed_turn.as_ref() != Some(&turn))
            }
            Err(err) => {
                warn!("thread/backgroundTerminals/pending failed: {err}");
                false
            }
        }
    }
}

const INTERRUPT_WARNING: &str =
    "codex exec was interrupted while it waited for a scheduled wakeup; it is cancelled now.";

/// `4m 30s`, `55m 10s`, `60m 0s`.
fn describe(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

fn clock(unix_seconds: i64) -> String {
    DateTime::<Utc>::from_timestamp(unix_seconds, 0)
        .map(|at| at.format("%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// Control characters of the model's reason become `?` (R10.4).
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '?' } else { ch })
        .collect()
}

/// R7.2: what exec waits for, until when, and how to stop it.
pub(crate) fn waiting_message(
    state: &ThreadWakeupState,
    now: DateTime<Utc>,
    left: Duration,
) -> Option<String> {
    let wakeup = state.wakeup.as_ref();
    let mut details = Vec::new();
    let when = match wakeup {
        Some(wakeup) if !wakeup.due => {
            let in_seconds = u64::try_from(wakeup.due_at - now.timestamp()).unwrap_or_default();
            details.push(format!("in {}", describe(Duration::from_secs(in_seconds))));
            format!("at {}", clock(wakeup.due_at))
        }
        Some(_) => "as soon as the thread is idle".to_string(),
        None => "after the current iteration".to_string(),
    };
    let subject = match &state.loop_state {
        Some(loop_state) => format!(
            "/loop iteration {} of at most {}",
            loop_state.iteration + 1,
            loop_state.max_iterations
        ),
        None => {
            wakeup?;
            details.push(format!(
                "wakeup {} of at most {}",
                state.chain.fired + 1,
                state.chain.max_wakeups
            ));
            "a scheduled wakeup".to_string()
        }
    };
    let mut text = format!("codex exec is waiting for {subject} {when}");
    if !details.is_empty() {
        text.push_str(&format!(" ({})", details.join("; ")));
    }
    if let Some(reason) = wakeup.and_then(|wakeup| wakeup.reason.as_deref()) {
        text.push_str(&format!(": {}", sanitize(reason)));
    }
    text.push_str(&format!("; up to {} more; Ctrl-C to stop", describe(left)));
    Some(text)
}

/// R7.3: the wait timed out.
pub(crate) fn timeout_warning(waited: Duration, due_at: Option<i64>) -> String {
    let due = due_at
        .and_then(|due_at| DateTime::<Utc>::from_timestamp(due_at, 0))
        .map(|due_at| {
            format!(
                " (it was due at {})",
                due_at.to_rfc3339_opts(SecondsFormat::Secs, /*use_z*/ true)
            )
        })
        .unwrap_or_default();
    format!(
        "codex exec stopped waiting for the scheduled wakeup after {}; it is cancelled now{due}.",
        describe(waited)
    )
}

#[cfg(test)]
#[path = "wakeup_wait_tests.rs"]
mod tests;
