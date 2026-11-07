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
