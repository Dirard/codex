//! Append-only JSONL journal of a workflow run (UC6, R6.1, R6.6).
//!
//! Persistent runs keep `script.js`, `args.json` and `journal.jsonl` in
//! `$CODEX_HOME/workflows/<thread_id>/<run_id>/`; ephemeral threads keep the journal in
//! memory. Every record is flushed before the call that produced it returns to the script.

use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::PoisonError;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

/// Tokens charged to a run, with cached input reported separately (UC5, R5.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCharge {
    /// Non-cached input plus output tokens.
    pub charged: u64,
    pub cached: u64,
}

impl TokenCharge {
    pub fn add(&mut self, other: TokenCharge) {
        self.charged = self.charged.saturating_add(other.charged);
        self.cached = self.cached.saturating_add(other.cached);
    }
}

/// One journal event.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JournalEvent {
    RunStarted {
        run_id: String,
        name: String,
        script_sha256: String,
        args_sha256: String,
        parent_turn_id: String,
        resume_from: Option<String>,
    },
    Phase {
        title: String,
    },
    Log {
        message: String,
    },
    AgentStarted {
        key: String,
        ordinal: u64,
        label: Option<String>,
        phase: Option<String>,
        thread_id: String,
        agent_path: String,
    },
    AgentFinished {
        key: String,
        ordinal: u64,
        ok: bool,
        value: Option<Value>,
        reason: Option<String>,
        tokens: TokenCharge,
    },
    RunFinished {
        status: String,
        result_bytes: usize,
        tokens: TokenCharge,
        error: Option<String>,
    },
}

/// A journal event with its sequence number in the run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JournalRecord {
    pub seq: u64,
    #[serde(flatten)]
    pub event: JournalEvent,
}

enum Sink {
    File(File),
    Memory(Vec<String>),
}

struct JournalState {
    next_seq: u64,
    sink: Sink,
}

/// Writer of one run's journal.
pub struct Journal {
    dir: Option<PathBuf>,
    state: Mutex<JournalState>,
}

impl Journal {
    /// UC6: create the run directory, persist the script and args and open the journal.
    pub fn create(dir: &Path, script: &str, args_json: &str) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("script.js"), script)?;
        std::fs::write(dir.join("args.json"), args_json)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("journal.jsonl"))?;
        Ok(Self {
            dir: Some(dir.to_path_buf()),
            state: Mutex::new(JournalState {
                next_seq: 1,
                sink: Sink::File(file),
            }),
        })
    }

    /// An in-memory journal for ephemeral threads (R6.6).
    pub fn in_memory() -> Self {
        Self {
            dir: None,
            state: Mutex::new(JournalState {
                next_seq: 1,
                sink: Sink::Memory(Vec::new()),
            }),
        }
    }

    /// Directory with `script.js`, `args.json` and `journal.jsonl`, when persisted.
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Append one event and flush it. Returns the record's sequence number.
    pub fn append(&self, event: JournalEvent) -> io::Result<u64> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let seq = state.next_seq;
        let record = JournalRecord { seq, event };
        let line = serde_json::to_string(&record).map_err(io::Error::other)?;
        match &mut state.sink {
            Sink::File(file) => {
                file.write_all(line.as_bytes())?;
                file.write_all(b"\n")?;
                file.flush()?;
            }
            Sink::Memory(lines) => lines.push(line),
        }
        state.next_seq += 1;
        Ok(seq)
    }

    /// Records written so far, read back from the sink.
    pub fn records(&self) -> io::Result<Vec<JournalRecord>> {
        let lines = {
            let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            match &state.sink {
                Sink::Memory(lines) => lines.clone(),
                Sink::File(_) => {
                    let Some(dir) = &self.dir else {
                        return Ok(Vec::new());
                    };
                    std::fs::read_to_string(dir.join("journal.jsonl"))?
                        .lines()
                        .map(str::to_string)
                        .collect()
                }
            }
        };
        lines
            .iter()
            .map(|line| serde_json::from_str(line).map_err(io::Error::other))
            .collect()
    }
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
