//! Fork: `/loop` and the display of scheduled wakeups
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC9).
//!
//! This module holds the pure parts: the `/loop` argument parser and the app events. The
//! display state lives in `chatwidget/wakeup_fork.rs`; app-server requests in
//! `app/wakeups_fork.rs`.

use crate::legacy_core::config::ScheduledWakeupsConfig;
use codex_app_server_protocol::ThreadWakeupState;
use codex_protocol::ThreadId;

/// What `/loop <args>` asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LoopCommand {
    /// `/loop`: show the shown thread's wakeup and loop.
    Show,
    /// `/loop stop`: cancel them.
    Stop,
    /// `/loop [<interval>] <prompt>`: a fixed-interval loop, or a self-paced one without an
    /// interval.
    Start {
        interval_s: Option<u32>,
        prompt: String,
    },
    /// An invalid interval or a missing prompt; no request is sent.
    Error(String),
}

/// R9.1: the first word is an interval when it reads `<N>s`, `<N>m` or `<N>h`.
pub(crate) fn parse_loop_args(args: &str, limits: &ScheduledWakeupsConfig) -> LoopCommand {
    let args = args.trim();
    if args.is_empty() {
        return LoopCommand::Show;
    }
    if args == "stop" {
        return LoopCommand::Stop;
    }
    let (first, rest) = args
        .split_once(char::is_whitespace)
        .map_or((args, ""), |(first, rest)| (first, rest.trim()));
    let Some(interval_s) = parse_interval(first) else {
        return LoopCommand::Start {
            interval_s: None,
            prompt: args.to_string(),
        };
    };
    let Some(interval_s) = interval_s.filter(|interval_s| {
        (limits.min_delay_seconds..=limits.max_delay_seconds).contains(interval_s)
    }) else {
        return LoopCommand::Error(format!(
            "The /loop interval must be from {} to {}.",
            describe_seconds(limits.min_delay_seconds),
            describe_seconds(limits.max_delay_seconds)
        ));
    };
    if rest.is_empty() {
        return LoopCommand::Error("Usage: /loop [<interval>] <prompt>".to_string());
    }
    LoopCommand::Start {
        interval_s: Some(interval_s),
        prompt: rest.to_string(),
    }
}

/// `Some(None)` for an interval-shaped word whose value overflows.
fn parse_interval(word: &str) -> Option<Option<u32>> {
    let unit = word.chars().last()?;
    let multiplier = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3_600,
        _ => return None,
    };
    let number = &word[..word.len() - 1];
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some(
        number
            .parse::<u32>()
            .ok()
            .and_then(|value| value.checked_mul(multiplier)),
    )
}

/// `45s`, `5m`, `1h`, `1h 30m`.
pub(crate) fn describe_seconds(seconds: u32) -> String {
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3_600 if seconds.is_multiple_of(60) => format!("{}m", seconds / 60),
        60..3_600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ if seconds.is_multiple_of(3_600) => format!("{}h", seconds / 3_600),
        _ => format!("{}h {}m", seconds / 3_600, (seconds % 3_600) / 60),
    }
}

/// Payload of `AppEvent::Wakeups`: `/loop` actions and the app-server answers.
#[derive(Debug)]
pub(crate) enum WakeupsEvent {
    StartLoop {
        interval_s: Option<u32>,
        prompt: String,
    },
    Stop,
    Show,
    /// Answer of `thread/loop/start`.
    LoopStarted {
        thread_id: ThreadId,
        result: Result<ThreadWakeupState, String>,
    },
    /// Answer of `thread/wakeup/cancel`: whether anything was cancelled.
    Cancelled {
        thread_id: ThreadId,
        result: Result<(bool, ThreadWakeupState), String>,
    },
    /// Answer of `thread/wakeup/read`; `show` when `/loop` alone asked for it.
    Loaded {
        thread_id: ThreadId,
        show: bool,
        result: Result<(bool, ThreadWakeupState), String>,
    },
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
