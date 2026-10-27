//! Fork: tool plan hooks of `spec_plan.rs`.
//!
//! - Guardian reviewers get core tools and extension executors only.
//! - Code Mode keeps MCP tools that the exposure policy omitted from Code Mode out of `exec`,
//!   and gives direct-only tools a stub that says to call them directly
//!   (`docs/fork/03-tools-mcp-code-mode.md`).
//! - Exit notifications, the explorer profile, worktree isolation, Default-mode questions and
//!   workflows adjust the tools they own.

use super::*;

/// A Guardian reviewer (basic session) gets no MCP, dynamic or hosted tools.
pub(super) fn basic_tool_router(
    session: &Session,
    turn_context: &TurnContext,
    model_info: &ModelInfo,
    step_store: &ExtensionData,
    mut registry: ToolRegistry,
) -> CodexResult<ToolRouter> {
    let _standalone_web_search_tool = append_extension_tool_executors(
        turn_context,
        model_info,
        extension_tool_executors(session, step_store),
        &mut registry,
    );
    finalize_tool_router(
        turn_context,
        model_info,
        registry,
        Vec::new(),
        &HashMap::new(),
        &session.services.tool_search_handler_cache,
    )
}

/// Whether a tool of this exposure is offered to Code Mode; direct-only tools get a stub.
pub(super) fn available_in_code_mode(exposure: ToolExposure) -> bool {
    exposure.is_available_in_code_mode() || exposure == ToolExposure::DirectModelOnly
}

/// Registry entries in the order `code_mode_tools` deduplicates their Code Mode names: tools
/// that Code Mode calls come first, so the stub of a direct-only tool takes only a name no
/// nested tool has (R3 in `docs/fork/03-tools-mcp-code-mode.md`).
pub(super) fn code_mode_entries(
    registry: &ToolRegistry,
) -> impl Iterator<Item = &RegisteredTool> + '_ {
    let direct_only = |tool: &&RegisteredTool| tool.exposure == ToolExposure::DirectModelOnly;
    registry
        .entries()
        .filter(move |tool| !direct_only(tool))
        .chain(registry.entries().filter(direct_only))
}

/// The fork's filter over the nested Code Mode tools.
pub(super) struct CodeModeTools<'a> {
    turn_context: &'a TurnContext,
    model_info: &'a ModelInfo,
    mcp_omitted_exposures: &'a HashMap<ToolName, ToolExposures>,
    /// Stubs of direct-only tools, listed by `exec` without a route.
    pub(super) direct_tool_stubs: Vec<codex_code_mode::ToolDefinition>,
}

impl<'a> CodeModeTools<'a> {
    pub(super) fn new(
        turn_context: &'a TurnContext,
        model_info: &'a ModelInfo,
        mcp_omitted_exposures: &'a HashMap<ToolName, ToolExposures>,
    ) -> Self {
        Self {
            turn_context,
            model_info,
            mcp_omitted_exposures,
            direct_tool_stubs: Vec::new(),
        }
    }

    /// Whether `tool` stays out of the nested Code Mode tools. Direct-only tools leave a stub;
    /// they are not nested tools and stay out of Code Mode routing metadata.
    pub(super) fn skips(&mut self, spec: &ToolSpec, tool: &RegisteredTool) -> bool {
        let tool_name = tool.runtime.tool_name();
        if !code_mode_only_strict_3p_tools(self.turn_context, self.model_info)
            && self
                .mcp_omitted_exposures
                .get(&tool_name)
                .is_some_and(|exposures| exposures.contains(ToolExposures::CODE_MODE))
        {
            return true;
        }
        if tool.exposure != ToolExposure::DirectModelOnly {
            return false;
        }
        self.direct_tool_stubs.extend(
            collect_code_mode_exec_prompt_tool_definitions(
                std::iter::once(spec),
                self.turn_context.config.code_mode.tool_input_schema_max_bytes,
            )
            .into_iter()
            .map(|mut definition| {
                let name = definition.name.clone();
                definition.description = format!(
                    "Direct model tool only. Call the matching tool directly; do not invoke `{name}` through `exec` or `tools.*`.\n\n{}",
                    definition.description
                );
                definition.input_schema = None;
                definition.output_schema = None;
                definition
            }),
        );
        true
    }
}

/// Whether background `exec_command` processes can notify this thread on exit.
pub(super) fn exec_exit_notifications_enabled(turn_context: &TurnContext) -> bool {
    let features = turn_context.config.features.get();
    features.enabled(Feature::UnifiedExec) && features.enabled(Feature::ExecExitNotifications)
}

/// A catalog description replaces the bundled `wait_agent` one, so it gets the
/// exit-notification sentence too (background-exec-notifications R4.6).
pub(super) fn wait_agent_description(
    model_messages: ResolvedModelMessages<'_>,
    exit_notifications: bool,
) -> Option<String> {
    model_messages
        .multi_agent_tool_description_override("wait_agent")
        .map(|description| {
            if exit_notifications {
                let guidance = crate::tools::handlers::multi_agents_spec::WAIT_AGENT_EXIT_NOTIFICATION_GUIDANCE;
                format!("{description}{guidance}")
            } else {
                description.to_string()
            }
        })
}

/// The explorer profile needs `agent_type` even without user-defined roles.
pub(super) fn expose_agent_type(config: &crate::config::Config) -> bool {
    !config.agent_roles.is_empty() || crate::agent::explorer_profile::enabled(config)
}

/// A read-only explorer does not see the tools it may not call (explorer-and-questions, R2.9).
pub(super) fn hide_explorer_tools(
    turn_context: &TurnContext,
    mcp: &codex_mcp::McpBinding,
    registry: &mut ToolRegistry,
) {
    crate::agent::explorer_profile::hide_unavailable_tools(
        &turn_context.config,
        &turn_context.session_source,
        mcp.tools(),
        registry,
    );
}

/// The `spawn_agent` (V2) options of the explorer profile and worktree isolation.
pub(super) fn spawn_agent_options(
    config: &crate::config::Config,
) -> crate::tools::handlers::multi_agents_spec::SpawnAgentForkOptions {
    crate::tools::handlers::multi_agents_spec::SpawnAgentForkOptions {
        explorer_profile: crate::agent::explorer_profile::enabled(config),
        worktree_isolation: config.features.enabled(Feature::AgentWorktreeIsolation),
    }
}

/// `apply_agent_worktree` beside the other MultiAgentV2 tools while worktree isolation is
/// enabled; a read-only explorer does not get it (agent-worktree-isolation, R11.1).
pub(super) fn add_apply_agent_worktree_tool(
    turn_context: &TurnContext,
    registry: &mut ToolRegistry,
    exposure: ToolExposure,
) {
    let config = &turn_context.config;
    if !config.features.enabled(Feature::AgentWorktreeIsolation)
        || crate::agent::explorer_profile::applies_to_source(config, &turn_context.session_source)
    {
        return;
    }
    registry.register_trusted_with_exposure(
        multi_agent_v2_handler(
            crate::tools::handlers::multi_agents_v2::apply_agent_worktree_fork::Handler,
            config.multi_agent_v2.tool_namespace.as_deref(),
            /*description_override*/ None,
            /*parameters_override*/ None,
            config.multi_agent_v2.message_delivery,
        ),
        exposure,
    );
}

/// Default mode is offered only to interactive root threads.
pub(super) fn request_user_input_modes(
    modes: Vec<codex_protocol::config_types::ModeKind>,
    turn_context: &TurnContext,
) -> Vec<codex_protocol::config_types::ModeKind> {
    crate::tools::handlers::request_user_input_spec::modes_for_session(
        modes,
        &turn_context.session_source,
    )
}

/// Background workflow runs (dynamic workflow, UC1).
pub(super) fn add_workflow_tools(context: &CoreToolPlanContext<'_>, registry: &mut ToolRegistry) {
    crate::workflow::add_workflow_tools(
        context.turn_context,
        context.workflows_available,
        registry,
    );
}
