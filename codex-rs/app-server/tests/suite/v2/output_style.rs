//! Fork: `thread/outputStyle/list` and `thread/outputStyle/set` over the output style of a
//! loaded thread (`specs/output-styles-2026-11-07.md`, UC4 R4.4).

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ConfigBatchWriteParams;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::ConfigWriteResponse;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::JSONRPCError;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::MergeStrategy;
use codex_app_server_protocol::OutputStyleSource;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadOutputStyleListParams;
use codex_app_server_protocol::ThreadOutputStyleListResponse;
use codex_app_server_protocol::ThreadOutputStyleSetParams;
use codex_app_server_protocol::ThreadOutputStyleSetResponse;
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
const OPEN: &str = "<output_style>";

fn write_style(dot_codex: &Path, file: &str, name: &str) -> Result<()> {
    let dir = dot_codex.join("output-styles");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(file),
        format!("---\nname: {name}\ndescription: {name} style.\n---\nWrite like {name}.\n"),
    )?;
    Ok(())
}

async fn list(app: &mut TestAppServer, thread_id: &str) -> Result<ThreadOutputStyleListResponse> {
    app.request(|request_id| ClientRequest::ThreadOutputStyleList {
        request_id,
        params: ThreadOutputStyleListParams {
            thread_id: thread_id.to_string(),
        },
    })
    .await
}

async fn set_error(app: &mut TestAppServer, thread_id: &str, name: &str) -> Result<JSONRPCError> {
    let request_id = app
        .send_request(
            "thread/outputStyle/set",
            Some(json!({ "threadId": thread_id, "name": name })),
        )
        .await?;
    timeout(
        READ_TIMEOUT,
        app.read_stream_until_error_message(RequestId::Integer(request_id)),
    )
    .await?
}

async fn run_turn(app: &mut TestAppServer, thread_id: &str, text: &str) -> Result<()> {
    let TurnStartResponse { turn } = app
        .request(|request_id| ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: thread_id.to_string(),
                input: vec![UserInput::Text {
                    text: text.to_string(),
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
            return Ok(());
        }
    }
}

/// R4.4, R1.2: the list holds the built-in styles, a user style and the style of a trusted
/// project, but not the style of an untrusted project.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn list_shows_trusted_project_styles_only() -> Result<()> {
    let server = responses::start_mock_server().await;
    let codex_home = TempDir::new()?;
    let trusted = TempDir::new()?;
    let untrusted = TempDir::new()?;
    let trusted_key = trusted.path().to_string_lossy().replace('\\', "\\\\");
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::OutputStyles)
        .with_extra_config(&format!(
            "[projects.\"{trusted_key}\"]\ntrust_level = \"trusted\""
        ))
        .write(codex_home.path())?;
    write_style(codex_home.path(), "mine.md", "mine")?;
    write_style(&trusted.path().join(".codex"), "team.md", "team")?;
    write_style(&untrusted.path().join(".codex"), "evil.md", "evil")?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_managed_config()
        .build_initialized()
        .await?;

    for (cwd, expected) in [
        (
            trusted.path(),
            vec![
                ("default", OutputStyleSource::Builtin),
                ("outcome-first", OutputStyleSource::Builtin),
                ("explanatory", OutputStyleSource::Builtin),
                ("learning", OutputStyleSource::Builtin),
                ("mine", OutputStyleSource::User),
                ("team", OutputStyleSource::Project),
            ],
        ),
        (
            untrusted.path(),
            vec![
                ("default", OutputStyleSource::Builtin),
                ("outcome-first", OutputStyleSource::Builtin),
                ("explanatory", OutputStyleSource::Builtin),
                ("learning", OutputStyleSource::Builtin),
                ("mine", OutputStyleSource::User),
            ],
        ),
    ] {
        let thread = app
            .start_thread(ThreadStartParams {
                cwd: Some(cwd.display().to_string()),
                ..Default::default()
            })
            .await?
            .thread;
        let response = list(&mut app, &thread.id).await?;
        assert_eq!(response.current, "default");
        let styles = response
            .styles
            .iter()
            .map(|style| (style.name.as_str(), style.source))
            .collect::<Vec<_>>();
        assert_eq!(styles, expected, "{}", cwd.display());
        assert!(response.styles[0].path.is_none());
        assert!(response.styles[4].path.is_some());
    }
    Ok(())
}

/// R4.2–R4.4, R4.6: `set` changes the style of the next request; an unknown name, a disabled
/// feature and an unloaded thread are `invalid_request` errors with their reason; the initial
/// style comes from `config` of `thread/start`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn set_applies_to_the_next_request_and_refuses_with_reasons() -> Result<()> {
    let server = responses::start_mock_server().await;
    let requests = responses::mount_sse_sequence(
        &server,
        (0..3)
            .map(|index| responses::sse_completed(&format!("resp-{index}")))
            .collect(),
    )
    .await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;

    let styled = app
        .start_thread(ThreadStartParams {
            config: Some(HashMap::from([
                ("output_style".to_string(), json!("outcome-first")),
                ("features".to_string(), json!({ "output_styles": true })),
            ])),
            ..Default::default()
        })
        .await?
        .thread;
    run_turn(&mut app, &styled.id, "one").await?;
    let ThreadOutputStyleSetResponse { style } = app
        .request(|request_id| ClientRequest::ThreadOutputStyleSet {
            request_id,
            params: ThreadOutputStyleSetParams {
                thread_id: styled.id.clone(),
                name: "Learning".to_string(),
            },
        })
        .await?;
    assert_eq!(style.name, "learning");
    assert_eq!(style.source, OutputStyleSource::Builtin);
    assert_eq!(list(&mut app, &styled.id).await?.current, "learning");
    run_turn(&mut app, &styled.id, "two").await?;

    let error = set_error(&mut app, &styled.id, "missing").await?;
    assert_eq!(error.error.code, -32600);
    assert!(
        error
            .error
            .message
            .contains("unknown output style `missing`")
            && error
                .error
                .message
                .contains("default, outcome-first, explanatory, learning"),
        "{}",
        error.error.message
    );

    let plain = app.start_thread(ThreadStartParams::default()).await?.thread;
    let error = set_error(&mut app, &plain.id, "learning").await?;
    assert_eq!(error.error.code, -32600);
    assert!(
        error
            .error
            .message
            .contains("output styles are off for this thread"),
        "{}",
        error.error.message
    );
    run_turn(&mut app, &plain.id, "plain").await?;

    let unloaded = codex_protocol::ThreadId::new().to_string();
    let error = set_error(&mut app, &unloaded, "learning").await?;
    assert_eq!(
        error.error.message,
        format!("thread not loaded: {unloaded}")
    );

    let requests = requests.requests();
    assert_eq!(requests.len(), 3);
    let fragments = |index: usize| {
        requests[index]
            .message_input_texts("developer")
            .into_iter()
            .filter(|text| text.starts_with(OPEN))
            .collect::<Vec<_>>()
    };
    let first = fragments(0);
    assert_eq!(first.len(), 1, "{first:?}");
    assert!(first[0].contains("Output style: `outcome-first`."));
    let second = fragments(1);
    assert_eq!(second.len(), 2, "{second:?}");
    assert!(second[1].contains("The user switched the output style to `learning`."));
    assert_eq!(fragments(2), Vec::<String>::new());
    Ok(())
}

/// Review O-1: a running thread keeps the features it started with, so turning the feature on
/// in the user config (what `/experimental` writes, with a reload) does not let `set` change
/// that thread; the refusal says to start a new thread, and a new thread can use styles.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn enabling_the_feature_later_applies_to_new_threads() -> Result<()> {
    let server = responses::start_mock_server().await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let running = app.start_thread(ThreadStartParams::default()).await?.thread;

    let _: ConfigWriteResponse = app
        .request(|request_id| ClientRequest::ConfigBatchWrite {
            request_id,
            params: ConfigBatchWriteParams {
                edits: vec![ConfigEdit {
                    key_path: format!("features.{}", Feature::OutputStyles.key()),
                    value: json!(true),
                    merge_strategy: MergeStrategy::Replace,
                }],
                file_path: None,
                expected_version: None,
                reload_user_config: true,
            },
        })
        .await?;
    let error = set_error(&mut app, &running.id, "learning").await?;
    assert_eq!(error.error.code, -32600);
    assert!(
        error.error.message.contains("start a new thread"),
        "{}",
        error.error.message
    );
    assert_eq!(list(&mut app, &running.id).await?.current, "default");

    let fresh = app.start_thread(ThreadStartParams::default()).await?.thread;
    let ThreadOutputStyleSetResponse { style } = app
        .request(|request_id| ClientRequest::ThreadOutputStyleSet {
            request_id,
            params: ThreadOutputStyleSetParams {
                thread_id: fresh.id.clone(),
                name: "learning".to_string(),
            },
        })
        .await?;
    assert_eq!(style.name, "learning");
    Ok(())
}

/// R4.4: without the experimental API the methods are refused like other experimental
/// methods.
#[tokio::test]
async fn methods_require_the_experimental_api() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build()
        .await?;
    let init = app
        .initialize_with_capabilities(
            ClientInfo {
                name: "codex-app-server-tests".to_string(),
                title: None,
                version: "0.1.0".to_string(),
            },
            Some(InitializeCapabilities {
                experimental_api: false,
                ..Default::default()
            }),
        )
        .await?;
    let JSONRPCMessage::Response(_) = init else {
        anyhow::bail!("expected initialize response, got {init:?}");
    };
    for method in ["thread/outputStyle/list", "thread/outputStyle/set"] {
        let request_id = app
            .send_request(
                method,
                Some(json!({ "threadId": "thr", "name": "learning" })),
            )
            .await?;
        let error: JSONRPCError = timeout(
            READ_TIMEOUT,
            app.read_stream_until_error_message(RequestId::Integer(request_id)),
        )
        .await??;
        assert_eq!(error.error.code, -32600);
        assert_eq!(
            error.error.message,
            format!("{method} requires experimentalApi capability")
        );
    }
    Ok(())
}
