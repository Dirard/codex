//! Fork: `FeaturesToml` entries of the fork's features with a settings table
//! (`specs/explorer-and-questions-2026-10-25.md`,
//! `specs/agent-worktree-isolation-2026-10-25.md`).

use std::collections::BTreeMap;

use crate::Feature;
use crate::FeatureToml;
use crate::FeaturesToml;

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
    }
}
