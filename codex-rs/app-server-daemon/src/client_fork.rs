//! Fork: reports which build serves the daemon control socket. A fork client runs turns
//! only through its own build, and the version number alone cannot tell a fork release
//! from the upstream release of the same number (`specs/daemon-version-guard-2026-11-08.md`).

use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use tokio::time::timeout;

use super::CONTROL_SOCKET_RESPONSE_TIMEOUT;
use super::connect;
use super::initialize;
use super::parse_version_from_user_agent;
use super::send_message;

/// The build of the app-server behind a control socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppServerIdentity {
    /// Version from the server's `userAgent`; `None` when it cannot be parsed.
    pub version: Option<String>,
    /// Fork protocol digest from `initialize`; `None` for an upstream app-server.
    pub experimental_protocol_digest: Option<String>,
}

/// UC1 "initialize over the control socket → read version and fork protocol digest":
/// one passive `initialize` handshake, like the daemon's own readiness probe.
pub async fn probe_app_server_identity(socket_path: &Path) -> Result<AppServerIdentity> {
    timeout(CONTROL_SOCKET_RESPONSE_TIMEOUT, async {
        let mut websocket = connect(socket_path).await?;
        let info = initialize(&mut websocket, /*experimental_api*/ false).await?;
        let initialized = JSONRPCMessage::Notification(JSONRPCNotification {
            method: "initialized".to_string(),
            params: None,
        });
        send_message(&mut websocket, &initialized)
            .await
            .context("failed to send initialized notification")?;
        websocket.close(None).await.ok();
        Ok(AppServerIdentity {
            version: parse_version_from_user_agent(&info.user_agent).ok(),
            experimental_protocol_digest: info.experimental_protocol_digest,
        })
    })
    .await
    .with_context(|| {
        format!(
            "timed out identifying the app-server on control socket {}",
            socket_path.display()
        )
    })?
}

#[cfg(all(test, unix))]
#[path = "client_fork_tests.rs"]
mod tests;
