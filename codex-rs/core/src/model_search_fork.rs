//! Fork: one Search discovery per turn even when a later step recaptures its MCP binding
//! (`docs/fork/reports/2026-11-07-ci-web-search-race.md`).
//!
//! The runtime does not cache a binding captured while a required server was still starting,
//! so the step after the Apps server became ready gets an equivalent binding with a new
//! identity. Keying the turn's discovery on that identity alone sent the scoped `tools/list`
//! again on every such step.

use std::sync::Arc;

use codex_mcp::McpBinding;
use serde_json::Map;
use serde_json::Value;

use super::CachedSearch;
use crate::session::turn_context::TurnContext;

/// The turn's discovery for the same request, projected onto `base` when it reaches Search
/// over the same Apps connection. Replaces the cached entry so later steps on `base` reuse it.
pub(super) fn rebase_cached_search(
    turn: &TurnContext,
    base: &Arc<McpBinding>,
    request: &Map<String, Value>,
) -> Option<Arc<CachedSearch>> {
    let cached = turn
        .extension_data
        .get::<CachedSearch>()
        .filter(|cached| cached.request == *request)?;
    let discovery = base.fork_rebase_model_search(&cached.base, &cached.discovery)?;
    let rebased = Arc::new(CachedSearch {
        base: Arc::clone(base),
        request: request.clone(),
        discovery,
    });
    turn.extension_data.insert_arc(Arc::clone(&rebased));
    Some(rebased)
}
