//! Runs of one thread (UC1, R1.5; UC9, R9.3).

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use codex_workflow::RunSnapshot;
use codex_workflow::RunStatus;
use tokio::sync::broadcast;

use super::run::WorkflowRun;
use crate::session::session::Session;

/// Capacity of the change channel; slow subscribers re-read snapshots after a lag.
const UPDATE_CHANNEL_CAPACITY: usize = 256;

/// Workflow runs of a thread, kept in the thread's extension data.
pub(crate) struct WorkflowRegistry {
    runs: Mutex<Vec<Arc<WorkflowRun>>>,
    /// R6.8: index entries of finished runs of earlier processes, listed until a lookup by
    /// id registers the run itself.
    past: Mutex<Vec<RunSnapshot>>,
    /// P3-12: runs of earlier processes rebuilt for `thread/workflow/read`, most recently
    /// read last; not registered, so they never join the thread's runs.
    read_cache: Mutex<VecDeque<Arc<WorkflowRun>>>,
    updates: broadcast::Sender<String>,
}

/// Runs of earlier processes the read cache keeps.
const READ_CACHE_RUNS: usize = 8;

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self {
            runs: Mutex::new(Vec::new()),
            past: Mutex::new(Vec::new()),
            read_cache: Mutex::new(VecDeque::new()),
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

    /// P3-12: a run of an earlier process read before and still in the read cache.
    pub(crate) fn cached_read(&self, run_id: &str) -> Option<Arc<WorkflowRun>> {
        let mut cache = self
            .read_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let position = cache.iter().position(|run| run.id == run_id)?;
        let run = cache.remove(position)?;
        cache.push_back(Arc::clone(&run));
        Some(run)
    }

    /// P3-12: keep a run rebuilt for reading, dropping the least recently read one beyond
    /// [`READ_CACHE_RUNS`].
    pub(crate) fn cache_read(&self, run: Arc<WorkflowRun>) {
        let mut cache = self
            .read_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        cache.retain(|cached| cached.id != run.id);
        cache.push_back(run);
        while cache.len() > READ_CACHE_RUNS {
            cache.pop_front();
        }
    }

    /// R6.8: keep the index entries of the thread's runs of earlier processes.
    pub(crate) fn set_past(&self, past: Vec<RunSnapshot>) {
        *self.past.lock().unwrap_or_else(PoisonError::into_inner) = past;
    }

    /// R6.8: summaries of the thread's runs: runs of earlier processes that the registry does
    /// not hold, in start order and at most the `max_past` most recent of them, then the
    /// registered runs. Returns them with the number of earlier runs left out.
    pub(crate) fn summaries(&self, max_past: usize) -> (Vec<RunSnapshot>, usize) {
        let registered: Vec<RunSnapshot> = self
            .all()
            .iter()
            .map(|run| run.snapshot(/*include_agents*/ false))
            .collect();
        let mut summaries: Vec<RunSnapshot> = self
            .past
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|past| !registered.iter().any(|run| run.run_id == past.run_id))
            .cloned()
            .collect();
        summaries.sort_by_key(|past| past.started_at_ms);
        let omitted = summaries.len().saturating_sub(max_past);
        summaries.drain(..omitted);
        summaries.extend(registered);
        (summaries, omitted)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::run::RestoredRun;
    use codex_workflow::Journal;
    use codex_workflow::RunTracker;
    use codex_workflow::WorkflowMeta;
    use pretty_assertions::assert_eq;

    fn summary(run_id: &str, started_at_ms: i64) -> RunSnapshot {
        let mut tracker = RunTracker::new(
            run_id.to_string(),
            run_id.to_string(),
            String::new(),
            &[],
            /*budget_tokens*/ None,
            started_at_ms,
        );
        tracker.finish(RunStatus::Completed, started_at_ms + 1, None, None);
        tracker.snapshot(/*include_agents*/ false)
    }

    /// R6.8: past runs come first in start order, a registered run replaces its index entry,
    /// and only the most recent past runs are kept when asked.
    #[test]
    fn summaries_put_past_runs_before_registered_ones() {
        let registry = WorkflowRegistry::default();
        registry.set_past(vec![
            summary("wf-00000003", 30),
            summary("wf-00000001", 10),
            summary("wf-00000002", 20),
        ]);
        let mut tracker = RunTracker::new(
            "wf-00000002".to_string(),
            "registered".to_string(),
            String::new(),
            &[],
            None,
            20,
        );
        tracker.finish(RunStatus::Interrupted, 21, None, None);
        registry.insert_restored(Arc::new(WorkflowRun::restored(
            RestoredRun {
                id: "wf-00000002".to_string(),
                meta: WorkflowMeta {
                    name: "registered".to_string(),
                    description: String::new(),
                    phases: Vec::new(),
                },
                tracker,
                journal: Journal::in_memory(),
                result_json: None,
            },
            registry.update_sender(),
        )));

        let names = |runs: &[RunSnapshot]| -> Vec<(String, String)> {
            runs.iter()
                .map(|run| (run.run_id.clone(), run.name.clone()))
                .collect()
        };
        let (all, omitted) = registry.summaries(usize::MAX);
        assert_eq!(omitted, 0);
        assert_eq!(
            names(&all),
            vec![
                ("wf-00000001".to_string(), "wf-00000001".to_string()),
                ("wf-00000003".to_string(), "wf-00000003".to_string()),
                ("wf-00000002".to_string(), "registered".to_string()),
            ]
        );
        let (recent, omitted) = registry.summaries(1);
        assert_eq!(omitted, 1);
        assert_eq!(
            names(&recent),
            vec![
                ("wf-00000003".to_string(), "wf-00000003".to_string()),
                ("wf-00000002".to_string(), "registered".to_string()),
            ]
        );
    }
}
