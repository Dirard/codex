//! Fork: agents started by workflow runs (`specs/dynamic-workflow-2026-10-25.md`, UC3, UC4).
//!
//! A run registers each child's agent path before spawning it, so even a child that finishes
//! its first turn immediately reports its terminal outcome to the run and never to the
//! parent's mailbox. Registration also selects the separate residency pool and bypasses the
//! tree's execution limiter: a run's own semaphore bounds its agents.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::PoisonError;

use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::CodexErrorInfo;
use codex_protocol::protocol::SessionSource;
use tokio::sync::mpsc;

use super::LocalAgentRuntime;
use crate::agent::api::AgentTurnOutcome;
use crate::codex_thread::CodexThread;

/// Terminal outcome of one turn of a workflow child.
#[derive(Clone, Debug)]
pub(crate) struct WorkflowChildOutcome {
    pub(crate) status: AgentStatus,
    pub(crate) error_info: Option<CodexErrorInfo>,
    /// The explorer profile's result limit for this child, if any.
    pub(crate) result_max_tokens: Option<usize>,
}

#[derive(Default)]
pub(crate) struct WorkflowChildren {
    state: Mutex<WorkflowChildrenState>,
}

#[derive(Default)]
struct WorkflowChildrenState {
    by_path: HashMap<String, WorkflowChildEntry>,
    by_thread: HashMap<ThreadId, String>,
}

struct WorkflowChildEntry {
    run_id: String,
    /// Present while the run still waits for this child's answer.
    outcomes: Option<mpsc::UnboundedSender<WorkflowChildOutcome>>,
}

impl WorkflowChildren {
    fn state(&self) -> std::sync::MutexGuard<'_, WorkflowChildrenState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Mark `path` as a child of `run_id` before it is spawned and return its outcomes.
    pub(crate) fn register(
        &self,
        path: &AgentPath,
        run_id: &str,
    ) -> mpsc::UnboundedReceiver<WorkflowChildOutcome> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.state().by_path.insert(
            path.to_string(),
            WorkflowChildEntry {
                run_id: run_id.to_string(),
                outcomes: Some(tx),
            },
        );
        rx
    }

    pub(crate) fn bind_thread(&self, path: &AgentPath, thread_id: ThreadId) {
        self.state().by_thread.insert(thread_id, path.to_string());
    }

    /// The run no longer waits for answers of `path`; it stays a workflow child.
    pub(crate) fn finish_call(&self, path: &AgentPath) {
        if let Some(entry) = self.state().by_path.get_mut(path.as_str()) {
            entry.outcomes = None;
        }
    }

    /// The run no longer waits for the child running as `thread_id`.
    pub(crate) fn finish_call_for_thread(&self, thread_id: ThreadId) {
        let mut state = self.state();
        let Some(path) = state.by_thread.get(&thread_id).cloned() else {
            return;
        };
        if let Some(entry) = state.by_path.get_mut(&path) {
            entry.outcomes = None;
        }
    }

    /// Forget a registration whose spawn failed.
    pub(crate) fn forget(&self, path: &AgentPath) {
        let mut state = self.state();
        state.by_path.remove(path.as_str());
        state
            .by_thread
            .retain(|_, registered| registered != path.as_str());
    }

    /// Route a terminal outcome to the owning run. Returns whether `path` belongs to a
    /// workflow, in which case the parent must not be notified.
    pub(crate) fn deliver(&self, path: &AgentPath, outcome: &AgentTurnOutcome) -> bool {
        let state = self.state();
        let Some(entry) = state.by_path.get(path.as_str()) else {
            return false;
        };
        if let Some(outcomes) = &entry.outcomes {
            let _ = outcomes.send(WorkflowChildOutcome {
                status: outcome.status.clone(),
                error_info: outcome.error_info.clone(),
                result_max_tokens: outcome.result_max_tokens,
            });
        }
        true
    }

    /// Run that owns the agent at `path`.
    /// `list_agents` names the run that owns an agent as `workflow:<run_id>`; finished
    /// workflow agents are omitted, since one run may start hundreds of them (R3.3).
    pub(crate) fn listed_owner(
        &self,
        agent: crate::agent::types::LiveAgent,
    ) -> Option<(crate::agent::types::LiveAgent, Option<String>)> {
        let owner = agent
            .metadata
            .agent_path
            .as_ref()
            .and_then(|path| self.owner(path.as_str()))
            .map(|run_id| format!("workflow:{run_id}"));
        if owner.is_some() && crate::agent::status::is_final(&agent.status) {
            return None;
        }
        Some((agent, owner))
    }

    pub(crate) fn owner(&self, path: &str) -> Option<String> {
        self.state()
            .by_path
            .get(path)
            .map(|entry| entry.run_id.clone())
    }

    pub(crate) fn is_workflow_thread(&self, thread_id: ThreadId) -> bool {
        self.state().by_thread.contains_key(&thread_id)
    }

    /// Whether a run still waits for this child, which then must stay loaded.
    pub(crate) fn has_pending_call(&self, thread_id: ThreadId) -> bool {
        let state = self.state();
        state
            .by_thread
            .get(&thread_id)
            .and_then(|path| state.by_path.get(path))
            .is_some_and(|entry| entry.outcomes.is_some())
    }

    pub(crate) fn is_workflow_source(&self, source: &SessionSource) -> bool {
        source
            .get_agent_path()
            .is_some_and(|path| self.state().by_path.contains_key(path.as_str()))
    }
}

impl LocalAgentRuntime {
    pub(crate) fn workflow_children(&self) -> &Arc<WorkflowChildren> {
        &self.workflow_children
    }

    /// Checkpoint an isolated workflow child's worktree and describe how to apply its changes.
    pub(crate) async fn workflow_child_worktree_report(
        &self,
        thread_id: ThreadId,
    ) -> Option<String> {
        let binding = self.worktrees.get(thread_id)?;
        Some(crate::agent::worktree_isolation::checkpoint_report(&binding).await)
    }

    /// The loaded runtime of a workflow child.
    pub(crate) async fn workflow_child_thread(
        &self,
        thread_id: ThreadId,
    ) -> CodexResult<Arc<CodexThread>> {
        self.upgrade()?.get_thread(thread_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn outcome(path: &AgentPath) -> AgentTurnOutcome {
        AgentTurnOutcome {
            thread_id: ThreadId::new(),
            turn_id: "turn-1".to_string(),
            source: SessionSource::SubAgent(
                codex_protocol::protocol::SubAgentSource::ThreadSpawn {
                    parent_thread_id: ThreadId::new(),
                    depth: 1,
                    agent_path: Some(path.clone()),
                    agent_nickname: None,
                    agent_role: None,
                },
            ),
            parent_turn_id: None,
            initiating_agent_path: None,
            status: AgentStatus::Completed(Some("done".to_string())),
            error_info: None,
            result_max_tokens: None,
        }
    }

    #[test]
    fn outcomes_reach_the_run_and_registration_outlives_the_call() {
        let children = WorkflowChildren::default();
        let path = AgentPath::root().join("wf_1_1").expect("path");
        let other = AgentPath::root().join("worker").expect("path");
        let thread_id = ThreadId::new();
        let mut outcomes = children.register(&path, "wf-00000001");
        children.bind_thread(&path, thread_id);
        assert!(children.has_pending_call(thread_id));
        assert!(children.deliver(&path, &outcome(&path)));
        assert!(!children.deliver(&other, &outcome(&other)));
        assert_eq!(
            outcomes.try_recv().expect("outcome").status,
            AgentStatus::Completed(Some("done".to_string()))
        );

        children.finish_call(&path);
        assert!(!children.has_pending_call(thread_id));
        assert!(children.is_workflow_thread(thread_id));
        assert_eq!(
            children.owner(path.as_str()).as_deref(),
            Some("wf-00000001")
        );
        // Later turns stay with the workflow and are dropped.
        assert!(children.deliver(&path, &outcome(&path)));

        children.forget(&path);
        assert!(!children.is_workflow_thread(thread_id));
        assert_eq!(children.owner(path.as_str()), None);
    }
}
