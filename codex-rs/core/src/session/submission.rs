//! Core-owned queue metadata; forwarding transfers the residency guard with the operation.

use codex_protocol::protocol::Op;
use codex_protocol::protocol::W3cTraceContext;
use tokio::sync::OwnedRwLockReadGuard;

use crate::agent::types::TurnSpawnBudget;

#[derive(Debug)]
#[expect(dead_code, reason = "Turn ancestry is retained in Debug diagnostics.")]
pub(crate) struct Submission {
    pub id: String,
    pub op: Op,
    /// Optional W3C trace carrier propagated across async submission handoffs.
    pub trace: Option<W3cTraceContext>,
    pub parent_turn_id: Option<String>,
    pub root_turn_id: Option<String>,
    /// Runtime-only delegation epoch; never crosses the protocol boundary.
    pub turn_spawn_budget: Option<TurnSpawnBudget>,
    /// Keeps a V2 recipient resident until this submission is handled or dropped.
    pub residency_guard: Option<OwnedRwLockReadGuard<()>>,
}
