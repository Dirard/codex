//! OTEL counters of scheduled wakeups: `codex.wakeup.scheduled`, `codex.wakeup.fired` and
//! `codex.wakeup.cancelled{reason}` (`specs/scheduled-wakeups-2026-11-07.md`, checklist 7).

use crate::state::CancelReason;
use crate::state::WakeupChange;
use crate::state::WakeupUpdate;

pub(crate) fn record(update: &WakeupUpdate) {
    let (name, reason) = match update.change {
        WakeupChange::Scheduled | WakeupChange::Replaced
            if update
                .snapshot
                .wakeup
                .as_ref()
                .is_some_and(|wakeup| !wakeup.due) =>
        {
            ("codex.wakeup.scheduled", None)
        }
        WakeupChange::Fired => ("codex.wakeup.fired", None),
        WakeupChange::Cancelled | WakeupChange::LoopEnded => {
            ("codex.wakeup.cancelled", update.cancel_reason)
        }
        WakeupChange::Scheduled | WakeupChange::Replaced | WakeupChange::LoopStarted => return,
    };
    let Some(metrics) = codex_otel::global() else {
        return;
    };
    let tags = reason
        .map(|reason| vec![("reason", reason_tag(reason))])
        .unwrap_or_default();
    let _ = metrics.counter(name, /*inc*/ 1, &tags);
}

fn reason_tag(reason: CancelReason) -> &'static str {
    match reason {
        CancelReason::Model => "model",
        CancelReason::User => "user",
        CancelReason::Interrupt => "interrupt",
        CancelReason::Limit => "limit",
        CancelReason::Failures => "failures",
        CancelReason::Replaced => "replaced",
        CancelReason::Completed => "completed",
        CancelReason::ThreadClosed => "thread_closed",
    }
}
