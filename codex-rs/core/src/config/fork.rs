//! Fork: settings of the fork's features and their resolution, moved out of
//! `config/mod.rs` unchanged. `config/mod.rs` re-exports this module, so the public
//! paths (`crate::config::WorkflowsConfig`, ...) stay the same, and calls it once per
//! setting from `Config::load_config_with_layer_stack` where the inline code was.
//!
//! - `[workflows]`: `specs/dynamic-workflow-2026-10-25.md`.
//! - `[output_truncation]`: `docs/fork/02-compaction-context.md`.
//! - `features.explorer_profile`: `specs/explorer-and-questions-2026-10-25.md`.
//! - `features.agent_worktree_isolation`: `specs/agent-worktree-isolation-2026-10-25.md`.
//! - `features.multi_agent_v2.message_delivery`, the removed `agents.max_spawned_threads_per_turn`:
//!   `docs/fork/01-multi-agent-runtime.md`, `docs/fork/03-tools-mcp-code-mode.md`.

use codex_config::config_toml::ConfigToml;
use codex_config::config_toml::OutputTruncationToml;
use codex_features::AgentIsolation;
// `config_tests.rs` names these two through `use super::*`.
pub(super) use codex_features::AgentWorktreeBase;
use codex_features::AgentWorktreeIsolationConfigToml;
use codex_features::Feature;
use codex_features::FeatureToml;
use codex_features::FeaturesToml;
pub(super) use codex_features::MultiAgentMessageDelivery;
use codex_protocol::openai_models::ReasoningEffort;
use serde::Serialize;

use super::DEFAULT_MULTI_AGENT_V2_TOOL_NAMESPACE;
use super::ManagedFeatures;
use super::MultiAgentV2Config;

/// Fork: whether a workflow run needs the user's approval before it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowApproval {
    /// Ask unless the approval policy is `never`.
    #[default]
    Auto,
    Always,
    Never,
}

/// Fork: what a workflow run does once its agents spent the token budget (R5.2, R5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowBudgetMode {
    /// Calls that have not started resolve to null; running agents finish their turn.
    #[default]
    Soft,
    /// As `Soft`, and running agents are interrupted above the overshoot threshold.
    Hard,
}

impl WorkflowBudgetMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Soft => "soft",
            Self::Hard => "hard",
        }
    }
}

/// Fork: how workflow agents are asked for schema answers (R3.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowStructuredOutput {
    /// Native until the provider rejects it, then the schema in the task.
    #[default]
    Auto,
    Native,
    Prompt,
}

/// Fork: resolved `[workflows]` limits (`specs/dynamic-workflow-2026-10-25.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkflowsConfig {
    pub max_active_runs: usize,
    pub max_concurrent_agents: usize,
    pub max_agents_per_run: usize,
    pub capacity_wait_ms: u64,
    pub default_budget_tokens: Option<u64>,
    pub notification_max_tokens: usize,
    pub max_heap_bytes: usize,
    pub exec_wait_timeout_ms: u64,
    pub require_approval: WorkflowApproval,
    pub budget_mode: WorkflowBudgetMode,
    /// At least 1.0.
    pub budget_hard_overshoot: f64,
    pub structured_output: WorkflowStructuredOutput,
    /// Reasoning effort `ultra` turns on workflow ultra mode
    /// (`specs/fork-workflows-ultra-mode-2026-10-28.md`).
    pub ultra_mode: bool,
}

impl WorkflowsConfig {
    /// `min(16, max(1, available_parallelism - 2))`.
    pub fn default_max_concurrent_agents() -> usize {
        let cpus = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        cpus.saturating_sub(2).clamp(1, 16)
    }

    /// Most `budget_hard_overshoot` accepted; a larger value is a configuration error.
    pub const MAX_BUDGET_HARD_OVERSHOOT: f64 = 100.0;

    /// `[workflows]` with defaults for absent keys. A `budget_hard_overshoot` above
    /// [`Self::MAX_BUDGET_HARD_OVERSHOOT`] or not finite is rejected rather than cut, so a
    /// typo such as `120` for `1.20` does not silently allow 100 times the budget (R5.4).
    pub(super) fn from_toml(
        toml: Option<&codex_config::config_toml::WorkflowsToml>,
    ) -> std::io::Result<Self> {
        use codex_config::config_toml::WorkflowApprovalToml;
        use codex_config::config_toml::WorkflowBudgetModeToml;
        use codex_config::config_toml::WorkflowStructuredOutputToml;
        let defaults = Self::default();
        let Some(toml) = toml else {
            return Ok(defaults);
        };
        let budget_hard_overshoot = match toml.budget_hard_overshoot {
            None => defaults.budget_hard_overshoot,
            Some(overshoot)
                if overshoot.is_finite() && overshoot <= Self::MAX_BUDGET_HARD_OVERSHOOT =>
            {
                overshoot.max(1.0)
            }
            Some(overshoot) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "workflows.budget_hard_overshoot must be a number from 1.0 to {} (values below 1.0 count as 1.0), got {overshoot}",
                        Self::MAX_BUDGET_HARD_OVERSHOOT
                    ),
                ));
            }
        };
        Ok(Self {
            max_active_runs: toml
                .max_active_runs
                .unwrap_or(defaults.max_active_runs)
                .max(1),
            max_concurrent_agents: toml
                .max_concurrent_agents
                .unwrap_or(defaults.max_concurrent_agents)
                .max(1),
            max_agents_per_run: toml
                .max_agents_per_run
                .unwrap_or(defaults.max_agents_per_run),
            capacity_wait_ms: toml.capacity_wait_ms.unwrap_or(defaults.capacity_wait_ms),
            default_budget_tokens: toml.default_budget_tokens,
            notification_max_tokens: toml
                .notification_max_tokens
                .unwrap_or(defaults.notification_max_tokens),
            max_heap_bytes: toml.max_heap_bytes.unwrap_or(defaults.max_heap_bytes),
            exec_wait_timeout_ms: toml
                .exec_wait_timeout_ms
                .unwrap_or(defaults.exec_wait_timeout_ms),
            require_approval: match toml.require_approval.unwrap_or_default() {
                WorkflowApprovalToml::Auto => WorkflowApproval::Auto,
                WorkflowApprovalToml::Always => WorkflowApproval::Always,
                WorkflowApprovalToml::Never => WorkflowApproval::Never,
            },
            budget_mode: match toml.budget_mode.unwrap_or_default() {
                WorkflowBudgetModeToml::Soft => WorkflowBudgetMode::Soft,
                WorkflowBudgetModeToml::Hard => WorkflowBudgetMode::Hard,
            },
            budget_hard_overshoot,
            structured_output: match toml.structured_output.unwrap_or_default() {
                WorkflowStructuredOutputToml::Auto => WorkflowStructuredOutput::Auto,
                WorkflowStructuredOutputToml::Native => WorkflowStructuredOutput::Native,
                WorkflowStructuredOutputToml::Prompt => WorkflowStructuredOutput::Prompt,
            },
            ultra_mode: toml.ultra_mode.unwrap_or(defaults.ultra_mode),
        })
    }

    /// R5.4: charged tokens above which a hard-mode run interrupts its running agents.
    pub fn hard_budget_threshold(&self, budget_tokens: u64) -> u64 {
        let threshold = (budget_tokens as f64) * self.budget_hard_overshoot;
        if threshold >= u64::MAX as f64 {
            u64::MAX
        } else {
            threshold.ceil() as u64
        }
    }
}

impl Default for WorkflowsConfig {
    fn default() -> Self {
        Self {
            max_active_runs: 3,
            max_concurrent_agents: Self::default_max_concurrent_agents(),
            max_agents_per_run: 1000,
            capacity_wait_ms: 600_000,
            default_budget_tokens: None,
            notification_max_tokens: 2000,
            max_heap_bytes: 256 * 1024 * 1024,
            exec_wait_timeout_ms: 7_200_000,
            require_approval: WorkflowApproval::Auto,
            budget_mode: WorkflowBudgetMode::Soft,
            budget_hard_overshoot: 1.0,
            structured_output: WorkflowStructuredOutput::Auto,
            ultra_mode: true,
        }
    }
}

/// Resolved `[output_truncation]` limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutputTruncationConfig {
    pub max_bytes: Option<usize>,
    pub max_lines: Option<usize>,
    pub mcp_max_lines: Option<usize>,
}

/// Fork: resolved `[features.explorer_profile]` settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExplorerProfileConfig {
    /// Model for explorer children; `None` keeps the parent's model.
    pub model: Option<String>,
    /// Reasoning effort for explorer children; `None` selects the cheapest supported effort.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Largest explorer final answer, in tokens, delivered to the parent.
    pub max_result_tokens: usize,
}

pub const DEFAULT_EXPLORER_PROFILE_MAX_RESULT_TOKENS: usize = 2_000;

/// Fork: resolved `[features.exec_exit_notifications]` settings
/// (`specs/background-exec-notifications-2026-10-25.md`, UC7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExecExitNotificationsConfig {
    /// How long `codex exec` waits for background terminals before it gives up.
    pub exec_wait_timeout_ms: u64,
    /// How long an output watch alone is waited for after its last matching line (review
    /// 2026-10-27, P2-3).
    pub output_watch_quiet_ms: u64,
    /// How long an output watch alone is waited for before its first matching line, from the
    /// end of the turn that started it (tails 2026-10-27).
    pub output_watch_first_match_ms: u64,
}

pub const DEFAULT_EXEC_WAIT_TIMEOUT_MS: u64 = 30 * 60 * 1000;
pub const DEFAULT_OUTPUT_WATCH_QUIET_MS: u64 = 60 * 1000;
pub const DEFAULT_OUTPUT_WATCH_FIRST_MATCH_MS: u64 = 5 * 60 * 1000;

impl Default for ExecExitNotificationsConfig {
    fn default() -> Self {
        Self {
            exec_wait_timeout_ms: DEFAULT_EXEC_WAIT_TIMEOUT_MS,
            output_watch_quiet_ms: DEFAULT_OUTPUT_WATCH_QUIET_MS,
            output_watch_first_match_ms: DEFAULT_OUTPUT_WATCH_FIRST_MATCH_MS,
        }
    }
}

impl ExecExitNotificationsConfig {
    /// Resolves the settings from a features table, keeping defaults for omitted values.
    pub(crate) fn from_features_toml(features: Option<&FeaturesToml>) -> Self {
        let configured =
            match features.and_then(|features| features.exec_exit_notifications.as_ref()) {
                Some(FeatureToml::Config(config)) => Some(config),
                Some(FeatureToml::Enabled(_)) | None => None,
            };
        Self {
            exec_wait_timeout_ms: configured
                .and_then(|config| config.exec_wait_timeout_ms)
                .unwrap_or(DEFAULT_EXEC_WAIT_TIMEOUT_MS),
            output_watch_quiet_ms: configured
                .and_then(|config| config.output_watch_quiet_ms)
                .unwrap_or(DEFAULT_OUTPUT_WATCH_QUIET_MS),
            output_watch_first_match_ms: configured
                .and_then(|config| config.output_watch_first_match_ms)
                .unwrap_or(DEFAULT_OUTPUT_WATCH_FIRST_MATCH_MS),
        }
    }
}

/// Fork: resolved `[features.tool_output_spill]` limits
/// (`specs/tool-output-spill-2026-11-07.md`, UC6, R6.2, R6.3). The feature flag gates the
/// behavior; these limits apply while it is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ToolOutputSpillConfig {
    /// Largest saved file; a larger output keeps its first and last halves.
    pub max_file_bytes: u64,
    /// Largest size of one thread's directory; the oldest files go first.
    pub max_thread_bytes: u64,
    /// Largest size of all threads' saved outputs, checked once per process.
    pub max_total_bytes: u64,
    /// Days after which a saved output is removed, checked once per process.
    pub max_age_days: u64,
}

pub const DEFAULT_TOOL_OUTPUT_SPILL_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_TOOL_OUTPUT_SPILL_MAX_THREAD_BYTES: u64 = 256 * 1024 * 1024;
pub const DEFAULT_TOOL_OUTPUT_SPILL_MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const DEFAULT_TOOL_OUTPUT_SPILL_MAX_AGE_DAYS: u64 = 7;
pub const MIN_TOOL_OUTPUT_SPILL_MAX_FILE_BYTES: u64 = 64 * 1024;

impl Default for ToolOutputSpillConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_TOOL_OUTPUT_SPILL_MAX_FILE_BYTES,
            max_thread_bytes: DEFAULT_TOOL_OUTPUT_SPILL_MAX_THREAD_BYTES,
            max_total_bytes: DEFAULT_TOOL_OUTPUT_SPILL_MAX_TOTAL_BYTES,
            max_age_days: DEFAULT_TOOL_OUTPUT_SPILL_MAX_AGE_DAYS,
        }
    }
}

impl ToolOutputSpillConfig {
    /// Resolves the limits from a features table, keeping defaults for omitted values; a limit
    /// below its minimum is an error that names the key (R6.3).
    pub(crate) fn from_features_toml(features: Option<&FeaturesToml>) -> std::io::Result<Self> {
        let configured = match features.and_then(|features| features.tool_output_spill.as_ref()) {
            Some(FeatureToml::Config(config)) => Some(config),
            Some(FeatureToml::Enabled(_)) | None => None,
        };
        let defaults = Self::default();
        let resolved = Self {
            max_file_bytes: configured
                .and_then(|config| config.max_file_bytes)
                .unwrap_or(defaults.max_file_bytes),
            max_thread_bytes: configured
                .and_then(|config| config.max_thread_bytes)
                .unwrap_or(defaults.max_thread_bytes),
            max_total_bytes: configured
                .and_then(|config| config.max_total_bytes)
                .unwrap_or(defaults.max_total_bytes),
            max_age_days: configured
                .and_then(|config| config.max_age_days)
                .unwrap_or(defaults.max_age_days),
        };
        let invalid = |message: String| {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("features.tool_output_spill.{message}"),
            ))
        };
        if resolved.max_file_bytes < MIN_TOOL_OUTPUT_SPILL_MAX_FILE_BYTES {
            return invalid(format!(
                "max_file_bytes must be at least {MIN_TOOL_OUTPUT_SPILL_MAX_FILE_BYTES}"
            ));
        }
        if resolved.max_thread_bytes < resolved.max_file_bytes {
            return invalid(format!(
                "max_thread_bytes must be at least max_file_bytes ({})",
                resolved.max_file_bytes
            ));
        }
        if resolved.max_total_bytes < resolved.max_thread_bytes {
            return invalid(format!(
                "max_total_bytes must be at least max_thread_bytes ({})",
                resolved.max_thread_bytes
            ));
        }
        if resolved.max_age_days == 0 {
            return invalid("max_age_days must be at least 1".to_string());
        }
        Ok(resolved)
    }
}
pub const MIN_EXPLORER_PROFILE_MAX_RESULT_TOKENS: usize = 200;

impl Default for ExplorerProfileConfig {
    fn default() -> Self {
        Self {
            model: None,
            reasoning_effort: None,
            max_result_tokens: DEFAULT_EXPLORER_PROFILE_MAX_RESULT_TOKENS,
        }
    }
}

/// Fork: resolved `[features.agent_worktree_isolation]` settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AgentWorktreeIsolationConfig {
    /// Isolation used when `spawn_agent` omits `isolation`; role config files may set it.
    pub default_isolation: AgentIsolation,
    /// Commit an isolated child's worktree starts from.
    pub base: AgentWorktreeBase,
    /// Removal of forgotten unchanged agent worktrees (agent-worktree-isolation, UC10).
    pub cleanup: AgentWorktreeCleanup,
}

/// Fork: limits of the sweep of forgotten unchanged agent worktrees (R10.4, R10.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AgentWorktreeCleanup {
    pub enabled: bool,
    /// Unchanged worktrees kept among the most recently active ones.
    pub keep_count: u32,
    /// Hours without activity after which an unchanged worktree is removed; at least 1.
    pub max_idle_hours: u32,
}

impl Default for AgentWorktreeCleanup {
    fn default() -> Self {
        Self {
            enabled: true,
            keep_count: 15,
            max_idle_hours: 24,
        }
    }
}

impl AgentWorktreeIsolationConfig {
    /// Resolves the settings from a features table, keeping defaults for omitted values.
    pub(crate) fn from_features_toml(features: Option<&FeaturesToml>) -> std::io::Result<Self> {
        let configured = agent_worktree_isolation_toml_config(features);
        let defaults = AgentWorktreeCleanup::default();
        let cleanup = AgentWorktreeCleanup {
            enabled: configured
                .and_then(|config| config.cleanup)
                .unwrap_or(defaults.enabled),
            keep_count: configured
                .and_then(|config| config.cleanup_keep_count)
                .unwrap_or(defaults.keep_count),
            max_idle_hours: configured
                .and_then(|config| config.cleanup_max_idle_hours)
                .unwrap_or(defaults.max_idle_hours),
        };
        if cleanup.max_idle_hours == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "features.agent_worktree_isolation.cleanup_max_idle_hours must be at least 1",
            ));
        }
        Ok(Self {
            default_isolation: configured
                .and_then(|config| config.default_isolation)
                .unwrap_or_default(),
            base: configured
                .and_then(|config| config.base)
                .unwrap_or_default(),
            cleanup,
        })
    }
}

fn explorer_profile_toml_config(
    features: Option<&FeaturesToml>,
) -> Option<&codex_features::ExplorerProfileConfigToml> {
    match features?.explorer_profile.as_ref()? {
        FeatureToml::Enabled(_) => None,
        FeatureToml::Config(config) => Some(config),
    }
}

fn resolve_explorer_profile_config(
    config_toml: &ConfigToml,
) -> std::io::Result<ExplorerProfileConfig> {
    let Some(base) = explorer_profile_toml_config(config_toml.features.as_ref()) else {
        return Ok(ExplorerProfileConfig::default());
    };
    let max_result_tokens = base
        .max_result_tokens
        .unwrap_or(DEFAULT_EXPLORER_PROFILE_MAX_RESULT_TOKENS);
    if max_result_tokens < MIN_EXPLORER_PROFILE_MAX_RESULT_TOKENS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "features.explorer_profile.max_result_tokens must be at least {MIN_EXPLORER_PROFILE_MAX_RESULT_TOKENS}"
            ),
        ));
    }
    let model = base
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned);
    Ok(ExplorerProfileConfig {
        model,
        reasoning_effort: base.reasoning_effort.clone(),
        max_result_tokens,
    })
}

pub(crate) fn agent_worktree_isolation_toml_config(
    features: Option<&FeaturesToml>,
) -> Option<&AgentWorktreeIsolationConfigToml> {
    match features?.agent_worktree_isolation.as_ref()? {
        FeatureToml::Enabled(_) => None,
        FeatureToml::Config(config) => Some(config),
    }
}

/// Fork: namespace of the MultiAgentV2 tools while worktree isolation is enabled and
/// `features.multi_agent_v2.tool_namespace` is unset (agent-worktree-isolation, R1.5).
pub(crate) const AGENT_WORKTREE_ISOLATION_TOOL_NAMESPACE: &str = "agents";

/// Startup warning of a session whose worktree isolation is off because the configured
/// namespace is the reserved default (agent-worktree-isolation, R1.5).
pub(crate) const AGENT_WORKTREE_ISOLATION_RESERVED_NAMESPACE_WARNING: &str = "Subagent worktrees (`features.agent_worktree_isolation`) are off for this session: `features.multi_agent_v2.tool_namespace` is set to the reserved namespace \"collaboration\", whose `spawn_agent` tool the Responses API rejects with the extra `isolation` parameter. To use subagent worktrees, remove `tool_namespace` (Codex then uses \"agents\") or set it to another namespace such as \"agents\".";

/// Whether `features.multi_agent_v2.tool_namespace` is set.
fn multi_agent_v2_tool_namespace_is_set(features_toml: Option<&FeaturesToml>) -> bool {
    features_toml
        .and_then(|features| features.multi_agent_v2.as_ref())
        .is_some_and(|feature| {
            matches!(feature, FeatureToml::Config(config) if config.tool_namespace.is_some())
        })
}

/// `[output_truncation]` limits; omitted values keep the defaults.
pub(super) fn output_truncation_config(
    output_truncation: Option<OutputTruncationToml>,
) -> OutputTruncationConfig {
    output_truncation.map_or_else(
        OutputTruncationConfig::default,
        |OutputTruncationToml {
             max_bytes,
             max_lines,
             mcp_max_lines,
         }| OutputTruncationConfig {
            max_bytes,
            max_lines,
            mcp_max_lines,
        },
    )
}

/// Explorer profile and worktree isolation settings of `cfg`.
pub(super) fn resolve_agent_profiles(
    cfg: &ConfigToml,
) -> std::io::Result<(ExplorerProfileConfig, AgentWorktreeIsolationConfig)> {
    let explorer_profile = resolve_explorer_profile_config(cfg)?;
    let agent_worktree_isolation =
        AgentWorktreeIsolationConfig::from_features_toml(cfg.features.as_ref())?;
    Ok((explorer_profile, agent_worktree_isolation))
}

/// `agents.max_spawned_threads_per_turn` limited the children of one root input until the
/// fork removed its cumulative spawn budget (2026-10-26, docs/fork/01-multi-agent-runtime.md
/// F1). A config that still sets it loads; the key is ignored with a startup warning.
pub(super) fn warn_removed_agents_keys(
    agents: Option<&codex_config::config_toml::AgentsToml>,
    startup_warnings: &mut Vec<String>,
) {
    if agents.is_some_and(|agents| agents.max_spawned_threads_per_turn.is_some()) {
        startup_warnings.push(
            "`agents.max_spawned_threads_per_turn` is deprecated and ignored. Codex no longer limits the total number of agents spawned for one input; remove this setting from config.toml or profile configuration files.".to_owned(),
        );
    }
}

/// Plaintext message delivery and worktree isolation need a tool namespace other than the
/// reserved default: the Responses backend checks the tools of that namespace against its own
/// definitions, and worktree isolation adds an `isolation` parameter to `spawn_agent` that it
/// rejects on every request (agent-worktree-isolation, R1.5).
///
/// - Worktree isolation with `features.multi_agent_v2.tool_namespace` unset: every MultiAgentV2
///   tool uses [`AGENT_WORKTREE_ISOLATION_TOOL_NAMESPACE`], as if the user had set it, so turning
///   the feature on from `/experimental` or `codex features enable` needs nothing else. The
///   check is not limited to `features.multi_agent_v2`: the model catalog selects MultiAgentV2
///   for most models without it.
/// - Worktree isolation with the namespace set to the reserved default: the session runs
///   without isolation (no `isolation` parameter, `apply_agent_worktree` or worktree sweep) and
///   starts with a warning that says how to get it back, instead of failing to load.
/// - Plaintext delivery keeps its own check on the resulting namespace, so it still fails
///   without a namespace unless worktree isolation chose one.
///
/// The upstream namespace check accepts both the reserved default and
/// [`AGENT_WORKTREE_ISOLATION_TOOL_NAMESPACE`], so running it first does not matter.
pub(super) fn resolve_tool_namespace(
    features_toml: Option<&FeaturesToml>,
    mut features: ManagedFeatures,
    mut multi_agent_v2: MultiAgentV2Config,
    startup_warnings: &mut Vec<String>,
) -> std::io::Result<(ManagedFeatures, MultiAgentV2Config)> {
    let isolation_enabled = features.enabled(Feature::AgentWorktreeIsolation);
    if isolation_enabled && !multi_agent_v2_tool_namespace_is_set(features_toml) {
        tracing::debug!(
            "features.agent_worktree_isolation is enabled and features.multi_agent_v2.tool_namespace is unset; MultiAgentV2 tools use the namespace \"{AGENT_WORKTREE_ISOLATION_TOOL_NAMESPACE}\""
        );
        multi_agent_v2.tool_namespace = Some(AGENT_WORKTREE_ISOLATION_TOOL_NAMESPACE.to_string());
    }
    let reserved = multi_agent_v2
        .tool_namespace
        .as_deref()
        .is_none_or(|namespace| namespace == DEFAULT_MULTI_AGENT_V2_TOOL_NAMESPACE);
    if multi_agent_v2.message_delivery == MultiAgentMessageDelivery::Plaintext && reserved {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "features.multi_agent_v2.message_delivery = \"plaintext\" requires features.multi_agent_v2.tool_namespace to be set to a non-reserved namespace",
        ));
    }
    if isolation_enabled && reserved {
        features
            .disable(Feature::AgentWorktreeIsolation)
            .map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "features.agent_worktree_isolation cannot be turned off ({err}), but it needs features.multi_agent_v2.tool_namespace to be unset or a non-reserved namespace such as \"{AGENT_WORKTREE_ISOLATION_TOOL_NAMESPACE}\""
                    ),
                )
            })?;
        startup_warnings.push(AGENT_WORKTREE_ISOLATION_RESERVED_NAMESPACE_WARNING.to_owned());
    }
    Ok((features, multi_agent_v2))
}

/// Fork: whether `ConfigEdit::ClearPath { segments }` of a fork feature should write
/// `enabled = false` instead, because the feature's table in `root` holds settings besides
/// `enabled` (explorer-and-questions spec, R1.6).
pub(crate) fn clear_keeps_feature_table(root: &toml_edit::Item, segments: &[String]) -> bool {
    segments
        .iter()
        .try_fold(root, |item, segment| item.as_table_like()?.get(segment))
        .and_then(toml_edit::Item::as_table_like)
        .is_some_and(|table| {
            codex_config::fork_feature_clear_keeps_table(segments, table.iter().map(|(key, _)| key))
        })
}

/// Fork: resolved `[features.scheduled_wakeups]` settings
/// (`specs/scheduled-wakeups-2026-11-07.md`, R1.1, R1.5). The feature flag gates the behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScheduledWakeupsConfig {
    /// Shortest wakeup delay and `/loop` interval, in seconds.
    pub min_delay_seconds: u32,
    /// Longest wakeup delay and `/loop` interval, in seconds.
    pub max_delay_seconds: u32,
    /// Wakeups per chain since the user's last message, and iterations per `/loop`.
    pub max_wakeups: u32,
    /// How long one chain or `/loop` may last, in minutes.
    pub max_duration_minutes: u32,
    /// How long `codex exec` waits for wakeups before it cancels them.
    pub exec_wait_timeout_ms: u64,
}

pub const DEFAULT_WAKEUP_MIN_DELAY_SECONDS: u32 = 60;
pub const DEFAULT_WAKEUP_MAX_DELAY_SECONDS: u32 = 3_600;
pub const DEFAULT_MAX_WAKEUPS: u32 = 50;
pub const DEFAULT_WAKEUP_MAX_DURATION_MINUTES: u32 = 720;
pub const DEFAULT_WAKEUP_EXEC_WAIT_TIMEOUT_MS: u64 = 60 * 60 * 1000;

impl Default for ScheduledWakeupsConfig {
    fn default() -> Self {
        Self {
            min_delay_seconds: DEFAULT_WAKEUP_MIN_DELAY_SECONDS,
            max_delay_seconds: DEFAULT_WAKEUP_MAX_DELAY_SECONDS,
            max_wakeups: DEFAULT_MAX_WAKEUPS,
            max_duration_minutes: DEFAULT_WAKEUP_MAX_DURATION_MINUTES,
            exec_wait_timeout_ms: DEFAULT_WAKEUP_EXEC_WAIT_TIMEOUT_MS,
        }
    }
}

impl ScheduledWakeupsConfig {
    /// Resolves the settings from a features table, keeping defaults for omitted values; a
    /// value outside its range is an error that names the key and the range (R1.5).
    pub(crate) fn from_features_toml(features: Option<&FeaturesToml>) -> std::io::Result<Self> {
        let configured = match features.and_then(|features| features.scheduled_wakeups.as_ref()) {
            Some(FeatureToml::Config(config)) => Some(config),
            Some(FeatureToml::Enabled(_)) | None => None,
        };
        let defaults = Self::default();
        let resolved = Self {
            min_delay_seconds: configured
                .and_then(|config| config.min_delay_seconds)
                .unwrap_or(defaults.min_delay_seconds),
            max_delay_seconds: configured
                .and_then(|config| config.max_delay_seconds)
                .unwrap_or(defaults.max_delay_seconds),
            max_wakeups: configured
                .and_then(|config| config.max_wakeups)
                .unwrap_or(defaults.max_wakeups),
            max_duration_minutes: configured
                .and_then(|config| config.max_duration_minutes)
                .unwrap_or(defaults.max_duration_minutes),
            exec_wait_timeout_ms: configured
                .and_then(|config| config.exec_wait_timeout_ms)
                .unwrap_or(defaults.exec_wait_timeout_ms),
        };
        let out_of_range = |key: &str, range: String| {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("features.scheduled_wakeups.{key} must be {range}"),
            ))
        };
        if !(1..=resolved.max_delay_seconds).contains(&resolved.min_delay_seconds) {
            return out_of_range(
                "min_delay_seconds",
                format!(
                    "from 1 to max_delay_seconds ({})",
                    resolved.max_delay_seconds
                ),
            );
        }
        if resolved.max_delay_seconds > 86_400 {
            return out_of_range(
                "max_delay_seconds",
                format!(
                    "from min_delay_seconds ({}) to 86400",
                    resolved.min_delay_seconds
                ),
            );
        }
        if !(1..=1_000).contains(&resolved.max_wakeups) {
            return out_of_range("max_wakeups", "from 1 to 1000".to_owned());
        }
        if !(1..=10_080).contains(&resolved.max_duration_minutes) {
            return out_of_range("max_duration_minutes", "from 1 to 10080".to_owned());
        }
        if resolved.exec_wait_timeout_ms < 1_000 {
            return out_of_range("exec_wait_timeout_ms", "at least 1000".to_owned());
        }
        Ok(resolved)
    }
}

#[cfg(test)]
#[path = "fork_tests.rs"]
mod tests;
