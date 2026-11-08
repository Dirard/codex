//! Fork: real TUI startup refuses a background server of another build and runs the
//! session through the embedded app-server (`specs/daemon-version-guard-2026-11-08.md`).

use super::focus_palette::PtyCodex;
use super::focus_palette::write_test_config;
use anyhow::Result;
use anyhow::ensure;
use codex_app_server_protocol::JSONRPCMessage;
use futures::SinkExt;
use futures::StreamExt;
use serde_json::json;
use std::cell::Cell;
use tokio::net::UnixListener;
use tokio_tungstenite::tungstenite::Message;

thread_local! {
    static CHECK_DAEMON_BUILD: Cell<bool> = const { Cell::new(false) };
}

/// Whether a PTY Codex started on this thread may use a daemon of another build.
pub(super) fn allow_other_daemon_build() -> bool {
    !CHECK_DAEMON_BUILD.get()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_of_another_version_is_replaced_by_the_embedded_app_server() -> Result<()> {
    let cwd = codex_utils_cargo_bin::repo_root()?;
    let home = tempfile::tempdir_in("/tmp")?;
    write_test_config(home.path(), &cwd)?;
    let socket = codex_app_server_client::app_server_control_socket_path(home.path())?;
    std::fs::create_dir_all(socket.parent().unwrap())?;
    let listener = UnixListener::bind(socket.as_path())?;
    // Every connection gets an identity handshake: the guard's probe and the daemon
    // status query. Any other request means the session used this daemon.
    let server: tokio::task::JoinHandle<Result<()>> = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await?;
            let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                continue;
            };
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let JSONRPCMessage::Request(request) = serde_json::from_str(&text)? else {
                    continue;
                };
                anyhow::ensure!(
                    request.method == "initialize",
                    "the session used a daemon of another build: {}",
                    request.method
                );
                let response = json!({"id": request.id, "result": {
                    "userAgent": "daemon-test/0.0.1",
                    "codexHome": "/tmp/daemon-test",
                    "platformFamily": "unix",
                    "platformOs": "linux",
                    "experimentalProtocolDigest": "another-build",
                }});
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await?;
            }
        }
    });

    CHECK_DAEMON_BUILD.set(true);
    let started = PtyCodex::start(&cwd, home, &[]);
    CHECK_DAEMON_BUILD.set(false);
    let mut terminal = started?;
    terminal.wait_for_startup()?;
    terminal.wait_for_screen("warning")?;
    terminal.write_input(b"\x14")?;
    terminal.wait_for_screen("Running without the shared background server")?;
    let version = env!("CARGO_PKG_VERSION");
    ensure!(
        terminal.screen_contains(&format!(
            "it runs Codex 0.0.1, not this session's Codex {version}"
        )),
        "missing version mismatch warning:\n{}",
        terminal.screen_contents()
    );
    terminal.write_input(b"\x14")?;
    terminal.write_input(b"/status")?;
    terminal.wait_for_screen("show current session configuration")?;
    terminal.read_output(std::time::Duration::from_millis(/*millis*/ 200))?;
    terminal.write_input(b"\r")?;
    terminal.wait_for_screen("Model:")?;
    ensure!(!terminal.screen_contains("unix://"));
    if server.is_finished() {
        server.await??;
        anyhow::bail!("the fake daemon stopped");
    }
    server.abort();
    Ok(())
}
