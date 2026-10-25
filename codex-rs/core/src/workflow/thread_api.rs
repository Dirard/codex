//! Workflow runs of a thread for clients: app-server, TUI and `codex exec` (UC8, UC9, R7.5).

use codex_workflow::RunSnapshot;
use tokio::sync::broadcast;

use super::registry::WorkflowRegistry;
use crate::codex_thread::CodexThread;

impl CodexThread {
    /// Fork: snapshots of every workflow run of this thread, without per-call lists.
    pub fn workflow_runs(&self) -> Vec<RunSnapshot> {
        WorkflowRegistry::existing(&self.session)
            .map(|registry| {
                registry
                    .all()
                    .iter()
                    .map(|run| run.snapshot(/*include_agents*/ false))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Fork: one run's snapshot with its per-call list, and its full result when asked.
    pub fn workflow_run(
        &self,
        run_id: &str,
        include_result: bool,
    ) -> Option<(RunSnapshot, Option<String>)> {
        let run = WorkflowRegistry::existing(&self.session)?.get(run_id)?;
        let result = include_result.then(|| run.result_json()).flatten();
        Some((run.snapshot(/*include_agents*/ true), result))
    }

    /// Fork: cancel a run (R9.1); a finished run is returned unchanged (R9.4).
    pub async fn cancel_workflow_run(&self, run_id: &str) -> Option<RunSnapshot> {
        let run = WorkflowRegistry::existing(&self.session)?.get(run_id)?;
        Some(super::cancel_run(&run).await)
    }

    /// Fork: ids of runs whose snapshot changed; subscribe before reading snapshots.
    pub fn subscribe_workflow_updates(&self) -> broadcast::Receiver<String> {
        WorkflowRegistry::for_session(&self.session).subscribe()
    }

    /// Fork: ids of runs that have not finished yet.
    pub fn active_workflow_run_ids(&self) -> Vec<String> {
        WorkflowRegistry::existing(&self.session)
            .map(|registry| registry.active_run_ids())
            .unwrap_or_default()
    }
}
