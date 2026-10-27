use super::*;
use pretty_assertions::assert_eq;

fn terminal(process_id: i32, command: &str) -> JournaledTerminal {
    JournaledTerminal {
        process_id,
        command: command.to_string(),
        started_at_unix_ms: 1_700_000_000_000,
    }
}

/// D6: the journal lists the armed terminals, an empty list removes it, and a resume takes it
/// once.
#[tokio::test]
async fn the_journal_is_rewritten_removed_and_taken_once() {
    let home = tempfile::tempdir().expect("tempdir");
    let thread_id = ThreadId::new();
    let path = journal_path(home.path(), thread_id);
    assert_eq!(
        path,
        home.path()
            .join("background_terminals")
            .join(format!("{thread_id}.json"))
    );

    write_journal(&path, &[terminal(7, "./build.sh")]).await;
    write_journal(
        &path,
        &[terminal(7, "./build.sh"), terminal(9, "npm run dev")],
    )
    .await;
    assert!(path.exists());
    write_journal(&path, &[]).await;
    assert!(!path.exists(), "an empty journal is removed");
    write_journal(&path, &[]).await;

    write_journal(&path, &[terminal(9, "npm run dev")]).await;
    assert_eq!(take_journal(&path).await, vec![terminal(9, "npm run dev")]);
    assert!(!path.exists());
    assert_eq!(take_journal(&path).await, Vec::new());

    std::fs::write(&path, b"not json").expect("write");
    assert_eq!(take_journal(&path).await, Vec::new());
    assert!(!path.exists(), "an unreadable journal is dropped");
}

#[test]
fn a_lost_terminal_says_what_happened() {
    assert_eq!(
        terminal(9809, "./build.sh <x>").lost_notice(),
        "Background terminal session 9809 was lost: the Codex process that ran it stopped without shutting down, so the process is gone and its exit status and unread output are unknown. Its session ID no longer works; start the command again if you still need it.\nCommand: ./build.sh &lt;x>"
    );
}
