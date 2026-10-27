//! Fork: explorer profile for `explorer` children (specs/explorer-and-questions-2026-10-25.md).
//!
//! While `explorer_profile` is enabled, an `explorer` child runs read-only without escalation,
//! starts on a cheaper model or reasoning effort, and delivers a capped conclusion to its parent.
//! Sessions enforce the sandbox themselves so spawn, reload and cold resume share one rule.

use crate::config::Config;
use crate::config::ExplorerProfileConfig;
use crate::environment_selection::TurnEnvironmentSnapshot;
use crate::environment_selection::TurnEnvironmentState;
use codex_config::Constrained;
use codex_config::ConstraintError;
use codex_config::RequirementSource;
use codex_features::Feature;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::PermissionProfile;
use codex_protocol::models::PermissionProfileSnapshot;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::openai_models::ReasoningEffortPreset;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EnvironmentConfig;
use codex_protocol::protocol::EnvironmentConfigState;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::TurnEnvironmentSelection;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::approx_token_count;
use codex_utils_output_truncation::truncate_text;

/// Role name that the profile applies to, built in or user defined.
pub(crate) const EXPLORER_ROLE_NAME: &str = "explorer";

/// First line of the appended developer instructions; also keeps the append idempotent.
const INSTRUCTIONS_HEADING: &str = "# Explorer profile";

pub(crate) fn enabled(config: &Config) -> bool {
    config.features.enabled(Feature::ExplorerProfile)
}

/// The `agent_type` description of `spawn_agent`; the profile adds the `explorer` type.
pub(crate) fn agent_type_description(config: &Config) -> String {
    if enabled(config) {
        crate::agent::role::spawn_tool_spec::build_with_explorer_profile(
            &config.agent_roles,
            &config.explorer_profile,
            config.agent_default_subagent_model.as_deref(),
        )
    } else {
        crate::agent::role::spawn_tool_spec::build(&config.agent_roles)
    }
}

/// Whether a spawn that requested `role_name` creates a profiled explorer.
pub(crate) fn applies_to_role(config: &Config, role_name: Option<&str>) -> bool {
    enabled(config) && role_name.map(str::trim) == Some(EXPLORER_ROLE_NAME)
}

/// Whether a session started from `source` is a profiled explorer.
pub(crate) fn applies_to_source(config: &Config, source: &SessionSource) -> bool {
    enabled(config) && source.get_agent_role().as_deref() == Some(EXPLORER_ROLE_NAME)
}

/// Narrows a permission profile to read-only access, keeping its denied reads.
/// Externally sandboxed profiles become Codex's managed read-only profile.
pub(crate) fn read_only_ceiling(profile: &PermissionProfile) -> PermissionProfile {
    profile
        .intersect_with_read_only()
        .unwrap_or_else(PermissionProfile::read_only)
}

/// Explorer sessions: narrow the session config so the child cannot write or escalate.
///
/// The permission profile and approval policy are locked, so later settings updates on the
/// child thread cannot widen them. Fails when requirements disallow `approval_policy = never`.
pub(crate) fn enforce_read_only_session_config(config: &mut Config) -> CodexResult<()> {
    let permission_profile = read_only_ceiling(config.permissions.permission_profile());
    config
        .permissions
        .replace_permission_profile_from_session_snapshot(PermissionProfileSnapshot::legacy(
            permission_profile.clone(),
        ))
        .map_err(|err| {
            CodexErr::InvalidRequest(format!(
                "explorer_profile could not apply the read-only sandbox: {err}"
            ))
        })?;
    if let Some(network) = config.permissions.network.as_ref() {
        config.permissions.network = Some(
            network
                .recompute_for_permission_profile(&permission_profile)
                .map_err(|err| {
                    CodexErr::InvalidRequest(format!(
                        "explorer_profile could not restrict network access: {err}"
                    ))
                })?,
        );
    }
    config
        .permissions
        .approval_policy
        .can_set(&AskForApproval::Never)
        .map_err(|err| {
            CodexErr::InvalidRequest(format!(
                "explorer_profile requires approval_policy `never`, which configuration requirements disallow: {err}"
            ))
        })?;
    config.permissions.approval_policy = Constrained::new(AskForApproval::Never, |candidate| {
        if *candidate == AskForApproval::Never {
            Ok(())
        } else {
            Err(ConstraintError::InvalidValue {
                field_name: "approval_policy",
                candidate: format!("{candidate}"),
                allowed: "[never] for explorer_profile children".to_string(),
                requirement_source: RequirementSource::Unknown,
            })
        }
    })
    .map_err(|err| CodexErr::InvalidRequest(format!("explorer_profile: {err}")))?;
    // Requirements may pin the tool on; approval `never` still denies its requests.
    let _ = config.features.disable(Feature::RequestPermissionsTool);
    append_developer_instructions(config);
    Ok(())
}

/// Appends the explorer instructions to `config.developer_instructions` once.
///
/// Spawn calls this before a full-history fork captures the child's developer instructions
/// (review P2); session start calls it again for reload and resume, which keeps one copy.
pub(crate) fn append_developer_instructions(config: &mut Config) {
    let instructions = developer_instructions(&config.explorer_profile);
    config.developer_instructions = match config.developer_instructions.take() {
        Some(existing) if existing.contains(INSTRUCTIONS_HEADING) => Some(existing),
        Some(existing) if !existing.trim().is_empty() => {
            Some(format!("{existing}\n\n{instructions}"))
        }
        _ => Some(instructions),
    };
}

/// Removes the explorer instructions that [`append_developer_instructions`] added.
fn strip_developer_instructions(config: &mut Config) {
    let Some(existing) = config.developer_instructions.take() else {
        return;
    };
    let instructions = developer_instructions(&config.explorer_profile);
    let stripped = existing
        .replace(&format!("\n\n{instructions}"), "")
        .replace(&instructions, "");
    config.developer_instructions = (!stripped.trim().is_empty()).then_some(stripped);
}

/// Explorer sessions keep their sandbox for every command: an exec-policy `allow` rule
/// skips approval but neither bypasses nor widens the read-only sandbox (review P1).
pub(crate) fn forbids_sandbox_bypass(config: &Config, source: &SessionSource) -> bool {
    applies_to_source(config, source)
}

/// Explorer sessions spawn no agents (R2.10, review 2026-10-27 P1).
///
/// A child of an explorer is not an explorer: it would inherit the read-only sandbox and
/// approval `never`, but not the explorer's own rules, so an exec-policy `allow` rule or an MCP
/// tool without `readOnlyHint` would write through it. Every spawn (V1, V2, full-history fork,
/// workflow child) builds its config in `child_config::build_agent_spawn_config`, which calls
/// this first.
pub(crate) fn refuse_spawn(turn: &crate::session::turn_context::TurnContext) -> Result<(), String> {
    if applies_to_source(&turn.config, &turn.session_source) {
        return Err(
            "explorer_profile: a read-only explorer cannot spawn agents. Answer from what you can read yourself and tell your parent what needs another agent."
                .to_string(),
        );
    }
    Ok(())
}

/// Config for reloading another agent from an explorer's message (review P9).
///
/// The reload keeps the explorer's runtime authority (read-only, approval `never`), as the
/// upstream rule "reload with the sender's permissions" requires, but not the explorer's
/// locks or instructions: those belong to the explorer, not to the agent it wakes up. The
/// configuration requirements bound later changes again, as they do for any agent.
pub(crate) fn release_session_locks_for_message_resume(config: &mut Config) -> Result<(), String> {
    let requirements = config.config_layer_stack.requirements();
    let mut approval_policy = requirements.approval_policy.value.clone();
    let permission_profile_requirement = requirements.permission_profile.value.clone();
    approval_policy
        .set(config.permissions.approval_policy.value())
        .map_err(|err| format!("approval_policy is invalid: {err}"))?;
    let permission_profile_state =
        crate::config::PermissionProfileState::from_constrained_snapshot(
            permission_profile_requirement,
            config.permissions.permission_profile_state().snapshot(),
        )
        .map_err(|err| format!("permission_profile is invalid: {err}"))?;
    config.permissions.approval_policy = approval_policy;
    config
        .permissions
        .set_permission_profile_state(permission_profile_state);
    strip_developer_instructions(config);
    Ok(())
}

/// Per-turn budget shared by an explorer's messages to other agents (review P3).
#[derive(Debug)]
pub(crate) struct ExplorerMessageBudget {
    remaining_tokens: std::sync::Mutex<usize>,
}

impl ExplorerMessageBudget {
    pub(crate) fn new(max_tokens: usize) -> Self {
        Self {
            remaining_tokens: std::sync::Mutex::new(max_tokens),
        }
    }

    /// Caps `message` to the budget left in this turn and charges it.
    ///
    /// Returns the delivered text, truncated with a note when it exceeds the remaining
    /// budget, or an error for the model once the budget is spent.
    pub(crate) fn take(&self, message: String, max_tokens: usize) -> Result<String, String> {
        let mut remaining = self
            .remaining_tokens
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *remaining == 0 {
            return Err(format!(
                "explorer_profile: your messages to other agents already used this turn's budget of about {max_tokens} tokens, so this message was not sent. Put the rest of your conclusions in your final answer."
            ));
        }
        let tokens = approx_token_count(&message);
        if tokens <= *remaining {
            *remaining -= tokens;
            return Ok(message);
        }
        let limit = *remaining;
        *remaining = 0;
        let truncated = truncate_text(&message, TruncationPolicy::Tokens(limit));
        Ok(format!(
            "{truncated}\n\n[explorer_profile: this message was truncated to about {limit} tokens. The explorer's messages share a budget of about {max_tokens} tokens per turn; ask it a narrower follow-up question for the omitted part.]"
        ))
    }
}

/// Limits an explorer's message to another agent; other senders pass through unchanged.
pub(crate) fn cap_outgoing_message(
    config: &Config,
    source: &SessionSource,
    turn_data: &codex_extension_api::ExtensionData,
    message: String,
) -> Result<String, String> {
    if !applies_to_source(config, source) {
        return Ok(message);
    }
    let max_tokens = config.explorer_profile.max_result_tokens;
    turn_data
        .get_or_init(|| ExplorerMessageBudget::new(max_tokens))
        .take(message, max_tokens)
}

/// Explorer sessions call only MCP tools annotated `readOnlyHint: true` (review P4).
///
/// MCP servers run outside the Codex sandbox, and a tool that needs no approval (an
/// auto-approved server, a non-destructive hint, a remembered approval) would otherwise run
/// with the server's own access. A tool without the annotation counts as writing.
/// Why an MCP call is skipped: the app configuration blocks it first; otherwise a read-only
/// explorer calls only MCP tools annotated `readOnlyHint: true` (R2.8).
pub(crate) fn mcp_call_blocked_message(
    app_blocked: bool,
    turn_context: &crate::session::turn_context::TurnContext,
    server: &str,
    tool: &str,
    annotations: Option<&rmcp::model::ToolAnnotations>,
) -> Option<String> {
    if app_blocked {
        return Some("MCP tool call blocked by app configuration".to_string());
    }
    mcp_tool_denial(
        &turn_context.config,
        &turn_context.session_source,
        server,
        tool,
        annotations,
    )
}

pub(crate) fn mcp_tool_denial(
    config: &Config,
    source: &SessionSource,
    server: &str,
    tool: &str,
    annotations: Option<&rmcp::model::ToolAnnotations>,
) -> Option<String> {
    (applies_to_source(config, source) && !is_read_only(annotations)).then(|| {
        format!(
            "explorer_profile: MCP tool `{server}/{tool}` is not annotated read-only (readOnlyHint), so a read-only explorer cannot call it. Report what you need to your parent instead."
        )
    })
}

/// Explorer sessions do not call client-defined dynamic tools, which run in the client
/// without the Codex sandbox and carry no read-only annotation (review P4).
pub(crate) fn dynamic_tool_denial(
    config: &Config,
    source: &SessionSource,
    tool: &str,
) -> Option<String> {
    applies_to_source(config, source).then(|| {
        format!(
            "explorer_profile: client tool `{tool}` is not available to a read-only explorer. Report what you need to your parent instead."
        )
    })
}

/// Built-in and extension tools whose only purpose is writing files (R2.9).
const WRITE_TOOL_NAMES: [&str; 2] = ["apply_patch", "multi_env_copy_file"];

/// The spawn tool of every multi-agent version and namespace (R2.10).
const SPAWN_TOOL_NAME: &str = "spawn_agent";

/// Explorer sessions: hides the tools an explorer may not call (spec UC2, R2.9, R2.10).
///
/// `apply_patch`, `multi_env_copy_file`, `spawn_agent`, MCP tools not annotated
/// `readOnlyHint: true` and client-defined tools leave the tool list, `tool_search` and the
/// Code Mode nested tools. They stay registered as [`ToolExposure::Hidden`], so a call that
/// names one anyway still reaches its handler and gets the refusal of R2.6, R2.8 or R2.10.
///
/// `mcp_tools` carries the annotations of the session's MCP tools; an MCP tool missing from
/// it counts as writing.
pub(crate) fn hide_unavailable_tools<'a>(
    config: &Config,
    source: &SessionSource,
    mcp_tools: impl IntoIterator<Item = &'a codex_mcp::ToolInfo>,
    registry: &mut crate::tools::registry::ToolRegistry,
) {
    if !applies_to_source(config, source) {
        return;
    }
    let read_only_mcp_tools = mcp_tools
        .into_iter()
        .filter(|tool| is_read_only(tool.tool.annotations.as_ref()))
        .map(codex_mcp::ToolInfo::canonical_tool_name)
        .collect::<std::collections::HashSet<_>>();
    for tool in registry.entries_mut() {
        let name = tool.runtime.tool_name();
        let hidden = if tool.runtime.mcp_server_name().is_some() {
            !read_only_mcp_tools.contains(&name)
        } else {
            tool.runtime.is_third_party_tool()
                || name.name == SPAWN_TOOL_NAME
                || (name.is_default_namespace() && WRITE_TOOL_NAMES.contains(&name.name.as_str()))
        };
        if hidden {
            tool.exposure = codex_tools::ToolExposure::Hidden;
        }
    }
}

fn is_read_only(annotations: Option<&rmcp::model::ToolAnnotations>) -> bool {
    annotations.and_then(|annotations| annotations.read_only_hint) == Some(true)
}

/// MCP servers that offer a tool without annotations, sorted and deduplicated (R2.9).
///
/// A cached catalog carries no annotations, so these are the servers an explorer must reach
/// live before it can tell their read-only tools apart.
pub(crate) fn servers_without_annotations(tools: &[codex_mcp::ToolInfo]) -> Vec<String> {
    tools
        .iter()
        .filter(|tool| tool.tool.annotations.is_none())
        .map(|tool| tool.server_name.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Worktree note of an explorer in an isolated worktree (spec UC9, R9.3).
///
/// The note of other children asks them to keep their edits in the worktree and says Codex
/// records their changes; an explorer cannot change files, so its note says that instead.
pub(crate) fn worktree_developer_note(
    cwd: &std::path::Path,
    base_description: &str,
    base_short: &str,
    source_root: &std::path::Path,
) -> String {
    format!(
        "You are working in an isolated git worktree at {cwd}. It was created from the {base_description} ({base_short}) of the parent checkout {source_root}. Both the worktree and the parent checkout are read-only for you: you are a read-only explorer, so read files and run read-only commands there (prefer relative paths), and do not try to create, edit, move or delete files, run git commands that write repository metadata, or request escalated permissions. Codex appends an <agent_worktree> block to your result; do not write that block yourself.",
        cwd = cwd.display(),
        source_root = source_root.display(),
    )
}

/// Default worktree isolation does not apply to explorers: they never write, so a worktree
/// would only cost a checkout (review P7). An explicit request still isolates them.
pub(crate) fn skips_default_isolation(config: &Config, role_name: Option<&str>) -> bool {
    applies_to_role(config, role_name)
}

impl TurnEnvironmentSnapshot {
    /// A copy whose ready environments have `update` applied to their resolved
    /// configuration; starting and failed environments are kept as they are.
    ///
    /// Used by `explorer_profile` to judge a worktree by the explorer's read-only access.
    pub(crate) fn with_environment_configs(&self, update: impl Fn(&mut EnvironmentConfig)) -> Self {
        let environments = self
            .environments
            .iter()
            .map(|environment| match environment {
                TurnEnvironmentState::Ready(ready) => {
                    let mut ready = ready.clone();
                    if let EnvironmentConfigState::Ready(config) = &mut ready.selection.config {
                        update(config);
                    }
                    TurnEnvironmentState::Ready(ready)
                }
                other => other.clone(),
            })
            .collect();
        Self { environments }
    }
}

/// Narrows every ready environment of a snapshot to read-only access for an explorer, so the
/// worktree note judges the parent checkout by the access the explorer actually gets.
pub(crate) fn clamp_environment_snapshot(
    environments: &crate::environment_selection::TurnEnvironmentSnapshot,
) -> crate::environment_selection::TurnEnvironmentSnapshot {
    environments.with_environment_configs(|config| {
        clamp_environment_permission_profile(&mut config.permission_profile);
    })
}

/// Explorer sessions: narrow an owner-provided environment configuration to read-only access.
pub(crate) fn clamp_environment_selection(selection: &mut TurnEnvironmentSelection) {
    if let EnvironmentConfigState::Ready(config) = &mut selection.config {
        clamp_environment_permission_profile(&mut config.permission_profile);
    }
}

pub(crate) fn clamp_environment_permission_profile(profile: &mut PermissionProfileSnapshot) {
    *profile = PermissionProfileSnapshot::legacy(read_only_ceiling(profile.permission_profile()));
}

/// Cheapest reasoning effort a model supports: the lowest effort not below `low`, or the
/// lowest effort when the model supports nothing at `low` or above. Unknown efforts are ignored.
pub(crate) fn cheapest_reasoning_effort(
    supported: &[ReasoningEffortPreset],
) -> Option<ReasoningEffort> {
    let ranked = supported
        .iter()
        .filter_map(|preset| effort_rank(&preset.effort).map(|rank| (rank, &preset.effort)));
    let low = effort_rank(&ReasoningEffort::Low).unwrap_or_default();
    ranked
        .clone()
        .filter(|(rank, _)| *rank >= low)
        .min_by_key(|(rank, _)| *rank)
        .or_else(|| ranked.min_by_key(|(rank, _)| *rank))
        .map(|(_, effort)| effort.clone())
}

fn effort_rank(effort: &ReasoningEffort) -> Option<u8> {
    match effort {
        ReasoningEffort::None => Some(0),
        ReasoningEffort::Minimal => Some(1),
        ReasoningEffort::Low => Some(2),
        ReasoningEffort::Medium => Some(3),
        ReasoningEffort::High => Some(4),
        ReasoningEffort::XHigh => Some(5),
        ReasoningEffort::Max => Some(6),
        ReasoningEffort::Ultra => Some(7),
        ReasoningEffort::Persistent | ReasoningEffort::Custom(_) => None,
    }
}

/// Result limit for a session's final answer when it is delivered to the parent.
pub(crate) fn result_max_tokens(config: &Config, source: &SessionSource) -> Option<usize> {
    applies_to_source(config, source).then_some(config.explorer_profile.max_result_tokens)
}

/// Result limit for a listed child, judged by the parent's config and the child's role.
pub(crate) fn result_max_tokens_for_role(config: &Config, role: Option<&str>) -> Option<usize> {
    applies_to_role(config, role).then_some(config.explorer_profile.max_result_tokens)
}

/// Limits an explorer's final answer before it reaches the parent's context.
///
/// The explorer's own history keeps the full answer; only the delivered copy is shortened.
/// A completed result capped at the explorer profile's limit, when the child has one.
pub(crate) fn cap_status(
    status: codex_protocol::protocol::AgentStatus,
    max_tokens: Option<usize>,
) -> codex_protocol::protocol::AgentStatus {
    match (status, max_tokens) {
        (codex_protocol::protocol::AgentStatus::Completed(Some(message)), Some(max_tokens)) => {
            codex_protocol::protocol::AgentStatus::Completed(Some(cap_result(&message, max_tokens)))
        }
        (status, _) => status,
    }
}

pub(crate) fn cap_result(message: &str, max_tokens: usize) -> String {
    if approx_token_count(message) <= max_tokens {
        return message.to_string();
    }
    let truncated = truncate_text(message, TruncationPolicy::Tokens(max_tokens));
    format!(
        "{truncated}\n\n[explorer_profile: this result was truncated to about {max_tokens} tokens. Ask this explorer a narrower follow-up question for the omitted part instead of re-reading the code yourself.]"
    )
}

/// Developer instructions appended to every explorer session.
pub(crate) fn developer_instructions(profile: &ExplorerProfileConfig) -> String {
    let max_result_tokens = profile.max_result_tokens;
    format!(
        "{INSTRUCTIONS_HEADING}\nYou are a read-only explorer working for a parent agent. Your sandbox cannot write files anywhere, including any worktree you run in, and you cannot request escalated or additional permissions, so do not try to create, edit, move or delete files. Only MCP tools annotated read-only are available to you; client-defined tools are not. You cannot spawn agents; tell your parent what needs another agent.\nAnswer the parent's question with conclusions, not raw material:\n- Lead with the direct answer, then the evidence that supports it.\n- Cite evidence as `path:line` or `path:start-end` references instead of pasting file contents; quote at most a few lines when a quote is essential.\n- Do not dump whole files, long command output, search results or directory listings.\n- Read excerpts (search, then targeted line ranges) instead of whole files.\n- Keep your final answer under about {max_result_tokens} tokens. The parent receives at most that much; a longer answer is truncated.\n- Messages you send to other agents share one budget of about {max_result_tokens} tokens per turn and are truncated past it; put your conclusions in your final answer.\n- If the codebase cannot answer the question, say what is missing instead of guessing."
    )
}

/// Spawn-tool role entry for the built-in explorer while the profile is enabled.
///
/// `default_subagent_model` is `[agents] default_subagent_model`, which an explorer uses
/// when the profile names no model.
pub(crate) fn role_description(
    profile: &ExplorerProfileConfig,
    default_subagent_model: Option<&str>,
) -> String {
    let model = profile
        .model
        .as_deref()
        .or(default_subagent_model)
        .map(|model| format!("model `{model}`"))
        .unwrap_or_else(|| "your model".to_string());
    let effort = profile
        .reasoning_effort
        .as_ref()
        .map(|effort| format!("`{effort}` reasoning effort"))
        .unwrap_or_else(|| "the model's cheapest reasoning effort (not below `low`)".to_string());
    let max_result_tokens = profile.max_result_tokens;
    format!(
        r#"Use `explorer` for specific, well-scoped questions about the codebase when you need the conclusion, not the file contents.
Explorers are read-only: they cannot edit workspace files, run commands outside their read-only sandbox (command allow rules do not lift it), request escalated permissions, call MCP tools that are not annotated read-only, or spawn agents. Unless you pass `model` or `reasoning_effort`, they run on {model} at {effort}. They start without your conversation history unless you pass `fork_turns="all"` (put every needed detail in the message), and return a conclusion with `path:line` references capped at about {max_result_tokens} tokens; their messages to other agents share the same cap per turn.
Rules:
- Ask one specific question per explorer. Spawn several explorers in parallel for independent questions and keep working on other things while they run.
- Trust explorer results; read the cited lines yourself only for details the answer does not cover.
- Ask an existing explorer a follow-up question for more detail or for a related question.
- Do not use explorers for work that must change files; do it yourself or use a `worker`."#
    )
}

/// Summary appended to a user-defined `explorer` role while the profile is enabled.
pub(crate) fn role_profile_summary(profile: &ExplorerProfileConfig) -> String {
    format!(
        "Explorer profile: read-only without escalation (command allow rules keep the sandbox; only MCP tools annotated read-only; no spawned agents), a cheaper reasoning effort by default, starts without your history unless `fork_turns=\"all\"`, result and messages to other agents capped at about {} tokens.",
        profile.max_result_tokens
    )
}

#[cfg(test)]
#[path = "explorer_profile_tests.rs"]
mod tests;
