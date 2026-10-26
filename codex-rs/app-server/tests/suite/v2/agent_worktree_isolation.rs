//! Fork: an isolated MultiAgentV2 child appears through the unchanged `Thread` fields.
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, R8.1.

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::write_models_cache;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_utils_absolute_path::AbsolutePathBuf;
use core_test_support::responses;
use serde_json::json;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

/// Worktree isolation requires a custom MultiAgentV2 tool namespace (R1.5).
const NAMESPACE: &str = "agents";
const PARENT_PROMPT: &str = "spawn an isolated worker";
const CHILD_PROMPT: &str = "work in the isolated checkout";
const SPAWN_CALL_ID: &str = "spawn-isolated-worker";

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn thread_read_reports_the_isolated_child_worktree() -> Result<()> {
    let server = responses::start_mock_server().await;
    let spawn_args = json!({
        "message": CHILD_PROMPT,
        "task_name": "worker",
        "fork_turns": "none",
        "isolation": "worktree",
    });
    responses::mount_sse_once_match(
        &server,
        |request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            body.contains(PARENT_PROMPT) && !body.contains(SPAWN_CALL_ID)
        },
        responses::sse(vec![
            responses::ev_response_created("parent-spawn"),
            responses::ev_function_call_with_namespace(
                SPAWN_CALL_ID,
                NAMESPACE,
                "spawn_agent",
                &serde_json::to_string(&spawn_args)?,
            ),
            responses::ev_completed("parent-spawn"),
        ]),
    )
    .await;
    let child_request = responses::mount_sse_once_match(
        &server,
        |request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            body.contains(CHILD_PROMPT) && body.contains(r#"\"agent_name\":\"/root/worker\""#)
        },
        responses::sse(vec![
            responses::ev_response_created("child-work"),
            responses::ev_assistant_message("child-message", "child complete"),
            responses::ev_completed("child-work"),
        ]),
    )
    .await;
    responses::mount_sse_once_match(
        &server,
        |request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            body.contains(SPAWN_CALL_ID) && body.contains(r#"\"agent_name\":\"/root\""#)
        },
        responses::sse(vec![
            responses::ev_response_created("parent-complete"),
            responses::ev_assistant_message("parent-message", "parent complete"),
            responses::ev_completed("parent-complete"),
        ]),
    )
    .await;

    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .with_model("gpt-5.4")
        .with_extra_config(
            "[features.multi_agent_v2]\nenabled = true\ntool_namespace = \"agents\"\n\n[features.agent_worktree_isolation]\nenabled = true",
        )
        .write(codex_home.path())?;
    write_models_cache(codex_home.path()).await?;
    let repository = TempDir::new()?;
    let repository_path = std::fs::canonicalize(repository.path())?;
    git(&repository_path, &["init", "--quiet", "-b", "main"]);
    std::fs::write(repository_path.join("a.txt"), "base\n")?;
    git(&repository_path, &["add", "."]);
    git(
        &repository_path,
        &[
            "-c",
            "user.name=Codex Test",
            "-c",
            "user.email=codex@example.invalid",
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-m",
            "initial",
        ],
    );

    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    // Run the parent's local environment in the repository instead of the harness directory.
    let mut environment = app_server.auto_env_params()?;
    environment.cwd = AbsolutePathBuf::from_absolute_path(&repository_path)?.into();
    let start = app_server
        .send_thread_start_request(ThreadStartParams {
            model: Some("gpt-5.4".to_string()),
            cwd: Some(repository_path.display().to_string()),
            environments: Some(vec![environment]),
            ..Default::default()
        })
        .await?;
    let ThreadStartResponse { thread, .. } =
        timeout(Duration::from_secs(20), app_server.read_response(start)).await??;
    let _: TurnStartResponse = app_server
        .request(|request_id| ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: thread.id.clone(),
                input: vec![UserInput::Text {
                    text: PARENT_PROMPT.to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await?;
    timeout(Duration::from_secs(20), async {
        while child_request.requests().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await?;

    let loaded: ThreadLoadedListResponse = app_server
        .request(|request_id| ClientRequest::ThreadLoadedList {
            request_id,
            params: ThreadLoadedListParams::default(),
        })
        .await?;
    let child_id = loaded
        .data
        .iter()
        .find(|id| **id != thread.id)
        .cloned()
        .expect("loaded child thread");
    let ThreadReadResponse { thread: child, .. } = app_server
        .request(|request_id| ClientRequest::ThreadRead {
            request_id,
            params: ThreadReadParams {
                thread_id: child_id,
                include_turns: false,
            },
        })
        .await?;

    let managed_root = std::fs::canonicalize(codex_home.path())?.join("worktrees");
    assert!(
        child.cwd.as_path().starts_with(&managed_root),
        "child cwd {} is outside {}",
        child.cwd.display(),
        managed_root.display()
    );
    assert_ne!(child.cwd.as_path(), repository_path.as_path());
    if let Some(environments) = &child.environments {
        let environment = environments.first().expect("child environment");
        assert!(
            Path::new(environment.cwd.as_str()).starts_with(&managed_root),
            "child environment cwd {:?} is outside the managed root",
            environment.cwd
        );
    }
    Ok(())
}
