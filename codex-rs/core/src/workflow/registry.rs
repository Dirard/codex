//! Runs of one thread (UC1, R1.5; UC9, R9.3).

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use codex_workflow::RunStatus;
use tokio::sync::broadcast;

use super::run::WorkflowRun;
use crate::session::session::Session;

/// Capacity of the change channel; slow subscribers re-read snapshots after a lag.
const UPDATE_CHANNEL_CAPACITY: usize = 256;

/// Workflow runs of a thread, kept in the thread's extension data.
pub(crate) struct WorkflowRegistry {
    runs: Mutex<Vec<Arc<WorkflowRun>>>,
    updates: broadcast::Sender<String>,
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self {
            runs: Mutex::new(Vec::new()),
            updates: broadcast::channel(UPDATE_CHANNEL_CAPACITY).0,
        }
    }
}

impl WorkflowRegistry {
    /// The registry of `session`, created on first use.
    pub(crate) fn for_session(session: &Session) -> Arc<Self> {
        session
            .services
            .thread_extension_data
            .get_or_init(Self::default)
    }

    /// The registry of `session` if a run was ever started there.
    pub(crate) fn existing(session: &Session) -> Option<Arc<Self>> {
        session.services.thread_extension_data.get::<Self>()
    }

    fn runs(&self) -> std::sync::MutexGuard<'_, Vec<Arc<WorkflowRun>>> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ids of runs that are not terminal yet.
    pub(crate) fn active_run_ids(&self) -> Vec<String> {
        self.runs()
            .iter()
            .filter(|run| !run.status().is_terminal())
            .map(|run| run.id.clone())
            .collect()
    }

    /// A fresh `wf-` id with eight hex characters, unique in this thread: not used by a run of
    /// this process nor `taken` by an earlier one (R6.7).
    pub(crate) fn allocate_id(&self, taken: impl Fn(&str) -> bool) -> String {
        let runs = self.runs();
        loop {
            let id = format!("wf-{:08x}", rand::random::<u32>());
            if !runs.iter().any(|run| run.id == id) && !taken(&id) {
                return id;
            }
        }
    }

    /// Register a run rebuilt from disk unless a run with its id is already known; returns
    /// the registered run (R6.4, R6.7).
    pub(crate) fn insert_restored(&self, run: Arc<WorkflowRun>) -> Arc<WorkflowRun> {
        let mut runs = self.runs();
        if let Some(existing) = runs.iter().find(|known| known.id == run.id) {
            return Arc::clone(existing);
        }
        runs.push(Arc::clone(&run));
        run
    }

    pub(crate) fn insert(&self, run: Arc<WorkflowRun>) {
        self.runs().push(run);
    }

    pub(crate) fn get(&self, run_id: &str) -> Option<Arc<WorkflowRun>> {
        self.runs().iter().find(|run| run.id == run_id).cloned()
    }

    /// Every run of the thread in start order.
    pub(crate) fn all(&self) -> Vec<Arc<WorkflowRun>> {
        self.runs().clone()
    }

    /// Sender of run ids whose snapshot changed.
    pub(crate) fn update_sender(&self) -> broadcast::Sender<String> {
        self.updates.clone()
    }

    /// Run ids whose snapshot changed, for clients such as app-server.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<String> {
        self.updates.subscribe()
    }

    /// UC9, R9.3: the thread shuts down; interrupt every active run and wait for it.
    pub(crate) async fn shutdown(&self) {
        let runs: Vec<Arc<WorkflowRun>> = self
            .runs()
            .iter()
            .filter(|run| !run.status().is_terminal())
            .cloned()
            .collect();
        for run in &runs {
            run.cancel(RunStatus::Interrupted);
        }
        for run in runs {
            run.wait_finished().await;
        }
    }
}
