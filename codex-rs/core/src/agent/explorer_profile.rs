//! Fork: explorer profile for `explorer` children (specs/explorer-and-questions-2026-10-25.md).
//!
//! While `explorer_profile` is enabled, an `explorer` child runs read-only without escalation,
//! starts on a cheaper model or reasoning effort, and delivers a capped conclusion to its parent.
//! Sessions enforce the sandbox themselves so spawn, reload and cold resume share one rule.

use crate::config::Config;
use crate::config::ExplorerProfileConfig;
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
    let instructions = developer_instructions(&config.explorer_profile);
    config.developer_instructions = match config.developer_instructions.take() {
        Some(existing) if existing.contains(INSTRUCTIONS_HEADING) => Some(existing),
        Some(existing) if !existing.trim().is_empty() => {
            Some(format!("{existing}\n\n{instructions}"))
        }
        _ => Some(instructions),
    };
    Ok(())
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
        "{INSTRUCTIONS_HEADING}\nYou are a read-only explorer working for a parent agent. Your sandbox cannot write files and you cannot request escalated or additional permissions, so do not try to create, edit, move or delete files.\nAnswer the parent's question with conclusions, not raw material:\n- Lead with the direct answer, then the evidence that supports it.\n- Cite evidence as `path:line` or `path:start-end` references instead of pasting file contents; quote at most a few lines when a quote is essential.\n- Do not dump whole files, long command output, search results or directory listings.\n- Read excerpts (search, then targeted line ranges) instead of whole files.\n- Keep your final answer under about {max_result_tokens} tokens. The parent receives at most that much; a longer answer is truncated.\n- If the codebase cannot answer the question, say what is missing instead of guessing."
    )
}

/// Spawn-tool role entry for the built-in explorer while the profile is enabled.
pub(crate) fn role_description(profile: &ExplorerProfileConfig) -> String {
    let model = profile
        .model
        .as_deref()
        .map(|model| format!("model `{model}`"))
        .unwrap_or_else(|| "your model".to_string());
    let effort = profile
        .reasoning_effort
        .as_ref()
        .map(|effort| format!("`{effort}` reasoning effort"))
        .unwrap_or_else(|| "its cheapest reasoning effort".to_string());
    let max_result_tokens = profile.max_result_tokens;
    format!(
        r#"Use `explorer` for specific, well-scoped questions about the codebase when you need the conclusion, not the file contents.
Explorers are read-only: they cannot edit files or request escalated permissions. They run on {model} at {effort}, start without your conversation history unless you pass `fork_turns="all"` (put every needed detail in the message), and return a conclusion with `path:line` references capped at about {max_result_tokens} tokens.
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
        "Explorer profile: read-only without escalation, cheaper model or reasoning effort, starts without your history unless `fork_turns=\"all\"`, result capped at about {} tokens.",
        profile.max_result_tokens
    )
}

#[cfg(test)]
#[path = "explorer_profile_tests.rs"]
mod tests;
