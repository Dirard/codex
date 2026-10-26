//! Fork: explorer sessions stay read-only without escalation
//! (specs/explorer-and-questions-2026-10-25.md).

use std::collections::HashSet;
use std::sync::Arc;

use codex_mcp::McpBinding;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TurnEnvironmentSelection;

use super::Session;
use super::session::SessionConfiguration;
use super::turn_context::TurnContext;
use crate::agent::explorer_profile;
use crate::config::Config;

/// Fork: explorer sessions run read-only without escalation on every start path (spawn,
/// reload, cold resume).
pub(super) fn on_session_start(
    mut config: Config,
    session_source: &SessionSource,
    environment_selections: &mut [TurnEnvironmentSelection],
) -> CodexResult<Config> {
    if explorer_profile::applies_to_source(&config, session_source) {
        explorer_profile::enforce_read_only_session_config(&mut config)?;
        environment_selections
            .iter_mut()
            .for_each(explorer_profile::clamp_environment_selection);
    }
    Ok(config)
}

impl Session {
    /// The step's MCP binding with the required servers started (`mcp_runtime_for_step`).
    ///
    /// Fork: an explorer judges MCP tools by live annotations (R2.9). A sub-agent starts MCP
    /// servers lazily from the process-wide tool catalog cache, and the cache drops tool
    /// annotations because only a live connection may supply them; the explorer's plan would
    /// then hide every MCP tool, read-only ones included. So an explorer's step also waits for
    /// the servers whose tools arrived without annotations and takes their live catalog.
    pub(super) async fn fork_binding_with_requirements(
        &self,
        turn_context: &TurnContext,
        required_servers: &[String],
        required_plugins: &HashSet<String>,
    ) -> Option<Arc<McpBinding>> {
        let runtime = &self.services.mcp_runtime;
        let binding = runtime
            .current_binding_with_requirements(required_servers, required_plugins)
            .await?;
        if !explorer_profile::applies_to_source(&turn_context.config, &turn_context.session_source)
        {
            return Some(binding);
        }
        let unannotated = explorer_profile::servers_without_annotations(binding.tools());
        if unannotated
            .iter()
            .all(|server| required_servers.contains(server))
        {
            return Some(binding);
        }
        let required_servers = required_servers
            .iter()
            .cloned()
            .chain(unannotated)
            .collect::<Vec<_>>();
        Some(
            runtime
                .current_binding_with_requirements(&required_servers, required_plugins)
                .await
                .unwrap_or(binding),
        )
    }
}

/// Fork: an owner's ready environment config cannot widen an explorer (R2.5).
pub(super) fn clamp_environment_update(
    configuration: &SessionConfiguration,
    environments: &mut [TurnEnvironmentSelection],
) {
    if explorer_profile::applies_to_source(
        &configuration.original_config_do_not_use,
        &configuration.session_source,
    ) {
        environments
            .iter_mut()
            .for_each(explorer_profile::clamp_environment_selection);
    }
}
