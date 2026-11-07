//! Fork: app-server API types for scheduled wakeups and `/loop`
//! (`specs/scheduled-wakeups-2026-11-07.md`, UC8). Every method and the notification are
//! experimental.

use crate::JsonSchema;
use crate::TS;
use serde::Deserialize;
use serde::Serialize;

/// A thread's pending wakeup, its `/loop` and the model's wakeup chain.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupState {
    pub wakeup: Option<ThreadWakeup>,
    #[serde(rename = "loop")]
    #[ts(rename = "loop")]
    pub loop_state: Option<ThreadLoop>,
    pub chain: ThreadWakeupChain,
}

/// Who scheduled a wakeup.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ThreadWakeupOrigin {
    Model,
    Loop,
}

/// The pending wakeup of a thread.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeup {
    pub id: String,
    pub origin: ThreadWakeupOrigin,
    /// Unix time in seconds.
    #[ts(type = "number")]
    pub due_at: i64,
    /// The effective delay in seconds.
    pub delay_seconds: u32,
    pub requested_delay_seconds: u32,
    /// Due and waiting for the thread to become idle.
    pub due: bool,
    /// Model-authored; control characters are replaced.
    pub reason: Option<String>,
    /// The model's note for the wakeup.
    pub prompt: Option<String>,
    pub scheduled_in_turn_id: Option<String>,
}

/// The `/loop` of a thread.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadLoop {
    pub id: String,
    /// The user's prompt each iteration runs.
    pub prompt: String,
    /// Null for a self-paced loop, whose next iteration the model schedules.
    pub interval_seconds: Option<u32>,
    /// Iterations started so far.
    pub iteration: u32,
    pub max_iterations: u32,
    /// Unix time in seconds.
    #[ts(type = "number")]
    pub started_at: i64,
    /// Unix time in seconds.
    #[ts(type = "number")]
    pub ends_at: i64,
}

/// The model's wakeups since the user's last message.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupChain {
    pub fired: u32,
    pub max_wakeups: u32,
    /// Unix time in seconds; null before the chain's first wakeup.
    #[ts(type = "number | null")]
    pub ends_at: Option<i64>,
}

/// What changed in `thread/wakeup/updated`.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ThreadWakeupChange {
    Scheduled,
    Replaced,
    Fired,
    Cancelled,
    LoopStarted,
    LoopEnded,
}

/// Why a wakeup or loop stopped.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum ThreadWakeupCancelReason {
    Model,
    User,
    Interrupt,
    Limit,
    Failures,
    Replaced,
    Completed,
    ThreadClosed,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupUpdatedNotification {
    pub thread_id: String,
    pub change: ThreadWakeupChange,
    pub cancel_reason: Option<ThreadWakeupCancelReason>,
    /// The turn a `fired` change started.
    pub turn_id: Option<String>,
    pub state: ThreadWakeupState,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupReadParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupReadResponse {
    /// Whether the thread may use wakeups: the feature is on and the thread qualifies.
    pub available: bool,
    pub state: ThreadWakeupState,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupCancelParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWakeupCancelResponse {
    /// Whether a wakeup or loop was cancelled; false when a wakeup's turn already started.
    pub cancelled: bool,
    pub state: ThreadWakeupState,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadLoopStartParams {
    pub thread_id: String,
    /// The prompt each iteration runs as user input.
    pub prompt: String,
    /// Repeat this many seconds after each iteration ends; omit for a self-paced loop.
    #[serde(default)]
    pub interval_seconds: Option<u32>,
    /// At most this many iterations; defaults to `features.scheduled_wakeups.max_wakeups`.
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadLoopStartResponse {
    pub state: ThreadWakeupState,
}
