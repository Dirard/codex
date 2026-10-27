//! Fork: `thread/backgroundTerminals/pending` over the background terminal notifications a
//! loaded thread still owes the model (`specs/background-exec-notifications-2026-10-25.md`,
//! UC7 R7.4).

use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::create_fake_rollout;
use app_test_support::create_mock_responses_server_repeating_assistant;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadArchiveResponse;
use codex_app_server_protocol::ThreadBackgroundTerminalsPendingParams;
use codex_app_server_protocol::ThreadBackgroundTerminalsPendingResponse;
use codex_app_server_protocol::ThreadBackgroundTerminalsTerminateParams;
use codex_app_server_protocol::ThreadBackgroundTerminalsTerminateResponse;
use codex_app_server_protocol::ThreadDeleteParams;
use codex_app_server_protocol::ThreadDeleteResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_features::Feature;
use core_test_support::responses;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

const READ_TIMEOUT: Duration = Duration::from_secs(30);

async fn pending(
    app: &mut TestAppServer,
    thread_id: &str,
) -> Result<ThreadBackgroundTerminalsPendingResponse> {
    app.request(
        |request_id| ClientRequest::ThreadBackgroundTerminalsPending {
            request_id,
            params: ThreadBackgroundTerminalsPendingParams {
                thread_id: thread_id.to_string(),
            },
        },
    )
    .await
}

/// R7.4: a live terminal with an armed exit notification is pending until a client stops
/// it; the passive notification of that stop starts no turn, so nothing stays pending.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pending_lists_armed_terminals_of_a_loaded_thread() -> Result<()> {
    let server = responses::start_mock_server().await;
    let arguments = json!({
        "cmd": "sleep 60",
        "yield_time_ms": 250,
        "login": false,
        "notify_on_exit": true,
        "notify_on_output": "ready",
    })
    .to_string();
    responses::mount_sse_sequence(
        &server,
        vec![
            responses::sse(vec![
                responses::ev_response_created("resp-1"),
                responses::ev_function_call("bg-call", "exec_command", &arguments),
                responses::ev_completed("resp-1"),
            ]),
            responses::sse(vec![
                responses::ev_response_created("resp-2"),
                responses::ev_assistant_message("msg-2", "Started."),
                responses::ev_completed("resp-2"),
            ]),
        ],
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::ExecExitNotifications)
        .with_provider_config("supports_websockets = false")
        .write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;

    let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
    let idle = pending(&mut app, &thread.id).await?;
    assert_eq!(
        idle,
        ThreadBackgroundTerminalsPendingResponse {
            terminals: Vec::new(),
            notification_queued: false,
            active_turn_id: None,
            turn_starting: false,
        }
    );

    let TurnStartResponse { turn } = app
        .request(|request_id| ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: thread.id.clone(),
                input: vec![UserInput::Text {
                    text: "start a long command".to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await?;
    loop {
        let completed: TurnCompletedNotification =
            timeout(READ_TIMEOUT, app.read_notification("turn/completed")).await??;
        if completed.turn.id == turn.id {
            break;
        }
    }

    let armed = pending(&mut app, &thread.id).await?;
    assert_eq!(armed.terminals.len(), 1, "{armed:?}");
    let terminal = &armed.terminals[0];
    assert_eq!(terminal.command, "sleep 60");
    assert_eq!(terminal.output_watch.as_deref(), Some("ready"));
    assert!(!armed.notification_queued);
    assert!(!armed.turn_starting);

    let ThreadBackgroundTerminalsTerminateResponse { terminated } = app
        .request(
            |request_id| ClientRequest::ThreadBackgroundTerminalsTerminate {
                request_id,
                params: ThreadBackgroundTerminalsTerminateParams {
                    thread_id: thread.id.clone(),
                    process_id: terminal.process_id.clone(),
                },
            },
        )
        .await?;
    assert!(terminated);
    let settled = timeout(READ_TIMEOUT, async {
        loop {
            let pending = pending(&mut app, &thread.id).await?;
            if pending.terminals.is_empty() {
                return anyhow::Ok(pending);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await??;
    assert!(!settled.notification_queued, "{settled:?}");

    let unknown_thread = codex_protocol::ThreadId::new().to_string();
    let request_id = app
        .send_request(
            "thread/backgroundTerminals/pending",
            Some(json!({ "threadId": unknown_thread })),
        )
        .await?;
    let error: JSONRPCError = timeout(
        READ_TIMEOUT,
        app.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await??;
    assert_eq!(
        error.error.message,
        format!("thread not loaded: {unknown_thread}")
    );
    Ok(())
}

/// R2.14 (tails review 2026-10-27, P3-5): archiving or deleting a thread removes the background
/// terminal journals that dead Codex processes left for it, since no resume will report them.
/// A process that still runs the thread, holding its journal lock, keeps its journal.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn archive_and_delete_remove_journals_of_dead_processes() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let archived = create_fake_rollout(
        codex_home.path(),
        "2025-01-01T00-00-00",
        "2025-01-01T00:00:00Z",
        "archived",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let deleted = create_fake_rollout(
        codex_home.path(),
        "2025-01-01T00-01-00",
        "2025-01-01T00:01:00Z",
        "deleted",
        Some("mock_provider"),
        /*git_info*/ None,
    )?;
    let dir = codex_home.path().join("background_terminals");
    std::fs::create_dir_all(&dir)?;
    let journal = |thread: &str, owner: &str| -> Result<std::fs::File> {
        std::fs::write(
            dir.join(format!("{thread}.{owner}.json")),
            r#"[{"process_id":1000,"command":"npm run dev","started_at_unix_ms":0}]"#,
        )?;
        Ok(std::fs::File::create(
            dir.join(format!("{thread}.{owner}.lock")),
        )?)
    };
    journal(&archived, "1-dead")?;
    journal(&deleted, "2-dead")?;
    let live = journal(&archived, "3-live")?;
    live.lock()?;
    let files = || -> Result<Vec<String>> {
        let mut names: Vec<String> = std::fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        names.sort();
        Ok(names)
    };

    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let _: ThreadArchiveResponse = app
        .request(|request_id| ClientRequest::ThreadArchive {
            request_id,
            params: ThreadArchiveParams {
                thread_id: archived.clone(),
            },
        })
        .await?;
    let mut expected = vec![
        format!("{archived}.3-live.json"),
        format!("{archived}.3-live.lock"),
        format!("{deleted}.2-dead.json"),
        format!("{deleted}.2-dead.lock"),
    ];
    expected.sort();
    assert_eq!(files()?, expected);

    let _: ThreadDeleteResponse = app
        .request(|request_id| ClientRequest::ThreadDelete {
            request_id,
            params: ThreadDeleteParams {
                thread_id: deleted.clone(),
            },
        })
        .await?;
    assert_eq!(
        files()?,
        vec![
            format!("{archived}.3-live.json"),
            format!("{archived}.3-live.lock"),
        ]
    );
    drop(live);
    Ok(())
}
