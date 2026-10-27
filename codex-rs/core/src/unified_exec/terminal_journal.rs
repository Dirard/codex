//! Fork: a journal of the armed background terminals of a thread, so that a thread resumed
//! after its Codex process died tells the model which terminals it lost (combined live run
//! 2026-10-27, D6; `specs/background-exec-notifications-2026-10-25.md`, R2.11).
//!
//! A shutdown records a notice for every armed terminal it stops (R2.5). A process killed
//! with `kill -9` or a crash records nothing, and the model used to learn about the loss only
//! by polling a session ID that no longer existed, or never, if it waited for the exit
//! notification. The journal, `$CODEX_HOME/background_terminals/<thread id>.json`, lists the
//! armed terminals that have not reported yet; it is rewritten when one is armed or reports
//! and removed when the list is empty or a shutdown recorded its notices. A resumed thread
//! that finds a journal records one notice per terminal and removes the journal.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use codex_protocol::ThreadId;
use serde::Deserialize;
use serde::Serialize;
use tokio::time::Instant;
use tracing::warn;

use super::ExitNotifier;
use super::command_line;
use crate::context::BackgroundTerminalNotification;
use crate::context::ContextualUserFragment;
use crate::session::session::Session;

/// One journal write at a time, so an older list never overwrites a newer one.
static JOURNAL_WRITES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const JOURNAL_DIR: &str = "background_terminals";

/// An armed terminal that has not reported yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct JournaledTerminal {
    pub(crate) process_id: i32,
    pub(crate) command: String,
    /// Milliseconds since the Unix epoch when the process started.
    pub(crate) started_at_unix_ms: i64,
}

impl JournaledTerminal {
    fn from_notifier(notifier: &ExitNotifier, now: Instant, now_unix_ms: i64) -> Self {
        let running_for = now.saturating_duration_since(notifier.started_at());
        Self {
            process_id: notifier.process_id(),
            command: notifier.command().to_string(),
            started_at_unix_ms: now_unix_ms
                .saturating_sub(i64::try_from(running_for.as_millis()).unwrap_or(i64::MAX)),
        }
    }

    /// What the model learns about a terminal its previous Codex process lost.
    pub(crate) fn lost_notice(&self) -> String {
        format!(
            "Background terminal session {} was lost: the Codex process that ran it stopped without shutting down, so the process is gone and its exit status and unread output are unknown. Its session ID no longer works; start the command again if you still need it.\n{}",
            self.process_id,
            command_line(&self.command)
        )
    }
}

pub(crate) fn journal_path(codex_home: &Path, thread_id: ThreadId) -> PathBuf {
    codex_home
        .join(JOURNAL_DIR)
        .join(format!("{thread_id}.json"))
}

/// Replaces the journal with `terminals`, or removes it when there are none.
pub(crate) async fn write_journal(path: &Path, terminals: &[JournaledTerminal]) {
    let result = if terminals.is_empty() {
        match tokio::fs::remove_file(path).await {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        }
    } else {
        write_atomically(path, terminals).await
    };
    if let Err(err) = result {
        warn!(path = %path.display(), %err, "failed to update the background terminal journal");
    }
}

async fn write_atomically(path: &Path, terminals: &[JournaledTerminal]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let json = serde_json::to_vec(terminals).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, json).await?;
    tokio::fs::rename(&tmp, path).await
}

/// Reads and removes the journal; a missing or unreadable journal lists nothing.
pub(crate) async fn take_journal(path: &Path) -> Vec<JournaledTerminal> {
    let terminals = match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|err| {
            warn!(path = %path.display(), %err, "ignoring an unreadable background terminal journal");
            Vec::new()
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            warn!(path = %path.display(), %err, "failed to read the background terminal journal");
            Vec::new()
        }
    };
    write_journal(path, &[]).await;
    terminals
}

impl crate::unified_exec::UnifiedExecProcessManager {
    /// Armed terminals of the thread that have not reported yet.
    pub(crate) async fn journaled_terminals(&self) -> Vec<JournaledTerminal> {
        let notifiers: Vec<Arc<ExitNotifier>> = self
            .process_store
            .lock()
            .await
            .processes
            .values()
            .filter_map(|entry| entry.exit_notifier.clone())
            .filter(|notifier| notifier.is_pending())
            .collect();
        let now = Instant::now();
        let now_unix_ms = chrono::Utc::now().timestamp_millis();
        let mut terminals: Vec<JournaledTerminal> = notifiers
            .iter()
            .map(|notifier| JournaledTerminal::from_notifier(notifier, now, now_unix_ms))
            .collect();
        terminals.sort_by_key(|terminal| terminal.process_id);
        terminals
    }
}

impl Session {
    async fn terminal_journal_path(&self) -> Option<PathBuf> {
        if !self
            .features()
            .enabled(codex_features::Feature::ExecExitNotifications)
        {
            return None;
        }
        let config = self.get_config().await;
        (!config.ephemeral).then(|| journal_path(&config.codex_home, self.thread_id))
    }

    /// Rewrites the journal after a terminal was armed or reported.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "journal writes are ordered: an older list must not overwrite a newer one"
    )]
    pub(crate) async fn fork_sync_terminal_journal(&self) {
        let Some(path) = self.terminal_journal_path().await else {
            return;
        };
        let _write = JOURNAL_WRITES.lock().await;
        let terminals = self
            .services
            .unified_exec_manager
            .journaled_terminals()
            .await;
        write_journal(&path, &terminals).await;
    }

    /// A shutdown recorded what it owes the model: the journal has nothing left to tell.
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "journal writes are ordered: an older list must not overwrite a newer one"
    )]
    pub(crate) async fn fork_clear_terminal_journal(&self) {
        let Some(path) = self.terminal_journal_path().await else {
            return;
        };
        let _write = JOURNAL_WRITES.lock().await;
        write_journal(&path, &[]).await;
    }

    /// Fork hook of a resumed thread, after its history is rebuilt: terminals that the
    /// previous Codex process armed and never reported are gone with it; one notice each
    /// tells the model, and nothing waits for them (D6, R2.11).
    #[expect(
        clippy::await_holding_invalid_type,
        reason = "journal writes are ordered: an older list must not overwrite a newer one"
    )]
    pub(crate) async fn fork_record_lost_terminals(&self) {
        let config = self.get_config().await;
        if config.ephemeral {
            return;
        }
        let path = journal_path(&config.codex_home, self.thread_id);
        let lost = {
            let _write = JOURNAL_WRITES.lock().await;
            take_journal(&path).await
        };
        if lost.is_empty() {
            return;
        }
        let items: Vec<_> = lost
            .iter()
            .map(|terminal| {
                ContextualUserFragment::into(BackgroundTerminalNotification::new(
                    terminal.lost_notice(),
                ))
            })
            .collect();
        let turn_context = self.new_inject_items_context().await;
        self.record_conversation_items(&turn_context, turn_context.model_info(), &items)
            .await;
    }
}

#[cfg(test)]
#[path = "terminal_journal_tests.rs"]
mod tests;
