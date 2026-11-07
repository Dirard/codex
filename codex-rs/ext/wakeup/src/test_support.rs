//! A fake host and a settable wall clock for unit tests.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use chrono::DateTime;
use chrono::TimeZone;
use chrono::Utc;
use codex_core::config::ScheduledWakeupsConfig;
use codex_core::fork_wakeup::WakeupBeacon;

use crate::scheduler::HostFuture;
use crate::scheduler::StartOutcome;
use crate::scheduler::StartRequest;
use crate::scheduler::WakeupHost;
use crate::state::Eligibility;
use crate::state::UpdateSink;
use crate::state::WakeupThreadState;
use crate::state::WakeupUpdate;

/// Answers starts from a queue (default: started) and, like core, runs `on_turn_start`
/// before it answers.
#[derive(Default)]
pub(crate) struct FakeHost {
    pub(crate) outcomes: Mutex<VecDeque<StartOutcome>>,
    pub(crate) requests: Mutex<Vec<StartRequest>>,
}

impl FakeHost {
    pub(crate) fn push(&self, outcome: StartOutcome) {
        self.outcomes.lock().unwrap().push_back(outcome);
    }

    pub(crate) fn requests(&self) -> Vec<StartRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl WakeupHost for FakeHost {
    fn start_turn<'a>(
        &'a self,
        state: &'a WakeupThreadState,
        request: StartRequest,
    ) -> HostFuture<'a> {
        Box::pin(async move {
            let number = {
                let mut requests = self.requests.lock().unwrap();
                requests.push(request);
                requests.len()
            };
            let outcome = self
                .outcomes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| StartOutcome::Started {
                    turn_id: format!("wake-turn-{number}"),
                });
            if let StartOutcome::Started { turn_id } = &outcome {
                state.on_turn_started(turn_id);
            }
            outcome
        })
    }
}

/// A wall clock the test moves.
#[derive(Clone)]
pub(crate) struct TestClock(Arc<Mutex<DateTime<Utc>>>);

impl TestClock {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Mutex::new(
            Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap(),
        )))
    }

    pub(crate) fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }

    pub(crate) fn advance(&self, seconds: i64) {
        *self.0.lock().unwrap() += chrono::Duration::seconds(seconds);
    }
}

pub(crate) struct Harness {
    pub(crate) state: Arc<WakeupThreadState>,
    pub(crate) host: Arc<FakeHost>,
    pub(crate) clock: TestClock,
    pub(crate) beacon: Arc<WakeupBeacon>,
    pub(crate) updates: Arc<Mutex<Vec<WakeupUpdate>>>,
}

impl Harness {
    pub(crate) fn new() -> Self {
        Self::with_limits(ScheduledWakeupsConfig::default())
    }

    pub(crate) fn with_limits(limits: ScheduledWakeupsConfig) -> Self {
        let host = Arc::new(FakeHost::default());
        let clock = TestClock::new();
        let beacon = Arc::new(WakeupBeacon::default());
        let clock_for_state = clock.clone();
        let state = WakeupThreadState::new(
            "thread-1".to_string(),
            /*enabled*/ true,
            limits,
            Eligibility::Eligible,
            Arc::clone(&beacon),
            host.clone(),
            Arc::new(move || clock_for_state.now()),
        );
        let updates = Arc::new(Mutex::new(Vec::new()));
        let sink_updates = Arc::clone(&updates);
        let sink: UpdateSink = Box::new(move |update| {
            sink_updates.lock().unwrap().push(update.clone());
            true
        });
        state.subscribe(sink);
        Self {
            state,
            host,
            clock,
            beacon,
            updates,
        }
    }

    pub(crate) fn take_updates(&self) -> Vec<WakeupUpdate> {
        std::mem::take(&mut *self.updates.lock().unwrap())
    }
}

/// A stand-in for an o200k token count (the workspace has no o200k tokenizer): a run of
/// letters is one token plus one per further 10 letters, a run of up to three digits one, a
/// run of other non-space characters one per three, a line break one. It tracks
/// `bytes / 4` closely on these texts.
pub(crate) fn estimated_tokens(text: &str) -> usize {
    #[derive(PartialEq)]
    enum Class {
        Letter,
        Digit,
        Other,
        Space,
        Newline,
    }
    let class = |ch: char| {
        if ch == '\n' {
            Class::Newline
        } else if ch.is_alphabetic() {
            Class::Letter
        } else if ch.is_ascii_digit() {
            Class::Digit
        } else if ch.is_whitespace() {
            Class::Space
        } else {
            Class::Other
        }
    };
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = 0;
    let mut index = 0;
    while index < chars.len() {
        let kind = class(chars[index]);
        let mut end = index + 1;
        while end < chars.len() && class(chars[end]) == kind && kind != Class::Newline {
            end += 1;
        }
        let len = end - index;
        tokens += match kind {
            Class::Letter => 1 + (len - 1) / 10,
            Class::Digit => len.div_ceil(3),
            Class::Other => len.div_ceil(3),
            Class::Newline => 1,
            Class::Space => 0,
        };
        index = end;
    }
    tokens
}
