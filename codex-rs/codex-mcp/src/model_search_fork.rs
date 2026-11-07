//! Fork: a turn's Search discovery carried to a later step's binding
//! (`docs/fork/reports/2026-11-07-ci-web-search-race.md`).
//!
//! A step that waits for the Apps server to finish starting captures a binding the runtime
//! cannot cache, so the next step of the same turn gets an equivalent binding with a new
//! identity. The scoped `tools/list` result does not depend on that identity, only on the
//! request and the Apps connection that answered it.

use std::sync::Arc;

use super::McpBinding;
use super::SearchDiscovery;
use super::is_model_search_tool;

impl McpBinding {
    /// `discovery`, made on `discovered_on` earlier in this turn, projected onto this binding.
    ///
    /// `None` unless both bindings route the Search tool to the same host-owned Apps
    /// transport, so the invocation pin never moves to another connection; the caller then
    /// discovers again. Dispatch, approval and policy come from this binding's own call.
    pub fn fork_rebase_model_search(
        &self,
        discovered_on: &McpBinding,
        discovery: &SearchDiscovery,
    ) -> Option<SearchDiscovery> {
        let (index, original) = self
            .tools
            .iter()
            .enumerate()
            .find(|(_, tool)| is_model_search_tool(tool))?;
        let key = (original.server_name.clone(), original.tool.name.to_string());
        let call = self
            .calls
            .get(&key)
            .filter(|call| call.is_host_owned_apps())?;
        let previous = discovered_on
            .calls
            .get(&key)
            .filter(|call| call.is_host_owned_apps())?;
        if !Arc::ptr_eq(&call.client.client, &previous.client.client) {
            return None;
        }
        let discovered = match discovery {
            SearchDiscovery::Ready(discovered) => discovered,
            &SearchDiscovery::Critical(reason) => return Some(SearchDiscovery::Critical(reason)),
            SearchDiscovery::Unavailable => return Some(SearchDiscovery::Unavailable),
        };
        // Mirrors the projection of `discover_model_search` onto this binding's catalog.
        let listed = discovered.calls.get(&key)?.tool_info();
        let mut projected = original.clone();
        projected.namespace_description = listed.namespace_description.clone();
        projected.tool = listed.tool.clone();
        let mut tools = self.tools.to_vec();
        tools[index] = projected.clone();
        let mut calls = self.calls.clone();
        let mut call = call.clone();
        call.tool_info = projected.into();
        calls.insert(key, call);
        Some(SearchDiscovery::Ready(Arc::new(Self::new(
            Arc::clone(&self.connections),
            Arc::clone(&self.clients),
            Arc::clone(&self.config),
            self.plugins_available,
            tools,
            calls,
        ))))
    }
}

#[cfg(test)]
#[path = "model_search_fork_tests.rs"]
mod tests;
