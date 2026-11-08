//! Fork: stages of the fork's features.

use crate::Feature;
use pretty_assertions::assert_eq;

/// Explorer-and-questions R1.1: the explorer profile is offered in `/experimental` with a menu
/// name and description, without a startup announcement; fork-features-default-on R1.1: it is on
/// by default.
#[test]
fn explorer_profile_is_experimental() {
    let stage = Feature::ExplorerProfile.stage();
    assert_eq!(
        stage.experimental_menu_name(),
        Some("Read-only explorer subagents")
    );
    let description = stage
        .experimental_menu_description()
        .expect("explorer_profile has a /experimental description");
    assert!(description.contains("only read"), "{description}");
    assert_eq!(stage.experimental_announcement(), None);
    assert!(Feature::ExplorerProfile.default_enabled());
}

/// Fork-features-experimental R2.1: background process notifications, dynamic workflows and
/// subagent worktrees are offered in `/experimental` with a menu name and a description that
/// names what the feature does and its main limitation, without a startup announcement, and keep
/// their keys out of `x-codex-beta-features`. Fork-features-default-on R1.1: notifications,
/// workflows and subagent worktrees (not on Windows) are on by default; the later features stay
/// off.
#[test]
fn background_workflow_and_worktree_features_are_experimental() {
    for (feature, name, does, limitation) in [
        (
            Feature::ExecExitNotifications,
            "Background process notifications",
            "notify the model when they exit or print a matching line",
            "codex exec waits",
        ),
        (
            Feature::Workflows,
            "Dynamic workflows",
            "JavaScript workflow that fans out subagents",
            "start without your conversation",
        ),
        // Builtin-workflows UC1, R9.1: built-in scripts and `/code-review` over `workflows`.
        (
            Feature::BuiltinWorkflows,
            "Built-in workflows",
            "built-in review, understand and research workflows and the /code-review command",
            "needs Dynamic workflows",
        ),
        // Scheduled-wakeups R1.1: model wakeups and `/loop`, only while Codex runs.
        (
            Feature::ScheduledWakeups,
            "Scheduled wakeups",
            "resume this thread later",
            "only while Codex runs",
        ),
        // Tool-output-spill R6.1: the full output of truncated tools, local environments only.
        (
            Feature::ToolOutputSpill,
            "Full output of truncated tools",
            "saves the full output",
            "not for remote environments or Windows",
        ),
        (
            Feature::AgentWorktreeIsolation,
            "Subagent worktrees",
            "applies their changes with apply_agent_worktree",
            "local git checkouts only, not on Windows",
        ),
        // Fork-tool-guidance UC6: a subagent's answer wakes an idle root.
        (
            Feature::WakeOnAgentMessages,
            "Wake on subagent results",
            "when a subagent reports back after Codex ended its turn",
            "Root threads only",
        ),
    ] {
        let stage = feature.stage();
        assert_eq!(stage.experimental_menu_name(), Some(name), "{feature:?}");
        let description = stage
            .experimental_menu_description()
            .unwrap_or_else(|| panic!("{feature:?} has no /experimental description"));
        assert!(description.contains(does), "{description}");
        assert!(description.contains(limitation), "{description}");
        assert_eq!(stage.experimental_announcement(), None, "{feature:?}");
        assert_eq!(
            feature.default_enabled(),
            expected_default_enabled(feature),
            "{feature:?}"
        );
        assert!(crate::fork_owns_stage(feature), "{feature:?}");
    }
}

/// Output-styles R2.2: `output_styles` is offered in `/experimental` with a menu name and a
/// description naming the command and the config key, without a startup announcement, stays
/// off by default and keeps its key out of `x-codex-beta-features`.
#[test]
fn output_styles_is_experimental() {
    let feature = Feature::OutputStyles;
    assert_eq!(feature.key(), "output_styles");
    let stage = feature.stage();
    assert_eq!(stage.experimental_menu_name(), Some("Output styles"));
    let description = stage
        .experimental_menu_description()
        .expect("output_styles has a /experimental description");
    assert!(description.contains("/output-style"), "{description}");
    assert!(
        description.contains("output_style in config.toml"),
        "{description}"
    );
    assert_eq!(stage.experimental_announcement(), None);
    assert!(!feature.default_enabled());
    assert!(crate::fork_owns_stage(feature));
}

/// Fork-features-default-on R1.1, R1.2, R5.1: the default of every feature whose stage the fork
/// owns. Background notifications, explorers and workflows are on everywhere, subagent worktrees
/// everywhere but Windows (each isolated spawn fails there); questions outside Plan mode stay off
/// by the owner's decision of 2026-11-07, and the features that landed later stay off.
fn expected_default_enabled(feature: Feature) -> bool {
    match feature {
        Feature::ExecExitNotifications
        | Feature::ExplorerProfile
        | Feature::Workflows
        | Feature::WakeOnAgentMessages => true,
        Feature::AgentWorktreeIsolation => !cfg!(windows),
        _ => false,
    }
}

/// Fork-features-default-on R1.1, R1.2, R2.1, R5.1: four fork features are on by default (subagent
/// worktrees not on Windows) and keep their Experimental stage, menu entry and fork-owned stage;
/// `default_mode_request_user_input` keeps its stage and stays off.
#[test]
fn fork_features_default_on() {
    for (feature, name) in [
        (
            Feature::ExecExitNotifications,
            "Background process notifications",
        ),
        (Feature::ExplorerProfile, "Read-only explorer subagents"),
        (Feature::AgentWorktreeIsolation, "Subagent worktrees"),
        (Feature::Workflows, "Dynamic workflows"),
        (Feature::WakeOnAgentMessages, "Wake on subagent results"),
        (
            Feature::DefaultModeRequestUserInput,
            "Questions outside Plan mode",
        ),
    ] {
        let stage = feature.stage();
        assert!(
            matches!(stage, crate::Stage::Experimental { .. }),
            "{feature:?}"
        );
        assert_eq!(stage.experimental_menu_name(), Some(name), "{feature:?}");
        assert!(
            stage.experimental_menu_description().is_some(),
            "{feature:?}"
        );
        assert_eq!(stage.experimental_announcement(), None, "{feature:?}");
        assert!(crate::fork_owns_stage(feature), "{feature:?}");
        assert_eq!(
            feature.default_enabled(),
            expected_default_enabled(feature),
            "{feature:?}"
        );
    }
    assert!(!Feature::DefaultModeRequestUserInput.default_enabled());
    let defaults = crate::Features::with_defaults();
    for feature in crate::fork_default_on_features() {
        assert!(defaults.enabled(feature), "{feature:?}");
    }
    assert!(!defaults.enabled(Feature::DefaultModeRequestUserInput));
}

/// Fork-features-default-on R6.2: the list the test harnesses turn off is derived from the
/// registry: every fork-owned feature that is on by default here, and nothing else.
#[test]
fn fork_default_on_features_matches_registry() {
    let mut expected = vec![Feature::ExecExitNotifications, Feature::Workflows];
    expected.push(Feature::ExplorerProfile);
    if !cfg!(windows) {
        expected.push(Feature::AgentWorktreeIsolation);
    }
    // Fork-tool-guidance UC6: the upstream flag is registered after the fork's own features.
    expected.push(Feature::WakeOnAgentMessages);
    assert_eq!(crate::fork_default_on_features(), expected);
    for spec in crate::FEATURES {
        assert_eq!(
            crate::fork_default_on_features().contains(&spec.id),
            spec.default_enabled && crate::fork_owns_stage(spec.id),
            "{}",
            spec.key
        );
        if crate::fork_owns_stage(spec.id) {
            assert_eq!(
                spec.default_enabled,
                expected_default_enabled(spec.id),
                "{}",
                spec.key
            );
        }
    }
}

/// Fork-features-default-on R2.2: the fork's exception in the upstream invariant
/// `default_enabled_features_have_shipped` covers only fork-owned stages. Every Experimental
/// feature that is on by default is fork-owned, and upstream Experimental features are not, so
/// turning one of them on by default would still fail the invariant.
#[test]
fn default_on_experimental_exception_is_limited_to_fork_stages() {
    let experimental =
        |spec: &&crate::FeatureSpec| matches!(spec.stage, crate::Stage::Experimental { .. });
    for spec in crate::FEATURES.iter().filter(experimental) {
        if spec.default_enabled {
            assert!(crate::fork_owns_stage(spec.id), "{}", spec.key);
        }
    }
    for feature in [
        Feature::AnalyticsPlanHistory,
        Feature::LoginShellPackagePath,
        Feature::RespectSystemProxy,
    ] {
        assert!(
            matches!(feature.stage(), crate::Stage::Experimental { .. }),
            "{feature:?}"
        );
        assert!(!crate::fork_owns_stage(feature), "{feature:?}");
    }
}

/// Fork-features-default-on R6.1: the unit-test pins turn off every default-on fork feature a
/// features table does not set; a boolean or the `enabled` of a settings table is kept, and a
/// settings table without `enabled` keeps its settings with the feature off.
#[test]
fn pins_keep_what_a_features_table_sets() {
    let mut features: crate::FeaturesToml = toml::from_str(
        "workflows = true\n\n[exec_exit_notifications]\nexec_wait_timeout_ms = 1\n\n[explorer_profile]\nenabled = true\n",
    )
    .expect("features table");
    features.pin_fork_default_on_features_off();
    let mut resolved = crate::Features::with_defaults();
    resolved.apply_map(&features.entries());
    for feature in crate::fork_default_on_features() {
        assert_eq!(
            resolved.enabled(feature),
            matches!(feature, Feature::Workflows | Feature::ExplorerProfile),
            "{feature:?}"
        );
    }
    assert!(matches!(
        features.exec_exit_notifications,
        Some(crate::FeatureToml::Config(
            crate::ExecExitNotificationsConfigToml {
                exec_wait_timeout_ms: Some(1),
                ..
            }
        ))
    ));
}
