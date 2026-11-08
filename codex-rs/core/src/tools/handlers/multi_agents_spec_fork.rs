//! Fork: `spawn_agent` options of the explorer profile and worktree isolation, the fork's
//! delegation guidance, and the fork's additions to the `wait_agent` description.
//!
//! Specs: `specs/explorer-and-questions-2026-10-25.md`,
//! `specs/agent-worktree-isolation-2026-10-25.md`,
//! `specs/background-exec-notifications-2026-10-25.md` (R4.6),
//! `specs/fork-tool-guidance-2026-11-08.md` (UC2).

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
    /// The caller has the `workflow` tool, which spawned agents never get.
    pub workflows: bool,
    /// `wait_agent` is offered beside `spawn_agent`.
    pub wait_agent: bool,
    /// A finished agent's answer starts a new turn of this idle caller
    /// (`wake_on_agent_messages`, root threads only).
    pub agent_results_wake: bool,
}

/// Fork additions to the `wait_agent` description.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WaitAgentForkOptions {
    /// Background `exec_command` processes can notify the thread (`exec_exit_notifications`).
    pub(crate) exit_notifications: bool,
    /// The caller has the `workflow` tool.
    pub(crate) workflows: bool,
    /// A finished agent's answer starts a new turn of this idle caller
    /// (`wake_on_agent_messages`, root threads only).
    pub(crate) agent_results_wake: bool,
}

/// The bundled `wait_agent` description; the fork owns it since the quiet wait.
pub(super) const WAIT_AGENT_DESCRIPTION: &str = "Wait until one of your agents sends you a message, finishes or fails. Does not return the content: the result is a short status (completed, interrupted by new user input, no active agents, or a timeout snapshot), and the agents' messages and final answers arrive as separate messages right after it; read them there instead of calling list_agents. For normal background work, omit `timeout_ms`. Passing `0` performs one immediate check. A positive value sets one absolute deadline; new mail or steered user input can return earlier.";

/// `wait_agent` addition when background terminals can notify the thread.
const WAIT_AGENT_EXIT_NOTIFICATION_GUIDANCE: &str = " It also returns when a background terminal started with exec_command notify_on_exit or notify_on_output exits or reports matching output. Without active agents it waits for such terminals: at most 5 minutes when timeout_ms is omitted, and for a terminal with only an output watch, until its first matching line (awaited a few minutes after your turn ends), then while matching lines keep coming. To wait only for terminals, end your turn instead: their notifications start a new turn.";

/// `wait_agent` addition while agent results do not wake an idle caller (the default).
const WAIT_AGENT_RESULTS_DO_NOT_WAKE_GUIDANCE: &str = " While agents you started are running, do not end your turn: their answers do not start a new turn by themselves, and once your turn ends they wait until a new message from the user or your parent starts your next turn. Call wait_agent to collect them, and in between do work that does not overlap theirs.";

/// `wait_agent` addition while agent results wake an idle root (`wake_on_agent_messages`).
const WAIT_AGENT_RESULTS_WAKE_GUIDANCE: &str = " If you end your turn while agents you started are still running, their answers start a new turn when they arrive (not in Plan mode or after the user interrupts), so you may end your turn instead of waiting idly; call wait_agent when you need a result before you can continue.";

/// `wait_agent` addition when the caller can run workflows.
const WAIT_AGENT_WORKFLOW_GUIDANCE: &str = " It also returns early when a workflow run finishes (its notification follows), but it does not wait for workflow runs: their agents are not counted here.";

/// The upstream start of the `agent_type` description that the explorer profile replaces.
const AGENT_TYPE_UPSTREAM_PREFIX: &str =
    "Agent type override for the new agent. Omit unless explicitly asked.";

const AGENT_TYPE_EXPLORER_PREFIX: &str = "Role of the new agent: pick the role below whose description fits the task, such as `explorer` for read-only questions about the code, or omit it for a `default` agent.";

/// The upstream sentence of the `spawn_agent` description that promises every tool.
const SPAWN_AGENT_SAME_TOOLS: &str =
    "The spawned agent will have the same tools as you and the ability to spawn its own subagents.";

const SPAWN_AGENT_DELEGATION_GUIDANCE: &str = "When you delegate: an agent costs a full model run and returns only its final answer, so hand off work that can proceed while you do something else (independent questions, separate parts of a change), not a lookup you can finish in a few tool calls. Start independent agents in the same response so they run in parallel, keep working on things that do not overlap theirs, and do not redo their task. With `fork_turns=\"none\"` the message is the agent's only context: give the goal, what you already know or ruled out, where to start, and what to return.";

const SPAWN_AGENT_COLLECT_RESULTS: &str =
    " Collect their results with `wait_agent` before you end your turn.";

const SPAWN_AGENT_WORKFLOW_GUIDANCE: &str = " For about five or more independent pieces of work, or a find-and-verify pipeline, use the `workflow` tool instead: one script runs many agents and only its result reaches your context.";

const SPAWN_AGENT_NO_WORKFLOWS: &str = " Spawned agents cannot start workflows.";

const SPAWN_AGENT_ISOLATION_DESCRIPTION: &str = "Where the new agent works. `none` shares your checkout: the agent's edits land in your files as it makes them. `worktree` gives it a private git worktree made from a snapshot of your working tree (uncommitted and untracked files included; ignored files such as build output and dependencies are not, so builds there start cold); its edits stay invisible to you and other agents until you apply them. Use `worktree` only for an agent that edits files while you or other agents edit the same checkout, never for `explorer` agents; it needs a local git checkout. Edits to the same file can still conflict on apply, so give each agent its own files and leave them alone until then. Omit for the agent type's default (normally `none`). When a worktree agent finishes a turn, Codex appends an `<agent_worktree>` block with its branch and changed files to its result; do not ask the agent to write that block. Apply the changes with `apply_agent_worktree` and the agent's task name: it merges them over your current edits like apply_patch, without committing. Run the block's `fallback` git command only for binary files, executables and symlinks that the tool refuses.";

/// The fork parameters of `spawn_agent` (V2).
///
/// The `agent_type` and `isolation` texts implement `specs/fork-tool-guidance-2026-11-08.md`
/// UC2 (R2.1): the explorer profile drops the upstream "Omit unless explicitly asked", and
/// `isolation` says when a worktree pays off and what it costs.
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
        if let Some(description) = properties
            .get_mut("agent_type")
            .and_then(|schema| schema.description.as_mut())
        {
            *description = description
                .replacen(AGENT_TYPE_UPSTREAM_PREFIX, AGENT_TYPE_EXPLORER_PREFIX, 1)
                .into_boxed_str();
        }
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

/// The fork's part of the `spawn_agent` (V2) description: what spawned agents lack and how to
/// delegate well (`specs/fork-tool-guidance-2026-11-08.md` UC2, R2.1, R2.3).
///
/// Without a fork feature the description stays as upstream composed it. The guidance comes
/// after a catalog description and the configured usage hint too.
pub(super) fn extend_spawn_agent_description(
    description: String,
    options: &SpawnAgentForkOptions,
) -> String {
    if !(options.explorer_profile || options.worktree_isolation || options.workflows) {
        return description;
    }
    let mut same_tools = String::from("The spawned agent will have the same tools as you");
    if options.workflows {
        same_tools.push_str(" except `workflow`,");
    }
    same_tools.push_str(" and the ability to spawn its own subagents");
    if options.explorer_profile {
        same_tools.push_str("; an `explorer` gets only read-only tools and cannot spawn");
    }
    same_tools.push('.');
    let names_missing_tools = description.contains(SPAWN_AGENT_SAME_TOOLS);
    let mut description = description.replacen(SPAWN_AGENT_SAME_TOOLS, &same_tools, 1);

    description.push_str("\n\n");
    description.push_str(SPAWN_AGENT_DELEGATION_GUIDANCE);
    if options.wait_agent && !options.agent_results_wake {
        description.push_str(SPAWN_AGENT_COLLECT_RESULTS);
    }
    if options.workflows {
        description.push_str(SPAWN_AGENT_WORKFLOW_GUIDANCE);
        if !names_missing_tools {
            description.push_str(SPAWN_AGENT_NO_WORKFLOWS);
        }
    }
    description
}

/// The fork's additions to the `wait_agent` description, in the order the model reads them:
/// background terminals, then agent results, then workflow runs
/// (`specs/fork-tool-guidance-2026-11-08.md` UC2, R1.4, R2.2, R2.3).
///
/// Terminals come first, so a catalog description stays the prefix of the terminal sentence.
pub(crate) fn wait_agent_guidance(options: WaitAgentForkOptions) -> String {
    let mut guidance = String::new();
    if options.exit_notifications {
        guidance.push_str(WAIT_AGENT_EXIT_NOTIFICATION_GUIDANCE);
    }
    guidance.push_str(if options.agent_results_wake {
        WAIT_AGENT_RESULTS_WAKE_GUIDANCE
    } else {
        WAIT_AGENT_RESULTS_DO_NOT_WAKE_GUIDANCE
    });
    if options.workflows {
        guidance.push_str(WAIT_AGENT_WORKFLOW_GUIDANCE);
    }
    guidance
}

/// The fork's additions to a catalog `wait_agent` description.
///
/// A catalog text replaces the bundled one, so it keeps the upstream wording while no fork
/// feature adds a notification that ends a wait; with one, the agent-result rule follows it,
/// because terminals and workflow runs start new turns and agents do not
/// (`specs/fork-tool-guidance-2026-11-08.md` UC2, R2.2).
pub(crate) fn wait_agent_catalog_guidance(options: WaitAgentForkOptions) -> String {
    if options.exit_notifications || options.workflows {
        wait_agent_guidance(options)
    } else {
        String::new()
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
