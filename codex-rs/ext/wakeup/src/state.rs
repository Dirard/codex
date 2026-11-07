//! A thread's scheduled wakeup, `/loop` and limits (`specs/scheduled-wakeups-2026-11-07.md`,
//! UC2, UC4, UC5).
//!
//! One [`WakeupThreadState`] lives in the thread store of each thread runtime. It holds at
//! most one pending wakeup (D1), the active loop, the wakeup chain since the user's last
//! message, the notice owed to the model after a cancel it did not make, and the turn the
//! extension started last. Every change is published at once to the core beacon and to the
//! subscribed clients (R2.7, R8.4).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::PoisonError;
use std::sync::Weak;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::DateTime;
use chrono::Utc;
use codex_core::config::ScheduledWakeupsConfig;
use codex_core::fork_wakeup::PendingWakeupView;
use codex_core::fork_wakeup::WakeupBeacon;
use tokio_util::task::AbortOnDropHandle;

use crate::fragments;
use crate::scheduler::WakeupHost;

/// Longest model-authored wakeup note, in bytes (R2.3).
pub(crate) const MAX_PROMPT_BYTES: usize = 4_000;
/// Longest model-authored reason, in characters (R2.3).
pub(crate) const MAX_REASON_CHARS: usize = 200;
/// Longest `/loop` prompt, in bytes (R5.1).
pub(crate) const MAX_LOOP_PROMPT_BYTES: usize = 16_000;
/// Turns started for wakeups that may fail in a row before the wakeups stop (R3.8).
pub(crate) const MAX_CONSECUTIVE_FAILED_TURNS: u32 = 3;
/// How often a timer compares the wall clock with the due time (R3.6, D7).
pub(crate) const WALL_CLOCK_RECHECK: Duration = Duration::from_secs(30);

/// Who scheduled a wakeup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeupOrigin {
    Model,
    Loop,
}

/// What changed in a thread's wakeup state (R8.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeupChange {
    Scheduled,
    Replaced,
    Fired,
    Cancelled,
    LoopStarted,
    LoopEnded,
}

/// Why a wakeup or loop stopped (R4.1–R4.5, R5.4, R5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    Model,
    User,
    Interrupt,
    Limit,
    Failures,
    Replaced,
    Completed,
    ThreadClosed,
}

impl CancelReason {
    /// Cancels the model did not make itself leave it a notice in the next turn (R4.5).
    fn leaves_notice(self) -> bool {
        matches!(
            self,
            Self::User | Self::Interrupt | Self::Limit | Self::Failures | Self::Replaced
        )
    }
}

/// The pending wakeup as clients see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeupView {
    pub id: String,
    pub origin: WakeupOrigin,
    /// Unix seconds.
    pub due_at: i64,
    pub delay_seconds: u32,
    pub requested_delay_seconds: u32,
    /// Due and waiting for the thread to become idle.
    pub due: bool,
    pub reason: Option<String>,
    pub prompt: Option<String>,
    pub scheduled_in_turn_id: Option<String>,
}

/// The active `/loop` as clients see it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopView {
    pub id: String,
    pub prompt: String,
    /// `None` for a self-paced loop.
    pub interval_seconds: Option<u32>,
    /// Iterations started so far.
    pub iteration: u32,
    pub max_iterations: u32,
    pub started_at: i64,
    pub ends_at: i64,
}

/// The model's wakeup chain since the user's last message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChainView {
    pub fired: u32,
    pub max_wakeups: u32,
    pub ends_at: Option<i64>,
}

/// A thread's wakeup state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WakeupSnapshot {
    pub wakeup: Option<WakeupView>,
    pub loop_state: Option<LoopView>,
    pub chain: ChainView,
}

/// One published change with the state after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeupUpdate {
    pub change: WakeupChange,
    pub cancel_reason: Option<CancelReason>,
    /// The turn a `Fired` change started.
    pub turn_id: Option<String>,
    pub snapshot: WakeupSnapshot,
}

/// Receives every published update; returns `false` once it no longer wants them.
pub type UpdateSink = Box<dyn Fn(&WakeupUpdate) -> bool + Send + Sync>;

/// Whether a thread may use wakeups (R1.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Eligibility {
    Eligible,
    /// A MultiAgentV2 child, until its registration shows it is not a workflow agent.
    PendingCheck,
    NotEligible,
}

#[derive(Debug)]
pub(crate) struct PendingWakeup {
    pub(crate) id: String,
    pub(crate) origin: WakeupOrigin,
    pub(crate) scheduled_at: DateTime<Utc>,
    pub(crate) due_at: DateTime<Utc>,
    pub(crate) effective_delay_s: u32,
    pub(crate) requested_delay_s: u32,
    /// The model's note; for a self-paced loop it goes into the iteration's loop context.
    pub(crate) prompt: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) scheduled_in_turn: Option<String>,
    pub(crate) due: bool,
    /// Held for its drop, which stops the timer with the wakeup.
    pub(crate) _timer: Option<AbortOnDropHandle<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoopMode {
    Fixed { interval_s: u32 },
    SelfPaced,
}

#[derive(Clone, Debug)]
pub(crate) struct LoopState {
    pub(crate) id: String,
    pub(crate) prompt: String,
    pub(crate) mode: LoopMode,
    /// Iterations started so far.
    pub(crate) iteration: u32,
    pub(crate) max_iterations: u32,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) ends_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ChainState {
    pub(crate) fired: u32,
    pub(crate) started_at: Option<DateTime<Utc>>,
}

/// What a turn the extension starts is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TurnKind {
    Wakeup {
        wakeup_id: String,
    },
    LoopIteration {
        wakeup_id: String,
        loop_id: String,
        iteration: u32,
        note: Option<String>,
    },
}

impl TurnKind {
    fn wakeup_id(&self) -> &str {
        match self {
            Self::Wakeup { wakeup_id } | Self::LoopIteration { wakeup_id, .. } => wakeup_id,
        }
    }
}

/// The last turn the extension started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnTurn {
    pub(crate) turn_id: String,
    pub(crate) kind: TurnKind,
}

#[derive(Debug, Default)]
pub(crate) struct Inner {
    pub(crate) pending: Option<PendingWakeup>,
    pub(crate) loop_state: Option<LoopState>,
    pub(crate) chain: ChainState,
    pub(crate) failures: u32,
    pub(crate) notice: Option<String>,
    pub(crate) closed: bool,
    /// Set while a start the extension requested may still reach `on_turn_start`.
    pub(crate) arming: Option<TurnKind>,
    pub(crate) own_turn: Option<OwnTurn>,
    /// Whether the turn that ended last was one the extension started.
    pub(crate) last_finished_own: Option<bool>,
}

/// What a cancel stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cancelled {
    pub(crate) wakeup: Option<(String, DateTime<Utc>, WakeupOrigin)>,
    pub(crate) ended_loop: Option<EndedLoop>,
}

/// A loop a cancel ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EndedLoop {
    pub(crate) id: String,
    pub(crate) iterations: u32,
    pub(crate) max_iterations: u32,
}

impl Cancelled {
    pub(crate) fn anything(&self) -> bool {
        self.wakeup.is_some() || self.ended_loop.is_some()
    }
}

/// Parameters of a `/loop` start (R5.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoopStartRequest {
    pub prompt: String,
    pub interval_seconds: Option<u32>,
    pub max_iterations: Option<u32>,
}

/// Arguments of `clock.schedule_wakeup` after parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScheduleArgs {
    pub(crate) delay_seconds: u64,
    pub(crate) prompt: Option<String>,
    pub(crate) reason: Option<String>,
}

pub(crate) type WallClock = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// The wakeup state of one thread runtime.
pub struct WakeupThreadState {
    pub(crate) thread_id: String,
    weak_self: Weak<Self>,
    enabled: AtomicBool,
    limits: Mutex<ScheduledWakeupsConfig>,
    eligibility: Mutex<Eligibility>,
    inner: Mutex<Inner>,
    /// Serializes starting a turn with schedules and cancels, so a wakeup starts at most one
    /// turn and a cancel lands either before the start or after it (R3.5, R4.6).
    pub(crate) fire_lock: tokio::sync::Mutex<()>,
    subscribers: Mutex<Vec<UpdateSink>>,
    beacon: Arc<WakeupBeacon>,
    pub(crate) host: Arc<dyn WakeupHost>,
    clock: WallClock,
}

impl std::fmt::Debug for WakeupThreadState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WakeupThreadState")
            .field("thread_id", &self.thread_id)
            .finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `wk-1a2b3c4d` or `loop-1a2b3c4d`.
fn new_id(prefix: &str) -> String {
    format!("{prefix}-{:08x}", rand::random::<u32>())
}

/// Control characters of a model-authored reason become `?` (R3.7, R10.4).
pub(crate) fn sanitize(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '?' } else { ch })
        .collect()
}

impl WakeupThreadState {
    pub(crate) fn new(
        thread_id: String,
        enabled: bool,
        limits: ScheduledWakeupsConfig,
        eligibility: Eligibility,
        beacon: Arc<WakeupBeacon>,
        host: Arc<dyn WakeupHost>,
        clock: WallClock,
    ) -> Arc<Self> {
        Arc::new_cyclic(|weak_self| Self {
            thread_id,
            weak_self: weak_self.clone(),
            enabled: AtomicBool::new(enabled),
            limits: Mutex::new(limits),
            eligibility: Mutex::new(eligibility),
            inner: Mutex::new(Inner::default()),
            fire_lock: tokio::sync::Mutex::new(()),
            subscribers: Mutex::new(Vec::new()),
            beacon,
            host,
            clock,
        })
    }

    /// The thread this state belongs to.
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub(crate) fn now(&self) -> DateTime<Utc> {
        (self.clock)()
    }

    pub(crate) fn inner(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    pub(crate) fn limits(&self) -> ScheduledWakeupsConfig {
        *lock(&self.limits)
    }

    pub(crate) fn set_config(&self, enabled: bool, limits: ScheduledWakeupsConfig) {
        self.enabled.store(enabled, Ordering::Relaxed);
        *lock(&self.limits) = limits;
    }

    pub(crate) fn set_eligibility(&self, eligibility: Eligibility) {
        *lock(&self.eligibility) = eligibility;
    }

    /// Whether this thread may use wakeups now: the feature is on and the thread qualifies.
    pub fn available(&self) -> bool {
        self.enabled.load(Ordering::Relaxed) && *lock(&self.eligibility) == Eligibility::Eligible
    }

    pub(crate) fn eligibility(&self) -> Eligibility {
        *lock(&self.eligibility)
    }

    pub fn snapshot(&self) -> WakeupSnapshot {
        let inner = self.inner();
        self.snapshot_of(&inner)
    }

    fn snapshot_of(&self, inner: &Inner) -> WakeupSnapshot {
        let limits = self.limits();
        WakeupSnapshot {
            wakeup: inner.pending.as_ref().map(|pending| WakeupView {
                id: pending.id.clone(),
                origin: pending.origin,
                due_at: pending.due_at.timestamp(),
                delay_seconds: pending.effective_delay_s,
                requested_delay_seconds: pending.requested_delay_s,
                due: pending.due,
                reason: pending.reason.clone(),
                prompt: pending.prompt.clone(),
                scheduled_in_turn_id: pending.scheduled_in_turn.clone(),
            }),
            loop_state: inner.loop_state.as_ref().map(|state| LoopView {
                id: state.id.clone(),
                prompt: state.prompt.clone(),
                interval_seconds: match state.mode {
                    LoopMode::Fixed { interval_s } => Some(interval_s),
                    LoopMode::SelfPaced => None,
                },
                iteration: state.iteration,
                max_iterations: state.max_iterations,
                started_at: state.started_at.timestamp(),
                ends_at: state.ends_at.timestamp(),
            }),
            chain: ChainView {
                fired: inner.chain.fired,
                max_wakeups: limits.max_wakeups,
                ends_at: inner
                    .chain
                    .started_at
                    .map(|started_at| chain_deadline(started_at, &limits).timestamp()),
            },
        }
    }

    /// Adds a sink for every later update.
    pub fn subscribe(&self, sink: UpdateSink) {
        lock(&self.subscribers).push(sink);
    }

    /// Publishes a change to the core beacon and the subscribers, in the caller's task, so a
    /// tool call's update reaches the thread listener before the call completes (R2.7).
    pub(crate) fn publish(
        &self,
        inner: &Inner,
        change: WakeupChange,
        cancel_reason: Option<CancelReason>,
        turn_id: Option<String>,
    ) {
        self.beacon
            .set(inner.pending.as_ref().map(|pending| PendingWakeupView {
                due_at: pending.due_at,
                loop_iteration: match (pending.origin, inner.loop_state.as_ref()) {
                    (WakeupOrigin::Loop, Some(state)) => {
                        Some((state.iteration + 1, state.max_iterations))
                    }
                    _ => None,
                },
            }));
        let update = WakeupUpdate {
            change,
            cancel_reason,
            turn_id,
            snapshot: self.snapshot_of(inner),
        };
        crate::metrics::record(&update);
        lock(&self.subscribers).retain(|sink| sink(&update));
    }

    fn spawn_timer(
        &self,
        id: &str,
        due_at: DateTime<Utc>,
        delay: Duration,
    ) -> AbortOnDropHandle<()> {
        let deadline = tokio::time::Instant::now() + delay;
        AbortOnDropHandle::new(tokio::spawn(crate::scheduler::run_timer(
            self.weak_self.clone(),
            id.to_string(),
            due_at,
            deadline,
        )))
    }

    /// `clock.schedule_wakeup`: check arguments, the loop mode and the chain's limits, then
    /// schedule or replace the thread's wakeup and publish it (UC2). Returns the model's
    /// output or the error it reads.
    pub(crate) fn schedule_from_model(
        &self,
        args: ScheduleArgs,
        turn_id: &str,
    ) -> Result<String, String> {
        // R2.3: arguments first; a rejected call keeps the state.
        if args.delay_seconds == 0 {
            return Err(
                "Wakeup not scheduled: delay_seconds must be a positive integer.".to_string(),
            );
        }
        if let Some(prompt) = args.prompt.as_deref()
            && prompt.len() > MAX_PROMPT_BYTES
        {
            return Err(format!(
                "Wakeup not scheduled: prompt is {} bytes; at most {MAX_PROMPT_BYTES}.",
                prompt.len()
            ));
        }
        if let Some(reason) = args.reason.as_deref()
            && reason.chars().count() > MAX_REASON_CHARS
        {
            return Err(format!(
                "Wakeup not scheduled: reason is {} characters; at most {MAX_REASON_CHARS}.",
                reason.chars().count()
            ));
        }
        let limits = self.limits();
        let now = self.now();
        let effective = clamp_delay(args.delay_seconds, &limits);
        let requested = u32::try_from(args.delay_seconds).unwrap_or(u32::MAX);
        let due_at = now + chrono::Duration::seconds(i64::from(effective));
        let prompt = args.prompt.filter(|prompt| !prompt.trim().is_empty());
        let reason = args
            .reason
            .map(|reason| sanitize(&reason))
            .filter(|reason| !reason.trim().is_empty());

        let mut inner = self.inner();
        if inner.closed {
            return Err("Wakeup not scheduled: this thread is closing.".to_string());
        }
        // R2.5: the loop mode decides what the call schedules.
        let (origin, output_kind) = match inner.loop_state.as_ref() {
            Some(LoopState {
                mode: LoopMode::Fixed { interval_s },
                ..
            }) => {
                let next = match inner.pending.as_ref() {
                    Some(pending) => format!(
                        "its next iteration is at {}",
                        fragments::hms(pending.due_at)
                    ),
                    None => format!(
                        "its next iteration starts {} after this one ends",
                        fragments::describe_seconds(u64::from(*interval_s))
                    ),
                };
                return Err(format!(
                    "Wakeup not scheduled: a fixed-interval /loop owns this thread's wakeups; {next}. Call clock.cancel_wakeup to stop the loop."
                ));
            }
            Some(
                state @ LoopState {
                    mode: LoopMode::SelfPaced,
                    ..
                },
            ) => {
                let next = state.iteration + 1;
                if next > state.max_iterations {
                    return Err(format!(
                        "Wakeup not scheduled: this /loop already ran its {} iterations (maxIterations). Report to the user instead.",
                        state.max_iterations
                    ));
                }
                if due_at > state.ends_at {
                    return Err(format!(
                        "Wakeup not scheduled: it would be due after this /loop's end at {} (features.scheduled_wakeups.max_duration_minutes). Report to the user instead.",
                        fragments::hm(state.ends_at)
                    ));
                }
                (
                    WakeupOrigin::Loop,
                    OutputKind::Loop {
                        next,
                        max: state.max_iterations,
                    },
                )
            }
            None => {
                if prompt.is_none() {
                    return Err("Wakeup not scheduled: prompt is required outside a self-paced /loop; say what to do on wakeup.".to_string());
                }
                // R2.4: the chain's count and deadline.
                if inner.chain.fired >= limits.max_wakeups {
                    return Err(format!(
                        "Wakeup not scheduled: this thread already ran {} wakeups since the user's last message (features.scheduled_wakeups.max_wakeups). Report to the user instead.",
                        limits.max_wakeups
                    ));
                }
                let chain_start = inner.chain.started_at.unwrap_or(now);
                let chain_end = chain_deadline(chain_start, &limits);
                if due_at > chain_end {
                    return Err(format!(
                        "Wakeup not scheduled: it would be due after this chain's end at {} (features.scheduled_wakeups.max_duration_minutes). Report to the user instead.",
                        fragments::hm(chain_end)
                    ));
                }
                inner.chain.started_at = Some(chain_start);
                (
                    WakeupOrigin::Model,
                    OutputKind::Chain {
                        left: limits.max_wakeups - inner.chain.fired - 1,
                        max: limits.max_wakeups,
                        until: chain_end,
                    },
                )
            }
        };
        // R2.2: a new call replaces the pending wakeup.
        let replaced = inner.pending.take().map(|pending| pending.due_at);
        let id = new_id("wk");
        let timer = self.spawn_timer(&id, due_at, Duration::from_secs(u64::from(effective)));
        inner.pending = Some(PendingWakeup {
            id: id.clone(),
            origin,
            scheduled_at: now,
            due_at,
            effective_delay_s: effective,
            requested_delay_s: requested,
            prompt,
            reason,
            scheduled_in_turn: Some(turn_id.to_string()),
            due: false,
            _timer: Some(timer),
        });
        let change = if replaced.is_some() {
            WakeupChange::Replaced
        } else {
            WakeupChange::Scheduled
        };
        self.publish(&inner, change, None, None);
        Ok(fragments::schedule_output(&fragments::ScheduleOutput {
            id: &id,
            due_at,
            effective,
            requested: args.delay_seconds,
            limits: &limits,
            replaced,
            kind: output_kind,
        }))
    }

    /// Cancels the pending wakeup and ends the loop (R4.1–R4.5). The caller holds
    /// `fire_lock` when a start may race the cancel.
    pub(crate) fn cancel(&self, reason: CancelReason) -> Cancelled {
        let mut inner = self.inner();
        self.cancel_locked(&mut inner, reason)
    }

    pub(crate) fn cancel_locked(&self, inner: &mut Inner, reason: CancelReason) -> Cancelled {
        let pending = inner.pending.take();
        let ended_loop = inner.loop_state.take();
        if pending.is_none() && ended_loop.is_none() {
            return Cancelled::default();
        }
        let cancelled = Cancelled {
            wakeup: pending
                .as_ref()
                .map(|pending| (pending.id.clone(), pending.due_at, pending.origin)),
            ended_loop: ended_loop.as_ref().map(|state| EndedLoop {
                id: state.id.clone(),
                iterations: state.iteration,
                max_iterations: state.max_iterations,
            }),
        };
        if reason.leaves_notice() {
            let limits = self.limits();
            inner.notice = Some(fragments::cancel_notice(&cancelled, reason, &limits));
        }
        let change = if cancelled.ended_loop.is_some() {
            WakeupChange::LoopEnded
        } else {
            WakeupChange::Cancelled
        };
        if matches!(reason, CancelReason::Failures) {
            inner.failures = 0;
        }
        self.publish(inner, change, Some(reason), None);
        cancelled
    }

    /// Marks the wakeup `id` due when its timer fires and shows clients that it waits for the
    /// thread to become idle; `false` when it is no longer pending.
    pub(crate) fn mark_due(&self, id: &str) -> bool {
        let mut inner = self.inner();
        if inner.closed {
            return false;
        }
        match inner.pending.as_mut() {
            Some(pending) if pending.id == id => {
                pending.due = true;
                self.publish(&inner, WakeupChange::Scheduled, None, None);
                true
            }
            _ => false,
        }
    }

    /// `thread/loop/start` (R5.1, R5.7): check the request, replace a pending wakeup or loop
    /// and make iteration 1 due at once.
    pub(crate) fn start_loop(&self, request: LoopStartRequest) -> Result<WakeupSnapshot, String> {
        if !self.available() {
            return Err("scheduled wakeups are not available for this thread".to_string());
        }
        let limits = self.limits();
        let prompt = request.prompt;
        if prompt.trim().is_empty() {
            return Err("prompt must not be empty".to_string());
        }
        if prompt.len() > MAX_LOOP_PROMPT_BYTES {
            return Err(format!(
                "prompt is {} bytes; at most {MAX_LOOP_PROMPT_BYTES}",
                prompt.len()
            ));
        }
        let mode = match request.interval_seconds {
            Some(interval_s)
                if !(limits.min_delay_seconds..=limits.max_delay_seconds).contains(&interval_s) =>
            {
                return Err(format!(
                    "intervalSeconds must be from {} to {}",
                    limits.min_delay_seconds, limits.max_delay_seconds
                ));
            }
            Some(interval_s) => LoopMode::Fixed { interval_s },
            None => LoopMode::SelfPaced,
        };
        let max_iterations = match request.max_iterations {
            Some(max) if !(1..=limits.max_wakeups).contains(&max) => {
                return Err(format!(
                    "maxIterations must be from 1 to {}",
                    limits.max_wakeups
                ));
            }
            Some(max) => max,
            None => limits.max_wakeups,
        };
        let now = self.now();
        let mut inner = self.inner();
        if inner.closed {
            return Err("the thread is closing".to_string());
        }
        self.cancel_locked(&mut inner, CancelReason::Replaced);
        inner.loop_state = Some(LoopState {
            id: new_id("loop"),
            prompt,
            mode,
            iteration: 0,
            max_iterations,
            started_at: now,
            ends_at: chain_deadline(now, &limits),
        });
        inner.pending = Some(PendingWakeup {
            id: new_id("wk"),
            origin: WakeupOrigin::Loop,
            scheduled_at: now,
            due_at: now,
            effective_delay_s: 0,
            requested_delay_s: 0,
            prompt: None,
            reason: None,
            scheduled_in_turn: None,
            due: true,
            _timer: None,
        });
        self.publish(&inner, WakeupChange::LoopStarted, None, None);
        Ok(self.snapshot_of(&inner))
    }

    /// Schedules the next iteration of a fixed-interval loop `interval` after its turn ended,
    /// or ends the loop at its limits (R5.3, R5.5).
    fn schedule_next_fixed_iteration(&self, inner: &mut Inner, interval_s: u32) {
        let Some(state) = inner.loop_state.as_ref() else {
            return;
        };
        let now = self.now();
        let due_at = now + chrono::Duration::seconds(i64::from(interval_s));
        if state.iteration >= state.max_iterations || due_at > state.ends_at {
            self.cancel_locked(inner, CancelReason::Limit);
            return;
        }
        let id = new_id("wk");
        let timer = self.spawn_timer(&id, due_at, Duration::from_secs(u64::from(interval_s)));
        inner.pending = Some(PendingWakeup {
            id,
            origin: WakeupOrigin::Loop,
            scheduled_at: now,
            due_at,
            effective_delay_s: interval_s,
            requested_delay_s: interval_s,
            prompt: None,
            reason: None,
            scheduled_in_turn: None,
            due: false,
            _timer: Some(timer),
        });
        self.publish(inner, WakeupChange::Scheduled, None, None);
    }

    /// A turn started: it is the extension's when a start it requested is armed.
    pub(crate) fn on_turn_started(&self, turn_id: &str) {
        let mut inner = self.inner();
        inner.own_turn = inner.arming.take().map(|kind| OwnTurn {
            turn_id: turn_id.to_string(),
            kind,
        });
    }

    /// A turn ended without an interrupt: a fixed loop schedules its next iteration, a
    /// self-paced loop without a pending wakeup ends (R5.3, R5.4). Runs before the turn's
    /// completion is published, so a parent's mail sees the next wakeup (R6.1).
    pub(crate) fn on_turn_finished(&self, turn_id: &str) {
        let mut inner = self.inner();
        let own = inner
            .own_turn
            .as_ref()
            .filter(|own| own.turn_id == turn_id)
            .map(|own| own.kind.clone());
        inner.last_finished_own = Some(own.is_some());
        let Some(TurnKind::LoopIteration { loop_id, .. }) = own else {
            return;
        };
        let Some(state) = inner.loop_state.as_ref() else {
            return;
        };
        if state.id != loop_id {
            return;
        }
        match state.mode {
            LoopMode::Fixed { interval_s } => {
                if inner.pending.is_none() {
                    self.schedule_next_fixed_iteration(&mut inner, interval_s);
                }
            }
            LoopMode::SelfPaced => {
                if inner.pending.is_none() {
                    let reason = if state.iteration >= state.max_iterations {
                        CancelReason::Limit
                    } else {
                        CancelReason::Completed
                    };
                    self.cancel_locked(&mut inner, reason);
                }
            }
        }
    }

    /// A turn was interrupted (or budget-limited): the wakeup it started or scheduled, and a
    /// loop it was an iteration of, stop (R4.3). Another interrupted turn only holds a due
    /// wakeup back (R3.3).
    pub(crate) fn on_turn_interrupted(&self, turn_id: &str) {
        let mut inner = self.inner();
        let own = inner
            .own_turn
            .as_ref()
            .is_some_and(|own| own.turn_id == turn_id);
        inner.last_finished_own = Some(own);
        let scheduled_here = inner
            .pending
            .as_ref()
            .is_some_and(|pending| pending.scheduled_in_turn.as_deref() == Some(turn_id));
        if own || scheduled_here {
            self.cancel_locked(&mut inner, CancelReason::Interrupt);
        }
    }

    /// The thread became idle after a completed or failed turn: count failures of the
    /// extension's turns (R3.8).
    pub(crate) fn on_idle(&self, failed: bool) {
        let mut inner = self.inner();
        let Some(own) = inner.last_finished_own.take() else {
            return;
        };
        if !own {
            return;
        }
        if failed {
            inner.failures += 1;
            if inner.failures >= MAX_CONSECUTIVE_FAILED_TURNS {
                self.cancel_locked(&mut inner, CancelReason::Failures);
            }
        } else {
            inner.failures = 0;
        }
    }

    /// A turn the extension did not start received user input: the model's chain starts
    /// over (R2.4, R5.6).
    pub(crate) fn on_user_input(&self, turn_id: &str) {
        let mut inner = self.inner();
        if inner
            .own_turn
            .as_ref()
            .is_some_and(|own| own.turn_id == turn_id)
        {
            return;
        }
        inner.chain = ChainState::default();
    }

    /// The runtime stops: no turn starts any more and subscribers hear `threadClosed` (R4.4).
    pub(crate) fn close(&self) {
        let mut inner = self.inner();
        inner.closed = true;
        inner.notice = None;
        self.cancel_locked(&mut inner, CancelReason::ThreadClosed);
        inner.notice = None;
        self.beacon.set(None);
    }

    /// The notice owed after a cancel the model did not make, once (R4.5).
    pub(crate) fn take_notice(&self) -> Option<String> {
        self.inner().notice.take()
    }

    /// The loop context of `turn_id` when it is a loop iteration (R5.2).
    pub(crate) fn loop_context(&self, turn_id: &str) -> Option<String> {
        let inner = self.inner();
        let own = inner
            .own_turn
            .as_ref()
            .filter(|own| own.turn_id == turn_id)?;
        let TurnKind::LoopIteration {
            loop_id,
            iteration,
            note,
            ..
        } = &own.kind
        else {
            return None;
        };
        let state = inner
            .loop_state
            .as_ref()
            .filter(|state| &state.id == loop_id)?;
        Some(fragments::loop_context(
            state,
            *iteration,
            note.as_deref(),
            &self.limits(),
        ))
    }

    /// Takes the due wakeup for a start: the request and the arming for `on_turn_start`.
    /// `None` when nothing is due, or a loop's limits end it here (R5.5).
    pub(crate) fn prepare_start(&self) -> Option<crate::scheduler::StartRequest> {
        let mut inner = self.inner();
        if inner.closed {
            return None;
        }
        let pending = inner.pending.as_ref().filter(|pending| pending.due)?;
        let now = self.now();
        let request = match pending.origin {
            WakeupOrigin::Model => {
                let limits = self.limits();
                let chain_start = inner.chain.started_at.unwrap_or(pending.scheduled_at);
                let input = fragments::WakeInput {
                    id: &pending.id,
                    scheduled_at: pending.scheduled_at,
                    now,
                    turn_id: pending.scheduled_in_turn.as_deref(),
                    number: inner.chain.fired + 1,
                    max: limits.max_wakeups,
                    chain_ends_at: chain_deadline(chain_start, &limits),
                    prompt: pending.prompt.as_deref().unwrap_or_default(),
                };
                crate::scheduler::StartRequest::Wakeup {
                    kind: TurnKind::Wakeup {
                        wakeup_id: pending.id.clone(),
                    },
                    body: fragments::wake_input_body(
                        &input,
                        &codex_utils_string::untrusted::UntrustedBoundary::random(),
                    ),
                }
            }
            WakeupOrigin::Loop => {
                let Some(state) = inner.loop_state.as_ref() else {
                    // A loop wakeup without its loop cannot run.
                    self.cancel_locked(&mut inner, CancelReason::Completed);
                    return None;
                };
                let next = state.iteration + 1;
                if next > state.max_iterations || now > state.ends_at {
                    self.cancel_locked(&mut inner, CancelReason::Limit);
                    return None;
                }
                crate::scheduler::StartRequest::LoopIteration {
                    kind: TurnKind::LoopIteration {
                        wakeup_id: pending.id.clone(),
                        loop_id: state.id.clone(),
                        iteration: next,
                        note: pending.prompt.clone(),
                    },
                    prompt: state.prompt.clone(),
                }
            }
        };
        inner.arming = Some(request.kind().clone());
        Some(request)
    }

    /// Records the host's answer to a start (R3.1–R3.5).
    pub(crate) fn finish_start(&self, kind: &TurnKind, outcome: crate::scheduler::StartOutcome) {
        let mut inner = self.inner();
        let consumed = inner.arming.take().is_none();
        match outcome {
            crate::scheduler::StartOutcome::Started { turn_id } => {
                inner.own_turn = Some(OwnTurn {
                    turn_id: turn_id.clone(),
                    kind: kind.clone(),
                });
                let still_pending = inner
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.id == kind.wakeup_id());
                if !still_pending {
                    // Cancelled while the start was on its way; the turn runs (R4.6).
                    return;
                }
                let pending = inner.pending.take();
                match kind {
                    TurnKind::Wakeup { .. } => inner.chain.fired += 1,
                    TurnKind::LoopIteration { iteration, .. } => {
                        if let Some(state) = inner.loop_state.as_mut() {
                            state.iteration = *iteration;
                        }
                    }
                }
                drop(pending);
                self.publish(&inner, WakeupChange::Fired, None, Some(turn_id));
            }
            crate::scheduler::StartOutcome::Held => {
                if consumed && inner.own_turn.as_ref().is_some_and(|own| own.kind == *kind) {
                    // `on_turn_start` took the arming for another turn.
                    inner.own_turn = None;
                }
                // The wakeup stays due and fires at the next idle (R3.2, R3.3); `due` is
                // already set, so clients saw it when it became due.
            }
            crate::scheduler::StartOutcome::Closed => {
                inner.own_turn = None;
                self.cancel_locked(&mut inner, CancelReason::ThreadClosed);
            }
        }
    }
}

impl crate::scheduler::StartRequest {
    pub(crate) fn kind(&self) -> &TurnKind {
        match self {
            Self::Wakeup { kind, .. } | Self::LoopIteration { kind, .. } => kind,
        }
    }
}

/// What a schedule's output reports besides the due time.
pub(crate) enum OutputKind {
    Chain {
        left: u32,
        max: u32,
        until: DateTime<Utc>,
    },
    Loop {
        next: u32,
        max: u32,
    },
}

pub(crate) fn clamp_delay(requested: u64, limits: &ScheduledWakeupsConfig) -> u32 {
    let clamped = requested.clamp(
        u64::from(limits.min_delay_seconds),
        u64::from(limits.max_delay_seconds),
    );
    u32::try_from(clamped).unwrap_or(limits.max_delay_seconds)
}

pub(crate) fn chain_deadline(
    started_at: DateTime<Utc>,
    limits: &ScheduledWakeupsConfig,
) -> DateTime<Utc> {
    started_at + chrono::Duration::minutes(i64::from(limits.max_duration_minutes))
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
