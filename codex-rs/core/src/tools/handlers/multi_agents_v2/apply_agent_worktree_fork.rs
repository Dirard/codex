//! Fork: `apply_agent_worktree` applies an isolated agent's branch to the caller's checkout.
//!
//! The merge runs host-side on objects only; the result is written through the `apply_patch`
//! path, so approval, sandbox and patch events match any other file edit.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC11.

use super::*;
use crate::agent::worktree_isolation::AgentWorktreeBinding;
use crate::agent::worktree_isolation::apply_refusal;
use crate::agent::worktree_isolation::merge_for_parent;
use crate::tools::context::FunctionToolOutput;
use crate::tools::sandboxing::ToolCtx;
use codex_apply_patch::ApplyPatchFileChange;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use codex_utils_path_uri::PathUri;
use codex_worktree::CleanMerge;
use codex_worktree::MergeOutcome;
use codex_worktree::MergedChange;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) const TOOL_NAME: &str = "apply_agent_worktree";

const DESCRIPTION: &str = "Apply an isolated agent's recorded changes (its codex/agents/* branch from the <agent_worktree> block) to your checkout as uncommitted edits, with a three-way merge over your current files. The write is approved and sandboxed like apply_patch. If the merge conflicts with your edits, nothing is written and the result lists the conflicting files and the git command from the block.";

pub(crate) struct Handler;

impl ToolExecutor<ToolInvocation> for Handler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        let properties = BTreeMap::from([(
            "target".to_string(),
            JsonSchema::string(Some(
                "Agent id or canonical task name of an agent spawned with isolation `worktree`."
                    .to_string(),
            )),
        )]);
        ToolSpec::Function(ResponsesApiTool {
            name: TOOL_NAME.to_string(),
            description: DESCRIPTION.to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec!["target".to_string()]),
                Some(false.into()),
            ),
            output_schema: None,
        })
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(async move {
            handle_apply(invocation)
                .await
                .map(|output| Box::new(output) as Box<dyn ToolOutput>)
        })
    }
}

impl CoreToolRuntime for Handler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyAgentWorktreeArgs {
    target: String,
}

/// Spec UC11: resolve the child and its branch, merge, then write through `apply_patch`.
async fn handle_apply(invocation: ToolInvocation) -> Result<FunctionToolOutput, FunctionCallError> {
    let ToolInvocation {
        session,
        turn,
        step_context,
        cancellation_token,
        tracker,
        call_id,
        tool_name,
        payload,
        ..
    } = invocation;
    if !turn
        .config
        .features
        .enabled(codex_features::Feature::AgentWorktreeIsolation)
    {
        return Err(FunctionCallError::RespondToModel(
            "apply_agent_worktree requires the `agent_worktree_isolation` feature".to_string(),
        ));
    }
    let arguments = function_arguments(payload)?;
    let args: ApplyAgentWorktreeArgs = parse_arguments(&arguments)?;
    let agent_id = resolve_agent_target(&session, &turn, &args.target).await?;
    let binding = session
        .services
        .local_agent_runtime
        .worktrees
        .get(agent_id)
        .ok_or_else(|| {
            FunctionCallError::RespondToModel(format!(
                "agent {} does not work in its own worktree; its changes are already in the shared checkout",
                args.target
            ))
        })?;
    let environment = step_context
        .environments
        .single_local_environment()
        .cloned()
        .ok_or_else(|| {
            FunctionCallError::RespondToModel(
                "apply_agent_worktree requires a single local environment".to_string(),
            )
        })?;
    let caller_cwd = environment.cwd().to_abs_path().map_err(|_| {
        FunctionCallError::RespondToModel(
            "apply_agent_worktree requires a local working directory".to_string(),
        )
    })?;
    if !lies_in(caller_cwd.as_path(), binding.source_root.as_path()) {
        return Err(FunctionCallError::RespondToModel(format!(
            "agent {}'s worktree was created from {}, which is not your checkout",
            args.target,
            binding.source_root.display()
        )));
    }

    let merge = match merge_for_parent(&binding).await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("cannot apply agent {}: {err}", args.target))
    })? {
        MergeOutcome::Clean(merge) => merge,
        MergeOutcome::NoBranch {
            pending_worktree_changes,
        } => {
            let pending = if pending_worktree_changes {
                "; its worktree has changes that Codex records when its turn ends"
            } else {
                ""
            };
            return Err(FunctionCallError::RespondToModel(format!(
                "agent {} has no recorded changes (branch: none){pending}",
                args.target
            )));
        }
        MergeOutcome::AlreadyApplied { tip } => {
            let metadata = binding.metadata();
            return Ok(FunctionToolOutput::from_text(
                format!(
                    "Your checkout already contains the changes of {} at {}; nothing was written.",
                    metadata.branch,
                    short(&tip)
                ),
                Some(true),
            ));
        }
        MergeOutcome::Conflicts(paths) => {
            return Err(FunctionCallError::RespondToModel(apply_refusal(
                &binding,
                &args.target,
                "the merge conflicts with your checkout in",
                &paths,
            )));
        }
        MergeOutcome::Unrepresentable { path, reason } => {
            return Err(FunctionCallError::RespondToModel(apply_refusal(
                &binding,
                &args.target,
                &format!("apply_patch cannot write {reason} in"),
                std::slice::from_ref(&path),
            )));
        }
        MergeOutcome::Unsupported(detail) => {
            return Err(FunctionCallError::RespondToModel(apply_refusal(
                &binding,
                &args.target,
                &format!(
                    "Git cannot merge without a working tree (Git 2.40 or later is required: {detail}) for"
                ),
                &[],
            )));
        }
    };

    let action = verified_action(&binding, &environment, &args.target, &merge).await?;
    let tool_ctx = ToolCtx {
        session,
        step_context,
        cancellation_token,
        call_id,
        tool_name,
    };
    let content = crate::tools::handlers::apply_patch::execute_verified_patch(
        action,
        environment,
        Some(&tracker),
        tool_ctx,
    )
    .await?;
    let metadata = binding.metadata();
    let mut text = format!(
        "Applied {} at {} to {} as uncommitted edits.\n{content}",
        metadata.branch,
        short(&merge.tip),
        binding.source_root.display()
    );
    if merge.pending_worktree_changes {
        text.push_str(
            "\nThe agent's worktree has newer changes that Codex records when its turn ends; they were not applied.",
        );
    }
    Ok(FunctionToolOutput::from_text(text, Some(true)))
}

/// Builds the patch and checks that `apply_patch` reproduces every merged file exactly (R11.6).
async fn verified_action(
    binding: &Arc<AgentWorktreeBinding>,
    environment: &crate::session::turn_context::TurnEnvironment,
    target: &str,
    merge: &CleanMerge,
) -> Result<codex_apply_patch::ApplyPatchAction, FunctionCallError> {
    let refuse = |path: &str, reason: &str| {
        FunctionCallError::RespondToModel(apply_refusal(
            binding,
            target,
            &format!("apply_patch cannot write {reason} in"),
            &[path.to_string()],
        ))
    };
    let args = codex_apply_patch::parse_patch(&patch_text(&merge.changes))
        .map_err(|err| refuse("the merged files", &format!("this change ({err})")))?;
    let cwd = PathUri::from_abs_path(&binding.source_root);
    let fs = environment.environment.get_filesystem();
    let sandbox = environment.sandbox_context(/*additional_permissions*/ None);
    let action =
        match codex_apply_patch::verify_apply_patch_args(args, &cwd, fs.as_ref(), Some(&sandbox))
            .await
        {
            MaybeApplyPatchVerified::Body(action) => action,
            MaybeApplyPatchVerified::CorrectnessError(err) => {
                return Err(refuse(
                    "the merged files",
                    &format!("the merge result because your files changed meanwhile ({err})"),
                ));
            }
            MaybeApplyPatchVerified::ShellParseError(_)
            | MaybeApplyPatchVerified::NotApplyPatch => {
                return Err(refuse("the merged files", "this change"));
            }
        };
    for change in &merge.changes {
        let path = cwd
            .join(&change.path)
            .map_err(|_| refuse(&change.path, "this path"))?;
        let exact = match (action.changes().get(&path), change.new.as_deref()) {
            (Some(ApplyPatchFileChange::Add { content }), Some(new)) => content == new,
            (Some(ApplyPatchFileChange::Update { new_content, .. }), Some(new)) => {
                new_content == new
            }
            (Some(ApplyPatchFileChange::Delete { .. }), None) => true,
            _ => false,
        };
        if !exact {
            return Err(refuse(
                &change.path,
                "the exact content (line endings or final newline)",
            ));
        }
    }
    Ok(action)
}

/// One `apply_patch` operation per file; an update replaces every line, which matches only
/// the current content and leaves the diff for the approval to `apply_patch` itself.
fn patch_text(changes: &[MergedChange]) -> String {
    let mut patch = String::from("*** Begin Patch\n");
    for change in changes {
        match (&change.old, &change.new) {
            (None, Some(new)) => {
                patch.push_str(&format!("*** Add File: {}\n", change.path));
                push_lines(&mut patch, '+', new);
            }
            (Some(_), None) => patch.push_str(&format!("*** Delete File: {}\n", change.path)),
            (Some(old), Some(new)) => {
                patch.push_str(&format!("*** Update File: {}\n@@\n", change.path));
                push_lines(&mut patch, '-', old);
                push_lines(&mut patch, '+', new);
            }
            (None, None) => {}
        }
    }
    patch.push_str("*** End Patch");
    patch
}

fn push_lines(patch: &mut String, marker: char, content: &str) {
    for line in content.lines() {
        patch.push(marker);
        patch.push_str(line);
        patch.push('\n');
    }
}

fn lies_in(path: &std::path::Path, root: &std::path::Path) -> bool {
    let canonical =
        |path: &std::path::Path| dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(path).starts_with(canonical(root))
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

#[cfg(test)]
#[path = "apply_agent_worktree_fork_tests.rs"]
mod tests;
