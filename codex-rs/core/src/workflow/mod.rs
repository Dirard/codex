//! Fork: background workflow runs (`specs/dynamic-workflow-2026-10-25.md`).
//!
//! The `workflow` tool validates a script with `codex-workflow`, registers a run on the
//! thread and executes the script in its own Code Mode session with a run-owned delegate,
//! so the run makes progress whether or not the parent thread has a turn. `agent()` calls
//! start fresh MAv2 children whose answers go to the script, never to the parent; the parent
//! receives one notification through the background-notification queue when the run ends.

mod agent_call;
mod delegate;
mod gate;
mod registry;
mod resume;
mod run;
mod saved;
mod thread_api;
mod tools;
mod ultra_mode;
mod usage;
mod world_state;

pub(crate) use registry::WorkflowRegistry;
pub(crate) use resume::fork_restore_runs;
pub(crate) use tools::LocalAgentBackend;
pub(crate) use tools::add_workflow_tools;
use tools::cancel_run;
pub(crate) use tools::workflows_available;
pub(crate) use ultra_mode::WorkflowUltraModeState;
pub(crate) use usage::fork_charge_child_usage;
pub(crate) use world_state::WorkflowRunsState;

/// Turn trigger of a turn started by a workflow completion notification (UC7).
pub(crate) const WORKFLOW_TURN_TRIGGER: &str = "workflow";

/// Milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}
