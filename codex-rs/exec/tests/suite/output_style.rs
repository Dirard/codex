//! Fork: `codex exec` selects an output style through `-c` without a flag of its own
//! (`specs/output-styles-2026-11-07.md`, UC6 R6.1).

use core_test_support::responses;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex_exec::test_codex_exec;
use wiremock::MockServer;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_overrides_select_the_output_style() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));
    let test = test_codex_exec();
    let server = MockServer::start().await;
    for styled in [true, false] {
        let id = if styled { "resp-styled" } else { "resp-plain" };
        let mock = responses::mount_sse_once(
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
        if styled {
            command
                .arg("-c")
                .arg("features.output_styles=true")
                .arg("-c")
                .arg("output_style=\"outcome-first\"");
        }
        let output = command.arg("say done").output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body = mock.single_request().body_json().to_string();
        assert_eq!(
            body.contains("Output style: `outcome-first`."),
            styled,
            "styled {styled}"
        );
    }
    Ok(())
}
