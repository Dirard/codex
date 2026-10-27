//! Fork (live recheck 2026-10-27, R4): a startup warning of the configuration is reported
//! once per `codex exec` run, although it reaches exec both as a `configWarning` and as the
//! session's `warning`.

use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex_exec::test_codex_exec;
use wiremock::MockServer;

/// The start of the warning that worktree isolation is off under `collaboration`.
const RESERVED_NAMESPACE_WARNING: &str =
    "Subagent worktrees (`features.agent_worktree_isolation`) are off for this session";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_startup_warning_is_reported_once() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let test = test_codex_exec();
    std::fs::write(
        test.home_path().join("config.toml"),
        "[features.multi_agent_v2]\nenabled = true\ntool_namespace = \"collaboration\"\n\n[features.agent_worktree_isolation]\nenabled = true\n",
    )?;
    let server = MockServer::start().await;
    for (json, id) in [(true, "resp-json"), (false, "resp-human")] {
        responses::mount_sse_once(
            &server,
            responses::sse(vec![
                responses::ev_response_created(id),
                responses::ev_assistant_message(&format!("msg-{id}"), "done"),
                responses::ev_completed(id),
            ]),
        )
        .await;
        let mut command = test.cmd_with_server(&server);
        command.arg("--skip-git-repo-check");
        if json {
            command.arg("--json");
        }
        let output = command.arg("say done").output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            text.matches(RESERVED_NAMESPACE_WARNING).count(),
            1,
            "json {json}: {text}"
        );
    }
    Ok(())
}
