//! Fork: explorer sessions stay read-only without escalation
//! (specs/explorer-and-questions-2026-10-25.md); the session start hook also starts the
//! agent worktree sweep (specs/agent-worktree-isolation-2026-10-25.md, R10.1).

use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TurnEnvironmentSelection;

use super::session::SessionConfiguration;
use crate::agent::explorer_profile;
use crate::config::Config;

/// Fork: explorer sessions run read-only without escalation on every start path (spawn,
/// reload, cold resume). A root session also sweeps forgotten agent worktrees of its
/// repository in the background (agent-worktree-isolation, R10.1).
pub(super) fn on_session_start(
    mut config: Config,
    session_source: &SessionSource,
    environment_selections: &mut [TurnEnvironmentSelection],
) -> CodexResult<Config> {
    crate::agent::worktree_isolation::sweep_on_session_start(&config, session_source);
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
