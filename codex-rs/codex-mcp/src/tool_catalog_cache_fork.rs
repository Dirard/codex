//! Fork: the `readOnlyHint` of cached tools as a visibility hint for read-only explorers
//! (`specs/explorer-and-questions-2026-10-25.md`, R2.9; review 2026-10-27, P2).
//!
//! The cache drops tool annotations, because approval and parallelism decisions may only use
//! the annotations of a live connection. A read-only explorer that takes its catalog from the
//! cache still has to decide which of those tools to show; it uses the hint kept here, and the
//! call itself checks the live annotations again.

use std::collections::HashSet;
use std::sync::Arc;

use super::McpToolCatalogCacheContext;
use super::lock_unpoisoned;
use crate::ToolInfo;

/// Raw names of the tools that their server annotated `readOnlyHint: true`.
pub(crate) fn read_only_tool_names<'a>(
    tools: impl IntoIterator<Item = &'a ToolInfo>,
) -> Arc<HashSet<String>> {
    Arc::new(
        tools
            .into_iter()
            .filter(|tool| {
                tool.tool
                    .annotations
                    .as_ref()
                    .and_then(|annotations| annotations.read_only_hint)
                    == Some(true)
            })
            .map(|tool| tool.tool.name.to_string())
            .collect(),
    )
}

impl McpToolCatalogCacheContext {
    /// Raw names of the cached tools that the live `tools/list` behind the cached catalog
    /// annotated `readOnlyHint: true`; `None` while the cache holds no usable catalog.
    pub(crate) fn fork_read_only_tool_names(&self) -> Option<Arc<HashSet<String>>> {
        let state = lock_unpoisoned(&self.entry.state);
        if state.disabled_by_server {
            return None;
        }
        state
            .snapshot
            .as_ref()
            .filter(|snapshot| self.accepts_snapshot(snapshot))
            .map(|snapshot| Arc::clone(&snapshot.read_only_tools))
    }
}

#[cfg(test)]
#[path = "tool_catalog_cache_fork_tests.rs"]
mod tests;
