//! Shared native MAv2 child preparation and spawning.
//!
//! Callers supply captured step settings; the configured agent backend owns child lifetimes.

use std::sync::Arc;

use crate::agent::api::AgentInput;
use crate::agent::api::SpawnRequest;
use crate::agent::child_config::SpawnConfigOptions;
use crate::agent::child_config::SpawnConfigVersion;
use crate::agent::child_config::prepare_agent_spawn_config;
use crate::agent::next_thread_spawn_depth;
use crate::agent::types::AgentMessage;
use crate::agent::types::LiveAgent;
use crate::agent::types::MessageDeliveryMode;
use crate::agent::types::SpawnAgentForkMode;
use crate::agent::types::SpawnAgentOptions;
use crate::agent::types::TurnSpawnBudget;
use crate::agent::types::WorkflowChildSpawn;
use crate::agent::worktree_isolation::AgentWorktreeBinding;
use crate::agent::worktree_isolation::PreparedAgentWorktree;
use crate::agent::worktree_isolation::prepare_agent_worktree;
use crate::agent::worktree_isolation::resolve_isolation;
use crate::codex_thread::ThreadConfigSnapshot;
use crate::session::multi_agents::resolve_usage_hints;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use codex_features::AgentIsolation;
use codex_prompts::ResolvedModelMessages;
use codex_protocol::AgentPath;
use codex_protocol::error::CodexErr;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;

/// Child creation options; omitted overrides retain native MAv2 inheritance and role rules.
pub(crate) struct ThreadSpawnRequest {
    pub task_name: String,
    /// Explicit encoding: hosts normally provide plaintext; native tools can forward ciphertext.
    pub message: AgentMessage,
    /// `None` starts fresh; `FullHistory` uses native fork preparation.
    pub fork_mode: Option<SpawnAgentForkMode>,
    pub agent_type: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Fork: `None` uses the prepared child config's default, so a role config file can choose.
    pub isolation: Option<AgentIsolation>,
}

/// Accepted child and its effective configuration. Acceptance does not mean the turn completed.
pub(crate) struct ThreadSpawnResult {
    pub agent: LiveAgent,
    pub config: ThreadConfigSnapshot,
    pub task_name: AgentPath,
    /// Fork: the child's managed worktree when it was spawned with worktree isolation.
    pub worktree: Option<Arc<AgentWorktreeBinding>>,
}

/// Fork: settings of a child started by a workflow run (dynamic workflow, UC3).
pub(crate) struct WorkflowSpawnOptions {
    /// The run's own spawn budget replaces the parent's per-input budget (UC4, R4.2).
    pub run_spawn_budget: TurnSpawnBudget,
    pub final_output_json_schema: Option<serde_json::Value>,
    /// Appended to the child's developer instructions.
    pub developer_instructions: String,
}

/// Separates preparation errors from backend failures to preserve native tool error reporting.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ThreadSpawnError {
    #[error("{0}")]
    Preparation(String),
    /// Fork: worktree isolation was requested but cannot be provided; no child was spawned.
    #[error("{0}")]
    IsolationUnavailable(String),
    #[error(transparent)]
    Backend(#[from] CodexErr),
}

pub(crate) async fn spawn(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    call_id: &str,
    request: ThreadSpawnRequest,
) -> Result<ThreadSpawnResult, ThreadSpawnError> {
    spawn_with(
        session,
        step_context,
        call_id,
        request,
        /*workflow*/ None,
    )
    .await
}

/// Fork: start a fresh child for a workflow run with collaboration tools disabled, the
/// workflow agent instructions and the run's spawn budget (dynamic workflow, UC3, R3.1, R3.4).
pub(crate) async fn spawn_workflow_child(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    call_id: &str,
    request: ThreadSpawnRequest,
    workflow: WorkflowSpawnOptions,
) -> Result<ThreadSpawnResult, ThreadSpawnError> {
    spawn_with(session, step_context, call_id, request, Some(workflow)).await
}

async fn spawn_with(
    session: &Arc<Session>,
    step_context: &Arc<StepContext>,
    call_id: &str,
    request: ThreadSpawnRequest,
    workflow: Option<WorkflowSpawnOptions>,
) -> Result<ThreadSpawnResult, ThreadSpawnError> {
    let turn = &step_context.turn;
    let ThreadSpawnRequest {
        task_name,
        message,
        fork_mode,
        agent_type,
        model,
        reasoning_effort,
        isolation,
    } = request;
    let message_text = match &message {
        AgentMessage::Plaintext(text) | AgentMessage::Encrypted(text) => text,
    };
    if message_text.trim().is_empty() {
        return Err(ThreadSpawnError::Preparation(
            "Empty message can't be sent to an agent".to_string(),
        ));
    }
    let role_name = agent_type
        .as_deref()
        .map(str::trim)
        .filter(|role| !role.is_empty());
    let prepared = prepare_agent_spawn_config(
        session,
        step_context,
        SpawnConfigOptions {
            version: SpawnConfigVersion::V2,
            full_history_fork: matches!(fork_mode, Some(SpawnAgentForkMode::FullHistory)),
            role_name,
            model: model.as_deref(),
            reasoning_effort,
        },
    )
    .await
    .map_err(ThreadSpawnError::Preparation)?;
    let mut config = prepared.config;
    if let Some(workflow) = &workflow {
        // Fork: workflow agents answer their script and do not delegate (R3.4).
        config.multi_agent_v2.disable_native_tools = true;
        config.developer_instructions = Some(match config.developer_instructions.take() {
            Some(existing) if !existing.trim().is_empty() => {
                format!("{existing}\n\n{}", workflow.developer_instructions)
            }
            _ => workflow.developer_instructions.clone(),
        });
    }
    let task_name = turn
        .session_source
        .get_agent_path()
        .unwrap_or_else(AgentPath::root)
        .join(&task_name)
        .map_err(ThreadSpawnError::Preparation)?;
    // Fork: resolve isolation after the role config file has shaped the child config, then
    // move the child into its own worktree before anything captures its cwd (spec UC1, UC2).
    // Fork: a read-only explorer gets a worktree only when the spawn asks for one explicitly
    // (explorer-and-questions spec, R9.1).
    let explorer = crate::agent::explorer_profile::skips_default_isolation(&turn.config, role_name);
    let isolation = match isolation {
        None if explorer => AgentIsolation::None,
        isolation => {
            resolve_isolation(isolation, &config).map_err(ThreadSpawnError::IsolationUnavailable)?
        }
    };
    let prepared_worktree = if isolation == AgentIsolation::Worktree {
        if !session.services.agent_control.supports_worktree_isolation() {
            return Err(ThreadSpawnError::IsolationUnavailable(
                "worktree isolation requires the local agent backend".to_string(),
            ));
        }
        let thread_id = session.services.local_agent_runtime.generate_thread_id();
        // An explorer's worktree note judges the parent checkout by its read-only access.
        let inherited_environments = if explorer {
            crate::agent::explorer_profile::clamp_environment_snapshot(
                &step_context.inherited_environments,
            )
        } else {
            step_context.inherited_environments.clone()
        };
        Some(
            prepare_agent_worktree(thread_id, &inherited_environments, &mut config, &task_name)
                .await
                .map_err(ThreadSpawnError::IsolationUnavailable)?,
        )
    } else {
        None
    };
    let environments = prepared_worktree
        .as_ref()
        .map(|prepared| prepared.environments.clone())
        .unwrap_or_else(|| step_context.inherited_environments.clone());
    let spawn_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: session.thread_id,
        depth: next_thread_spawn_depth(&turn.session_source),
        agent_path: Some(task_name.clone()),
        agent_nickname: None,
        agent_role: prepared.role_name,
    });
    let multi_agent_v2_usage_hints = if matches!(fork_mode, Some(SpawnAgentForkMode::FullHistory))
        && turn.multi_agent_version == MultiAgentVersion::V2
    {
        let child_model_info = match config.model.as_deref() {
            Some(model) if model != turn.model_info().slug => Some(
                session
                    .services
                    .models_manager
                    .get_model_info(model, &config.to_models_manager_config())
                    .await,
            ),
            _ => None,
        };
        let child_multi_agent_messages = ResolvedModelMessages::from_model(
            child_model_info.as_ref().unwrap_or(turn.model_info()),
        )
        .multi_agent();
        Some(resolve_usage_hints(
            &config.multi_agent_v2,
            child_multi_agent_messages,
            !config.update_plan_enabled && config.model_catalog.is_none(),
        ))
    } else {
        None
    };
    let spawned = session
        .services
        .agent_control
        .spawn(SpawnRequest {
            caller: session.thread_id,
            config,
            input: AgentInput::Message {
                message,
                mode: MessageDeliveryMode::TriggerTurn,
            },
            source: spawn_source,
            options: SpawnAgentOptions {
                fork_parent_spawn_call_id: fork_mode.as_ref().map(|_| call_id.to_string()),
                fork_mode,
                parent_thread_id: Some(session.thread_id),
                parent_turn_id: Some(turn.sub_id.clone()),
                root_turn_id: turn.turn_metadata_state.root_turn_id(),
                turn_trigger: turn.turn_metadata_state.current_turn_trigger(),
                environments: Some(environments),
                disabled_plugin_ids: Some(turn.disabled_plugin_ids.clone()),
                multi_agent_v2_usage_hints,
                cyber_access_program: turn.cyber_access_program,
                turn_spawn_budget: Some(match &workflow {
                    Some(workflow) => workflow.run_spawn_budget.clone(),
                    None => step_context.turn_spawn_budget.clone(),
                }),
                worktree: prepared_worktree
                    .as_ref()
                    .map(PreparedAgentWorktree::spawn_worktree),
                workflow_child: workflow.map(|workflow| WorkflowChildSpawn {
                    final_output_json_schema: workflow.final_output_json_schema,
                }),
            },
        })
        .await;
    let (agent, config) = match spawned {
        Ok(spawned) => spawned,
        Err(err) => {
            // The backend never registered the child, so its worktree holds no work (R2.7).
            if let Some(prepared) = prepared_worktree {
                prepared.discard().await;
            }
            return Err(err.into());
        }
    };
    Ok(ThreadSpawnResult {
        agent,
        config,
        task_name,
        worktree: prepared_worktree.map(|prepared| prepared.binding),
    })
}
