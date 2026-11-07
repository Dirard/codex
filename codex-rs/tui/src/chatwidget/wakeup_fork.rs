//! Fork: scheduled wakeups in the chat widget (`specs/scheduled-wakeups-2026-11-07.md`, UC9).
//!
//! The status line shows the shown thread's pending wakeup or `/loop` (R9.2); a fired
//! wakeup, a loop's start and end and a wakeup cancelled other than by the model become one
//! info cell each, only from live notifications (R9.3, D5). `/loop` sends its requests
//! through `AppEvent::Wakeups` (R9.1, R9.4).

use chrono::DateTime;
use chrono::Utc;
use codex_app_server_protocol::ThreadWakeupCancelReason;
use codex_app_server_protocol::ThreadWakeupChange;
use codex_app_server_protocol::ThreadWakeupOrigin;
use codex_app_server_protocol::ThreadWakeupState;
use codex_app_server_protocol::ThreadWakeupUpdatedNotification;
use codex_features::Feature;
use codex_protocol::ThreadId;

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::wakeups::LoopCommand;
use crate::wakeups::WakeupsEvent;
use crate::wakeups::describe_seconds;
use crate::wakeups::parse_loop_args;

/// The shown thread's wakeup state, as the last notification or read left it.
#[derive(Debug, Default)]
pub(super) struct WakeupState {
    state: Option<ThreadWakeupState>,
}

impl ChatWidget {
    /// R9.1: `/loop` exists only while `features.scheduled_wakeups` is enabled.
    pub(crate) fn fork_sync_wakeups_command(&mut self) {
        self.bottom_pane
            .set_wakeups_command_enabled(self.config.features.enabled(Feature::ScheduledWakeups));
    }

    /// R9.1, R9.4: `/loop [<interval>] <prompt>`, `/loop stop` and `/loop`, also during a turn.
    pub(super) fn fork_loop_command(&mut self, args: &str) {
        if !self.config.features.enabled(Feature::ScheduledWakeups) {
            return;
        }
        let event = match parse_loop_args(args, &self.config.scheduled_wakeups) {
            LoopCommand::Show => WakeupsEvent::Show,
            LoopCommand::Stop => WakeupsEvent::Stop,
            LoopCommand::Start { interval_s, prompt } => {
                WakeupsEvent::StartLoop { interval_s, prompt }
            }
            LoopCommand::Error(message) => {
                self.add_error_message(message);
                return;
            }
        };
        self.app_event_tx.send(AppEvent::Wakeups(event));
    }

    /// R9.2, R9.3: follow the shown thread's wakeup changes.
    pub(super) fn on_wakeup_updated(&mut self, notification: ThreadWakeupUpdatedNotification) {
        if self
            .thread_id
            .is_some_and(|thread_id| thread_id.to_string() != notification.thread_id)
        {
            return;
        }
        let previous = self.workflows.wakeups.state.take();
        if let Some(text) = history_cell_text(&notification, previous.as_ref()) {
            self.add_info_message(text, /*hint*/ None);
        }
        self.workflows.wakeups.state = Some(notification.state);
        self.fork_refresh_status_indicator();
    }

    /// The answer of `thread/loop/start`.
    pub(crate) fn on_loop_started(
        &mut self,
        thread_id: ThreadId,
        result: Result<ThreadWakeupState, String>,
    ) {
        match result {
            Ok(state) => self.set_wakeup_state(thread_id, state),
            Err(error) => self.add_error_message(format!("Could not start the /loop: {error}")),
        }
    }

    /// The answer of `thread/wakeup/cancel` (R4.2).
    pub(crate) fn on_wakeup_cancelled(
        &mut self,
        thread_id: ThreadId,
        result: Result<(bool, ThreadWakeupState), String>,
    ) {
        match result {
            Ok((cancelled, state)) => {
                if !cancelled {
                    self.add_info_message(
                        "No scheduled wakeup or /loop to stop.".to_string(),
                        /*hint*/ None,
                    );
                }
                self.set_wakeup_state(thread_id, state);
            }
            Err(error) => {
                self.add_error_message(format!("Could not stop the /loop: {error}"));
            }
        }
    }

    /// The answer of `thread/wakeup/read`; `/loop` alone shows it.
    pub(crate) fn on_wakeup_loaded(
        &mut self,
        thread_id: ThreadId,
        show: bool,
        result: Result<(bool, ThreadWakeupState), String>,
    ) {
        match result {
            Ok((available, state)) => {
                if show {
                    self.add_info_message(describe_state(available, &state), /*hint*/ None);
                }
                self.set_wakeup_state(thread_id, state);
            }
            Err(error) if show => {
                self.add_error_message(format!("Could not read the scheduled wakeups: {error}"));
            }
            Err(_) => {}
        }
    }

    fn set_wakeup_state(&mut self, thread_id: ThreadId, state: ThreadWakeupState) {
        if self.thread_id.is_some_and(|shown| shown != thread_id) {
            return;
        }
        self.workflows.wakeups.state = Some(state);
        self.fork_refresh_status_indicator();
    }

    /// The status line's fork part: workflow runs and the wakeup, side by side.
    pub(super) fn fork_refresh_status_indicator(&mut self) {
        let workflow = super::workflow::workflow_status_indicator(&self.workflows.active);
        let wakeup = self
            .workflows
            .wakeups
            .state
            .as_ref()
            .and_then(wakeup_status_indicator);
        let indicator = match (workflow, wakeup) {
            (Some(workflow), Some(wakeup)) => Some(format!("{workflow} | {wakeup}")),
            (workflow, wakeup) => workflow.or(wakeup),
        };
        self.bottom_pane.set_workflow_status_indicator(indicator);
    }
}

/// `12:05:00Z`
fn clock(unix_seconds: i64) -> String {
    DateTime::<Utc>::from_timestamp(unix_seconds, 0)
        .map(|at| at.format("%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// Control characters of model-authored text become `?` (R10.4).
fn sanitize(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '?' } else { ch })
        .collect()
}

/// R9.2: `⏰ wakeup at 12:05:00Z · CI still running` or `⏰ /loop 3/50 · next at 12:05:00Z`.
/// The due time is absolute, so the line stays right without a redraw timer.
pub(super) fn wakeup_status_indicator(state: &ThreadWakeupState) -> Option<String> {
    if let Some(loop_state) = &state.loop_state {
        let mut text = format!(
            "⏰ /loop {}/{}",
            loop_state.iteration, loop_state.max_iterations
        );
        match &state.wakeup {
            Some(wakeup) if wakeup.due => text.push_str(" · next when idle"),
            Some(wakeup) => text.push_str(&format!(" · next at {}", clock(wakeup.due_at))),
            None => {}
        }
        return Some(text);
    }
    let wakeup = state.wakeup.as_ref()?;
    let mut text = if wakeup.due {
        "⏰ wakeup due, waiting for the turn to end".to_string()
    } else {
        format!("⏰ wakeup at {}", clock(wakeup.due_at))
    };
    if let Some(reason) = &wakeup.reason {
        text.push_str(&format!(" · {}", sanitize(reason)));
    }
    Some(text)
}

/// R9.3: the info cell of a change, if it gets one.
fn history_cell_text(
    notification: &ThreadWakeupUpdatedNotification,
    previous: Option<&ThreadWakeupState>,
) -> Option<String> {
    let previous_wakeup = previous.and_then(|state| state.wakeup.as_ref());
    let previous_loop = previous.and_then(|state| state.loop_state.as_ref());
    match notification.change {
        ThreadWakeupChange::Fired => {
            let wakeup = previous_wakeup?;
            match (wakeup.origin, &notification.state.loop_state) {
                (ThreadWakeupOrigin::Loop, Some(loop_state)) => Some(format!(
                    "⏰ /loop iteration {} of at most {}",
                    loop_state.iteration, loop_state.max_iterations
                )),
                _ => Some(match &wakeup.reason {
                    Some(reason) => format!("⏰ Scheduled wakeup fired: {}", sanitize(reason)),
                    None => "⏰ Scheduled wakeup fired".to_string(),
                }),
            }
        }
        ThreadWakeupChange::LoopStarted => {
            let loop_state = notification.state.loop_state.as_ref()?;
            let pace = match loop_state.interval_seconds {
                Some(interval) => format!("every {}", describe_seconds(interval)),
                None => "at the model's pace".to_string(),
            };
            Some(format!(
                "⏰ /loop started: {pace}, at most {} iterations",
                loop_state.max_iterations
            ))
        }
        ThreadWakeupChange::LoopEnded => {
            let iterations = previous_loop.map_or(0, |state| state.iteration);
            let cause = match notification.cancel_reason {
                Some(ThreadWakeupCancelReason::Completed) => {
                    "the model did not schedule the next one"
                }
                Some(ThreadWakeupCancelReason::Limit) => "it reached its limit",
                Some(ThreadWakeupCancelReason::User) => "you stopped it",
                Some(ThreadWakeupCancelReason::Interrupt) => "you interrupted its turn",
                Some(ThreadWakeupCancelReason::Failures) => "its turns failed three times in a row",
                Some(ThreadWakeupCancelReason::Replaced) => "a new /loop replaced it",
                Some(ThreadWakeupCancelReason::Model) => "the model stopped it",
                Some(ThreadWakeupCancelReason::ThreadClosed) | None => "the thread closed",
            };
            Some(format!(
                "⏰ /loop ended after {iterations} iterations: {cause}"
            ))
        }
        ThreadWakeupChange::Cancelled => {
            let cause = match notification.cancel_reason? {
                ThreadWakeupCancelReason::Model => return None,
                ThreadWakeupCancelReason::User => "you cancelled it",
                ThreadWakeupCancelReason::Interrupt => "you interrupted its turn",
                ThreadWakeupCancelReason::Failures => "its turns failed three times in a row",
                ThreadWakeupCancelReason::Replaced => "a /loop replaced it",
                ThreadWakeupCancelReason::Limit => "it reached its limit",
                ThreadWakeupCancelReason::Completed => "it completed",
                ThreadWakeupCancelReason::ThreadClosed => "the thread closed",
            };
            Some(format!("⏰ Scheduled wakeup cancelled: {cause}"))
        }
        ThreadWakeupChange::Scheduled | ThreadWakeupChange::Replaced => None,
    }
}

/// `/loop` alone: what the shown thread has scheduled.
fn describe_state(available: bool, state: &ThreadWakeupState) -> String {
    if !available {
        return "Scheduled wakeups are not available for this thread.".to_string();
    }
    match wakeup_status_indicator(state) {
        Some(status) => {
            let mut text = status;
            if let Some(loop_state) = &state.loop_state {
                text.push_str(&format!(" · prompt: {}", sanitize(&loop_state.prompt)));
            }
            text.push_str(" · /loop stop cancels it");
            text
        }
        None => "No scheduled wakeup or /loop on this thread. Usage: /loop [<interval>] <prompt>"
            .to_string(),
    }
}
