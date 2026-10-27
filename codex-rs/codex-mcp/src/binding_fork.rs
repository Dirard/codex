//! Fork: which cached tools of a binding are read-only, for read-only explorers
//! (`specs/explorer-and-questions-2026-10-25.md`, R2.9; review 2026-10-27, P2).

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use super::McpBinding;

impl McpBinding {
    /// `(server, raw tool name)` of the tools this binding took from a catalog cache (no live
    /// client in the binding) that their server's last live `tools/list` annotated
    /// `readOnlyHint: true`.
    ///
    /// A cached catalog carries no annotations, so without this a spawned read-only explorer
    /// would have to start every MCP server before it could show a read-only tool. The hint
    /// only decides what the explorer sees; a call takes the live annotations again.
    pub fn fork_cached_read_only_tools(&self) -> HashSet<(String, String)> {
        let mut by_server: HashMap<&str, Option<Arc<HashSet<String>>>> = HashMap::new();
        let mut read_only = HashSet::new();
        for tool in self.tools.iter() {
            let key = (tool.server_name.clone(), tool.tool.name.to_string());
            if self.calls.contains_key(&key) {
                continue;
            }
            let names = by_server
                .entry(tool.server_name.as_str())
                .or_insert_with(|| {
                    self.connections
                        .fork_cached_read_only_tools(&tool.server_name)
                });
            if names.as_ref().is_some_and(|names| names.contains(&key.1)) {
                read_only.insert(key);
            }
        }
        read_only
    }
}
