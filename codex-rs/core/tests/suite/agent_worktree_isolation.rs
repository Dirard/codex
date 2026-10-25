//! Fork: MultiAgentV2 children isolated in managed git worktrees (`isolation: "worktree"`).
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`. Each test drives a real root and child
//! through the mock Responses server; children run real commands in their worktrees.

use anyhow::Context;
use anyhow::Result;
use codex_core::ThreadEvictionOutcome;
use codex_core::config::AgentRoleConfig;
use codex_core::config::Config;
use codex_features::AgentWorktreeBase;
use codex_features::Feature;
use codex_protocol::ThreadId;
use codex_protocol::models::PermissionProfile;
use codex_protocol::models::PermissionProfileSnapshot;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_protocol::protocol::EnvironmentConfigState;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::protocol::TurnEnvironmentRequests;
use codex_thread_store::ReadThreadParams;
use core_test_support::responses::ResponseMock;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_exec_command_call_with_args;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::sse;
use core_test_support::responses::sse_failed;
use core_test_support::responses::start_mock_server;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::timeout;
use wiremock::MockServer;

const NAMESPACE: &str = "collaboration";
/// Every root request carries this first prompt in its history; child requests never do.
const ROOT_PROMPT: &str = "root: coordinate isolated workers";
const PARENT_FILE: &str = "base\nparent edit\n";
const UNTRACKED_FILE: &str = "untracked parent file\n";

// ---------------------------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------------------------

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output is UTF-8")
        .trim()
        .to_owned()
}

fn git_succeeds(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git")
        .status
        .success()
}

/// A committed `a.txt`, then an uncommitted edit and an untracked file, like a busy parent.
fn init_repository(cwd: &Path) -> Result<()> {
    git(cwd, &["init", "--quiet", "-b", "main"]);
    git(cwd, &["config", "user.name", "Codex Test"]);
    git(cwd, &["config", "user.email", "codex@example.invalid"]);
    std::fs::write(cwd.join(".gitignore"), "target/\n")?;
    std::fs::write(cwd.join("a.txt"), "base\n")?;
    git(cwd, &["add", "."]);
    git(
        cwd,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "initial"],
    );
    std::fs::write(cwd.join("a.txt"), PARENT_FILE)?;
    std::fs::write(cwd.join("untracked.txt"), UNTRACKED_FILE)?;
    Ok(())
}

fn enable_isolation(config: &mut Config) {
    for feature in [
        Feature::Collab,
        Feature::MultiAgentV2,
        Feature::AgentWorktreeIsolation,
    ] {
        config
            .features
            .enable(feature)
            .expect("test config should allow feature update");
    }
    config.multi_agent_v2.max_concurrent_threads_per_session = 4;
}

async fn isolation_codex(
    server: &MockServer,
    git_repository: bool,
    configure: impl FnOnce(&mut Config) + Send + 'static,
) -> Result<TestCodex> {
    isolation_codex_with_model(server, /*model*/ None, git_repository, configure).await
}

/// Children get collaboration tools only with a model that supports MultiAgentV2 children.
const NESTING_MODEL: &str = "gpt-5.6-sol";

async fn isolation_codex_with_model(
    server: &MockServer,
    model: Option<&str>,
    git_repository: bool,
    configure: impl FnOnce(&mut Config) + Send + 'static,
) -> Result<TestCodex> {
    isolation_builder(model, git_repository, configure)
        .build_with_auto_env(server)
        .await
}

fn isolation_builder(
    model: Option<&str>,
    git_repository: bool,
    configure: impl FnOnce(&mut Config) + Send + 'static,
) -> TestCodexBuilder {
    let mut builder = test_codex();
    if let Some(model) = model {
        builder = builder.with_model(model);
    }
    let mut builder = builder.with_config(move |config| {
        enable_isolation(config);
        // The checkout is the workspace, as in a normal session.
        config.workspace_roots = vec![config.cwd.clone()];
        configure(config);
    });
    if git_repository {
        builder =
            builder.with_workspace_setup(|cwd, _fs| async move { init_repository(cwd.as_path()) });
    }
    builder
}

/// Workspace-write without `/tmp` and `$TMPDIR`, so only workspace roots are writable.
fn workspace_only_profile() -> PermissionProfile {
    PermissionProfile::workspace_write_with(
        &[],
        NetworkSandboxPolicy::Restricted,
        /*exclude_tmpdir_env_var*/ true,
        /*exclude_slash_tmp*/ true,
    )
}

fn body(request: &wiremock::Request) -> Value {
    let zstd = request
        .headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("zstd"));
    let bytes = if zstd {
        zstd::stream::decode_all(std::io::Cursor::new(&request.body)).unwrap_or_default()
    } else {
        request.body.clone()
    };
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn body_text(request: &wiremock::Request) -> String {
    body(request).to_string()
}

fn has_output(request: &wiremock::Request, call_id: &str) -> bool {
    body(request)["input"].as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item["type"] == "function_call_output" && item["call_id"].as_str() == Some(call_id)
        })
    })
}

fn is_root_request(request: &wiremock::Request) -> bool {
    body_text(request).contains(ROOT_PROMPT)
}

fn final_answer(id: &str, text: &str) -> Value {
    let mut answer = ev_assistant_message(&format!("msg-{id}"), text);
    answer["item"]["phase"] = json!("final_answer");
    answer
}

/// Mounts one root collaboration call answered by a final message once its output arrives.
async fn mount_root_call(
    server: &MockServer,
    prompt: &'static str,
    call_id: &'static str,
    tool: &'static str,
    arguments: Value,
) -> ResponseMock {
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| {
            is_root_request(request)
                && body_text(request).contains(prompt)
                && !has_output(request, call_id)
        },
        sse(vec![
            ev_response_created(&format!("resp-{call_id}")),
            ev_function_call_with_namespace(call_id, NAMESPACE, tool, &arguments.to_string()),
            ev_completed(&format!("resp-{call_id}")),
        ]),
    )
    .await;
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| is_root_request(request) && has_output(request, call_id),
        sse(vec![
            ev_response_created(&format!("resp-{call_id}-done")),
            final_answer(call_id, "root step completed"),
            ev_completed(&format!("resp-{call_id}-done")),
        ]),
    )
    .await
}

/// Mounts a root turn that reads queued child mail; returns the request that carries it.
///
/// Fresh turn input is sampled before queued mail is drained, so an empty first response
/// lets the turn continue with the mail.
async fn mount_root_answer(server: &MockServer, prompt: &'static str) -> ResponseMock {
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| {
            is_root_request(request)
                && body_text(request).contains(prompt)
                && !has_child_mail_after(request, prompt)
        },
        sse(vec![ev_completed(&format!("resp-{prompt}-before-mail"))]),
    )
    .await;
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| {
            is_root_request(request)
                && body_text(request).contains(prompt)
                && has_child_mail_after(request, prompt)
        },
        sse(vec![
            ev_response_created(&format!("resp-{prompt}")),
            final_answer(prompt, "root read the results"),
            ev_completed(&format!("resp-{prompt}")),
        ]),
    )
    .await
}

/// Whether child mail follows the user message `prompt` in the request input.
fn has_child_mail_after(request: &wiremock::Request, prompt: &str) -> bool {
    let body = body(request);
    let Some(items) = body["input"].as_array() else {
        return false;
    };
    let Some(prompt_index) = items
        .iter()
        .rposition(|item| item["type"] == "message" && item.to_string().contains(prompt))
    else {
        return false;
    };
    items[prompt_index..].iter().any(|item| {
        item["type"] == "agent_message"
            && item["author"]
                .as_str()
                .is_some_and(|author| author.starts_with("/root/"))
    })
}

/// A root command step: runs `command` through `exec_command` in the root's checkout.
async fn mount_root_command(
    server: &MockServer,
    prompt: &'static str,
    call_id: &'static str,
    command: String,
) -> ResponseMock {
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| {
            is_root_request(request)
                && body_text(request).contains(prompt)
                && !has_output(request, call_id)
        },
        sse(vec![
            ev_response_created(&format!("resp-{call_id}")),
            ev_exec_command_call_with_args(call_id, &shell_args(&command)),
            ev_completed(&format!("resp-{call_id}")),
        ]),
    )
    .await;
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| is_root_request(request) && has_output(request, call_id),
        sse(vec![
            ev_response_created(&format!("resp-{call_id}-done")),
            final_answer(call_id, "root command completed"),
            ev_completed(&format!("resp-{call_id}-done")),
        ]),
    )
    .await
}

fn shell_args(command: &str) -> Value {
    json!({ "cmd": command, "shell": "bash", "login": false })
}

/// Scripted child: each step runs a shell command, then the child answers `done`.
///
/// A child request is recognized by its task text and the absence of the root prompt; the
/// outputs it already carries select the step it is waiting for.
async fn mount_child(
    server: &MockServer,
    task: &'static str,
    steps: &[(&'static str, String)],
) -> Vec<ResponseMock> {
    let mut mocks = Vec::new();
    for index in 0..=steps.len() {
        let previous = index.checked_sub(1).map(|previous| steps[previous].0);
        let current = steps.get(index).map(|(call_id, _)| *call_id);
        let matcher = move |request: &wiremock::Request| {
            let text = body_text(request);
            text.contains(task)
                && !text.contains(ROOT_PROMPT)
                && previous.is_none_or(|call_id| has_output(request, call_id))
                && current.is_none_or(|call_id| !has_output(request, call_id))
        };
        let response_id = format!("resp-{task}-{index}");
        let events = match steps.get(index) {
            Some((call_id, command)) => vec![
                ev_response_created(&response_id),
                ev_exec_command_call_with_args(call_id, &shell_args(command)),
                ev_completed(&response_id),
            ],
            None => vec![
                ev_response_created(&response_id),
                final_answer(&response_id, "worker done"),
                ev_completed(&response_id),
            ],
        };
        mocks.push(mount_sse_once_match(server, matcher, sse(events)).await);
    }
    mocks
}

fn spawn_arguments(task: &str, name: &str, isolation: Option<&str>) -> Value {
    let mut arguments = json!({ "message": task, "task_name": name, "fork_turns": "none" });
    if let Some(isolation) = isolation {
        arguments["isolation"] = json!(isolation);
    }
    arguments
}

fn spawn_output(mock: &ResponseMock, call_id: &str) -> Value {
    let output = mock
        .function_call_output_text(call_id)
        .unwrap_or_else(|| panic!("spawn output for {call_id}"));
    serde_json::from_str(&output).unwrap_or_else(|_| json!({ "error": output }))
}

struct Child {
    id: ThreadId,
    thread: Arc<codex_core::CodexThread>,
}

async fn next_child(
    created: &mut broadcast::Receiver<ThreadId>,
    test: &TestCodex,
) -> Result<Child> {
    let id = timeout(Duration::from_secs(20), created.recv())
        .await
        .context("child thread was not created")??;
    Ok(Child {
        id,
        thread: test.thread_manager.get_thread(id).await?,
    })
}

async fn wait_turn_complete(child: &Child) {
    wait_for_event(&child.thread, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
}

fn output_of(mocks: &[ResponseMock], call_id: &str) -> String {
    mocks
        .iter()
        .find_map(|mock| mock.function_call_output_text(call_id))
        .unwrap_or_else(|| panic!("child output for {call_id}"))
}

fn managed_buckets(test: &TestCodex) -> Vec<PathBuf> {
    match std::fs::read_dir(test.codex_home_path().join("worktrees")) {
        Ok(entries) => entries
            .map(|entry| entry.expect("managed root entry").path())
            .filter(|path| path.is_dir())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn agent_branches(cwd: &Path) -> Vec<String> {
    git(
        cwd,
        &[
            "for-each-ref",
            "--format=%(refname:short)",
            "refs/heads/codex/agents",
        ],
    )
    .lines()
    .map(str::to_owned)
    .collect()
}

fn worktree_field(output: &Value, field: &str) -> String {
    output["worktree"][field]
        .as_str()
        .unwrap_or_else(|| panic!("worktree.{field} in {output}"))
        .to_owned()
}

fn mail_block(request: &ResponsesRequest) -> String {
    // Only child mail carries a block; tool descriptions also mention the tag. The newest
    // mail is the last one because earlier turns' mail stays in the history.
    let text = request
        .inputs_of_type("agent_message")
        .iter()
        .map(Value::to_string)
        .rfind(|item| item.contains("<agent_worktree>\\n"))
        .unwrap_or_else(|| panic!("no worktree block in {}", request.body_json()));
    let start = text.rfind("<agent_worktree>\\n").expect("block start");
    let end = text[start..]
        .find("</agent_worktree>")
        .map(|end| start + end + "</agent_worktree>".len())
        .expect("closed worktree block");
    // The block sits inside a JSON string; restore its line breaks for assertions.
    text[start..end].replace("\\n", "\n")
}

// ---------------------------------------------------------------------------------------------
// UC1: flag, argument and defaults
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worktree_isolation_requires_the_feature_and_a_known_value() -> Result<()> {
    let server = start_mock_server().await;
    let test = test_codex()
        .with_config(|config| {
            config.features.enable(Feature::Collab).expect("collab");
            config.features.enable(Feature::MultiAgentV2).expect("v2");
        })
        .with_workspace_setup(|cwd, _fs| async move { init_repository(cwd.as_path()) })
        .build_with_auto_env(&server)
        .await?;
    let disabled = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-disabled",
        "spawn_agent",
        spawn_arguments("disabled task", "disabled", Some("worktree")),
    )
    .await;
    let unknown = mount_root_call(
        &server,
        "spawn with an unknown isolation",
        "spawn-unknown",
        "spawn_agent",
        spawn_arguments("unknown task", "unknown", Some("elsewhere")),
    )
    .await;

    test.submit_turn(ROOT_PROMPT).await?;
    test.submit_turn("spawn with an unknown isolation").await?;

    let disabled = disabled
        .function_call_output_text("spawn-disabled")
        .context("disabled spawn output")?;
    assert!(
        disabled.contains("worktree isolation is not enabled"),
        "{disabled}"
    );
    let unknown = unknown
        .function_call_output_text("spawn-unknown")
        .context("unknown spawn output")?;
    assert_eq!(unknown, "isolation must be `none` or `worktree`");
    assert_eq!(
        test.thread_manager.list_thread_ids().await,
        vec![test.session_configured.thread_id]
    );
    assert_eq!(managed_buckets(&test), Vec::<PathBuf>::new());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn role_default_isolates_and_an_explicit_none_wins() -> Result<()> {
    const ROLE: &str = "isolated_worker";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |config| {
        let role_path = config.codex_home.join("isolated-worker-role.toml");
        std::fs::write(
            &role_path,
            "[features.agent_worktree_isolation]\ndefault_isolation = \"worktree\"\n",
        )
        .expect("write role config");
        config.agent_roles.insert(
            ROLE.to_string(),
            AgentRoleConfig {
                description: Some("Isolated worker".to_string()),
                config_file: Some(role_path.to_path_buf()),
                nickname_candidates: None,
            },
        );
    })
    .await?;
    let mut role_default = spawn_arguments("role default task", "role_default", None);
    role_default["agent_type"] = json!(ROLE);
    let mut explicit_none = spawn_arguments("explicit none task", "explicit_none", Some("none"));
    explicit_none["agent_type"] = json!(ROLE);
    let first = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-role-default",
        "spawn_agent",
        role_default,
    )
    .await;
    let second = mount_root_call(
        &server,
        "spawn with an explicit none",
        "spawn-explicit-none",
        "spawn_agent",
        explicit_none,
    )
    .await;
    let default_child = mount_child(&server, "role default task", &[]).await;
    let none_child = mount_child(&server, "explicit none task", &[]).await;
    let mut created = test.thread_manager.subscribe_thread_created();

    test.submit_turn(ROOT_PROMPT).await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;
    test.submit_turn("spawn with an explicit none").await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    let isolated = spawn_output(&first, "spawn-role-default");
    assert_eq!(worktree_field(&isolated, "base_kind"), "working_tree");
    let shared = spawn_output(&second, "spawn-explicit-none");
    assert!(shared.get("worktree").is_none(), "{shared}");
    assert_eq!(managed_buckets(&test).len(), 1);
    assert!(
        default_child[0]
            .single_request()
            .body_contains_text("isolated git worktree")
    );
    assert!(
        !none_child[0]
            .single_request()
            .body_contains_text("isolated git worktree")
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UC2–UC4 and UC7: an isolated child edits its worktree and the parent applies the result
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn isolated_child_works_on_a_snapshot_and_the_parent_applies_its_branch() -> Result<()> {
    const TASK: &str = "edit the files in your worktree";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let parent = test.config.cwd.to_path_buf();
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-worker",
        "spawn_agent",
        spawn_arguments(TASK, "worker", Some("worktree")),
    )
    .await;
    let child = mount_child(
        &server,
        TASK,
        &[
            (
                "child-inspect",
                "pwd; cat a.txt; cat untracked.txt".to_string(),
            ),
            (
                "child-edit",
                "printf 'child edit\\n' >> a.txt && printf 'new\\n' > new_file.txt".to_string(),
            ),
        ],
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();

    test.submit_turn(ROOT_PROMPT).await?;
    let worker = next_child(&mut created, &test).await?;
    wait_turn_complete(&worker).await;

    // The spawn result names the worktree; nothing touched the parent's checkout.
    let output = spawn_output(&spawn, "spawn-worker");
    let root = PathBuf::from(worktree_field(&output, "path"));
    let branch = worktree_field(&output, "branch");
    let base = worktree_field(&output, "base");
    assert!(root.starts_with(test.codex_home_path().join("worktrees")));
    assert!(branch.starts_with("codex/agents/worker-"), "{branch}");
    assert_eq!(worktree_field(&output, "base_kind"), "working_tree");
    assert_eq!(
        output["worktree"]["parent_checkout_protected"],
        json!(false)
    );
    assert_eq!(std::fs::read_to_string(parent.join("a.txt"))?, PARENT_FILE);
    assert!(!parent.join("new_file.txt").exists());

    // R2.2, R3.1: the child started in the worktree and saw the parent's uncommitted work.
    let inspect = output_of(&child, "child-inspect");
    assert!(inspect.contains(&root.display().to_string()), "{inspect}");
    assert!(inspect.contains("parent edit"), "{inspect}");
    assert!(inspect.contains(UNTRACKED_FILE.trim()), "{inspect}");
    let first_request = child[0].single_request();
    assert!(first_request.body_contains_text(&format!("<cwd>{}</cwd>", root.display())));
    assert!(first_request.body_contains_text(&format!(
        "You are working in an isolated git worktree at {}.",
        root.display()
    )));
    assert!(first_request.body_contains_text(&branch));
    let snapshot = worker.thread.config_snapshot().await;
    assert_eq!(snapshot.cwd().as_path(), root.as_path());
    let stored = test
        .thread_store
        .read_thread(ReadThreadParams {
            thread_id: worker.id,
            include_archived: true,
            include_history: false,
        })
        .await?;
    assert_eq!(stored.cwd, root);

    // R3.7: the child keeps the parent's approval policy and reviewer.
    let parent_snapshot = test.codex.config_snapshot().await;
    assert_eq!(snapshot.approval_policy, parent_snapshot.approval_policy);
    assert_eq!(
        snapshot.approvals_reviewer,
        parent_snapshot.approvals_reviewer
    );

    // R2.8: the owner record and the agent metadata name the child.
    let git_dir = git(&root, &["rev-parse", "--path-format=absolute", "--git-dir"]);
    let owner: Value = serde_json::from_str(&std::fs::read_to_string(
        Path::new(&git_dir).join("codex-thread.json"),
    )?)?;
    assert_eq!(owner["ownerThreadId"], json!(worker.id.to_string()));
    let config_worktree = Path::new(&git_dir).join("config.worktree");
    let config_worktree = config_worktree.to_string_lossy();
    assert_eq!(
        git(
            &root,
            &["config", "--file", &config_worktree, "codex.agentThread"]
        ),
        worker.id.to_string()
    );
    assert_eq!(
        git(
            &root,
            &["config", "--file", &config_worktree, "codex.agentBranch"]
        ),
        branch
    );
    assert_eq!(
        git(
            &root,
            &["config", "--file", &config_worktree, "codex.agentSource"]
        ),
        parent.display().to_string()
    );

    // R4.1, R4.2, R4.4: the host committed both edits and the parent's mail reports them.
    assert_eq!(agent_branches(&parent), vec![branch.clone()]);
    assert_eq!(
        git(&parent, &["show", &format!("{branch}:new_file.txt")]),
        "new"
    );
    assert_eq!(
        git(&parent, &["show", &format!("{branch}:a.txt")]),
        "base\nparent edit\nchild edit"
    );
    assert_eq!(git(&parent, &["rev-parse", &format!("{branch}~1")]), base);
    let collect = mount_root_answer(&server, "collect the worker results").await;
    test.submit_turn("collect the worker results").await?;
    let block = mail_block(&collect.single_request());
    assert!(block.contains(&format!("branch: {branch}\n")), "{block}");
    assert!(
        block.contains("changes: 2 files changed, 2 insertions(+)\n"),
        "{block}"
    );
    assert!(
        block.contains("files: M a.txt; A new_file.txt\n"),
        "{block}"
    );
    assert!(
        block.contains(&format!("checkout: {}\n", root.display())),
        "{block}"
    );
    let apply = format!("git diff --binary {base} {branch} | git apply");
    assert!(block.contains(&format!("apply: {apply}\n")), "{block}");

    // R7.1, R7.2: the parent applies the command in workspace-write without /tmp.
    let commits_before = git(&parent, &["rev-list", "--count", "--all"]);
    let applied = mount_root_command(&server, "apply the worker branch", "root-apply", apply).await;
    test.submit_turn_with_permission_profile("apply the worker branch", workspace_only_profile())
        .await?;
    let applied = applied
        .function_call_output_text("root-apply")
        .context("apply output")?;
    assert!(applied.contains("exited with code 0"), "{applied}");
    assert_eq!(
        std::fs::read_to_string(parent.join("a.txt"))?,
        "base\nparent edit\nchild edit\n"
    );
    assert_eq!(
        std::fs::read_to_string(parent.join("new_file.txt"))?,
        "new\n"
    );
    assert_eq!(
        git(&parent, &["rev-list", "--count", "--all"]),
        commits_before
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn head_base_and_unchanged_child_report_no_branch() -> Result<()> {
    const TASK: &str = "read the committed state";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |config| {
        config.agent_worktree_isolation.base = AgentWorktreeBase::Head;
        config.multi_agent_v2.hide_spawn_agent_metadata = true;
    })
    .await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-reader",
        "spawn_agent",
        spawn_arguments(TASK, "reader", Some("worktree")),
    )
    .await;
    let child = mount_child(
        &server,
        TASK,
        &[(
            "reader-inspect",
            "cat a.txt; test -e untracked.txt && echo has-untracked || echo no-untracked"
                .to_string(),
        )],
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn(ROOT_PROMPT).await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    // R2.3, R3.6: the HEAD base lacks the parent's edits and hidden metadata keeps `worktree`.
    let output = spawn_output(&spawn, "spawn-reader");
    assert!(output.get("nickname").is_none(), "{output}");
    assert_eq!(worktree_field(&output, "base_kind"), "head");
    assert_eq!(
        worktree_field(&output, "base"),
        git(&test.config.cwd, &["rev-parse", "HEAD"])
    );
    let inspect = output_of(&child, "reader-inspect");
    assert!(inspect.contains("base"), "{inspect}");
    assert!(!inspect.contains("parent edit"), "{inspect}");
    assert!(inspect.contains("no-untracked"), "{inspect}");

    // R4.3, R4.4: nothing changed, so there is no branch and the block says so.
    let collect = mount_root_answer(&server, "collect the reader results").await;
    test.submit_turn("collect the reader results").await?;
    let block = mail_block(&collect.single_request());
    assert!(block.contains("branch: none\n"), "{block}");
    assert!(block.contains("changes: none\n"), "{block}");
    assert!(!block.contains("apply:"), "{block}");
    assert_eq!(agent_branches(&test.config.cwd), Vec::<String>::new());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn isolation_outside_git_is_rejected_without_a_child() -> Result<()> {
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ false, |_| {}).await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-no-git",
        "spawn_agent",
        spawn_arguments("no git task", "no_git", Some("worktree")),
    )
    .await;
    test.submit_turn(ROOT_PROMPT).await?;

    // R2.4: the error names the directory, no child or worktree exists.
    let output = spawn
        .function_call_output_text("spawn-no-git")
        .context("spawn output")?;
    assert!(
        output.contains("worktree isolation is unavailable"),
        "{output}"
    );
    assert!(
        output.contains(&test.config.cwd.display().to_string()),
        "{output}"
    );
    assert_eq!(
        test.thread_manager.list_thread_ids().await,
        vec![test.session_configured.thread_id]
    );
    assert_eq!(managed_buckets(&test), Vec::<PathBuf>::new());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn isolation_requires_a_single_local_environment() -> Result<()> {
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-no-environment",
        "spawn_agent",
        spawn_arguments("no environment task", "no_environment", Some("worktree")),
    )
    .await;
    test.submit_turn_with_environments(ROOT_PROMPT, Some(Vec::new()))
        .await?;

    // R2.5: without exactly one ready local environment there is nothing to isolate.
    let output = spawn
        .function_call_output_text("spawn-no-environment")
        .context("spawn output")?;
    assert!(
        output.contains("requires the child to run in a single local environment"),
        "{output}"
    );
    assert_eq!(managed_buckets(&test), Vec::<PathBuf>::new());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_spawn_rejected_after_worktree_creation_leaves_no_worktree() -> Result<()> {
    const TASK: &str = "first of two in one turn";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |config| {
        config.max_spawned_threads_per_turn = 1;
    })
    .await?;
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            is_root_request(request) && !has_output(request, "spawn-first")
        },
        sse(vec![
            ev_response_created("resp-two-spawns"),
            ev_function_call_with_namespace(
                "spawn-first",
                NAMESPACE,
                "spawn_agent",
                &spawn_arguments(TASK, "first", Some("worktree")).to_string(),
            ),
            ev_function_call_with_namespace(
                "spawn-second",
                NAMESPACE,
                "spawn_agent",
                &spawn_arguments("second over the limit", "second", Some("worktree")).to_string(),
            ),
            ev_completed("resp-two-spawns"),
        ]),
    )
    .await;
    let done = mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            is_root_request(request)
                && has_output(request, "spawn-first")
                && has_output(request, "spawn-second")
        },
        sse(vec![
            ev_response_created("resp-two-spawns-done"),
            final_answer("two-spawns", "one spawn failed"),
            ev_completed("resp-two-spawns-done"),
        ]),
    )
    .await;
    mount_child(&server, TASK, &[]).await;
    mount_child(&server, "second over the limit", &[]).await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn(ROOT_PROMPT).await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    // R2.7: the rejected spawn created a worktree before the limit check and removed it.
    let request = done.single_request();
    let outputs = ["spawn-first", "spawn-second"].map(|call_id| {
        request
            .function_call_output_text(call_id)
            .unwrap_or_else(|| panic!("{call_id} output"))
    });
    assert_eq!(
        outputs
            .iter()
            .filter(|output| output.contains("limit"))
            .count(),
        1,
        "{outputs:?}"
    );
    assert_eq!(managed_buckets(&test).len(), 1);
    assert_eq!(
        git(&test.config.cwd, &["worktree", "list", "--porcelain"])
            .lines()
            .filter(|line| line.starts_with("worktree "))
            .count(),
        2
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn isolated_child_spawns_an_isolated_grandchild_from_its_own_snapshot() -> Result<()> {
    const CHILD_TASK: &str = "prepare and delegate";
    const GRANDCHILD_TASK: &str = "check the child snapshot";
    let server = start_mock_server().await;
    let test = isolation_codex_with_model(
        &server,
        Some(NESTING_MODEL),
        /*git_repository*/ true,
        |config| {
            config.agent_max_depth = 2;
        },
    )
    .await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-child",
        "spawn_agent",
        spawn_arguments(CHILD_TASK, "child", Some("worktree")),
    )
    .await;
    // The child edits its worktree, then spawns an isolated grandchild.
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            let text = body_text(request);
            text.contains(CHILD_TASK)
                && !text.contains(ROOT_PROMPT)
                && !has_output(request, "child-edit")
        },
        sse(vec![
            ev_response_created("resp-child-edit"),
            ev_exec_command_call_with_args(
                "child-edit",
                &shell_args("printf 'from child\\n' > child.txt"),
            ),
            ev_completed("resp-child-edit"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            let text = body_text(request);
            text.contains(CHILD_TASK)
                && !text.contains(ROOT_PROMPT)
                && has_output(request, "child-edit")
                && !has_output(request, "spawn-grandchild")
        },
        sse(vec![
            ev_response_created("resp-child-spawn"),
            ev_function_call_with_namespace(
                "spawn-grandchild",
                NAMESPACE,
                "spawn_agent",
                &spawn_arguments(GRANDCHILD_TASK, "grandchild", Some("worktree")).to_string(),
            ),
            ev_completed("resp-child-spawn"),
        ]),
    )
    .await;
    let child_spawn = mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            let text = body_text(request);
            text.contains(CHILD_TASK)
                && !text.contains(ROOT_PROMPT)
                && has_output(request, "spawn-grandchild")
        },
        sse(vec![
            ev_response_created("resp-child-done"),
            final_answer("child-done", "child delegated"),
            ev_completed("resp-child-done"),
        ]),
    )
    .await;
    let grandchild = mount_child(
        &server,
        GRANDCHILD_TASK,
        &[("grandchild-inspect", "pwd; cat child.txt".to_string())],
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn(ROOT_PROMPT).await?;
    let child = next_child(&mut created, &test).await?;
    wait_turn_complete(&child).await;
    let grandchild_output = spawn_output(&child_spawn, "spawn-grandchild");
    assert!(
        grandchild_output.get("worktree").is_some(),
        "{grandchild_output}"
    );
    let grandchild_thread = next_child(&mut created, &test).await?;
    wait_turn_complete(&grandchild_thread).await;

    let child_root = PathBuf::from(worktree_field(&spawn_output(&spawn, "spawn-child"), "path"));
    let grandchild_root = PathBuf::from(worktree_field(&grandchild_output, "path"));
    assert_ne!(child_root, grandchild_root);
    let inspect = output_of(&grandchild, "grandchild-inspect");
    assert!(
        inspect.contains(&grandchild_root.display().to_string()),
        "{inspect}"
    );
    assert!(inspect.contains("from child"), "{inspect}");
    assert_eq!(
        git(
            &test.config.cwd,
            &[
                "show",
                &format!("{}:child.txt", worktree_field(&grandchild_output, "base"))
            ]
        ),
        "from child"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UC3: sandbox boundaries
// ---------------------------------------------------------------------------------------------

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sandboxed_child_cannot_write_the_parent_checkout() -> Result<()> {
    const TASK: &str = "try to escape the worktree";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let parent = test.config.cwd.to_path_buf();
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-sandboxed",
        "spawn_agent",
        spawn_arguments(TASK, "sandboxed", Some("worktree")),
    )
    .await;
    let child = mount_child(
        &server,
        TASK,
        &[
            (
                "escape",
                format!("printf escaped > '{}'", parent.join("escape.txt").display()),
            ),
            (
                "inside",
                "printf inside > inside.txt && cat inside.txt".to_string(),
            ),
        ],
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn_with_permission_profile(ROOT_PROMPT, workspace_only_profile())
        .await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    // R3.3: the parent's checkout is read-only for the child, its worktree is writable.
    let output = spawn_output(&spawn, "spawn-sandboxed");
    assert_eq!(output["worktree"]["parent_checkout_protected"], json!(true));
    let escape = output_of(&child, "escape");
    assert!(escape.contains("Read-only file system"), "{escape}");
    assert!(!parent.join("escape.txt").exists());
    let inside = output_of(&child, "inside");
    assert!(inside.contains("exited with code 0"), "{inside}");
    assert!(
        PathBuf::from(worktree_field(&output, "path"))
            .join("inside.txt")
            .exists()
    );
    assert!(
        child[0]
            .single_request()
            .body_contains_text("which is read-only for you")
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_checkout_under_tmp_is_reported_as_unprotected() -> Result<()> {
    const TASK: &str = "note the unprotected checkout";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-tmp",
        "spawn_agent",
        spawn_arguments(TASK, "tmp_worker", Some("worktree")),
    )
    .await;
    let child = mount_child(&server, TASK, &[]).await;
    let mut created = test.thread_manager.subscribe_thread_created();
    let workspace_with_tmp = PermissionProfile::workspace_write_with(
        &[],
        NetworkSandboxPolicy::Restricted,
        /*exclude_tmpdir_env_var*/ false,
        /*exclude_slash_tmp*/ false,
    );
    test.submit_turn_with_permission_profile(ROOT_PROMPT, workspace_with_tmp)
        .await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    // R3.4, R3.5: the test checkout lies under a writable temporary root.
    assert!(test.config.cwd.as_path().starts_with(std::env::temp_dir()));
    let output = spawn_output(&spawn, "spawn-tmp");
    assert_eq!(
        output["worktree"]["parent_checkout_protected"],
        json!(false)
    );
    assert!(
        child[0]
            .single_request()
            .body_contains_text("which is still writable (the sandbox does not protect it)")
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UC4: checkpoint on errors and failures
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_errored_turn_and_a_failed_checkpoint_still_report_the_worktree() -> Result<()> {
    const ERRORED_TASK: &str = "edit then fail";
    const LOCKED_TASK: &str = "edit while refs are locked";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-errored",
        "spawn_agent",
        spawn_arguments(ERRORED_TASK, "errored", Some("worktree")),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            let text = body_text(request);
            text.contains(ERRORED_TASK)
                && !text.contains(ROOT_PROMPT)
                && !has_output(request, "errored-edit")
        },
        sse(vec![
            ev_response_created("resp-errored-edit"),
            ev_exec_command_call_with_args(
                "errored-edit",
                &shell_args("printf 'partial\\n' > partial.txt"),
            ),
            ev_completed("resp-errored-edit"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            let text = body_text(request);
            text.contains(ERRORED_TASK)
                && !text.contains(ROOT_PROMPT)
                && has_output(request, "errored-edit")
        },
        sse_failed(
            "resp-errored-fail",
            "invalid_prompt",
            "injected model failure",
        ),
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn(ROOT_PROMPT).await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    // R4.1: the errored turn still checkpoints and reports its branch.
    let collect = mount_root_answer(&server, "collect the errored results").await;
    test.submit_turn("collect the errored results").await?;
    let request = collect.single_request();
    assert!(request.body_contains_text("Agent errored"));
    let block = mail_block(&request);
    assert!(block.contains("branch: codex/agents/errored-"), "{block}");
    assert!(block.contains("A partial.txt"), "{block}");

    // R4.5: with the agent ref namespace blocked, mail still arrives with the reason.
    let parent = test.config.cwd.to_path_buf();
    let blocked_namespace = parent.join(".git/refs/heads/codex/agents");
    git(&parent, &["pack-refs", "--all"]);
    std::fs::remove_dir_all(&blocked_namespace).ok();
    std::fs::create_dir_all(parent.join(".git/refs/heads/codex"))?;
    std::fs::write(&blocked_namespace, "not a directory\n")?;
    let spawn = mount_root_call(
        &server,
        "spawn while refs are locked",
        "spawn-locked",
        "spawn_agent",
        spawn_arguments(LOCKED_TASK, "locked", Some("worktree")),
    )
    .await;
    mount_child(
        &server,
        LOCKED_TASK,
        &[("locked-edit", "printf 'kept\\n' > kept.txt".to_string())],
    )
    .await;
    test.submit_turn("spawn while refs are locked").await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;
    let root = PathBuf::from(worktree_field(
        &spawn_output(&spawn, "spawn-locked"),
        "path",
    ));
    let collect = mount_root_answer(&server, "collect the locked results").await;
    test.submit_turn("collect the locked results").await?;
    let block = mail_block(&collect.single_request());
    assert!(block.contains("checkpoint_failed: "), "{block}");
    assert!(
        block.contains(&format!("checkout: {}\n", root.display())),
        "{block}"
    );
    assert_eq!(std::fs::read_to_string(root.join("kept.txt"))?, "kept\n");
    assert!(git(&root, &["status", "--porcelain"]).contains("?? kept.txt"));
    assert!(!git_succeeds(&root, &["symbolic-ref", "-q", "HEAD"]));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UC5: eviction, cold reload and restart
// ---------------------------------------------------------------------------------------------

/// Spawns one isolated worker that runs `steps` and waits for its first turn.
async fn spawn_isolated_worker(
    server: &MockServer,
    test: &TestCodex,
    created: &mut broadcast::Receiver<ThreadId>,
    task: &'static str,
    steps: &[(&'static str, String)],
    profile: PermissionProfile,
) -> Result<(Child, Value)> {
    let spawn = mount_root_call(
        server,
        ROOT_PROMPT,
        "spawn-worker",
        "spawn_agent",
        spawn_arguments(task, "worker", Some("worktree")),
    )
    .await;
    mount_child(server, task, steps).await;
    test.submit_turn_with_permission_profile(ROOT_PROMPT, profile)
        .await?;
    let child = next_child(created, test).await?;
    wait_turn_complete(&child).await;
    Ok((child, spawn_output(&spawn, "spawn-worker")))
}

async fn evict(test: &TestCodex, child: Child) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let outcome = test
            .thread_manager
            .try_evict_v2_thread(Arc::clone(&child.thread))
            .await?;
        match outcome {
            ThreadEvictionOutcome::Evicted => break,
            ThreadEvictionOutcome::Busy if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            outcome => anyhow::bail!("eviction did not complete: {outcome:?}"),
        }
    }
    assert!(test.thread_manager.get_thread(child.id).await.is_err());
    Ok(())
}

fn followup_arguments(message: &str) -> Value {
    json!({ "target": "worker", "message": message })
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_evicted_child_reloads_into_its_worktree() -> Result<()> {
    const TASK: &str = "start before eviction";
    const FOLLOWUP: &str = "continue after reload";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let parent = test.config.cwd.to_path_buf();
    let mut created = test.thread_manager.subscribe_thread_created();
    let (child, output) = spawn_isolated_worker(
        &server,
        &test,
        &mut created,
        TASK,
        &[("first-edit", "printf 'one\\n' > one.txt".to_string())],
        workspace_only_profile(),
    )
    .await?;
    let root = PathBuf::from(worktree_field(&output, "path"));
    evict(&test, child).await?;

    // R6.3: eviction alone keeps the worktree.
    assert!(root.join("one.txt").exists());
    assert_eq!(managed_buckets(&test).len(), 1);

    mount_root_call(
        &server,
        "follow up with the worker",
        "followup-worker",
        "followup_task",
        followup_arguments(FOLLOWUP),
    )
    .await;
    let reloaded = mount_child(
        &server,
        FOLLOWUP,
        &[(
            "reloaded-check",
            format!(
                "pwd; printf escaped > '{}'",
                parent.join("escape-after-reload.txt").display()
            ),
        )],
    )
    .await;
    test.submit_turn_with_permission_profile("follow up with the worker", workspace_only_profile())
        .await?;
    let child = next_child(&mut created, &test).await?;
    wait_turn_complete(&child).await;

    // R5.1: the reloaded child runs in its worktree and still cannot write the parent's checkout.
    let check = output_of(&reloaded, "reloaded-check");
    assert!(check.contains(&root.display().to_string()), "{check}");
    assert!(check.contains("Read-only file system"), "{check}");
    assert!(!parent.join("escape-after-reload.txt").exists());
    assert_eq!(child.thread.config_snapshot().await.cwd().as_path(), root);
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reloaded_child_is_bounded_by_the_parents_narrowed_permissions() -> Result<()> {
    const TASK: &str = "write before narrowing";
    const FOLLOWUP: &str = "write after narrowing";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let mut created = test.thread_manager.subscribe_thread_created();
    let (child, output) = spawn_isolated_worker(
        &server,
        &test,
        &mut created,
        TASK,
        &[(
            "before",
            "printf before > before.txt && cat before.txt".to_string(),
        )],
        workspace_only_profile(),
    )
    .await?;
    let root = PathBuf::from(worktree_field(&output, "path"));
    assert!(root.join("before.txt").exists());
    evict(&test, child).await?;

    mount_root_call(
        &server,
        "follow up read-only",
        "followup-read-only",
        "followup_task",
        followup_arguments(FOLLOWUP),
    )
    .await;
    let reloaded = mount_child(
        &server,
        FOLLOWUP,
        &[("after", "printf after > after.txt".to_string())],
    )
    .await;
    test.submit_turn_with_permission_profile("follow up read-only", PermissionProfile::read_only())
        .await?;
    let child = next_child(&mut created, &test).await?;
    wait_turn_complete(&child).await;

    // R5.5: the parent's current read-only permissions bound the reloaded child.
    let after = output_of(&reloaded, "after");
    assert!(after.contains("Read-only file system"), "{after}");
    assert!(!root.join("after.txt").exists());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_worktree_fails_the_reload_without_falling_back() -> Result<()> {
    const TASK: &str = "work before removal";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let mut created = test.thread_manager.subscribe_thread_created();
    let (child, output) = spawn_isolated_worker(
        &server,
        &test,
        &mut created,
        TASK,
        &[("edit", "printf kept > kept.txt".to_string())],
        PermissionProfile::Disabled,
    )
    .await?;
    let root = PathBuf::from(worktree_field(&output, "path"));
    let branch = worktree_field(&output, "branch");
    evict(&test, child).await?;
    std::fs::remove_dir_all(&root)?;

    let followup = mount_root_call(
        &server,
        "follow up after removal",
        "followup-removed",
        "followup_task",
        followup_arguments("this must not run"),
    )
    .await;
    test.submit_turn("follow up after removal").await?;

    // R5.3: the error names the missing worktree and the branch with the earlier changes.
    let error = followup
        .function_call_output_text("followup-removed")
        .context("followup output")?;
    assert!(error.contains(&root.display().to_string()), "{error}");
    assert!(error.contains(&branch), "{error}");
    assert!(!test.config.cwd.join("kept.txt").exists());
    assert_eq!(
        test.thread_manager.list_thread_ids().await,
        vec![test.session_configured.thread_id]
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restarted_parent_reloads_its_child_into_the_worktree() -> Result<()> {
    const TASK: &str = "work before the restart";
    const FOLLOWUP: &str = "continue after the restart";
    let server = start_mock_server().await;
    let mut builder = isolation_builder(None, /*git_repository*/ true, |config| {
        config.features.enable(Feature::Sqlite).expect("sqlite");
    });
    let initial = builder.build_with_auto_env(&server).await?;
    let mut created = initial.thread_manager.subscribe_thread_created();
    let (child, output) = spawn_isolated_worker(
        &server,
        &initial,
        &mut created,
        TASK,
        &[("edit", "printf restarted > restarted.txt".to_string())],
        PermissionProfile::Disabled,
    )
    .await?;
    let root = PathBuf::from(worktree_field(&output, "path"));
    child.thread.shutdown_and_wait().await?;
    initial.codex.flush_rollout().await?;

    let resumed = builder.restart(&server, &initial).await?;
    // R6.2: the worktree with changes survived the previous process's tree shutdown.
    assert!(root.join("restarted.txt").exists());
    assert_eq!(resumed.thread_manager.list_thread_ids().await.len(), 1);
    mount_root_call(
        &server,
        "follow up after the restart",
        "followup-restart",
        "followup_task",
        followup_arguments(FOLLOWUP),
    )
    .await;
    let reloaded = mount_child(
        &server,
        FOLLOWUP,
        &[("where", "pwd; cat restarted.txt".to_string())],
    )
    .await;
    let mut created = resumed.thread_manager.subscribe_thread_created();
    resumed.submit_turn("follow up after the restart").await?;
    let child = next_child(&mut created, &resumed).await?;
    wait_turn_complete(&child).await;

    // R5.2, R5.4: the binding came back from the stored cwd and the owner record.
    let check = output_of(&reloaded, "where");
    assert!(check.contains(&root.display().to_string()), "{check}");
    assert!(check.contains("restarted"), "{check}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_worktree_owned_by_another_thread_is_not_accepted_after_restart() -> Result<()> {
    const TASK: &str = "work before tampering";
    let server = start_mock_server().await;
    let mut builder = isolation_builder(None, /*git_repository*/ true, |config| {
        config.features.enable(Feature::Sqlite).expect("sqlite");
    });
    let initial = builder.build_with_auto_env(&server).await?;
    let mut created = initial.thread_manager.subscribe_thread_created();
    let (child, output) = spawn_isolated_worker(
        &server,
        &initial,
        &mut created,
        TASK,
        &[("edit", "printf tampered > tampered.txt".to_string())],
        PermissionProfile::Disabled,
    )
    .await?;
    let root = PathBuf::from(worktree_field(&output, "path"));
    child.thread.shutdown_and_wait().await?;
    initial.codex.flush_rollout().await?;
    let git_dir = git(&root, &["rev-parse", "--path-format=absolute", "--git-dir"]);
    std::fs::write(
        Path::new(&git_dir).join("codex-thread.json"),
        format!(r#"{{"version":1,"ownerThreadId":"{}"}}"#, ThreadId::new()),
    )?;

    let resumed = builder.restart(&server, &initial).await?;
    let followup = mount_root_call(
        &server,
        "follow up a tampered worktree",
        "followup-tampered",
        "followup_task",
        followup_arguments("this must not run either"),
    )
    .await;
    resumed.submit_turn("follow up a tampered worktree").await?;

    // R5.2: a foreign owner record rejects the binding instead of reloading elsewhere.
    let error = followup
        .function_call_output_text("followup-tampered")
        .context("followup output")?;
    assert!(error.contains("is owned by"), "{error}");
    assert_eq!(resumed.thread_manager.list_thread_ids().await.len(), 1);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UC6: finalize when the agent tree stops
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tree_shutdown_keeps_changed_worktrees_and_removes_unchanged_ones() -> Result<()> {
    const CHANGED: &str = "change a tracked file";
    const UNCHANGED: &str = "only build ignored output";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let changed_spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-changed",
        "spawn_agent",
        spawn_arguments(CHANGED, "changed", Some("worktree")),
    )
    .await;
    let unchanged_spawn = mount_root_call(
        &server,
        "spawn the unchanged worker",
        "spawn-unchanged",
        "spawn_agent",
        spawn_arguments(UNCHANGED, "unchanged", Some("worktree")),
    )
    .await;
    mount_child(
        &server,
        CHANGED,
        &[("changed-edit", "printf 'changed\\n' >> a.txt".to_string())],
    )
    .await;
    mount_child(
        &server,
        UNCHANGED,
        &[(
            "unchanged-build",
            "mkdir -p target && printf binary > target/out.bin".to_string(),
        )],
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn(ROOT_PROMPT).await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;
    test.submit_turn("spawn the unchanged worker").await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;
    let changed_root = PathBuf::from(worktree_field(
        &spawn_output(&changed_spawn, "spawn-changed"),
        "path",
    ));
    let unchanged_root = PathBuf::from(worktree_field(
        &spawn_output(&unchanged_spawn, "spawn-unchanged"),
        "path",
    ));
    assert!(unchanged_root.join("target/out.bin").exists());
    assert_eq!(managed_buckets(&test).len(), 2);

    test.thread_manager
        .shutdown_all_threads_bounded(Duration::from_secs(10))
        .await;

    // R6.1, R6.2, R6.4: one worktree and one branch remain; ignored output did not block removal.
    assert!(changed_root.exists());
    assert!(!unchanged_root.exists());
    assert_eq!(managed_buckets(&test).len(), 1);
    let branches = agent_branches(&test.config.cwd);
    assert_eq!(branches.len(), 1, "{branches:?}");
    assert!(branches[0].starts_with("codex/agents/changed-"));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// UC7: a conflicting apply leaves the parent's tree alone
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_conflicting_apply_leaves_the_parent_tree_unchanged() -> Result<()> {
    const TASK: &str = "rewrite the first line";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let parent = test.config.cwd.to_path_buf();
    let mut created = test.thread_manager.subscribe_thread_created();
    let (_child, output) = spawn_isolated_worker(
        &server,
        &test,
        &mut created,
        TASK,
        &[(
            "rewrite",
            "printf 'child base\\nparent edit\\n' > a.txt".to_string(),
        )],
        PermissionProfile::Disabled,
    )
    .await?;
    let base = worktree_field(&output, "base");
    let branch = worktree_field(&output, "branch");
    // The parent keeps working on the same line before it merges.
    std::fs::write(parent.join("a.txt"), "parent base\nparent edit\n")?;

    let apply = mount_root_command(
        &server,
        "apply the conflicting branch",
        "root-conflict",
        format!("git diff --binary {base} {branch} | git apply"),
    )
    .await;
    test.submit_turn_with_permission_profile(
        "apply the conflicting branch",
        workspace_only_profile(),
    )
    .await?;

    // R7.2: `git apply` refuses atomically and the parent's file keeps its own edit.
    let applied = apply
        .function_call_output_text("root-conflict")
        .context("apply output")?;
    assert!(!applied.contains("exited with code 0"), "{applied}");
    assert_eq!(
        std::fs::read_to_string(parent.join("a.txt"))?,
        "parent base\nparent edit\n"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_history_fork_still_receives_the_isolation_note() -> Result<()> {
    const TASK: &str = "continue with the inherited history";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-forked",
        "spawn_agent",
        json!({ "message": TASK, "task_name": "forked", "isolation": "worktree" }),
    )
    .await;
    // A forked child's history contains the root prompt, so match its own task message.
    let child = mount_sse_once_match(
        &server,
        move |request: &wiremock::Request| {
            body(request)["input"].as_array().is_some_and(|items| {
                items.iter().any(|item| {
                    item["type"] == "agent_message"
                        && item["recipient"] == "/root/forked"
                        && item.to_string().contains(TASK)
                })
            })
        },
        sse(vec![
            ev_response_created("resp-forked"),
            final_answer("forked", "forked done"),
            ev_completed("resp-forked"),
        ]),
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_turn(ROOT_PROMPT).await?;
    wait_turn_complete(&next_child(&mut created, &test).await?).await;

    // R3.5: the note reaches a child that inherits the parent's whole history.
    let root = worktree_field(&spawn_output(&spawn, "spawn-forked"), "path");
    let request = child.single_request();
    assert!(request.body_contains_text(&format!(
        "You are working in an isolated git worktree at {root}."
    )));
    assert!(request.body_contains_text(&format!("<cwd>{root}</cwd>")));
    Ok(())
}

/// Sets the root's local environment to owner-provided configuration with `profile`.
async fn set_owner_environment(test: &TestCodex, profile: PermissionProfile) -> Result<()> {
    let cwd = test.config.cwd.clone();
    let mut selection = core_test_support::test_codex::local(cwd.clone());
    let mut config =
        core_test_support::test_codex::environment_config_for_selection(&test.config, &selection);
    config.permission_profile = PermissionProfileSnapshot::legacy(profile);
    selection.config = EnvironmentConfigState::Ready(config);
    core_test_support::submit_thread_settings(
        &test.codex,
        ThreadSettingsOverrides {
            environments: Some(TurnEnvironmentRequests::new(
                cwd,
                vec![selection.into_request()],
            )),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_owner_configured_child_is_bounded_by_the_parent_after_reload() -> Result<()> {
    const TASK: &str = "write with owner permissions";
    const FOLLOWUP: &str = "write after the owner narrowed";
    let server = start_mock_server().await;
    let test = isolation_codex(&server, /*git_repository*/ true, |_| {}).await?;
    set_owner_environment(&test, workspace_only_profile()).await?;
    let spawn = mount_root_call(
        &server,
        ROOT_PROMPT,
        "spawn-worker",
        "spawn_agent",
        spawn_arguments(TASK, "worker", Some("worktree")),
    )
    .await;
    let first = mount_child(
        &server,
        TASK,
        &[("owner-before", "printf before > before.txt".to_string())],
    )
    .await;
    let mut created = test.thread_manager.subscribe_thread_created();
    test.submit_text_turn(ROOT_PROMPT).await?;
    let child = next_child(&mut created, &test).await?;
    wait_turn_complete(&child).await;
    let root = PathBuf::from(worktree_field(
        &spawn_output(&spawn, "spawn-worker"),
        "path",
    ));
    let child_id = child.id;
    let before = output_of(&first, "owner-before");
    assert!(before.contains("exited with code 0"), "{before}");
    assert!(root.join("before.txt").exists());
    evict(&test, child).await?;

    // The owner narrows the parent's environment while the child is unloaded, then reloads the
    // child through its parent, as a host does before attaching to it.
    set_owner_environment(&test, PermissionProfile::read_only()).await?;
    test.thread_manager
        .ensure_multi_agent_v2_child_loaded(child_id)
        .await?;
    assert!(test.thread_manager.get_thread(child_id).await.is_ok());
    mount_root_call(
        &server,
        "follow up after the owner narrowed",
        "followup-owner",
        "followup_task",
        followup_arguments(FOLLOWUP),
    )
    .await;
    let reloaded = mount_child(
        &server,
        FOLLOWUP,
        &[("owner-after", "printf after > after.txt".to_string())],
    )
    .await;
    test.submit_text_turn("follow up after the owner narrowed")
        .await?;
    let child = Child {
        id: child_id,
        thread: test.thread_manager.get_thread(child_id).await?,
    };
    wait_turn_complete(&child).await;

    // R5.5: the parent's environment, moved onto the worktree, still bounds the child.
    let after = output_of(&reloaded, "owner-after");
    assert!(after.contains("Read-only file system"), "{after}");
    assert!(!root.join("after.txt").exists());
    Ok(())
}
