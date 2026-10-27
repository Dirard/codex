//! Fork: an isolated MultiAgentV2 child appears through the `Thread` fields and, since amend
//! 2026-10-27, through the experimental `Thread.extra.agentWorktree`.
//!
//! Spec: `specs/agent-worktree-isolation-2026-10-25.md`, R8.1, R9.3.

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::write_models_cache;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ThreadAgentWorktreeStatus;
use codex_app_server_protocol::ThreadListParams;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListParams;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadSourceKind;
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

    // Amend 2026-10-27, R9.3: the experimental `extra` names the child's worktree in
    // `thread/read` and `thread/list`; the root's `extra` stays empty.
    let worktree = child
        .extra
        .as_ref()
        .and_then(|extra| extra.agent_worktree.clone())
        .expect("agent worktree in thread/read");
    assert!(
        worktree.path.as_path().starts_with(&managed_root),
        "{worktree:?}"
    );
    assert_eq!(worktree.branch, None);
    assert_eq!(worktree.status, ThreadAgentWorktreeStatus::Clean);
    let ThreadReadResponse { thread: root, .. } = app_server
        .request(|request_id| ClientRequest::ThreadRead {
            request_id,
            params: ThreadReadParams {
                thread_id: thread.id.clone(),
                include_turns: false,
            },
        })
        .await?;
    assert_eq!(root.extra, None);
    let listed: ThreadListResponse = app_server
        .request(|request_id| ClientRequest::ThreadList {
            request_id,
            params: ThreadListParams {
                excluded_thread_ids: None,
                attached_to_thread_id: None,
                originators: None,
                cursor: None,
                limit: None,
                sort_key: None,
                sort_direction: None,
                model_providers: Some(Vec::new()),
                source_kinds: Some(vec![ThreadSourceKind::SubAgentThreadSpawn]),
                archived: None,
                section_id: None,
                project_id: None,
                cwd: None,
                use_state_db_only: false,
                search_term: None,
                parent_thread_id: Some(thread.id.clone()),
                ancestor_thread_id: None,
            },
        })
        .await?;
    if let Some(listed_child) = listed.data.iter().find(|listed| listed.id == child.id) {
        assert_eq!(
            listed_child
                .extra
                .as_ref()
                .and_then(|extra| extra.agent_worktree.as_ref()),
            Some(&worktree)
        );
    }
    Ok(())
}

/// Amend 2026-10-27 (deferred items), R5.9: a direct `thread/resume` of an isolated child whose
/// worktree was removed reloads it through its parent and restores the worktree from the host
/// record; without the record the error names the worktree and the reason.
#[cfg(unix)]
#[tokio::test]
async fn thread_resume_restores_a_removed_child_worktree() -> Result<()> {
    use codex_app_server_protocol::RequestId;
    use codex_app_server_protocol::ThreadResumeParams;
    use codex_app_server_protocol::ThreadResumeResponse;
    const RESUME_PROMPT: &str = "spawn an isolated worker to resume later";
    const RESUME_TASK: &str = "look at the checkout and stop";
    const RESUME_SPAWN_CALL_ID: &str = "spawn-resumable-worker";

    let server = responses::start_mock_server().await;
    let spawn_args = json!({
        "message": RESUME_TASK,
        "task_name": "worker",
        "fork_turns": "none",
        "isolation": "worktree",
    });
    responses::mount_sse_once_match(
        &server,
        |request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            body.contains(RESUME_PROMPT) && !body.contains(RESUME_SPAWN_CALL_ID)
        },
        responses::sse(vec![
            responses::ev_response_created("resume-parent-spawn"),
            responses::ev_function_call_with_namespace(
                RESUME_SPAWN_CALL_ID,
                NAMESPACE,
                "spawn_agent",
                &serde_json::to_string(&spawn_args)?,
            ),
            responses::ev_completed("resume-parent-spawn"),
        ]),
    )
    .await;
    let child_request = responses::mount_sse_once_match(
        &server,
        |request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            body.contains(RESUME_TASK) && body.contains(r#"\"agent_name\":\"/root/worker\""#)
        },
        responses::sse(vec![
            responses::ev_response_created("resume-child-work"),
            responses::ev_assistant_message("resume-child-message", "child complete"),
            responses::ev_completed("resume-child-work"),
        ]),
    )
    .await;
    responses::mount_sse_once_match(
        &server,
        |request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            body.contains(RESUME_SPAWN_CALL_ID) && body.contains(r#"\"agent_name\":\"/root\""#)
        },
        responses::sse(vec![
            responses::ev_response_created("resume-parent-complete"),
            responses::ev_assistant_message("resume-parent-message", "parent complete"),
            responses::ev_completed("resume-parent-complete"),
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

    // The first server spawns the child; its graceful stop removes the unchanged worktree.
    let (parent_id, child_id, worktree) = {
        let mut app_server = TestAppServer::builder()
            .with_codex_home(codex_home.path())
            .build_initialized()
            .await?;
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
                        text: RESUME_PROMPT.to_string(),
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
                    thread_id: child_id.clone(),
                    include_turns: false,
                },
            })
            .await?;
        let worktree = child
            .extra
            .and_then(|extra| extra.agent_worktree)
            .expect("agent worktree")
            .path;
        timeout(Duration::from_secs(30), app_server.shutdown_gracefully()).await??;
        (thread.id, child_id, worktree)
    };
    assert!(!worktree.as_path().exists(), "{}", worktree.display());
    let record = codex_home
        .path()
        .join("agent-worktrees")
        .join(format!("{child_id}.json"));
    let hidden = codex_home.path().join("hidden-record.json");
    std::fs::rename(&record, &hidden)?;

    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let _: ThreadResumeResponse = app_server
        .request(|request_id| ClientRequest::ThreadResume {
            request_id,
            params: ThreadResumeParams {
                thread_id: parent_id.clone(),
                ..Default::default()
            },
        })
        .await?;

    // Without the host record nothing names the parent checkout: an explicit error.
    let request_id = app_server
        .send_thread_resume_request(ThreadResumeParams {
            thread_id: child_id.clone(),
            ..Default::default()
        })
        .await?;
    let error = timeout(
        Duration::from_secs(20),
        app_server.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    let message = error.error.message;
    assert!(
        message.contains(&worktree.display().to_string()),
        "{message}"
    );
    assert!(message.contains("no record"), "{message}");
    assert!(!worktree.as_path().exists());

    // With the record the reload through the parent restores the worktree.
    std::fs::rename(&hidden, &record)?;
    let resumed: ThreadResumeResponse = app_server
        .request(|request_id| ClientRequest::ThreadResume {
            request_id,
            params: ThreadResumeParams {
                thread_id: child_id.clone(),
                ..Default::default()
            },
        })
        .await?;
    assert_eq!(resumed.thread.id, child_id);
    assert!(
        resumed.thread.cwd.as_path().starts_with(worktree.as_path()),
        "{}",
        resumed.thread.cwd.display()
    );
    assert!(worktree.as_path().join(".git").exists());
    assert_eq!(
        std::fs::read_to_string(worktree.as_path().join("a.txt"))?,
        "base\n"
    );
    Ok(())
}
