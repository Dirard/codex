//! Fork: `spawn_agent` (V2) arguments of worktree isolation and the explorer profile.
//!
//! Specs: `specs/agent-worktree-isolation-2026-10-25.md` (R1.1),
//! `specs/explorer-and-questions-2026-10-25.md`.

use super::*;

impl SpawnAgentArgs {
    /// `none` or `worktree`; omission uses the configured default.
    pub(super) fn isolation(
        &self,
    ) -> Result<Option<codex_features::AgentIsolation>, FunctionCallError> {
        match self.isolation.as_deref().map(str::trim) {
            None | Some("") => Ok(None),
            Some(value) if value.eq_ignore_ascii_case("none") => {
                Ok(Some(codex_features::AgentIsolation::None))
            }
            Some(value) if value.eq_ignore_ascii_case("worktree") => {
                Ok(Some(codex_features::AgentIsolation::Worktree))
            }
            Some(_) => Err(FunctionCallError::RespondToModel(
                "isolation must be `none` or `worktree`".to_string(),
            )),
        }
    }

    /// Profiled explorers start without parent history unless `fork_turns` says otherwise.
    pub(super) fn default_fork_turns(&self, config: &crate::config::Config) -> &'static str {
        let role_name = self
            .agent_type
            .as_deref()
            .map(str::trim)
            .filter(|role| !role.is_empty());
        if crate::agent::explorer_profile::applies_to_role(config, role_name) {
            "none"
        } else {
            "all"
        }
    }
}
