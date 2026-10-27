//! Fork: `spawn_agent` options of the explorer profile and worktree isolation, and the
//! `wait_agent` guidance for exit notifications.
//!
//! Specs: `specs/explorer-and-questions-2026-10-25.md`,
//! `specs/agent-worktree-isolation-2026-10-25.md`,
//! `specs/background-exec-notifications-2026-10-25.md` (R4.6).

use codex_tools::JsonSchema;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;

/// Fork options of the `spawn_agent` tool.
#[derive(Debug, Clone, Default)]
pub struct SpawnAgentForkOptions {
    /// `explorer_profile` is enabled, so explorers default to `fork_turns = "none"`.
    pub explorer_profile: bool,
    /// Expose `isolation` and the `worktree` result field (feature `agent_worktree_isolation`).
    pub worktree_isolation: bool,
}

/// `wait_agent` description addition when exit notifications are enabled.
pub(crate) const WAIT_AGENT_EXIT_NOTIFICATION_GUIDANCE: &str = " It also returns when a background terminal started with exec_command notify_on_exit or notify_on_output exits or reports matching output, and it keeps waiting while such a terminal is still running (for at most 5 minutes when timeout_ms is omitted; a terminal with only an output watch for its first matching line, which is awaited for a few minutes after your turn ends, then while matching lines keep coming).";

const SPAWN_AGENT_ISOLATION_DESCRIPTION: &str = "Where the new agent works. `none` shares your checkout. `worktree` gives the agent its own git worktree created from a snapshot of your working tree, so parallel agents can edit the same files without conflicts; your checkout stays read-only for it. Omit to use the default for the agent type (normally `none`). When a worktree agent finishes, Codex appends an `<agent_worktree>` block at the end of its result with its branch; do not ask the agent to write that block. Apply its changes with `apply_agent_worktree` and the agent's task name: it merges the branch over your current edits and writes the result like apply_patch, without committing. Run the block's `fallback` git command only for binary files, executables and symlinks that the tool refuses.";

/// The fork parameters of `spawn_agent` (V2).
pub(super) fn extend_spawn_agent_properties(
    properties: &mut BTreeMap<String, JsonSchema>,
    options: &SpawnAgentForkOptions,
) {
    if options.explorer_profile {
        properties.insert(
            "fork_turns".to_string(),
            JsonSchema::string(Some(
                "Parent history to inherit. Defaults to `all` (`none` for `explorer` agents); use `none` to start without parent history. Only `all` and `none` are supported.".to_string(),
            )),
        );
    }
    if options.worktree_isolation {
        properties.insert(
            "isolation".to_string(),
            JsonSchema::string_enum(
                vec![json!("none"), json!("worktree")],
                Some(SPAWN_AGENT_ISOLATION_DESCRIPTION.to_string()),
            ),
        );
    }
}

/// The `spawn_agent` (V2) output schema with the `worktree` field of isolated agents.
pub(super) fn output_schema_v2(
    hide_agent_metadata: bool,
    options: &SpawnAgentForkOptions,
) -> Value {
    let mut schema = super::spawn_agent_output_schema_v2(hide_agent_metadata);
    if options.worktree_isolation
        && let Some(properties) = schema["properties"].as_object_mut()
    {
        properties.insert(
            "worktree".to_string(),
            json!({
                "type": "object",
                "description": "Present when the agent works in its own git worktree.",
                "properties": {
                    "path": { "type": "string", "description": "Root of the agent's worktree." },
                    "branch": { "type": "string", "description": "Branch that receives the agent's changes when it finishes a turn." },
                    "base": { "type": "string", "description": "Commit the worktree started from." },
                    "base_kind": { "type": "string", "enum": ["working_tree", "head"] },
                    "parent_checkout_protected": { "type": "boolean", "description": "Whether the sandbox keeps your checkout read-only for the agent." }
                },
                "required": ["path", "branch", "base", "base_kind", "parent_checkout_protected"],
                "additionalProperties": false
            }),
        );
    }
    schema
}
