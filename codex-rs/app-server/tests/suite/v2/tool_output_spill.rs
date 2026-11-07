//! Fork: the saved full outputs of truncated tools go with a deleted thread and stay with an
//! archived one (`specs/tool-output-spill-2026-11-07.md`, R5.3, R5.4).

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::create_fake_rollout;
use app_test_support::create_mock_responses_server_repeating_assistant;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ThreadArchiveParams;
use codex_app_server_protocol::ThreadArchiveResponse;
use codex_app_server_protocol::ThreadDeleteParams;
use codex_app_server_protocol::ThreadDeleteResponse;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

/// R5.3, R5.4: `thread/archive` keeps a thread's saved outputs, `thread/delete` removes them,
/// and other threads keep theirs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn delete_removes_spill_files_and_archive_keeps_them() -> Result<()> {
    let server = create_mock_responses_server_repeating_assistant("Done").await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let thread = |name: &str, minute: u32| {
        create_fake_rollout(
            codex_home.path(),
            &format!("2025-01-01T00-0{minute}-00"),
            &format!("2025-01-01T00:0{minute}:00Z"),
            name,
            Some("mock_provider"),
            /*git_info*/ None,
        )
    };
    let archived = thread("archived", 0)?;
    let deleted = thread("deleted", 1)?;
    let other = thread("other", 2)?;
    let root = codex_home.path().join("tool_outputs");
    for thread_id in [&archived, &deleted, &other] {
        let dir = root.join(thread_id);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("0123456789abcdef.txt"), "full output")?;
    }
    let threads = || -> Result<Vec<String>> {
        let mut names: Vec<String> = std::fs::read_dir(&root)?
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
    let mut expected = vec![archived.clone(), deleted.clone(), other.clone()];
    expected.sort();
    assert_eq!(threads()?, expected);

    let _: ThreadDeleteResponse = app
        .request(|request_id| ClientRequest::ThreadDelete {
            request_id,
            params: ThreadDeleteParams {
                thread_id: deleted.clone(),
            },
        })
        .await?;
    let mut expected = vec![archived, other];
    expected.sort();
    assert_eq!(threads()?, expected);
    Ok(())
}
