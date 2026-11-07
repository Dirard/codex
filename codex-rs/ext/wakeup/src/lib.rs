//! Fork: scheduled wakeups and `/loop` (`specs/scheduled-wakeups-2026-11-07.md`).
//!
//! The model ends its turn with `clock.schedule_wakeup` and the thread resumes later with
//! its note as hidden input; `clock.cancel_wakeup` stops it. A client repeats a user prompt
//! with `thread/loop/start`. Wakeups live in this extension, start turns only through
//! `CodexThread::start_turn_if_idle` and last only while the thread runtime runs (D2, D3, D5).

mod api;
mod extension;
mod fragments;
mod metrics;
mod scheduler;
mod state;
#[cfg(test)]
mod test_support;
mod tools;

pub use api::cancel;
pub use api::read;
pub use api::start_loop;
pub use api::subscribe;
pub use api::thread_state;
pub use extension::WakeupExtension;
pub use extension::WakeupExtensionConfig;
pub use extension::install;
pub use state::CancelReason;
pub use state::ChainView;
pub use state::LoopStartRequest;
pub use state::LoopView;
pub use state::UpdateSink;
pub use state::WakeupChange;
pub use state::WakeupOrigin;
pub use state::WakeupSnapshot;
pub use state::WakeupThreadState;
pub use state::WakeupUpdate;
pub use state::WakeupView;
pub use tools::CANCEL_WAKEUP_TOOL_NAME;
pub use tools::CLOCK_NAMESPACE;
pub use tools::SCHEDULE_WAKEUP_TOOL_NAME;
