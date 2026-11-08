use anyhow::Result;
use codex_app_server_protocol::JSONRPCResponse;
use codex_uds::UnixListener;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::TempDir;
use tokio_tungstenite::accept_async;

use super::*;
use crate::client::read_message;

/// Serves one `initialize` handshake with `result` and checks the `initialized` notification.
async fn serve_initialize(listener: &mut UnixListener, result: serde_json::Value) -> Result<()> {
    let stream = listener.accept().await?;
    let mut websocket = accept_async(stream).await?;
    let JSONRPCMessage::Request(request) = read_message(&mut websocket).await? else {
        panic!("expected initialize request");
    };
    assert_eq!(request.method, "initialize");
    send_message(
        &mut websocket,
        &JSONRPCMessage::Response(JSONRPCResponse {
            id: request.id,
            result,
        }),
    )
    .await?;
    let JSONRPCMessage::Notification(initialized) = read_message(&mut websocket).await? else {
        panic!("expected initialized notification");
    };
    assert_eq!(initialized.method, "initialized");
    Ok(())
}

#[tokio::test]
async fn identity_reports_version_and_fork_digest_or_their_absence() -> Result<()> {
    let base = json!({
        "codexHome": "/tmp/codex-home",
        "platformFamily": "unix",
        "platformOs": "linux",
    });
    let with = |fields: serde_json::Value| {
        let mut result = base.clone();
        for (key, value) in fields.as_object().into_iter().flatten() {
            result[key] = value.clone();
        }
        result
    };
    for (name, result, expected) in [
        (
            "fork app-server",
            with(json!({
                "userAgent": "codex_cli_rs/0.167.0 (Ubuntu 26.4.0; x86_64) xterm",
                "stableProtocolDigest": "stable",
                "experimentalProtocolDigest": "fork-digest",
                "activeProtocolMode": "stable",
            })),
            AppServerIdentity {
                version: Some("0.167.0".to_string()),
                experimental_protocol_digest: Some("fork-digest".to_string()),
            },
        ),
        (
            "upstream app-server",
            with(json!({"userAgent": "codex_cli_rs/0.167.0"})),
            AppServerIdentity {
                version: Some("0.167.0".to_string()),
                experimental_protocol_digest: None,
            },
        ),
        (
            "user agent without version",
            with(json!({"userAgent": "codex_cli_rs"})),
            AppServerIdentity {
                version: None,
                experimental_protocol_digest: None,
            },
        ),
    ] {
        let dir = TempDir::new()?;
        let socket_path = dir.path().join("app-server.sock");
        let mut listener = UnixListener::bind(&socket_path).await?;
        let server = tokio::spawn(async move { serve_initialize(&mut listener, result).await });
        let identity = probe_app_server_identity(&socket_path).await?;
        server.await??;
        assert_eq!(identity, expected, "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn identity_probe_fails_without_a_server() -> Result<()> {
    let dir = TempDir::new()?;
    let missing = dir.path().join("missing.sock");
    assert!(probe_app_server_identity(&missing).await.is_err());
    Ok(())
}
