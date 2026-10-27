//! Fork: stages of the fork's features.

use crate::Feature;
use pretty_assertions::assert_eq;

/// Explorer-and-questions R1.1: the explorer profile is offered in `/experimental` with a menu
/// name and description, without a startup announcement, and stays off by default.
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
    assert!(!Feature::ExplorerProfile.default_enabled());
}

/// Fork-features-experimental R2.1: background process notifications, dynamic workflows and
/// subagent worktrees are offered in `/experimental` with a menu name and a description that
/// names what the feature does and its main limitation, without a startup announcement, stay
/// off by default and keep their keys out of `x-codex-beta-features`.
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
        (
            Feature::AgentWorktreeIsolation,
            "Subagent worktrees",
            "applies their changes with apply_agent_worktree",
            "local git checkouts only, not on Windows",
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
        assert!(!feature.default_enabled(), "{feature:?}");
        assert!(crate::fork_owns_stage(feature), "{feature:?}");
    }
}
