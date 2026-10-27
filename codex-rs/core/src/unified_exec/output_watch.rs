//! Fork: output watches of background unified exec processes, the Codex counterpart of the
//! Claude Code `Monitor` tool.
//!
//! Realizes UC6 of `specs/background-exec-notifications-2026-10-25.md`: with
//! `notify_on_output`, every new output line of a live process that matches a pattern is an
//! event, and Codex brings the events to the model as bounded background terminal
//! notifications (batched, rate limited, capped per watch), then reports the exit.
//!
//! The output path only splits chunks into lines under the output buffer lock. The pattern is
//! matched by the watch's delivery task on the blocking thread pool, outside every lock, so
//! an expensive pattern cannot stall the async runtime, the output buffer or the thread queue
//! (review 2026-10-27, P2-1).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;
use std::sync::Weak;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;

use codex_utils_output_truncation::OutputTruncation;
use codex_utils_string::truncate_middle_with_marker;
use codex_utils_string::untrusted::UntrustedBoundary;
use codex_utils_string::untrusted::escape_tags;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::ExitPreference;
use super::NOTIFICATION_COMMAND_MAX_BYTES;
use crate::session::session::Session;

/// Longest `notify_on_output` pattern accepted, in bytes.
pub(crate) const OUTPUT_WATCH_PATTERN_MAX_BYTES: usize = 256;
/// Bytes of one output line the pattern is matched against.
pub(crate) const OUTPUT_WATCH_MATCH_MAX_BYTES: usize = 4096;
/// How long events gather after the first undelivered one; an unterminated line that stays
/// unchanged this long is matched as it is.
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
/// Output lines that wait to be matched, in bytes; older lines beyond it are dropped unchecked
/// and counted, so a process that prints faster than the pattern matches cannot grow memory
/// or the work behind it (P2-1).
pub(crate) const OUTPUT_WATCH_BACKLOG_MAX_BYTES: usize = 64 * 1024;
/// How long a watch alone keeps `codex exec`, `wait_agent` and agent residency waiting
/// (review 2026-10-27, P2-3; `notify_on_exit` waits for the exit instead). Before its first
/// matching line the model waits for that line, such as a server's "ready", from the end of
/// the turn that started the watch: `first_match`. After a matching line it waits only while
/// more keep coming: `quiet` (tails 2026-10-27). `[features.exec_exit_notifications]
/// output_watch_first_match_ms` and `output_watch_quiet_ms` set them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputWatchWindows {
    pub(crate) first_match: Duration,
    pub(crate) quiet: Duration,
    /// How long `codex exec` waits for an exit that `notify_on_exit` asks for; the watch's
    /// notification names it (live recheck 2026-10-27, N1).
    pub(crate) exec_wait: Duration,
}

impl OutputWatchWindows {
    pub(crate) fn from_config(config: &crate::config::ExecExitNotificationsConfig) -> Self {
        Self {
            first_match: Duration::from_millis(config.output_watch_first_match_ms),
            quiet: Duration::from_millis(config.output_watch_quiet_ms),
            exec_wait: Duration::from_millis(config.exec_wait_timeout_ms),
        }
    }
}

/// How much longer an output watch keeps its process awaited without news (R6.14).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchWait {
    /// No match yet, and the turn that started the watch still runs: the wait for the first
    /// match lasts this long after that turn ends.
    FirstMatchAfterTurn(Duration),
    /// No match yet: this much of the wait for the first match is left.
    FirstMatch(Duration),
    /// After a match: this much is left until the watch counts as quiet.
    Quiet(Duration),
}

/// A remaining wait as the model reads it: "about 3 minutes", "about a minute", "40 seconds".
pub(crate) fn describe_remaining(left: Duration) -> String {
    let seconds = left.as_secs();
    if seconds >= 90 {
        format!("about {} minutes", (seconds + 30) / 60)
    } else if seconds >= 60 {
        "about a minute".to_string()
    } else {
        let seconds = seconds.max(1);
        format!("{seconds} second{}", if seconds == 1 { "" } else { "s" })
    }
}

/// A wait as the model reads it: "5 minutes", "90 seconds", "1.5 seconds".
pub(crate) fn describe_wait(wait: Duration) -> String {
    let seconds = wait.as_secs();
    if wait.subsec_millis() == 0 && seconds >= 60 && seconds.is_multiple_of(60) {
        let minutes = seconds / 60;
        format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" })
    } else if wait.subsec_millis() == 0 {
        format!("{seconds} second{}", if seconds == 1 { "" } else { "s" })
    } else {
        format!("{:.1} seconds", wait.as_secs_f64())
    }
}

impl Default for OutputWatchWindows {
    fn default() -> Self {
        Self::from_config(&crate::config::ExecExitNotificationsConfig::default())
    }
}
/// Compiled size limit of a pattern (`regex` NFA bytes). The lazy DFA matches realistic
/// patterns in microseconds per line; when a pattern makes the DFA give up, the fallback works
/// per byte in proportion to this size, so it bounds the cost of one line: about 40 ms for a
/// 4 KiB line at the limit in a release build (P2-1; Decision log of
/// `specs/background-exec-notifications-2026-10-25.md`, 2026-10-27 tails). Two Unicode classes
/// such as `\w+@\w+` fit; large counted repetitions of `.` or `\w`, such as `.{0,255}`, fit only
/// with ASCII classes (`OutputPattern::parse`).
pub(crate) const OUTPUT_WATCH_REGEX_SIZE_LIMIT: usize = 128 * 1024;
/// Cache of the lazy DFA of one pattern; past it, the DFA falls back as above.
const OUTPUT_WATCH_REGEX_DFA_SIZE_LIMIT: usize = 512 * 1024;
const OUTPUT_WATCH_REGEX_NEST_LIMIT: u32 = 64;
/// The error for a pattern over `OUTPUT_WATCH_REGEX_SIZE_LIMIT` even with ASCII classes.
const OUTPUT_PATTERN_TOO_COSTLY: &str = r"notify_on_output is too costly to match on every output line, even with ASCII classes; use a simpler pattern: + or * instead of large or nested counted repetitions such as (?:[ -~]?){1000} or (?:(?:.?){20}){20}";
/// The error for a pattern with non-ASCII text over `OUTPUT_WATCH_REGEX_SIZE_LIMIT`: ASCII
/// classes would change what it matches (review of the fixes, P3-4).
const OUTPUT_PATTERN_TOO_COSTLY_NON_ASCII: &str = r"notify_on_output is too costly to match on every output line with Unicode classes, and it has non-ASCII text, which ASCII classes would not match the same way; use + or * instead of large counted repetitions such as .{0,255}, and fewer Unicode classes such as \w or \b";
/// Label of the boundary lines around matching lines.
const MATCHES_LABEL: &str = "matches";
const LINE_TRUNCATION_MARKER: &str = "…";
const COMMAND_TRUNCATION_MARKER: &str = "…[truncated]…";
/// First line of an output watch notification, after the session ID.
const OUTPUT_WATCH_HEADLINE_MARK: &str = " matching the output watch \"";
/// How a watch notification says to stop the process: through its own session, never by name
/// (live recheck 2026-10-27, N3).
const STOP_GUIDANCE_PREFIX: &str =
    "To stop it, call write_stdin with chars \"\\u0003\" (Ctrl-C) and session_id ";
const STOP_GUIDANCE_SUFFIX: &str =
    "; do not kill it by name (pkill, killall), which can stop processes of other sessions.";

/// A validated `notify_on_output` pattern (R6.2) in `regex` syntax: Unicode-aware unless that
/// makes it too large, and matched in time linear in the line.
pub(crate) struct OutputPattern {
    source: String,
    regex: PatternRegex,
}

/// How a pattern matches: with Unicode classes, or with ASCII classes when Unicode classes
/// make it too large (tails review 2026-10-27, P3-9).
enum PatternRegex {
    Unicode(regex::Regex),
    Ascii(regex::bytes::Regex),
}

impl PatternRegex {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Self::Unicode(regex) => regex.is_match(line),
            Self::Ascii(regex) => regex.is_match(line.as_bytes()),
        }
    }
}

impl std::fmt::Debug for OutputPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("OutputPattern").field(&self.source).finish()
    }
}

impl OutputPattern {
    /// Compiles `source`; the error is meant for the model.
    ///
    /// Unicode classes make a pattern large: `\w` alone is about 49 KiB compiled. A pattern
    /// over `OUTPUT_WATCH_REGEX_SIZE_LIMIT` with them, such as `\w+@\w+\.\w+` or `.{0,255}Z`
    /// that `regex-lite` used to accept, is compiled again with ASCII classes (`unicode(false)`
    /// over the line's bytes, the semantics of `regex-lite`), under the same limits, so its
    /// cost per line stays bounded the same way, and the tool output says so. Only a pattern
    /// written entirely in ASCII gets that second chance: with non-ASCII text, ASCII classes
    /// change what it matches (`(?i)` no longer folds Cyrillic case, `\b` no longer sees
    /// Cyrillic word boundaries), so it is refused with advice instead (review of the fixes,
    /// P3-4).
    pub(crate) fn parse(source: &str) -> Result<Arc<Self>, String> {
        if source.len() > OUTPUT_WATCH_PATTERN_MAX_BYTES {
            return Err(format!(
                "notify_on_output is {} bytes long; use a pattern of at most {OUTPUT_WATCH_PATTERN_MAX_BYTES} bytes",
                source.len()
            ));
        }
        let regex = match regex::RegexBuilder::new(source)
            .size_limit(OUTPUT_WATCH_REGEX_SIZE_LIMIT)
            .dfa_size_limit(OUTPUT_WATCH_REGEX_DFA_SIZE_LIMIT)
            .nest_limit(OUTPUT_WATCH_REGEX_NEST_LIMIT)
            .build()
        {
            Ok(regex) => PatternRegex::Unicode(regex),
            Err(regex::Error::CompiledTooBig(_)) if !source.is_ascii() => {
                return Err(OUTPUT_PATTERN_TOO_COSTLY_NON_ASCII.to_string());
            }
            Err(regex::Error::CompiledTooBig(_)) => regex::bytes::RegexBuilder::new(source)
                .unicode(false)
                .size_limit(OUTPUT_WATCH_REGEX_SIZE_LIMIT)
                .dfa_size_limit(OUTPUT_WATCH_REGEX_DFA_SIZE_LIMIT)
                .nest_limit(OUTPUT_WATCH_REGEX_NEST_LIMIT)
                .build()
                .map(PatternRegex::Ascii)
                .map_err(|_| OUTPUT_PATTERN_TOO_COSTLY.to_string())?,
            Err(err) => {
                return Err(format!(
                    "notify_on_output is not a valid regular expression: {err}"
                ));
            }
        };
        Ok(Arc::new(Self {
            source: source.to_string(),
            regex,
        }))
    }

    pub(crate) fn source(&self) -> &str {
        self.source.as_str()
    }

    /// The pattern was too large with Unicode classes and is matched with ASCII classes.
    pub(crate) fn ascii_classes(&self) -> bool {
        matches!(self.regex, PatternRegex::Ascii(_))
    }
}

/// What a `write_stdin` call asks of the session's notifications.
#[derive(Clone, Debug, Default)]
pub(crate) struct NotificationChange {
    /// `notify_on_output`.
    pub(crate) output_watch: Option<OutputWatchChange>,
    /// `notify_on_exit`: wait for the exit again, or no longer (R6.15, live recheck N1).
    pub(crate) notify_on_exit: Option<bool>,
}

impl NotificationChange {
    /// The notification arguments of a `write_stdin` call; they count only while exit
    /// notifications are enabled (R6.1), and `notify_on_output` is validated before any input
    /// is written (R6.2).
    pub(crate) fn for_write_stdin(
        session: &Session,
        notify_on_output: Option<&str>,
        stop_output_watch: bool,
        notify_on_exit: Option<bool>,
    ) -> Result<Self, String> {
        if !session
            .features()
            .enabled(codex_features::Feature::ExecExitNotifications)
        {
            return Ok(Self::default());
        }
        Ok(Self {
            output_watch: OutputWatchChange::parse(notify_on_output, stop_output_watch)?,
            notify_on_exit,
        })
    }
}

/// What a `write_stdin` call asks of the session's output watch.
#[derive(Clone, Debug)]
pub(crate) enum OutputWatchChange {
    /// Watch the session's new output with this pattern, replacing a previous watch.
    Watch(Arc<OutputPattern>),
    /// Stop the session's watch (`stop_output_watch: true`).
    Stop,
}

impl OutputWatchChange {
    /// Parses `notify_on_output` and `stop_output_watch` of `write_stdin` (R6.2, R6.4). An empty
    /// `notify_on_output` changes nothing: a model that turned `notify_on_exit` off passed it
    /// along and lost its error watch, so stopping a watch takes the explicit
    /// `stop_output_watch` (live recheck 2 R5).
    pub(crate) fn parse(value: Option<&str>, stop: bool) -> Result<Option<Self>, String> {
        match (value, stop) {
            (Some(source), true) if !source.is_empty() => Err(
                "notify_on_output starts a watch and stop_output_watch stops it; pass only one"
                    .to_string(),
            ),
            (_, true) => Ok(Some(Self::Stop)),
            (None | Some(""), false) => Ok(None),
            (Some(source), false) => {
                OutputPattern::parse(source).map(|pattern| Some(Self::Watch(pattern)))
            }
        }
    }
}

/// Why a requested watch did not start (R6.8, review 2026-10-27 P3-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputWatchRefusal {
    /// The thread already runs `OUTPUT_WATCH_MAX_ACTIVE` watches.
    Limit,
    /// The process was started while exit notifications were disabled.
    NoNotifications,
    /// The process exited before the watch could start.
    Exited,
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
    /// The call asked for a watch that did not start.
    NotStarted(OutputWatchRefusal),
}

/// What Codex will report about the process of a tool response (R1.2, R6.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArmedNotifications {
    /// The exit of the running process will be reported.
    pub exit: bool,
    /// `notify_on_exit` is off because the model turned it off; `Some(true)` when the output
    /// watch still reports the exit (live recheck 2026-10-27, N1).
    pub exit_off: Option<bool>,
    pub output_watch: OutputWatchState,
    /// The watch the call started matches with ASCII classes (review of the fixes, P3-4).
    pub output_watch_ascii: bool,
}

/// Fork state of a process output buffer: when output last arrived (R4.5) and the watch
/// that matches new lines (R6.5).
#[derive(Default)]
pub(crate) struct OutputBuffersFork {
    pub(crate) last_output_at: Option<Instant>,
    pub(crate) watch: Option<Arc<OutputWatch>>,
}

impl OutputBuffersFork {
    /// Called for every chunk of process output under the output buffer lock; it only splits
    /// lines, the pattern is matched later outside the lock.
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

/// How much of a `write_stdin` response the model sees whole (review 2026-10-27, P3-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PollBudget {
    bytes: usize,
    lines: Option<usize>,
}

impl PollBudget {
    pub(crate) fn new(max_output_tokens: Option<usize>, truncation: &OutputTruncation) -> Self {
        let requested = codex_utils_string::approx_bytes_for_tokens(
            crate::unified_exec::resolve_max_tokens(max_output_tokens),
        );
        Self {
            bytes: requested.min(truncation.policy.byte_budget()),
            lines: truncation.max_lines,
        }
    }
}

/// Matching lines in the order the process printed them, with the count of lines that are
/// not quoted. `ends` holds the stream offset after each quoted line, `last_end` the one after
/// the last line, so lines that a poll showed can be told apart (P3-2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MatchedLines {
    lines: Vec<String>,
    ends: Vec<u64>,
    total: usize,
    last_end: u64,
}

impl MatchedLines {
    fn is_empty(&self) -> bool {
        self.total == 0
    }

    fn push(&mut self, line: String, end: u64) {
        self.total = self.total.saturating_add(1);
        self.last_end = self.last_end.max(end);
        if self.lines.len() < OUTPUT_WATCH_LINES_PER_NOTIFICATION {
            self.lines.push(truncate_middle_with_marker(
                &line,
                OUTPUT_WATCH_LINE_MAX_BYTES,
                LINE_TRUNCATION_MARKER,
            ));
            self.ends.push(end);
        }
    }

    fn append(&mut self, other: Self) {
        let room = OUTPUT_WATCH_LINES_PER_NOTIFICATION.saturating_sub(self.lines.len());
        self.lines.extend(other.lines.into_iter().take(room));
        self.ends.extend(other.ends.into_iter().take(room));
        self.total = self.total.saturating_add(other.total);
        self.last_end = self.last_end.max(other.last_end);
    }

    /// Forget the lines that end at or before `upto`. Unquoted lines have no offset, so
    /// they stay counted unless every line was read.
    fn drop_read(&mut self, upto: u64) {
        if self.last_end <= upto {
            *self = Self::default();
            return;
        }
        let read = self.ends.iter().take_while(|end| **end <= upto).count();
        self.lines.drain(..read);
        self.ends.drain(..read);
        self.total = self.total.saturating_sub(read);
    }
}

/// An output line waiting to be matched, and the stream offset right after it.
#[derive(Debug)]
struct RawLine {
    bytes: Vec<u8>,
    end: u64,
}

/// Where the unterminated line stands with the pattern.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PartialCheck {
    /// Not matched since it last changed.
    #[default]
    Unchecked,
    /// Being matched now.
    InFlight,
    /// Matched as it is now, without a match: its completion needs no new check.
    NoMatch,
    /// It matched and was reported; its completion is not reported again.
    Matched,
}

#[derive(Default)]
struct WatchState {
    /// The current unterminated line, at most `OUTPUT_WATCH_MATCH_MAX_BYTES`.
    partial: Vec<u8>,
    partial_changed_at: Option<Instant>,
    partial_check: PartialCheck,
    /// Counts completed lines; a check of an earlier unterminated line is discarded.
    line_generation: u64,
    /// Complete lines waiting to be matched.
    backlog: VecDeque<RawLine>,
    backlog_bytes: usize,
    /// Lines dropped from a full backlog without a check, reported with the next notification.
    unchecked: usize,
    /// Bytes this watch has seen, the origin of line offsets.
    observed: u64,
    /// Bytes up to which the last `write_stdin` response drained the output, and
    /// `line_generation` then.
    polled_upto: u64,
    polled_generation: u64,
    /// Bytes up to which a `write_stdin` response showed the output whole; matching lines
    /// before it are no news (P3-2).
    read_upto: u64,
    /// Matching lines not yet in the thread queue.
    pending: MatchedLines,
    first_event_at: Option<Instant>,
    /// Events of the notification this watch put in the queue last, and its number.
    queued: Option<(MatchedLines, usize)>,
    notifications: usize,
    last_queued_at: Option<Instant>,
    /// A new notification may not be queued before this (the interval of R6.7).
    retry_at: Option<Instant>,
    /// When the watch last found a matching line (P2-3).
    last_match_at: Option<Instant>,
    /// When the turn that started the watch ended; the wait for the first matching line
    /// counts from then (tails 2026-10-27).
    turn_ended_at: Option<Instant>,
    stopped: bool,
}

impl WatchState {
    fn extend_partial(&mut self, bytes: &[u8], now: Instant) {
        let room = OUTPUT_WATCH_MATCH_MAX_BYTES.saturating_sub(self.partial.len());
        let taken = bytes.len().min(room);
        if taken == 0 {
            return;
        }
        self.partial.extend_from_slice(&bytes[..taken]);
        self.partial_changed_at = Some(now);
        if self.partial_check != PartialCheck::Matched {
            self.partial_check = PartialCheck::Unchecked;
        }
    }

    /// The unterminated line ended at stream offset `end`.
    fn complete_line(&mut self, end: u64) {
        let line = std::mem::take(&mut self.partial);
        let check = std::mem::take(&mut self.partial_check);
        self.partial_changed_at = None;
        self.line_generation = self.line_generation.wrapping_add(1);
        if line.is_empty() || matches!(check, PartialCheck::Matched | PartialCheck::NoMatch) {
            return;
        }
        while self.backlog_bytes + line.len() > OUTPUT_WATCH_BACKLOG_MAX_BYTES {
            let Some(dropped) = self.backlog.pop_front() else {
                break;
            };
            self.backlog_bytes -= dropped.bytes.len();
            self.unchecked = self.unchecked.saturating_add(1);
        }
        self.backlog_bytes += line.len();
        self.backlog.push_back(RawLine { bytes: line, end });
    }

    fn partial_due(&self, now: Instant, final_batch: bool) -> bool {
        !self.partial.is_empty()
            && self.partial_check == PartialCheck::Unchecked
            && (final_batch
                || self
                    .partial_changed_at
                    .is_some_and(|changed| now >= changed + OUTPUT_WATCH_BATCH_WINDOW))
    }
}

/// Lines to match, taken out of the watch state.
struct MatchJob {
    lines: Vec<RawLine>,
    partial: Option<RawLine>,
    generation: u64,
}

/// What a match job found.
struct MatchResult {
    matches: Vec<(String, u64)>,
    /// The verdict on the unterminated line, if the job checked it.
    partial: Option<Option<(String, u64)>>,
    generation: u64,
    evaluated: usize,
}

impl MatchJob {
    /// Pure matching work; runs on the blocking thread pool.
    fn run(self, pattern: &OutputPattern) -> MatchResult {
        let mut evaluated = 0usize;
        let mut check = |line: &RawLine| {
            evaluated += 1;
            let text = clean_line(&line.bytes);
            (!text.is_empty() && pattern.regex.is_match(&text)).then_some((text, line.end))
        };
        let matches = self.lines.iter().filter_map(&mut check).collect();
        let partial = self.partial.as_ref().map(&mut check);
        MatchResult {
            matches,
            partial,
            generation: self.generation,
            evaluated,
        }
    }
}

/// When the delivery task of a watch acts next.
#[derive(Debug, PartialEq, Eq)]
enum NextDelivery {
    Stopped,
    Idle,
    /// Complete lines wait to be matched.
    Match,
    /// An unterminated line becomes quiet, or a batch becomes due.
    At(Instant),
}

/// One output watch of one background process (UC6).
///
/// Lines are split where output enters the process buffer, so no chunk is missed or seen
/// twice; the delivery task matches them outside every lock and turns the matching lines into
/// notifications in the thread queue.
pub(crate) struct OutputWatch {
    /// Key of this watch's notification in the thread queue.
    key: u64,
    process_id: i32,
    command: String,
    pattern: Arc<OutputPattern>,
    state: Mutex<WatchState>,
    wake: Notify,
    /// One match job at a time, so the final batch at the exit sees every line.
    matching: tokio::sync::Mutex<()>,
    /// Lines the pattern was evaluated on (tests count the work).
    evaluated: AtomicUsize,
    started_at: Instant,
    /// The turn that started the watch; only its end starts the wait for the first matching
    /// line (tails review 2026-10-27, P3-7).
    turn_id: String,
    /// How long the watch stays fresh without a matching line (P2-3).
    windows: OutputWatchWindows,
    /// `notify_on_exit` of the process (`ExitNotifier::exit_preference`): while it is on, the
    /// notification says what waits for the exit and how to turn that off (N1); once the model
    /// turned it off, the last notification does not promise the exit (review of the fixes,
    /// P3-5).
    exit: Arc<ExitPreference>,
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
    #[cfg(test)]
    pub(crate) fn new(process_id: i32, command: String, pattern: Arc<OutputPattern>) -> Arc<Self> {
        Self::with_windows(
            process_id,
            command,
            pattern,
            "turn".to_string(),
            OutputWatchWindows::default(),
            Arc::default(),
        )
    }

    pub(crate) fn with_windows(
        process_id: i32,
        command: String,
        pattern: Arc<OutputPattern>,
        turn_id: String,
        windows: OutputWatchWindows,
        exit: Arc<ExitPreference>,
    ) -> Arc<Self> {
        Arc::new(Self {
            key: super::next_notification_key(),
            process_id,
            command,
            pattern,
            state: Mutex::new(WatchState::default()),
            wake: Notify::new(),
            matching: tokio::sync::Mutex::new(()),
            evaluated: AtomicUsize::new(0),
            started_at: Instant::now(),
            turn_id,
            windows,
            exit,
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

    /// Whether the watch still expects news (P2-3): it is on and holds undelivered events, or
    /// its process printed a matching line less than `quiet` ago, or, before any matching
    /// line, the turn that started the watch ended (or is still running and started it) less
    /// than `first_match` ago (tails 2026-10-27).
    pub(crate) fn is_fresh(&self, now: Instant) -> bool {
        let state = self.lock();
        if state.stopped {
            return false;
        }
        if state.first_event_at.is_some() {
            return true;
        }
        match state.last_match_at {
            Some(last) => now < last + self.windows.quiet,
            None => {
                let waiting_since = state.turn_ended_at.unwrap_or(self.started_at);
                now < waiting_since.max(self.started_at) + self.windows.first_match
            }
        }
    }

    /// Turn `turn_id` ended, completed or interrupted. If it started the watch, the model now
    /// waits for the first matching line, so its window counts from here. Other turns, such
    /// as a later one after an interrupted start, do not move it (R6.14, P3-7).
    pub(crate) fn note_turn_finished(&self, turn_id: &str, now: Instant) {
        if self.turn_id == turn_id {
            self.lock().turn_ended_at.get_or_insert(now);
        }
    }

    /// Whether the watch found a matching line yet, or took over from a watch of the same
    /// process that had (`continue_after_match`).
    pub(crate) fn has_matched(&self) -> bool {
        self.lock().last_match_at.is_some()
    }

    /// The watch replaces one that already matched, such as a server's "ready" watch turned
    /// into an error watch: the process is up, so the watch is waited for only within the
    /// quiet window, as after a match, not for a first match (live recheck 2 R3).
    pub(crate) fn continue_after_match(&self, now: Instant) {
        self.lock().last_match_at.get_or_insert(now);
    }

    /// How much longer the watch keeps its process awaited without news (`is_fresh`), or
    /// `None` when it no longer does (review of the fixes P3-7, live recheck 2 R2).
    pub(crate) fn wait_left(&self, now: Instant) -> Option<WatchWait> {
        let state = self.lock();
        if state.stopped {
            return None;
        }
        if state.first_event_at.is_some() {
            return Some(WatchWait::Quiet(self.windows.quiet));
        }
        let left = |deadline: Instant| {
            deadline
                .checked_duration_since(now)
                .filter(|left| !left.is_zero())
        };
        match (state.last_match_at, state.turn_ended_at) {
            (Some(last), _) => left(last + self.windows.quiet).map(WatchWait::Quiet),
            (None, None) => Some(WatchWait::FirstMatchAfterTurn(self.windows.first_match)),
            (None, Some(ended)) => left(ended.max(self.started_at) + self.windows.first_match)
                .map(WatchWait::FirstMatch),
        }
    }

    /// Whether the watch reported or still holds matching lines.
    pub(crate) fn has_reported(&self) -> bool {
        let state = self.lock();
        state.notifications > 0 || !state.pending.is_empty()
    }

    /// Stop matching and delivering; undelivered events are dropped unless a final batch was
    /// taken first (R6.4, R6.9).
    pub(crate) fn stop(&self) {
        self.lock().stopped = true;
        self.wake.notify_one();
    }

    /// Under the output buffer lock: split `chunk` into lines and keep the complete ones for
    /// matching; nothing is matched here (R6.5, P2-1).
    pub(crate) fn observe_chunk(&self, chunk: &[u8]) {
        let mut state = self.lock();
        if state.stopped {
            return;
        }
        let now = Instant::now();
        let mut rest = chunk;
        while let Some(index) = rest.iter().position(|byte| matches!(byte, b'\n' | b'\r')) {
            state.extend_partial(&rest[..index], now);
            state.observed = state.observed.saturating_add(index as u64 + 1);
            let end = state.observed;
            state.complete_line(end);
            rest = &rest[index + 1..];
        }
        state.extend_partial(rest, now);
        state.observed = state.observed.saturating_add(rest.len() as u64);
        drop(state);
        self.wake.notify_one();
    }

    /// Count bytes that precede what this watch sees, so line offsets match the output stream
    /// that `write_stdin` reads.
    pub(crate) fn skip_bytes(&self, bytes: usize) {
        let mut state = self.lock();
        state.observed = state.observed.saturating_add(bytes as u64);
    }

    fn take_job(&self, now: Instant, final_batch: bool) -> Option<MatchJob> {
        let mut state = self.lock();
        if state.stopped {
            return None;
        }
        let lines: Vec<RawLine> = state.backlog.drain(..).collect();
        state.backlog_bytes = 0;
        let partial = state.partial_due(now, final_batch).then(|| {
            state.partial_check = PartialCheck::InFlight;
            RawLine {
                bytes: state.partial.clone(),
                end: state.observed,
            }
        });
        if lines.is_empty() && partial.is_none() {
            return None;
        }
        Some(MatchJob {
            lines,
            partial,
            generation: state.line_generation,
        })
    }

    fn apply(&self, result: MatchResult, now: Instant) {
        self.evaluated
            .fetch_add(result.evaluated, Ordering::Relaxed);
        let mut state = self.lock();
        if state.stopped {
            return;
        }
        let read_upto = state.read_upto;
        let mut found = false;
        for (text, end) in result.matches {
            if end > read_upto {
                state.pending.push(text, end);
                found = true;
            }
        }
        if let Some(verdict) = result.partial
            && result.generation == state.line_generation
        {
            match verdict {
                Some((text, end)) => {
                    state.partial_check = PartialCheck::Matched;
                    if end > read_upto {
                        state.pending.push(text, end);
                        found = true;
                    }
                }
                None if state.partial_check == PartialCheck::InFlight => {
                    state.partial_check = PartialCheck::NoMatch;
                }
                None => {}
            }
        }
        if found {
            state.first_event_at.get_or_insert(now);
            state.last_match_at = Some(now);
        }
    }

    /// Match the lines that wait, on the blocking thread pool and outside every lock (P2-1).
    /// `final_batch` (the process exited) matches all of them, the unterminated line too;
    /// otherwise one job runs, so a flood of output cannot hold back delivery.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "one match job at a time; the final batch waits for a job in flight"
    )]
    pub(crate) async fn match_waiting_lines(&self, final_batch: bool) {
        let _matching = self.matching.lock().await;
        loop {
            let Some(job) = self.take_job(Instant::now(), final_batch) else {
                return;
            };
            let pattern = Arc::clone(&self.pattern);
            let Ok(result) = tokio::task::spawn_blocking(move || job.run(&pattern)).await else {
                return;
            };
            self.apply(result, Instant::now());
            if !final_batch {
                return;
            }
        }
    }

    /// Matches the waiting lines on the calling thread (tests).
    #[cfg(test)]
    pub(crate) fn match_now(&self, final_batch: bool, now: Instant) {
        while let Some(job) = self.take_job(now, final_batch) {
            let result = job.run(&self.pattern);
            self.apply(result, now);
        }
    }

    /// Lines the pattern was evaluated on so far.
    #[cfg(test)]
    fn evaluated(&self) -> usize {
        self.evaluated.load(Ordering::Relaxed)
    }

    fn next_delivery(&self, now: Instant) -> NextDelivery {
        let state = self.lock();
        if state.stopped {
            return NextDelivery::Stopped;
        }
        if !state.backlog.is_empty() || state.partial_due(now, /*final_batch*/ false) {
            return NextDelivery::Match;
        }
        let partial_at = (!state.partial.is_empty()
            && state.partial_check == PartialCheck::Unchecked)
            .then_some(state.partial_changed_at)
            .flatten()
            .map(|changed| changed + OUTPUT_WATCH_BATCH_WINDOW);
        let batch_at = state.first_event_at.map(|first| {
            let due = first + OUTPUT_WATCH_BATCH_WINDOW;
            state.retry_at.map_or(due, |retry_at| retry_at.max(due))
        });
        match (partial_at, batch_at) {
            (Some(partial), Some(batch)) => NextDelivery::At(partial.min(batch)),
            (Some(at), None) | (None, Some(at)) => NextDelivery::At(at),
            (None, None) => NextDelivery::Idle,
        }
    }

    /// Under the thread-queue lock: the text to put in the queue now, if any (R6.7, R6.8).
    /// It only moves lines that were already matched and renders them (P2-1).
    ///
    /// `queued` says whether this watch's last notification still waits in the queue; the
    /// new events are then added to it. `exited` takes the final batch when the process
    /// exited: neither the batch window nor the interval applies (R6.9).
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
        let unchecked = std::mem::take(&mut state.unchecked);
        let text = render(
            &RenderInput {
                process_id: self.process_id,
                command: &self.command,
                pattern: self.pattern.source(),
                lines: &lines,
                number,
                exited,
                last,
                unchecked,
                exit_wait: self.exit.requested().then_some(self.windows.exec_wait),
                exit_reported: !self.exit.turned_off(),
            },
            boundary,
        );
        state.queued = Some((lines, number));
        Some(text)
    }

    /// A `write_stdin` response showed the process output up to the bytes that are still
    /// unread (`unread`). When the response showed what it drained whole, the matching lines
    /// in it are no news: they are dropped before they reach the queue (P3-2).
    pub(crate) fn mark_read(&self, unread: usize, budget: PollBudget) {
        let mut state = self.lock();
        let upto = state.observed.saturating_sub(unread as u64);
        let drained = upto.saturating_sub(state.polled_upto);
        // Lines completed since the last poll, unread ones included: an upper bound.
        let lines = state.line_generation.wrapping_sub(state.polled_generation);
        state.polled_upto = state.polled_upto.max(upto);
        state.polled_generation = state.line_generation;
        let whole = drained > 0
            && drained <= budget.bytes as u64
            && budget.lines.is_none_or(|max| lines <= max as u64);
        if !whole {
            return;
        }
        state.read_upto = upto;
        state.backlog.retain(|line| line.end > upto);
        state.backlog_bytes = state.backlog.iter().map(|line| line.bytes.len()).sum();
        state.pending.drop_read(upto);
        if state.pending.is_empty() {
            state.first_event_at = None;
            state.retry_at = None;
        }
    }

    /// Under the thread-queue lock, after `mark_read`: whether the notification this watch
    /// queued holds only lines a poll already showed, so it can be withdrawn (P3-2).
    pub(crate) fn forget_read_queued(&self) -> bool {
        let mut state = self.lock();
        let read = state
            .queued
            .as_ref()
            .is_some_and(|(lines, _)| lines.last_end <= state.read_upto);
        if read {
            state.queued = None;
        }
        read
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
    unchecked: usize,
    /// `notify_on_exit` is on: `codex exec` waits this long for the exit.
    exit_wait: Option<Duration>,
    /// The exit will be reported: the model did not turn `notify_on_exit` off.
    exit_reported: bool,
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
        unchecked,
        exit_wait,
        exit_reported,
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
            "Background terminal session {id} printed {count} new {noun}{OUTPUT_WATCH_HEADLINE_MARK}{}\" (notification {number} of at most {OUTPUT_WATCH_MAX_NOTIFICATIONS}){state}",
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
    if *unchecked > 0 {
        text.push(format!(
            "{unchecked} output {} not checked against the pattern because the process printed faster than the watch could match; write_stdin with session_id {id} returns the output you have not read yet.",
            if *unchecked == 1 { "line was" } else { "lines were" }
        ));
    }
    if *last && !*exited {
        text.push(format!(
            "This was the last notification of this output watch; call write_stdin with session_id {id} and notify_on_output to watch again. {}",
            if *exit_reported {
                "The exit is still reported."
            } else {
                "Its exit is not reported: notify_on_exit is off."
            }
        ));
    }
    if !*exited {
        // A server the model meant to leave running kept `codex exec` waiting for its exit
        // for the whole wait, and a model stopped a server with `pkill` by name, killing
        // another session's server (live recheck 2026-10-27, N1, N3).
        if let Some(exit_wait) = exit_wait {
            text.push(format!(
                "Its exit notification is on (notify_on_exit), so codex exec and wait_agent wait for it to exit (codex exec for up to {}). If it is meant to keep running while this Codex session runs, turn that off with write_stdin {{\"session_id\": {id}, \"notify_on_exit\": false}} and leave notify_on_output as it is, so the watch still reports errors and the exit; codex exec still stops the process when it exits.",
                describe_wait(*exit_wait)
            ));
        }
        text.push(format!("{STOP_GUIDANCE_PREFIX}{id}{STOP_GUIDANCE_SUFFIX}"));
    }
    text.join("\n")
}

/// Whether `text`, the body of a background terminal notification, is an output watch
/// notification rather than an exit notification (P3-3). Only the first line, which Codex
/// writes, is read.
pub(crate) fn is_output_watch_text(text: &str) -> bool {
    text.lines()
        .find(|line| line.starts_with("Background terminal session "))
        .is_some_and(|line| line.contains(OUTPUT_WATCH_HEADLINE_MARK))
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

/// Matches the lines of `watch` and turns its events into notifications until the watch
/// stops or the process exits. The exit itself delivers the final batch (`ExitWatch::report`).
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
            let now = Instant::now();
            match watch.next_delivery(now) {
                NextDelivery::Stopped => return,
                // Not cancelled by the exit: the lines it takes must reach the final batch.
                NextDelivery::Match => watch.match_waiting_lines(/*final_batch*/ false).await,
                NextDelivery::Idle => {
                    tokio::select! {
                        () = &mut notified => {}
                        () = exit_token.cancelled() => return,
                    }
                }
                NextDelivery::At(at) if at > now => {
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
                    if matches!(watch.next_delivery(Instant::now()), NextDelivery::At(at) if at <= Instant::now())
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
