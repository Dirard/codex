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
