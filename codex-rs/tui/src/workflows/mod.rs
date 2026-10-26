//! Fork: the `/workflows` screen (`specs/dynamic-workflow-2026-10-25.md`, UC 8.1, R8.6).
//!
//! This module holds the pure parts of the screen: its app events, the selection views of the
//! run list and of one run, and the `/subagents` layout that groups workflow agents under
//! their run. The state of an open screen lives in `chatwidget/workflow.rs`; app-server
//! requests and the picker hooks live in `app/workflows_fork.rs`.

pub(crate) mod picker;
#[cfg(test)]
pub(crate) mod test_fixtures;
pub(crate) mod view;

use codex_app_server_protocol::WorkflowRun;
use codex_protocol::ThreadId;

/// Payload of `AppEvent::Workflows`: user actions of the screen and the app-server answers
/// that come back to it. Rare and short-lived like the `AppEvent` that carries it, so the
/// inline run snapshot is not boxed.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum WorkflowsEvent {
    /// `/workflows`: list the runs of the session's primary thread.
    OpenList,
    /// Show one run, from a list row or from a run row of `/subagents`.
    OpenRun { thread_id: ThreadId, run_id: String },
    /// Cancel a run that is not terminal.
    Cancel { thread_id: ThreadId, run_id: String },
    /// The open run view wants its agent rows again after an update.
    Refresh { thread_id: ThreadId, run_id: String },
    /// Answer of `thread/workflow/list`.
    ListLoaded {
        thread_id: ThreadId,
        result: Result<Vec<WorkflowRun>, String>,
    },
    /// Answer of `thread/workflow/read` or `thread/workflow/cancel`.
    RunLoaded {
        thread_id: ThreadId,
        run_id: String,
        origin: RunLoadOrigin,
        result: Result<WorkflowRun, String>,
    },
}

/// Why a run was loaded; decides whether the answer may open a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunLoadOrigin {
    /// The user opened the run; the answer shows or replaces its view.
    Open,
    /// An update asked for fresh agent rows; the answer only replaces an open view.
    Refresh,
    /// `thread/workflow/cancel` settled; the answer carries no agent rows.
    Cancel,
}
