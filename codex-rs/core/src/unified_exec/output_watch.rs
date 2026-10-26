//! Fork: output watches of background unified exec processes, the Codex counterpart of the
//! Claude Code `Monitor` tool.
//!
//! Realizes UC6 of `specs/background-exec-notifications-2026-10-25.md`: with
//! `notify_on_output`, every new output line of a live process that matches a pattern is an
//! event, and Codex brings the events to the model as bounded background terminal
//! notifications (batched, rate limited, capped per watch), then reports the exit.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::Weak;
use std::time::Duration;

use codex_utils_string::truncate_middle_with_marker;
use codex_utils_string::untrusted::UntrustedBoundary;
use codex_utils_string::untrusted::escape_tags;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::NOTIFICATION_COMMAND_MAX_BYTES;
use crate::session::session::Session;

/// Longest `notify_on_output` pattern accepted, in bytes.
pub(crate) const OUTPUT_WATCH_PATTERN_MAX_BYTES: usize = 256;
/// Bytes of one output line the pattern is matched against.
pub(crate) const OUTPUT_WATCH_MATCH_MAX_BYTES: usize = 4096;
/// How long events gather after the first undelivered one.
pub(crate) const OUTPUT_WATCH_BATCH_WINDOW: Duration = Duration::from_millis(500);
/// Least time between two notifications a watch puts in the thread queue.
pub(crate) const OUTPUT_WATCH_MIN_INTERVAL: Duration = Duration::from_secs(5);
/// Matching lines one notification quotes.
pub(crate) const OUTPUT_WATCH_LINES_PER_NOTIFICATION: usize = 8;
/// Longest quoted line, in bytes.
pub(crate) const OUTPUT_WATCH_LINE_MAX_BYTES: usize = 240;
/// Notifications one watch queues before it stops.
pub(crate) const OUTPUT_WATCH_MAX_NOTIFICATIONS: usize = 10;
/// Watches one thread runs at a time.
pub(crate) const OUTPUT_WATCH_MAX_ACTIVE: usize = 8;
/// Compiled size limit of a pattern.
const OUTPUT_WATCH_REGEX_SIZE_LIMIT: usize = 1 << 20;
const OUTPUT_WATCH_REGEX_NEST_LIMIT: u32 = 64;
/// Label of the boundary lines around matching lines.
const MATCHES_LABEL: &str = "matches";
const LINE_TRUNCATION_MARKER: &str = "…";
const COMMAND_TRUNCATION_MARKER: &str = "…[truncated]…";

/// A validated `notify_on_output` pattern (R6.2).
pub(crate) struct OutputPattern {
    source: String,
    regex: regex_lite::Regex,
}

impl std::fmt::Debug for OutputPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OutputPattern").field(&self.source).finish()
    }
}

impl OutputPattern {
    /// Compiles `source`; the error is meant for the model.
    pub(crate) fn parse(source: &str) -> Result<Arc<Self>, String> {
        if source.len() > OUTPUT_WATCH_PATTERN_MAX_BYTES {
            return Err(format!(
                "notify_on_output is {} bytes long; use a pattern of at most {OUTPUT_WATCH_PATTERN_MAX_BYTES} bytes",
                source.len()
            ));
        }
        let regex = regex_lite::RegexBuilder::new(source)
            .size_limit(OUTPUT_WATCH_REGEX_SIZE_LIMIT)
            .nest_limit(OUTPUT_WATCH_REGEX_NEST_LIMIT)
            .build()
            .map_err(|err| format!("notify_on_output is not a valid regular expression: {err}"))?;
        Ok(Arc::new(Self {
            source: source.to_string(),
            regex,
        }))
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }
}

/// What a `write_stdin` call asks of the session's output watch.
#[derive(Clone, Debug)]
pub(crate) enum OutputWatchChange {
    /// Watch the session's new output with this pattern, replacing a previous watch.
    Watch(Arc<OutputPattern>),
    /// Stop the session's watch (an empty `notify_on_output`).
    Stop,
}

impl OutputWatchChange {
    /// `notify_on_output` of a `write_stdin` call; it counts only while exit notifications are
    /// enabled (R6.1) and is validated before any input is written (R6.2).
    pub(crate) fn for_write_stdin(
        session: &Session,
        value: Option<&str>,
    ) -> Result<Option<Self>, String> {
        if !session
            .features()
            .enabled(codex_features::Feature::ExecExitNotifications)
        {
            return Ok(None);
        }
        Self::parse(value)
    }

    /// Parses `notify_on_output` of `write_stdin` (R6.2, R6.4).
    pub(crate) fn parse(value: Option<&str>) -> Result<Option<Self>, String> {
        match value {
            None => Ok(None),
            Some("") => Ok(Some(Self::Stop)),
            Some(source) => OutputPattern::parse(source).map(|pattern| Some(Self::Watch(pattern))),
        }
    }
}

/// The output watch state a tool response reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OutputWatchState {
    /// The call did not touch the watch.
    #[default]
    Unchanged,
    /// The call started a watch with this pattern.
    On(String),
    /// The call stopped the watch.
    Off,
    /// The call asked for a watch, but the thread already runs the most watches.
    NotStarted,
}

/// What Codex will report about the process of a tool response (R1.2, R6.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArmedNotifications {
    /// The exit of the running process will be reported.
    pub exit: bool,
    pub output_watch: OutputWatchState,
}

/// Fork state of a process output buffer: when output last arrived (R4.5) and the watch
/// that matches new lines (R6.5).
#[derive(Default)]
pub(crate) struct OutputBuffersFork {
    pub(crate) last_output_at: Option<Instant>,
    pub(crate) watch: Option<Arc<OutputWatch>>,
}

impl OutputBuffersFork {
    /// Called for every chunk of process output under the output buffer lock.
    pub(crate) fn note_output(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        self.last_output_at = Some(Instant::now());
        if self.watch.as_ref().is_some_and(|watch| watch.is_stopped()) {
            self.watch = None;
        }
        if let Some(watch) = &self.watch {
            watch.observe_chunk(chunk);
        }
    }
}

/// Matching lines in the order the process printed them, with the count of lines that are
/// not quoted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MatchedLines {
    lines: Vec<String>,
    total: usize,
}

impl MatchedLines {
    fn is_empty(&self) -> bool {
        self.total == 0
    }

    fn push(&mut self, line: String) {
        self.total = self.total.saturating_add(1);
        if self.lines.len() < OUTPUT_WATCH_LINES_PER_NOTIFICATION {
            self.lines.push(truncate_middle_with_marker(
                &line,
                OUTPUT_WATCH_LINE_MAX_BYTES,
                LINE_TRUNCATION_MARKER,
            ));
        }
    }

    fn append(&mut self, other: Self) {
        let room = OUTPUT_WATCH_LINES_PER_NOTIFICATION.saturating_sub(self.lines.len());
        self.lines.extend(other.lines.into_iter().take(room));
        self.total = self.total.saturating_add(other.total);
    }
}

#[derive(Default)]
struct WatchState {
    /// The current unterminated line, at most `OUTPUT_WATCH_MATCH_MAX_BYTES`.
    partial: Vec<u8>,
    /// The unterminated line was already reported; its completion is not reported again.
    partial_reported: bool,
    /// The unterminated line matches and is reported when its batch is due.
    partial_candidate: bool,
    /// Events not yet in the thread queue.
    pending: MatchedLines,
    first_event_at: Option<Instant>,
    /// Events of the notification this watch put in the queue last, and its number.
    queued: Option<(MatchedLines, usize)>,
    notifications: usize,
    last_queued_at: Option<Instant>,
    /// A new notification may not be queued before this (the interval of R6.7).
    retry_at: Option<Instant>,
    stopped: bool,
}

/// When the delivery task of a watch acts next.
#[derive(Debug, PartialEq, Eq)]
enum NextDelivery {
    Stopped,
    Idle,
    At(Instant),
}

/// One output watch of one background process (UC6).
///
/// Lines are matched where output enters the process buffer, so no chunk is missed or seen
/// twice; a delivery task turns the matching lines into notifications in the thread queue.
pub(crate) struct OutputWatch {
    /// Key of this watch's notification in the thread queue.
    key: u64,
    process_id: i32,
    command: String,
    pattern: Arc<OutputPattern>,
    state: Mutex<WatchState>,
    wake: Notify,
}

impl std::fmt::Debug for OutputWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputWatch")
            .field("key", &self.key)
            .field("process_id", &self.process_id)
            .field("pattern", &self.pattern.source)
            .finish_non_exhaustive()
    }
}

impl OutputWatch {
    pub(crate) fn new(process_id: i32, command: String, pattern: Arc<OutputPattern>) -> Arc<Self> {
        Arc::new(Self {
            key: super::next_notification_key(),
            process_id,
            command,
            pattern,
            state: Mutex::new(WatchState::default()),
            wake: Notify::new(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, WatchState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn key(&self) -> u64 {
        self.key
    }

    pub(crate) fn pattern(&self) -> &str {
        self.pattern.source()
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.lock().stopped
    }

    /// Stop matching and delivering; undelivered events are dropped unless a final batch was
    /// taken first (R6.4, R6.9).
    pub(crate) fn stop(&self) {
        self.lock().stopped = true;
        self.wake.notify_one();
    }

    /// Match the complete lines of `chunk` and remember an unterminated last line (R6.5).
    pub(crate) fn observe_chunk(&self, chunk: &[u8]) {
        let mut state = self.lock();
        if state.stopped {
            return;
        }
        let mut found = false;
        for &byte in chunk {
            if byte == b'\n' || byte == b'\r' {
                let line = std::mem::take(&mut state.partial);
                let reported = std::mem::replace(&mut state.partial_reported, false);
                if !reported && let Some(text) = self.matching_line(&line) {
                    state.pending.push(text);
                    found = true;
                }
            } else if state.partial.len() < OUTPUT_WATCH_MATCH_MAX_BYTES {
                state.partial.push(byte);
            }
        }
        state.partial_candidate = !state.partial_reported
            && !state.partial.is_empty()
            && self.matching_line(&state.partial).is_some();
        if found || state.partial_candidate {
            state.first_event_at.get_or_insert_with(Instant::now);
            drop(state);
            self.wake.notify_one();
        }
    }

    fn matching_line(&self, raw: &[u8]) -> Option<String> {
        let text = clean_line(raw);
        (!text.is_empty() && self.pattern.regex.is_match(&text)).then_some(text)
    }

    fn next_delivery(&self) -> NextDelivery {
        let state = self.lock();
        if state.stopped {
            return NextDelivery::Stopped;
        }
        let Some(first_event_at) = state.first_event_at else {
            return NextDelivery::Idle;
        };
        let due = first_event_at + OUTPUT_WATCH_BATCH_WINDOW;
        NextDelivery::At(state.retry_at.map_or(due, |retry_at| retry_at.max(due)))
    }

    /// Under the thread-queue lock: the text to put in the queue now, if any (R6.7, R6.8).
    ///
    /// `queued` says whether this watch's last notification still waits in the queue; the
    /// new events are then added to it. `exited` takes the final batch when the process
    /// exited: the unterminated line counts, and neither the batch window nor the interval
    /// applies (R6.9).
    pub(crate) fn take_for_queue(
        &self,
        queued: bool,
        now: Instant,
        exited: bool,
        boundary: &UntrustedBoundary,
    ) -> Option<String> {
        let mut state = self.lock();
        // A stopped watch delivers nothing more; the exit takes its final batch before
        // stopping it.
        if state.stopped {
            return None;
        }
        let due = exited
            || state
                .first_event_at
                .is_some_and(|first| now >= first + OUTPUT_WATCH_BATCH_WINDOW);
        if !due {
            return None;
        }
        if state.partial_candidate && !state.partial_reported {
            let partial = std::mem::take(&mut state.partial);
            if let Some(text) = self.matching_line(&partial) {
                state.pending.push(text);
                state.partial_reported = true;
            }
            state.partial = partial;
            state.partial_candidate = false;
        }
        if state.pending.is_empty() {
            state.first_event_at = None;
            return None;
        }
        if !queued {
            state.queued = None;
        }
        let pending = std::mem::take(&mut state.pending);
        let (lines, number) = match state.queued.take() {
            Some((mut lines, number)) => {
                lines.append(pending);
                (lines, number)
            }
            None => {
                if !exited
                    && let Some(last) = state.last_queued_at
                    && now < last + OUTPUT_WATCH_MIN_INTERVAL
                {
                    state.pending = pending;
                    state.retry_at = Some(last + OUTPUT_WATCH_MIN_INTERVAL);
                    return None;
                }
                state.notifications = state.notifications.saturating_add(1);
                state.last_queued_at = Some(now);
                (pending, state.notifications)
            }
        };
        let last = number >= OUTPUT_WATCH_MAX_NOTIFICATIONS;
        if last {
            state.stopped = true;
        }
        state.first_event_at = None;
        state.retry_at = None;
        let text = render(
            &RenderInput {
                process_id: self.process_id,
                command: &self.command,
                pattern: self.pattern.source(),
                lines: &lines,
                number,
                exited,
                last,
            },
            boundary,
        );
        state.queued = Some((lines, number));
        Some(text)
    }
}

struct RenderInput<'a> {
    process_id: i32,
    command: &'a str,
    pattern: &'a str,
    lines: &'a MatchedLines,
    number: usize,
    exited: bool,
    last: bool,
}

/// The notification body; program output stays inside the untrusted boundary (R6.6, R6.11).
fn render(input: &RenderInput<'_>, boundary: &UntrustedBoundary) -> String {
    let RenderInput {
        process_id: id,
        command,
        pattern,
        lines,
        number,
        exited,
        last,
    } = input;
    let count = lines.total;
    let noun = if count == 1 { "line" } else { "lines" };
    let state = if *exited {
        " before it exited."
    } else {
        "; it is still running."
    };
    let mut text = vec![
        format!(
            "Background terminal session {id} printed {count} new {noun} matching the output watch \"{}\" (notification {number} of at most {OUTPUT_WATCH_MAX_NOTIFICATIONS}){state}",
            escape_tags(pattern)
        ),
        format!(
            "Command: {}",
            escape_tags(&truncate_middle_with_marker(
                command,
                NOTIFICATION_COMMAND_MAX_BYTES,
                COMMAND_TRUNCATION_MARKER
            ))
        ),
        boundary.block(
            MATCHES_LABEL,
            "Matching output",
            "untrusted program output",
            &lines.lines.join("\n"),
        ),
    ];
    let unshown = count.saturating_sub(lines.lines.len());
    if unshown > 0 {
        text.push(format!(
            "{unshown} more matching {} not shown; write_stdin with session_id {id} returns the output you have not read yet.",
            if unshown == 1 { "line is" } else { "lines are" }
        ));
    }
    if *last && !*exited {
        text.push(format!(
            "This was the last notification of this output watch; call write_stdin with session_id {id} and notify_on_output to watch again. The exit is still reported."
        ));
    }
    text.join("\n")
}

/// Text of one output line as the watch matches it: ANSI escape sequences and other control
/// characters removed and trailing whitespace trimmed (R6.5).
fn clean_line(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(current) = chars.next() {
        match current {
            '\u{1b}' => match chars.next() {
                // CSI: parameters and intermediates up to a final byte in `@..=~`.
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC: up to BEL or the string terminator `ESC \`.
                Some(']') => {
                    while let Some(next) = chars.next() {
                        if next == '\u{7}' {
                            break;
                        }
                        if next == '\u{1b}' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                // Any other escape is two characters long.
                _ => {}
            },
            '\t' => out.push(current),
            control if control.is_control() => {}
            other => out.push(other),
        }
    }
    out.truncate(out.trim_end().len());
    out
}

/// Turns the events of `watch` into notifications until the watch stops or the process exits.
/// The exit itself delivers the final batch (`ExitWatch::report`).
pub(crate) fn spawn_delivery(
    watch: Arc<OutputWatch>,
    session: &Arc<Session>,
    exit_token: CancellationToken,
) {
    let session: Weak<Session> = Arc::downgrade(session);
    tokio::spawn(async move {
        loop {
            let notified = watch.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            match watch.next_delivery() {
                NextDelivery::Stopped => return,
                NextDelivery::Idle => {
                    tokio::select! {
                        () = &mut notified => {}
                        () = exit_token.cancelled() => return,
                    }
                }
                NextDelivery::At(at) if at > Instant::now() => {
                    tokio::select! {
                        () = tokio::time::sleep_until(at) => {}
                        () = &mut notified => {}
                        () = exit_token.cancelled() => return,
                    }
                }
                NextDelivery::At(_) => {
                    let Some(session) = session.upgrade() else {
                        return;
                    };
                    session
                        .deliver_output_watch_notification(&watch, /*exited*/ false)
                        .await;
                    // Nothing was due (the interval holds the batch back): wait for news.
                    if matches!(watch.next_delivery(), NextDelivery::At(at) if at <= Instant::now())
                    {
                        tokio::select! {
                            () = &mut notified => {}
                            () = exit_token.cancelled() => return,
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "output_watch_tests.rs"]
mod tests;
