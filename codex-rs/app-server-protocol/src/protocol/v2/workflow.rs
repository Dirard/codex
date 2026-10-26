//! Fork: app-server API types for dynamic workflow runs
//! (`specs/dynamic-workflow-2026-10-25.md`, UC8 and UC9). Every method and the
//! notification are experimental.

use std::collections::BTreeMap;

use crate::JsonSchema;
use crate::TS;
pub use codex_extension_items::workflow::WorkflowRunStatus;
use serde::Deserialize;
use serde::Serialize;

/// Snapshot of a workflow run; `revision` grows on every change.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct WorkflowRun {
    pub run_id: String,
    pub name: String,
    pub description: String,
    pub status: WorkflowRunStatus,
    #[ts(type = "number")]
    pub revision: u64,
    pub current_phase: Option<String>,
    pub phases: Vec<WorkflowPhase>,
    /// `agent()` calls of the run by state.
    pub agent_counts: WorkflowAgentCounts,
    /// Calls that resolved to null, by reason.
    #[ts(type = "Record<string, number>")]
    pub null_reasons: BTreeMap<String, u64>,
    /// The last `log()` lines of the script.
    pub logs_tail: Vec<String>,
    /// Non-cached input plus output tokens of the run's agents.
    #[ts(type = "number")]
    pub charged_tokens: u64,
    #[ts(type = "number")]
    pub cached_tokens: u64,
    #[ts(type = "number | null")]
    pub budget_tokens: Option<u64>,
    /// Unix time in milliseconds.
    #[ts(type = "number")]
    pub started_at_ms: i64,
    /// Unix time in milliseconds; null while the run is active.
    #[ts(type = "number | null")]
    pub finished_at_ms: Option<i64>,
    pub result_preview: Option<String>,
    pub error: Option<String>,
    /// Every `agent()` call; returned only by `thread/workflow/read`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agents: Option<Vec<WorkflowAgent>>,
}

/// `agent()` calls attributed to a phase.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct WorkflowPhase {
    pub title: String,
    #[ts(type = "number")]
    pub started: u64,
    #[ts(type = "number")]
    pub finished: u64,
}

/// `agent()` calls of a run by state.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct WorkflowAgentCounts {
    #[ts(type = "number")]
    pub queued: u64,
    #[ts(type = "number")]
    pub running: u64,
    #[ts(type = "number")]
    pub awaiting_approval: u64,
    #[ts(type = "number")]
    pub completed: u64,
    /// Calls that resolved to null.
    #[serde(rename = "null")]
    #[ts(rename = "null", type = "number")]
    pub null_results: u64,
}

/// State of one `agent()` call.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum WorkflowAgentState {
    Queued,
    Running,
    AwaitingApproval,
    Completed,
    /// Resolved to null; `reason` says why.
    Null,
}

/// One `agent()` call of a run.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct WorkflowAgent {
    /// Call number in the script, starting at 1.
    #[ts(type = "number")]
    pub seq: u64,
    pub label: Option<String>,
    pub phase: Option<String>,
    pub state: WorkflowAgentState,
    /// Agent path of the child thread, once spawned.
    pub agent_path: Option<String>,
    pub thread_id: Option<String>,
    /// Why the call resolved to null.
    pub reason: Option<String>,
    #[ts(type = "number")]
    pub charged_tokens: u64,
    #[ts(type = "number")]
    pub cached_tokens: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowListParams {
    pub thread_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowListResponse {
    /// Runs of the loaded thread in start order, without `agents`.
    pub runs: Vec<WorkflowRun>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowReadParams {
    pub thread_id: String,
    pub run_id: String,
    /// When true, return the full result of a finished run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_result: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowReadResponse {
    /// The run with its `agents`.
    pub run: WorkflowRun,
    /// JSON text of the run result; set when `includeResult` was true and the run returned one.
    pub result: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowCancelParams {
    pub thread_id: String,
    pub run_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowCancelResponse {
    /// The run after cancellation settled; a finished run is returned unchanged.
    pub run: WorkflowRun,
}

/// A run of the thread changed. Sent at most every 250 ms per run, and always once with
/// the terminal snapshot.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadWorkflowUpdatedNotification {
    pub thread_id: String,
    /// The run without `agents`.
    pub run: WorkflowRun,
}

/// The public status of a core run status.
pub(crate) fn workflow_run_status(value: codex_workflow::RunStatus) -> WorkflowRunStatus {
    match value {
        codex_workflow::RunStatus::Running => WorkflowRunStatus::Running,
        codex_workflow::RunStatus::Completed => WorkflowRunStatus::Completed,
        codex_workflow::RunStatus::Failed => WorkflowRunStatus::Failed,
        codex_workflow::RunStatus::Cancelled => WorkflowRunStatus::Cancelled,
        codex_workflow::RunStatus::Interrupted => WorkflowRunStatus::Interrupted,
    }
}

impl From<codex_workflow::AgentState> for WorkflowAgentState {
    fn from(value: codex_workflow::AgentState) -> Self {
        match value {
            codex_workflow::AgentState::Queued => Self::Queued,
            codex_workflow::AgentState::Running => Self::Running,
            codex_workflow::AgentState::AwaitingApproval => Self::AwaitingApproval,
            codex_workflow::AgentState::Completed => Self::Completed,
            codex_workflow::AgentState::Null => Self::Null,
        }
    }
}

impl From<codex_workflow::AgentCounts> for WorkflowAgentCounts {
    fn from(value: codex_workflow::AgentCounts) -> Self {
        Self {
            queued: value.queued,
            running: value.running,
            awaiting_approval: value.awaiting_approval,
            completed: value.completed,
            null_results: value.null_results,
        }
    }
}

impl From<codex_workflow::AgentEntry> for WorkflowAgent {
    fn from(value: codex_workflow::AgentEntry) -> Self {
        Self {
            seq: value.seq,
            label: value.label,
            phase: value.phase,
            state: value.state.into(),
            agent_path: value.agent_path,
            thread_id: value.thread_id,
            reason: value.reason,
            charged_tokens: value.tokens.charged,
            cached_tokens: value.tokens.cached,
        }
    }
}

impl WorkflowRun {
    /// The public run of a core snapshot; `agents` is set only when `include_agents`.
    pub fn from_snapshot(snapshot: codex_workflow::RunSnapshot, include_agents: bool) -> Self {
        let codex_workflow::RunSnapshot {
            run_id,
            name,
            description,
            status,
            revision,
            current_phase,
            phases,
            agents,
            null_reasons,
            logs_tail,
            charged_tokens,
            cached_tokens,
            budget_tokens,
            started_at_ms,
            finished_at_ms,
            result_preview,
            error,
            agent_list,
        } = snapshot;
        Self {
            run_id,
            name,
            description,
            status: workflow_run_status(status),
            revision,
            current_phase,
            phases: phases
                .into_iter()
                .map(|phase| WorkflowPhase {
                    title: phase.title,
                    started: phase.started,
                    finished: phase.finished,
                })
                .collect(),
            agent_counts: agents.into(),
            null_reasons,
            logs_tail,
            charged_tokens,
            cached_tokens,
            budget_tokens,
            started_at_ms,
            finished_at_ms,
            result_preview,
            error,
            agents: include_agents
                .then(|| agent_list.into_iter().map(WorkflowAgent::from).collect()),
        }
    }
}

#[cfg(test)]
#[path = "workflow_tests.rs"]
mod tests;
