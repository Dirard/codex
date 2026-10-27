//! Fork: `FeaturesToml` entries of the fork's features with a settings table
//! (`specs/explorer-and-questions-2026-10-25.md`,
//! `specs/agent-worktree-isolation-2026-10-25.md`,
//! `specs/background-exec-notifications-2026-10-25.md`).

use std::collections::BTreeMap;

use crate::Feature;
use crate::FeatureToml;
use crate::FeaturesToml;

/// Whether the fork, not upstream, sets the stage of `feature`: the fork's own features and
/// upstream features that the fork moved to `Experimental`. Their keys stay out of the
/// `x-codex-beta-features` request header, which carries only what upstream itself would
/// send: a provider must not see keys it cannot know (review 2026-10-27, item 6).
pub fn fork_owns_stage(feature: Feature) -> bool {
    matches!(
        feature,
        Feature::ExplorerProfile
            | Feature::AgentWorktreeIsolation
            | Feature::ExecExitNotifications
            | Feature::Workflows
            | Feature::DefaultModeRequestUserInput
    )
}

impl FeaturesToml {
    /// Adds the `enabled` flags of the fork's feature tables to [`FeaturesToml::entries`].
    pub(crate) fn fork_entries(&self, entries: &mut BTreeMap<String, bool>) {
        if let Some(enabled) = self
            .explorer_profile
            .as_ref()
            .and_then(FeatureToml::enabled)
        {
            entries.insert(Feature::ExplorerProfile.key().to_string(), enabled);
        }
        if let Some(enabled) = self
            .agent_worktree_isolation
            .as_ref()
            .and_then(FeatureToml::enabled)
        {
            entries.insert(Feature::AgentWorktreeIsolation.key().to_string(), enabled);
        }
        if let Some(enabled) = self
            .exec_exit_notifications
            .as_ref()
            .and_then(FeatureToml::enabled)
        {
            entries.insert(Feature::ExecExitNotifications.key().to_string(), enabled);
        }
    }
}

#[cfg(test)]
#[path = "fork_tests.rs"]
mod tests;
