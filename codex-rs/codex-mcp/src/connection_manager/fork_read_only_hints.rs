//! Fork: cached `readOnlyHint`s of a server for read-only explorers
//! (`specs/explorer-and-questions-2026-10-25.md`, R2.9; review 2026-10-27, P2).

use std::collections::HashSet;
use std::sync::Arc;

use super::McpConnectionSet;
use crate::tool_catalog_cache::fork::read_only_tool_names;

impl McpConnectionSet {
    /// Raw names of `server_name`'s cached tools that its last live `tools/list` annotated
    /// `readOnlyHint: true`, from the Apps connector cache or the process tool catalog cache.
    /// Only a visibility hint: calls take the annotations of a live connection.
    pub(crate) fn fork_cached_read_only_tools(
        &self,
        server_name: &str,
    ) -> Option<Arc<HashSet<String>>> {
        let client = &self.servers.get(server_name)?.connection.client;
        if let Some(context) = client.codex_apps_tools_cache_context.as_ref() {
            let snapshot = context.current_catalog_snapshot()?;
            return Some(read_only_tool_names(snapshot.tools()));
        }
        client
            .tool_catalog_cache_context
            .as_ref()?
            .fork_read_only_tool_names()
    }
}
