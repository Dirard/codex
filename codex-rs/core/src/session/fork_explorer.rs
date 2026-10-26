//! Fork: explorer sessions stay read-only without escalation
//! (specs/explorer-and-questions-2026-10-25.md).

use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TurnEnvironmentSelection;

use super::session::SessionConfiguration;
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
