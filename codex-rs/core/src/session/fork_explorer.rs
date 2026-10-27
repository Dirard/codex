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

#[cfg(test)]
mod tests {
    use codex_features::Feature;
    use pretty_assertions::assert_eq;

    use super::super::Session;
    use crate::config::ConfigBuilder;

    /// Review 2026-10-27, item 6: `x-codex-beta-features` carries the upstream Experimental
    /// keys only; the fork's own Experimental features and the fork's Experimental stage of
    /// `default_mode_request_user_input` stay out of it (fork-features-experimental R2.1).
    #[tokio::test]
    async fn beta_features_header_leaves_out_fork_stages() {
        let home = tempfile::TempDir::new().expect("create temp dir");
        let mut config = ConfigBuilder::default()
            .codex_home(home.path().to_path_buf())
            .fallback_cwd(Some(home.path().to_path_buf()))
            .build()
            .await
            .expect("load test config");
        let fork_features = [
            Feature::ExecExitNotifications,
            Feature::Workflows,
            Feature::ExplorerProfile,
            Feature::AgentWorktreeIsolation,
            Feature::DefaultModeRequestUserInput,
        ];
        for feature in fork_features
            .into_iter()
            .chain([Feature::LoginShellPackagePath])
        {
            config
                .features
                .enable(feature)
                .expect("test config should allow feature update");
        }
        let header = Session::build_model_client_beta_features_header(&config).unwrap_or_default();
        let keys = header.split(',').collect::<Vec<_>>();
        assert!(keys.contains(&"login_shell_package_path"), "{header}");
        for fork_key in fork_features.map(Feature::key) {
            assert!(!keys.contains(&fork_key), "{fork_key} in {header}");
        }
        assert_eq!(
            codex_features::FEATURES
                .iter()
                .filter(|spec| spec.stage.experimental_menu_description().is_some())
                .filter(|spec| codex_features::fork_owns_stage(spec.id))
                .map(|spec| spec.key)
                .collect::<Vec<_>>(),
            fork_features.map(Feature::key).to_vec(),
            "the fork's Experimental features"
        );
    }
}
