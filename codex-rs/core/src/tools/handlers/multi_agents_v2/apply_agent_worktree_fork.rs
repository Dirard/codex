//! Fork: `apply_agent_worktree` applies an isolated agent's branch to the caller's checkout.
//!
//! The merge runs host-side on objects only; the result is written through the `apply_patch`
//! path, so approval, sandbox, patch events and the file-edit hooks match any other file
//! edit. The write is all or nothing: every file is checked before the write and again after
//! the approval, and a write that fails halfway is rolled back.
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, UC11.

use super::*;
use crate::agent::worktree_isolation::AgentWorktreeBinding;
use crate::agent::worktree_isolation::CloseOutcome;
use crate::agent::worktree_isolation::apply_refusal;
use crate::agent::worktree_isolation::checkpoint_for_close;
use crate::agent::worktree_isolation::close_worktree;
use crate::agent::worktree_isolation::merge_for_parent;
use crate::hook_runtime::PreToolUseHookResult;
use crate::hook_runtime::record_additional_contexts;
use crate::hook_runtime::run_post_tool_use_hooks;
use crate::hook_runtime::run_pre_tool_use_hooks;
use crate::tools::context::FunctionToolOutput;
use crate::tools::hook_names::HookToolName;
use crate::tools::sandboxing::ToolCtx;
use crate::tools::sandboxing::ToolError;
use codex_apply_patch::ApplyPatchAction;
use codex_apply_patch::ApplyPatchFileChange;
use codex_apply_patch::MaybeApplyPatchVerified;
use codex_protocol::ThreadId;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use codex_utils_path_uri::PathUri;
use codex_worktree::CleanMerge;
use codex_worktree::MergeOutcome;
use codex_worktree::MergedChange;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex as StdMutex;
use std::sync::PoisonError;

pub(crate) const TOOL_NAME: &str = "apply_agent_worktree";

const DESCRIPTION: &str = "Apply an isolated agent's recorded changes (its codex/agents/* branch from the <agent_worktree> block) to your checkout as uncommitted edits, with a three-way merge over your current files. The write is approved, sandboxed and hooked like apply_patch, and it is all or nothing: if the merge conflicts with your edits or a file changes before the write, nothing is written and the result lists the files and the git command from the block. With `close`, once the agent is done and its changes are applied, its worktree checkout is removed to free the disk; its branch stays, and a later task for the agent restores the worktree.";

pub(crate) struct Handler;

impl ToolExecutor<ToolInvocation> for Handler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        let properties = BTreeMap::from([
            (
                "target".to_string(),
                JsonSchema::string(Some(
                    "Agent id or canonical task name of an agent spawned with isolation `worktree`."
                        .to_string(),
                )),
            ),
            (
                "close".to_string(),
                JsonSchema::boolean(Some(
                    "After applying, unload the finished agent and remove its worktree checkout; its branch keeps the changes. Refused while the agent is still running or when the apply fails."
                        .to_string(),
                )),
            ),
            (
                "discard_changes".to_string(),
                JsonSchema::boolean(Some(
                    "With `close`: do not apply, and close even though your checkout lacks the agent's changes (they stay only on its branch)."
                        .to_string(),
                )),
            ),
        ]);
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
    /// UC12: remove the checkout of the finished agent after the apply.
    #[serde(default)]
    close: bool,
    /// UC12: with `close`, skip the apply; confirms that unapplied changes stay on the branch.
    #[serde(default)]
    discard_changes: bool,
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
    if args.discard_changes && !args.close {
        return Err(FunctionCallError::RespondToModel(
            "discard_changes only closes a worktree without applying it; set close as well. Nothing was changed."
                .to_string(),
        ));
    }
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
            // R2.5: the same reason as a refused spawn, a remote executor named as such.
            FunctionCallError::RespondToModel(format!(
                "cannot apply agent {} here: {}",
                args.target,
                crate::agent::worktree_isolation::isolation_environment_reason(
                    &step_context.environments
                )
            ))
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

    // UC12: the agent rests before anything is applied, and its pending work is recorded.
    if args.close {
        let not_closed = |reason: String| {
            FunctionCallError::RespondToModel(format!(
                "cannot close the worktree of agent {}: {reason}. Nothing was applied or removed.",
                args.target
            ))
        };
        session
            .services
            .local_agent_runtime
            .fork_unload_for_close(agent_id)
            .await
            .map_err(not_closed)?;
        checkpoint_for_close(&binding).await.map_err(not_closed)?;
    }

    let mut text = if args.discard_changes {
        String::new()
    } else {
        let call = ApplyCall {
            session: Arc::clone(&session),
            turn: Arc::clone(&turn),
            step_context: Arc::clone(&step_context),
            cancellation_token,
            tracker,
            call_id,
            tool_name,
            binding: Arc::clone(&binding),
            environment,
            target: args.target.clone(),
            closing: args.close,
        };
        match apply_branch(call).await {
            Ok(text) => text,
            // R12.4: a refused apply keeps the worktree.
            Err(FunctionCallError::RespondToModel(message)) if args.close => {
                return Err(FunctionCallError::RespondToModel(format!(
                    "{message}\nThe worktree {} was kept; close it after you take the changes, or with discard_changes if you do not want them.",
                    binding.root.display()
                )));
            }
            Err(err) => return Err(err),
        }
    };
    if args.close {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(
            &close_message(
                &session.services.local_agent_runtime.worktrees,
                &binding,
                &args,
            )
            .await,
        );
    }
    Ok(FunctionToolOutput::from_text(text, Some(true)))
}

/// Everything the apply of one call needs.
struct ApplyCall {
    session: Arc<crate::session::session::Session>,
    turn: Arc<crate::session::turn_context::TurnContext>,
    step_context: Arc<crate::session::step_context::StepContext>,
    cancellation_token: tokio_util::sync::CancellationToken,
    tracker: crate::tools::context::SharedTurnDiffTracker,
    call_id: String,
    tool_name: ToolName,
    binding: Arc<AgentWorktreeBinding>,
    environment: crate::session::turn_context::TurnEnvironment,
    target: String,
    /// The worktree closes after the apply, so an agent without changes is not an error.
    closing: bool,
}

/// UC11: merges the agent's branch and writes the result through `apply_patch`; the answer
/// text on success.
async fn apply_branch(call: ApplyCall) -> Result<String, FunctionCallError> {
    let ApplyCall {
        session,
        turn,
        step_context,
        cancellation_token,
        tracker,
        call_id,
        tool_name,
        binding,
        environment,
        target,
        closing,
    } = call;
    let refuse = |reason: &str, paths: &[String]| {
        FunctionCallError::RespondToModel(apply_refusal(&binding, &target, reason, paths))
    };

    let merge = match merge_for_parent(&binding).await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("cannot apply agent {target}: {err}"))
    })? {
        MergeOutcome::Clean(merge) => merge,
        MergeOutcome::NoBranch { .. } if closing => {
            return Ok(format!(
                "Agent {target} recorded no changes; there is nothing to apply."
            ));
        }
        MergeOutcome::NoBranch {
            pending_worktree_changes,
        } => {
            let pending = if pending_worktree_changes {
                "; its worktree has changes that Codex records when its turn ends"
            } else {
                ""
            };
            return Err(FunctionCallError::RespondToModel(format!(
                "agent {target} has no recorded changes (branch: none){pending}"
            )));
        }
        MergeOutcome::AlreadyApplied { tip } => {
            binding.record_applied(&tip);
            let metadata = binding.metadata();
            return Ok(format!(
                "Your checkout already contains the changes of {} at {}; nothing was written.",
                metadata.branch,
                short(&tip)
            ));
        }
        MergeOutcome::Conflicts(paths) => {
            return Err(refuse("the merge conflicts with your checkout in", &paths));
        }
        MergeOutcome::Unrepresentable { path, reason } => {
            return Err(refuse(
                &format!("apply_patch cannot write {reason} in"),
                std::slice::from_ref(&path),
            ));
        }
        MergeOutcome::Submodule { path, commit } => {
            return Err(FunctionCallError::RespondToModel(submodule_refusal(
                &target,
                &path,
                commit.as_deref(),
            )));
        }
        MergeOutcome::BaseNotAncestor { tip } => {
            return Err(refuse(
                &format!(
                    "the recorded base is not an ancestor of the tip {} of",
                    short(&tip)
                ),
                &[],
            ));
        }
        MergeOutcome::Unsupported(detail) => {
            return Err(refuse(
                &format!(
                    "Git cannot merge without a working tree (Git 2.40 or later is required: {detail}) for"
                ),
                &[],
            ));
        }
    };

    // P2-4: the write is a file edit, so the hooks of `apply_patch` (`Write`, `Edit`) see it
    // with the same input, and a PreToolUse hook can deny or rewrite it.
    let environment_id = environment.selection.environment_id.clone();
    let hook_environments = [environment_id.as_str()];
    let merged_patch = patch_text(&merge.changes);
    let patch = match run_pre_tool_use_hooks(
        &session,
        step_context.as_ref(),
        call_id.clone(),
        &HookToolName::apply_patch(),
        &serde_json::json!({ "command": merged_patch }),
        &hook_environments,
    )
    .await
    {
        // The hook's policy covers this write, so no way around it is offered.
        PreToolUseHookResult::Blocked(message) => {
            return Err(FunctionCallError::RespondToModel(format!(
                "cannot apply agent {target}: a PreToolUse hook denied the write. Nothing was written.\n{message}"
            )));
        }
        PreToolUseHookResult::Continue {
            updated_input: Some(updated),
        } => crate::tools::handlers::updated_hook_command(&updated)?.to_string(),
        PreToolUseHookResult::Continue {
            updated_input: None,
        } => merged_patch.clone(),
    };
    let rewritten = patch != merged_patch;
    let merged = (!rewritten).then_some(&merge);
    let action = verified_action(&binding, &environment, &target, &patch, merged).await?;
    let files = Arc::new(
        expected_files(&action, binding.source_root.as_path())
            .map_err(|(path, reason)| refuse(&reason, std::slice::from_ref(&path)))?,
    );

    let tool_ctx = ToolCtx {
        session: Arc::clone(&session),
        step_context: Arc::clone(&step_context),
        cancellation_token,
        call_id: call_id.clone(),
        tool_name,
    };
    let pending = PendingWrite::register(session.thread_id, &call_id, Arc::clone(&files));
    let written = crate::tools::handlers::apply_patch::execute_verified_patch(
        action,
        environment,
        Some(&tracker),
        tool_ctx,
    )
    .await;
    drop(pending);
    let content = match written {
        Ok(content) => content,
        Err(err) => {
            let blocking_files = Arc::clone(&files);
            let rollback = tokio::task::spawn_blocking(move || roll_back(&blocking_files))
                .await
                .unwrap_or_else(|err| RollBack {
                    restored: Vec::new(),
                    left: vec![(PathBuf::new(), format!("rollback task failed: {err}"))],
                });
            if !rollback.restored.is_empty() {
                // The turn diff saw the rolled-back edits; it no longer matches the files.
                tracker.lock().await.invalidate();
            }
            return Err(FunctionCallError::RespondToModel(failed_write_message(
                &binding, &target, &err, &rollback,
            )));
        }
    };

    let post = run_post_tool_use_hooks(
        &session,
        step_context.as_ref(),
        call_id,
        HookToolName::apply_patch().name().to_string(),
        HookToolName::apply_patch().matcher_aliases().to_vec(),
        serde_json::json!({ "command": patch }),
        serde_json::Value::String(content.clone()),
        &hook_environments,
    )
    .await;
    record_additional_contexts(&session, &turn, post.additional_contexts).await;
    if post.should_block {
        // Like any PostToolUse block, this rejects the result, not the completed write.
        return Err(FunctionCallError::RespondToModel(
            post.feedback_message
                .unwrap_or_else(|| "PostToolUse hook blocked the tool result".to_string()),
        ));
    }

    if !rewritten {
        // R12.7: the parent now holds this tip; the branch may go with the worktree.
        binding.record_applied(&merge.tip);
    }
    let metadata = binding.metadata();
    let mut text = format!(
        "Applied {} at {} ({}) to {} as uncommitted edits.\n{content}",
        metadata.branch,
        short(&merge.tip),
        merge.shortstat,
        binding.source_root.display()
    );
    if rewritten {
        text.push_str("\nA PreToolUse hook rewrote the patch; the files hold the hook's version.");
    }
    if merge.pending_worktree_changes {
        text.push_str(
            "\nThe agent's worktree has newer changes that Codex records when its turn ends; they were not applied.",
        );
    }
    if let Some(feedback) = post.feedback_message {
        text.push('\n');
        text.push_str(&feedback);
    }
    Ok(text)
}

/// R11.12: the parent updates a submodule itself; the block's `git apply` command would skip it.
fn submodule_refusal(target: &str, path: &str, commit: Option<&str>) -> String {
    let quoted = shell_quote(path);
    let (change, command) = match commit {
        Some(commit) => (
            format!("moves submodule {path} to {commit}"),
            format!("git -C {quoted} checkout {commit}"),
        ),
        None => (
            format!("removes submodule {path}"),
            format!("git rm --cached {quoted}"),
        ),
    };
    format!(
        "cannot apply agent {target}: its branch {change}, and apply_agent_worktree does not change submodules. Nothing was written. Update it yourself, then call apply_agent_worktree again for the other files.\nsubmodule: {command}"
    )
}

/// A POSIX shell word for `value`: as is when it is plain, otherwise single-quoted.
fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-./".contains(character))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

/// R12.5, R12.6: closes the worktree and says what happened.
async fn close_message(
    worktrees: &crate::agent::worktree_isolation::AgentWorktrees,
    binding: &Arc<AgentWorktreeBinding>,
    args: &ApplyAgentWorktreeArgs,
) -> String {
    let target = &args.target;
    let root = binding.root.display();
    match close_worktree(worktrees, binding).await {
        Ok(CloseOutcome::Removed {
            applied: Some(branch),
            ..
        }) => format!(
            "Closed the worktree of agent {target}: removed {root}. Your checkout holds the changes of branch {branch}, so the branch was deleted; a later task for the agent restores the worktree from its last commit."
        ),
        Ok(CloseOutcome::Removed {
            branch: Some(branch),
            ..
        }) => {
            let unapplied = if args.discard_changes {
                " without applying its changes"
            } else {
                ""
            };
            format!(
                "Closed the worktree of agent {target}{unapplied}: removed {root}. Branch {branch} keeps its changes; a later task for the agent restores the worktree."
            )
        }
        Ok(CloseOutcome::Removed { branch: None, .. }) => format!(
            "Closed the worktree of agent {target}: removed {root}. It recorded no changes; a later task for the agent restores the worktree."
        ),
        Ok(CloseOutcome::Kept(reason)) => format!(
            "The worktree {root} of agent {target} was kept: {}.",
            kept_reason(reason)
        ),
        Err(reason) => format!("The worktree {root} of agent {target} was kept: {reason}."),
    }
}

fn kept_reason(reason: codex_worktree::KeptReason) -> &'static str {
    match reason {
        codex_worktree::KeptReason::InUse => "another Codex process works in it",
        codex_worktree::KeptReason::Changes => "it has changes that its branch does not record",
        codex_worktree::KeptReason::Commits => "its HEAD has commits beyond its branch",
        codex_worktree::KeptReason::Branch => "it has its own branch",
    }
}

/// Builds the action of `patch` and, for the merge's own patch, checks that `apply_patch`
/// reproduces every merged file exactly (R11.6).
async fn verified_action(
    binding: &Arc<AgentWorktreeBinding>,
    environment: &crate::session::turn_context::TurnEnvironment,
    target: &str,
    patch: &str,
    merge: Option<&CleanMerge>,
) -> Result<ApplyPatchAction, FunctionCallError> {
    let refuse = |path: &str, reason: &str| {
        FunctionCallError::RespondToModel(apply_refusal(
            binding,
            target,
            &format!("apply_patch cannot write {reason} in"),
            &[path.to_string()],
        ))
    };
    let args = codex_apply_patch::parse_patch(patch)
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
    let Some(merge) = merge else {
        return Ok(action);
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
        // The current file must be exactly what the merge read: `apply_patch` would also
        // match an edit that changed only whitespace and overwrite it.
        if let Some(old) = change.old.as_deref()
            && std::fs::read(
                path.to_abs_path()
                    .map_err(|_| refuse(&change.path, "this path"))?,
            )
            .ok()
            .as_deref()
                != Some(old.as_bytes())
        {
            return Err(FunctionCallError::RespondToModel(apply_refusal(
                binding,
                target,
                "your file changed after the merge:",
                std::slice::from_ref(&change.path),
            )));
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

// ---------------------------------------------------------------------------------------------
// All or nothing (P2-3)
// ---------------------------------------------------------------------------------------------

/// What is on disk at a path.
#[derive(Clone, Debug, PartialEq, Eq)]
enum FileState {
    Absent,
    File(Vec<u8>),
    /// A directory, a symlink or something unreadable: never written through.
    Other,
}

fn file_state(path: &Path) -> FileState {
    match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => FileState::Absent,
        Ok(metadata) if metadata.file_type().is_file() => match std::fs::read(path) {
            Ok(content) => FileState::File(content),
            Err(_) => FileState::Other,
        },
        _ => FileState::Other,
    }
}

/// One file of the write: what it is before and after it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExpectedFile {
    path: PathBuf,
    before: FileState,
    after: FileState,
    /// Directories the write creates for an added file, outermost first.
    created_dirs: Vec<PathBuf>,
}

/// Reads the state before the write of every file `action` touches and checks it: an added
/// file and the directories on its way must be free, a changed or deleted file must be a
/// regular file. Errors name the path relative to `source_root` and the reason.
fn expected_files(
    action: &ApplyPatchAction,
    source_root: &Path,
) -> Result<Vec<ExpectedFile>, (String, String)> {
    let relative = |path: &Path| {
        path.strip_prefix(source_root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    };
    let mut files = Vec::with_capacity(action.changes().len());
    for (uri, change) in action.changes() {
        let path = uri
            .to_abs_path()
            .map_err(|_| {
                (
                    uri.to_string(),
                    "a path outside the checkout in".to_string(),
                )
            })?
            .to_path_buf();
        let before = file_state(&path);
        let (after, created_dirs) = match change {
            ApplyPatchFileChange::Add { content } => {
                if before != FileState::Absent {
                    return Err((relative(&path), "a file already exists at".to_string()));
                }
                let mut created_dirs = Vec::new();
                for ancestor in path.ancestors().skip(1) {
                    if ancestor == source_root || !ancestor.starts_with(source_root) {
                        break;
                    }
                    match std::fs::symlink_metadata(ancestor) {
                        Ok(metadata) if metadata.file_type().is_dir() => break,
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            created_dirs.push(ancestor.to_path_buf());
                        }
                        _ => {
                            return Err((
                                relative(&path),
                                "a file or symlink of your checkout blocks the directory of"
                                    .to_string(),
                            ));
                        }
                    }
                }
                created_dirs.reverse();
                (FileState::File(content.clone().into_bytes()), created_dirs)
            }
            ApplyPatchFileChange::Delete { .. } => (FileState::Absent, Vec::new()),
            ApplyPatchFileChange::Update {
                move_path: Some(_), ..
            } => {
                return Err((
                    relative(&path),
                    "a PreToolUse hook rewrote the patch into a move, which apply_agent_worktree does not write, in"
                        .to_string(),
                ));
            }
            ApplyPatchFileChange::Update { new_content, .. } => (
                FileState::File(new_content.clone().into_bytes()),
                Vec::new(),
            ),
        };
        if !matches!(change, ApplyPatchFileChange::Add { .. })
            && !matches!(before, FileState::File(_))
        {
            return Err((relative(&path), "not a regular file in".to_string()));
        }
        files.push(ExpectedFile {
            path,
            before,
            after,
            created_dirs,
        });
    }
    Ok(files)
}

/// Writes in flight, checked again by the `apply_patch` runtime after the approval.
static PENDING_WRITES: LazyLock<StdMutex<HashMap<(ThreadId, String), PendingWrite>>> =
    LazyLock::new(Default::default);

struct PendingWrite {
    files: Arc<Vec<ExpectedFile>>,
    attempts: u32,
}

/// Unregisters a write when the call ends.
struct PendingWriteGuard((ThreadId, String));

impl PendingWrite {
    fn register(
        thread_id: ThreadId,
        call_id: &str,
        files: Arc<Vec<ExpectedFile>>,
    ) -> PendingWriteGuard {
        let key = (thread_id, call_id.to_string());
        PENDING_WRITES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), PendingWrite { files, attempts: 0 });
        PendingWriteGuard(key)
    }
}

impl Drop for PendingWriteGuard {
    fn drop(&mut self) {
        PENDING_WRITES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.0);
    }
}

/// Fork hook of `ApplyPatchRuntime::run`, after the approval and right before the write: the
/// files of an `apply_agent_worktree` write must still be what the merge read, so an edit made
/// while the approval was pending is never overwritten (P2-3). A retry after a sandbox
/// denial also accepts files the first attempt already wrote. Other patches pass.
pub(crate) fn fork_check_before_write(ctx: &ToolCtx) -> Result<(), ToolError> {
    let key = (ctx.session.thread_id, ctx.call_id.clone());
    let (files, first_attempt) = {
        let mut pending = PENDING_WRITES
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(write) = pending.get_mut(&key) else {
            return Ok(());
        };
        write.attempts += 1;
        (Arc::clone(&write.files), write.attempts == 1)
    };
    for file in files.iter() {
        let now = file_state(&file.path);
        if now != file.before && (first_attempt || now != file.after) {
            return Err(ToolError::Rejected(format!(
                "{} changed while the write waited, so nothing was written",
                file.path.display()
            )));
        }
    }
    Ok(())
}

/// What a rollback did.
struct RollBack {
    /// Files written before the failure that hold their earlier content again.
    restored: Vec<PathBuf>,
    /// Files that are neither as before nor as written, with the reason; left alone.
    left: Vec<(PathBuf, String)>,
}

/// Puts back every file the failed write changed. A file is restored only while it holds
/// exactly what the write put there, so nobody else's edit is undone; directories the write
/// created are removed when empty.
fn roll_back(files: &[ExpectedFile]) -> RollBack {
    let mut rollback = RollBack {
        restored: Vec::new(),
        left: Vec::new(),
    };
    for file in files.iter().rev() {
        let now = file_state(&file.path);
        if now == file.before {
            continue;
        }
        if now != file.after {
            rollback.left.push((
                file.path.clone(),
                "it changed outside the write".to_string(),
            ));
            continue;
        }
        let restored = match &file.before {
            FileState::Absent => std::fs::remove_file(&file.path),
            FileState::File(content) => std::fs::write(&file.path, content),
            FileState::Other => continue,
        };
        match restored {
            Ok(()) => {
                for directory in file.created_dirs.iter().rev() {
                    let _ = std::fs::remove_dir(directory);
                }
                rollback.restored.push(file.path.clone());
            }
            Err(err) => rollback.left.push((file.path.clone(), err.to_string())),
        }
    }
    rollback
}

/// The answer to a write that failed, after its rollback.
fn failed_write_message(
    binding: &AgentWorktreeBinding,
    target: &str,
    err: &FunctionCallError,
    rollback: &RollBack,
) -> String {
    let (FunctionCallError::RespondToModel(error) | FunctionCallError::Fatal(error)) = err;
    let error = error.trim();
    let display = |path: &Path| {
        path.strip_prefix(binding.source_root.as_path())
            .unwrap_or(path)
            .display()
            .to_string()
    };
    let mut message = if rollback.left.is_empty() {
        let restored = if rollback.restored.is_empty() {
            String::new()
        } else {
            format!(
                " The files written before the failure were restored: {}.",
                rollback
                    .restored
                    .iter()
                    .map(|path| display(path))
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        format!(
            "cannot apply agent {target}: the write did not complete. Nothing was written.{restored}\n{error}"
        )
    } else {
        format!(
            "cannot apply agent {target}: the write did not complete, and these files could not be restored: {}. Check them before you retry.\n{error}",
            rollback
                .left
                .iter()
                .map(|(path, reason)| format!("{} ({reason})", display(path)))
                .collect::<Vec<_>>()
                .join("; ")
        )
    };
    let metadata = binding.metadata();
    message.push_str(&format!(
        "\napply: {}",
        crate::agent::worktree_isolation::apply_command(&metadata.base_sha, &metadata.branch)
    ));
    message
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
