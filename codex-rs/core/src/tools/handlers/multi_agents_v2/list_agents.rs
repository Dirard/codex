use super::analytics::ToolCallAnalytics;
use super::*;
use crate::tools::handlers::multi_agents_spec::create_list_agents_tool;
use codex_tools::ToolSpec;

pub(crate) struct Handler;

impl ToolExecutor<ToolInvocation> for Handler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("list_agents")
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        true
    }

    fn spec(&self) -> ToolSpec {
        create_list_agents_tool()
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(async move {
            let analytics = ToolCallAnalytics::new(&invocation, CollabAgentTool::ListAgents);
            let result = self.handle_call(invocation).await;
            analytics.finish(&result);
            result
        })
    }
}

impl Handler {
    async fn handle_call(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn crate::tools::context::ToolOutput>, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;
        let arguments = function_arguments(payload)?;
        let args: ListAgentsArgs = parse_arguments(&arguments)?;
        let mut agents = session
            .services
            .agent_control
            .list(
                session.thread_id,
                turn.parent_thread_id,
                &turn.session_source,
                args.path_prefix.as_deref(),
            )
            .await
            .map_err(wait::fork::collab_list_error("list"))?;
        // Fork: isolated agents that are not loaded keep their worktree visible (D7, R9.6).
        crate::agent::worktree_isolation::fork_add_unloaded(
            &session,
            &turn,
            &args.path_prefix,
            &mut agents,
        )
        .await;

        let workflow_children = session.services.local_agent_runtime.workflow_children();
        // Fork: each isolated agent's worktree (specs/agent-worktree-isolation-2026-10-25.md, R9.1).
        let mut worktrees = crate::agent::worktree_isolation::listed_worktrees(
            &turn.config,
            &session.services.local_agent_runtime.worktrees,
            agents.iter().map(|agent| agent.thread_id),
        )
        .await;
        let agents = agents
            .into_iter()
            // Fork: workflow runs own their agents; finished ones are omitted (dynamic workflow).
            .filter_map(|agent| workflow_children.listed_owner(agent))
            .map(|(agent, owner)| ListedAgent {
                worktree: worktrees.remove(&agent.thread_id),
                agent_name: agent
                    .metadata
                    .agent_path
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| agent.thread_id.to_string()),
                // Fork: an explorer's final answer is capped like its completion mail.
                agent_status: crate::agent::explorer_profile::cap_status(
                    agent.status,
                    crate::agent::explorer_profile::result_max_tokens_for_role(
                        &turn.config,
                        agent.metadata.agent_role.as_deref(),
                    ),
                ),
                owner,
            })
            .collect();
        Ok(boxed_tool_output(ListAgentsResult { agents }))
    }
}

impl CoreToolRuntime for Handler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListAgentsArgs {
    path_prefix: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListedAgent {
    agent_name: String,
    agent_status: AgentStatus,
    /// Fork: `workflow:<run_id>` for agents of a workflow run.
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    /// Fork: the agent's worktree, while `agent_worktree_isolation` is enabled (R9.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree: Option<crate::agent::worktree_isolation::ListedWorktree>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ListAgentsResult {
    agents: Vec<ListedAgent>,
}

impl ToolOutput for ListAgentsResult {
    fn log_output(&self) -> String {
        tool_output_json_text(self, "list_agents")
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        tool_output_response_item(call_id, payload, self, Some(true), "list_agents")
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> JsonValue {
        tool_output_code_mode_result(self, "list_agents")
    }
}
