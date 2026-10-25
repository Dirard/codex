//! Observable state of a workflow run with a revision that grows on every change (UC8, R8.1).

use std::collections::BTreeMap;
use std::collections::HashMap;

use serde::Deserialize;
use serde::Serialize;

use crate::journal::TokenCharge;

/// Longest recorded `log()` message, in characters (R2.8).
pub const MAX_LOG_CHARS: usize = 2000;
/// Number of log lines kept in a snapshot (R8.1).
pub const LOG_TAIL_LINES: usize = 20;

/// Lifecycle status of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        self != Self::Running
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

/// State of one `agent()` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Queued,
    Running,
    AwaitingApproval,
    Completed,
    /// Resolved to null; the reason is recorded with the call.
    Null,
}

/// Agent calls of a run by state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCounts {
    pub queued: u64,
    pub running: u64,
    pub awaiting_approval: u64,
    pub completed: u64,
    #[serde(rename = "null")]
    pub null_results: u64,
}

impl AgentCounts {
    pub fn total(&self) -> u64 {
        self.queued + self.running + self.awaiting_approval + self.completed + self.null_results
    }

    fn slot(&mut self, state: AgentState) -> &mut u64 {
        match state {
            AgentState::Queued => &mut self.queued,
            AgentState::Running => &mut self.running,
            AgentState::AwaitingApproval => &mut self.awaiting_approval,
            AgentState::Completed => &mut self.completed,
            AgentState::Null => &mut self.null_results,
        }
    }
}

/// Agent calls attributed to one phase: entered and resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseSnapshot {
    pub title: String,
    pub started: u64,
    pub finished: u64,
}

/// One `agent()` call as shown to clients.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Call number in the script, starting at 1.
    pub seq: u64,
    pub label: Option<String>,
    pub phase: Option<String>,
    pub state: AgentState,
    /// Agent path of the child, once spawned.
    pub agent_path: Option<String>,
    pub thread_id: Option<String>,
    /// Why the call resolved to null.
    pub reason: Option<String>,
    pub tokens: TokenCharge,
}

/// Snapshot of a run (R8.1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSnapshot {
    pub run_id: String,
    pub name: String,
    pub description: String,
    pub status: RunStatus,
    pub revision: u64,
    pub current_phase: Option<String>,
    pub phases: Vec<PhaseSnapshot>,
    pub agents: AgentCounts,
    /// Null results by reason.
    pub null_reasons: BTreeMap<String, u64>,
    pub logs_tail: Vec<String>,
    pub charged_tokens: u64,
    pub cached_tokens: u64,
    pub budget_tokens: Option<u64>,
    /// Unix time in milliseconds.
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub result_preview: Option<String>,
    pub error: Option<String>,
    /// Every agent call; omitted from summaries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_list: Vec<AgentEntry>,
}

impl RunSnapshot {
    /// Milliseconds from start to finish, or to `now_ms` while running.
    pub fn duration_ms(&self, now_ms: i64) -> u64 {
        let end = self.finished_at_ms.unwrap_or(now_ms);
        u64::try_from(end.saturating_sub(self.started_at_ms)).unwrap_or(0)
    }
}

/// Builds snapshots from run events.
#[derive(Debug)]
pub struct RunTracker {
    snapshot: RunSnapshot,
    phase_index: HashMap<String, usize>,
    current_phase_seq: u64,
    agents: BTreeMap<u64, AgentEntry>,
}

impl RunTracker {
    pub fn new(
        run_id: String,
        name: String,
        description: String,
        declared_phases: &[String],
        budget_tokens: Option<u64>,
        started_at_ms: i64,
    ) -> Self {
        let mut tracker = Self {
            snapshot: RunSnapshot {
                run_id,
                name,
                description,
                status: RunStatus::Running,
                revision: 1,
                current_phase: None,
                phases: Vec::new(),
                agents: AgentCounts::default(),
                null_reasons: BTreeMap::new(),
                logs_tail: Vec::new(),
                charged_tokens: 0,
                cached_tokens: 0,
                budget_tokens,
                started_at_ms,
                finished_at_ms: None,
                result_preview: None,
                error: None,
                agent_list: Vec::new(),
            },
            phase_index: HashMap::new(),
            current_phase_seq: 0,
            agents: BTreeMap::new(),
        };
        for phase in declared_phases {
            tracker.phase_slot(phase);
        }
        tracker
    }

    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }

    pub fn status(&self) -> RunStatus {
        self.snapshot.status
    }

    /// The snapshot, with the per-call list only when `include_agents` is set.
    pub fn snapshot(&self, include_agents: bool) -> RunSnapshot {
        let mut snapshot = self.snapshot.clone();
        if include_agents {
            snapshot.agent_list = self.agents.values().cloned().collect();
        }
        snapshot
    }

    pub fn agent(&self, seq: u64) -> Option<&AgentEntry> {
        self.agents.get(&seq)
    }

    fn bump(&mut self) {
        self.snapshot.revision += 1;
    }

    fn phase_slot(&mut self, title: &str) -> usize {
        if let Some(index) = self.phase_index.get(title) {
            return *index;
        }
        let index = self.snapshot.phases.len();
        self.snapshot.phases.push(PhaseSnapshot {
            title: title.to_string(),
            started: 0,
            finished: 0,
        });
        self.phase_index.insert(title.to_string(), index);
        index
    }

    /// R2.8: `phase(title)` with the script's event number, so late arrivals of earlier
    /// calls do not move the current phase back.
    pub fn set_phase(&mut self, event_seq: u64, title: &str) {
        self.phase_slot(title);
        if event_seq >= self.current_phase_seq {
            self.current_phase_seq = event_seq;
            self.snapshot.current_phase = Some(title.to_string());
        }
        self.bump();
    }

    /// R2.8: record a `log()` message truncated to [`MAX_LOG_CHARS`]; returns it.
    pub fn log(&mut self, message: &str) -> String {
        let message = truncate_chars(message, MAX_LOG_CHARS);
        self.snapshot.logs_tail.push(message.clone());
        let excess = self.snapshot.logs_tail.len().saturating_sub(LOG_TAIL_LINES);
        self.snapshot.logs_tail.drain(..excess);
        self.bump();
        message
    }

    pub fn agent_queued(&mut self, seq: u64, label: Option<String>, phase: Option<String>) {
        if let Some(phase) = &phase {
            let index = self.phase_slot(phase);
            self.snapshot.phases[index].started += 1;
        }
        self.agents.insert(
            seq,
            AgentEntry {
                seq,
                label,
                phase,
                state: AgentState::Queued,
                agent_path: None,
                thread_id: None,
                reason: None,
                tokens: TokenCharge::default(),
            },
        );
        self.snapshot.agents.queued += 1;
        self.bump();
    }

    fn transition(&mut self, seq: u64, state: AgentState) -> Option<&mut AgentEntry> {
        let previous = self.agents.get(&seq)?.state;
        if previous == state {
            return self.agents.get_mut(&seq);
        }
        let counts = &mut self.snapshot.agents;
        let slot = counts.slot(previous);
        *slot = slot.saturating_sub(1);
        *counts.slot(state) += 1;
        let entry = self.agents.get_mut(&seq)?;
        entry.state = state;
        Some(entry)
    }

    pub fn agent_started(&mut self, seq: u64, agent_path: String, thread_id: String) {
        if let Some(entry) = self.transition(seq, AgentState::Running) {
            entry.agent_path = Some(agent_path);
            entry.thread_id = Some(thread_id);
        }
        self.bump();
    }

    /// R10.2: a running child waits for an approval, or stopped waiting.
    pub fn agent_awaiting_approval(&mut self, seq: u64, waiting: bool) {
        let Some(state) = self.agents.get(&seq).map(|entry| entry.state) else {
            return;
        };
        let target = match (state, waiting) {
            (AgentState::Running, true) => AgentState::AwaitingApproval,
            (AgentState::AwaitingApproval, false) => AgentState::Running,
            _ => return,
        };
        self.transition(seq, target);
        self.bump();
    }

    pub fn agent_finished(
        &mut self,
        seq: u64,
        ok: bool,
        reason: Option<&str>,
        tokens: TokenCharge,
    ) {
        let state = if ok {
            AgentState::Completed
        } else {
            AgentState::Null
        };
        let phase = self.agents.get(&seq).and_then(|entry| entry.phase.clone());
        if let Some(entry) = self.transition(seq, state) {
            entry.reason = reason.map(str::to_string);
            entry.tokens = tokens;
        }
        if let Some(phase) = phase {
            let index = self.phase_slot(&phase);
            self.snapshot.phases[index].finished += 1;
        }
        if !ok {
            *self
                .snapshot
                .null_reasons
                .entry(reason.unwrap_or("unknown").to_string())
                .or_default() += 1;
        }
        self.snapshot.charged_tokens = self.snapshot.charged_tokens.saturating_add(tokens.charged);
        self.snapshot.cached_tokens = self.snapshot.cached_tokens.saturating_add(tokens.cached);
        self.bump();
    }

    pub fn finish(
        &mut self,
        status: RunStatus,
        finished_at_ms: i64,
        result_preview: Option<String>,
        error: Option<String>,
    ) {
        if self.snapshot.status.is_terminal() {
            return;
        }
        self.snapshot.status = status;
        self.snapshot.finished_at_ms = Some(finished_at_ms);
        self.snapshot.result_preview = result_preview;
        self.snapshot.error = error;
        self.bump();
    }
}

/// `text` cut to at most `max_chars` characters, ending with an ellipsis when cut.
pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.char_indices().nth(max_chars).is_none() {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let end = text
        .char_indices()
        .nth(keep)
        .map_or(text.len(), |(index, _)| index);
    format!("{}…", &text[..end])
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
