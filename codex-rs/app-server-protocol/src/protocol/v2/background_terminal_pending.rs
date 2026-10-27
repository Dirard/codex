//! Fork: app-server API types for the background terminal notifications a thread still
//! owes the model (`specs/background-exec-notifications-2026-10-25.md`, UC7 R7.4).
//! `codex exec` asks for them before it ends; the method is experimental.

use crate::JsonSchema;
use crate::TS;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadBackgroundTerminalsPendingParams {
    pub thread_id: String,
}

/// What a loaded thread still owes the model from background terminals.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadBackgroundTerminalsPendingResponse {
    /// Live terminals whose exit Codex will still report, by session ID.
    pub terminals: Vec<PendingBackgroundTerminal>,
    /// A queued background notification will start a turn of the thread.
    pub notification_queued: bool,
    /// The turn running on the thread, if any.
    pub active_turn_id: Option<String>,
    /// A turn for pending work was reserved and is about to start.
    pub turn_starting: bool,
}

/// A live background terminal whose exit Codex will still report.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct PendingBackgroundTerminal {
    /// The `session_id` of the terminal.
    pub process_id: String,
    pub command: String,
    #[ts(type = "number")]
    pub running_for_ms: u64,
    /// The pattern of the terminal's output watch, if one is on.
    pub output_watch: Option<String>,
    /// `notify_on_exit` is on: the terminal is awaited until it exits, not only while its
    /// output watch expects news.
    #[serde(default)]
    pub notify_on_exit: bool,
    /// How much longer, in milliseconds, the output watch keeps the terminal awaited without
    /// news: until its first matching line or, after one, until it goes quiet. Absent without
    /// a watch that is still awaited.
    #[ts(type = "number | null")]
    pub output_watch_wait_ms: Option<u64>,
    /// The output watch matched (or took over from a watch that did), so it is awaited only
    /// while matching lines keep coming.
    #[serde(default)]
    pub output_watch_matched: bool,
}
