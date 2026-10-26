use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

// Fork: dynamic workflow (`specs/dynamic-workflow-2026-10-25.md`, UC8 R8.3).

/// Lifecycle status of a dynamic workflow run.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum WorkflowRunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl WorkflowRunStatus {
    pub fn is_terminal(self) -> bool {
        self != Self::Running
    }
}

/// Display item of a dynamic workflow run, emitted by the `workflow` tool.
///
/// The launching turn records the start with status `running`; the turn that receives
/// the completion notification shows the terminal status and the notification text.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct WorkflowItem {
    pub id: String,
    pub run_id: String,
    pub name: String,
    pub status: WorkflowRunStatus,
    /// Completion notification delivered to the model; set once the run is terminal.
    #[serde(default)]
    pub notification: Option<String>,
}
