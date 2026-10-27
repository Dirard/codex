//! Workflow runs of a thread for clients: app-server, TUI and `codex exec` (UC8, UC9, R7.5).

use std::sync::Arc;

use codex_workflow::RunSnapshot;
use tokio::sync::broadcast;

use super::registry::WorkflowRegistry;
use super::run::WorkflowRun;
use crate::codex_thread::CodexThread;

impl CodexThread {
    /// Fork: snapshots of every workflow run of this thread, without per-call lists: runs of
    /// earlier processes from the run index, then the runs of this process (R6.8, R8.8).
    pub fn workflow_runs(&self) -> Vec<RunSnapshot> {
        WorkflowRegistry::existing(&self.session)
            .map(|registry| registry.summaries(usize::MAX).0)
            .unwrap_or_default()
    }

    /// Fork: one run's snapshot with its per-call list, and its full result when asked; a run
    /// of an earlier process is rebuilt from its journal first (R6.7, R8.8).
    pub async fn read_workflow_run(
        &self,
        run_id: &str,
        include_result: bool,
    ) -> Option<(RunSnapshot, Option<String>)> {
        let run = self.find_workflow_run(run_id).await?;
        let result = include_result.then(|| run.result_json()).flatten();
        Some((run.snapshot(/*include_agents*/ true), result))
    }

    /// A run of this thread by id, rebuilt from its journal when an earlier process ran it.
    async fn find_workflow_run(&self, run_id: &str) -> Option<Arc<WorkflowRun>> {
        let config = self.session.get_config().await;
        super::resume::find_or_restore_run(&self.session, &config, run_id)
            .await
            .ok()
    }

    /// Fork: one registered run's snapshot with its per-call list, and its full result when
    /// asked; [`Self::read_workflow_run`] also finds runs of earlier processes.
    pub fn workflow_run(
        &self,
        run_id: &str,
        include_result: bool,
    ) -> Option<(RunSnapshot, Option<String>)> {
        let run = WorkflowRegistry::existing(&self.session)?.get(run_id)?;
        let result = include_result.then(|| run.result_json()).flatten();
        Some((run.snapshot(/*include_agents*/ true), result))
    }

    /// Fork: cancel a run (R9.1); a finished run, including one of an earlier process, is
    /// returned unchanged (R9.4, R8.8).
    pub async fn cancel_workflow_run(&self, run_id: &str) -> Option<RunSnapshot> {
        let run = self.find_workflow_run(run_id).await?;
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
