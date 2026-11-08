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
            | Feature::ToolOutputSpill
            | Feature::Workflows
            | Feature::BuiltinWorkflows
            | Feature::ScheduledWakeups
            | Feature::OutputStyles
            | Feature::DefaultModeRequestUserInput
            | Feature::WakeOnAgentMessages
    )
}

/// The fork's features that are enabled by default on this platform
/// (`specs/fork-features-default-on-2026-11-07.md`, R6.2): the features whose stage the fork
/// owns ([`fork_owns_stage`]) with `default_enabled`, in registry order. The shared test
/// harnesses turn these off for upstream tests, so a fork feature that becomes default-on later
/// is covered without editing them.
pub fn fork_default_on_features() -> Vec<Feature> {
    crate::FEATURES
        .iter()
        .filter(|spec| spec.default_enabled && fork_owns_stage(spec.id))
        .map(|spec| spec.id)
        .collect()
}

impl FeaturesToml {
    /// Turns off every feature of [`fork_default_on_features`] that this table does not set, as a
    /// boolean or as the `enabled` of its settings table; a settings table without `enabled`
    /// keeps its settings and the feature stays off, as it did while the feature was off by
    /// default (`specs/fork-features-default-on-2026-11-07.md`, UC6, R6.1). Only the core unit
    /// test harness calls it: production defaults do not depend on a test switch (R6.3).
    pub fn pin_fork_default_on_features_off(&mut self) {
        let configured = self.entries();
        for feature in fork_default_on_features() {
            if !configured.contains_key(feature.key()) {
                self.entries.insert(feature.key().to_string(), false);
            }
        }
    }

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
        if let Some(enabled) = self
            .tool_output_spill
            .as_ref()
            .and_then(FeatureToml::enabled)
        {
            entries.insert(Feature::ToolOutputSpill.key().to_string(), enabled);
        }
        if let Some(enabled) = self
            .scheduled_wakeups
            .as_ref()
            .and_then(FeatureToml::enabled)
        {
            entries.insert(Feature::ScheduledWakeups.key().to_string(), enabled);
        }
    }
}

#[cfg(test)]
#[path = "fork_tests.rs"]
mod tests;
