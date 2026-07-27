//! Agent data and execution reservations shared by controllers and their callers.
//! Backend operations and local admission policy live in the implementations.

use crate::context::MultiAgentRoleInstructions;
use codex_protocol::AgentPath;
use codex_protocol::ThreadId;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::error::Result;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::TurnEnvironmentSelection;
use codex_protocol::turn_input::CyberAccessProgram;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

/// Registry identity shared by loaded and unloaded agents.
/// Registered agents have an `agent_id`; a reserved spawn can still be awaiting its ID.
#[derive(Clone, Debug, Default)]
pub struct AgentMetadata {
    pub agent_id: Option<ThreadId>,
    pub agent_path: Option<AgentPath>,
    pub agent_nickname: Option<String>,
    pub agent_role: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnAgentForkMode {
    FullHistory,
    LastNTurns(usize),
}

#[derive(Clone, Debug, Default)]
pub struct SpawnAgentOptions {
    pub fork_parent_spawn_call_id: Option<String>,
    pub fork_mode: Option<SpawnAgentForkMode>,
    pub parent_thread_id: Option<ThreadId>,
    pub parent_turn_id: Option<String>,
    /// Attribute delegated usage to the turn that initiated it.
    pub turn_trigger: Option<String>,
    pub root_turn_id: Option<String>,
    pub environments: Option<Vec<TurnEnvironmentSelection>>,
    pub multi_agent_v2_usage_hints: Option<ResolvedMultiAgentV2UsageHints>,
    pub cyber_access_program: Option<CyberAccessProgram>,
    pub turn_spawn_budget: Option<TurnSpawnBudget>,
}

/// Cumulative spawn budget for the root user input that requested a delegation.
#[derive(Clone, Debug)]
pub struct TurnSpawnBudget {
    inner: Arc<TurnSpawnBudgetInner>,
}

#[derive(Debug)]
struct TurnSpawnBudgetInner {
    limit: usize,
    reserved_or_committed: AtomicUsize,
}

impl TurnSpawnBudget {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            inner: Arc::new(TurnSpawnBudgetInner {
                limit,
                reserved_or_committed: AtomicUsize::new(0),
            }),
        }
    }

    pub(crate) fn reserve(&self) -> Result<TurnSpawnReservation> {
        let mut current = self.inner.reserved_or_committed.load(Ordering::Acquire);
        loop {
            if current >= self.inner.limit {
                return Err(CodexErr::new(CodexErrorDetails::AgentLimitReached {
                    max_threads: self.inner.limit,
                }));
            }
            match self.inner.reserved_or_committed.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(TurnSpawnReservation {
                        budget: self.clone(),
                        active: true,
                    });
                }
                Err(updated) => current = updated,
            }
        }
    }
}

pub(crate) struct TurnSpawnReservation {
    budget: TurnSpawnBudget,
    active: bool,
}

impl TurnSpawnReservation {
    pub(crate) fn commit(mut self) {
        self.active = false;
    }
}

impl Drop for TurnSpawnReservation {
    fn drop(&mut self) {
        if self.active {
            self.budget
                .inner
                .reserved_or_committed
                .fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// Identity and status observed from a loaded agent, without a handle to its runtime.
#[derive(Clone, Debug)]
pub struct LiveAgent {
    pub thread_id: ThreadId,
    pub metadata: AgentMetadata,
    pub status: AgentStatus,
}

#[derive(Clone, Debug, Default)]
pub struct ResolvedMultiAgentV2UsageHints {
    pub root: Option<MultiAgentRoleInstructions>,
    pub subagent: Option<MultiAgentRoleInstructions>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MessageDeliveryMode {
    /// Deliver to the mailbox without starting an idle agent.
    QueueOnly,
    /// Deliver to the active turn or start work if the agent is idle.
    TriggerTurn,
}

/// Keeps model-provided encrypted content distinct from text that needs a context wrapper.
pub enum AgentMessage {
    Plaintext(String),
    Encrypted(String),
}

/// Holds a backend-owned reservation until the turn ends or is cancelled.
///
/// The permit's destructor releases capacity or arranges backend cleanup. Remote backends
/// must also recover reservations after worker loss, when no Rust destructor can run.
#[must_use = "hold the execution guard for the lifetime of the admitted turn"]
pub struct AgentExecutionGuard {
    _permit: Box<dyn Send + Sync>,
}

impl AgentExecutionGuard {
    pub fn new(permit: impl Send + Sync + 'static) -> Self {
        Self {
            _permit: Box::new(permit),
        }
    }
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
