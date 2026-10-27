//! Append-only JSONL journal of a workflow run (UC6, R6.1, R6.6).
//!
//! Persistent runs keep `script.js`, `args.json`, `journal.jsonl` and, once a run that returned
//! a value recorded `run_finished`, `result.json` in `$CODEX_HOME/workflows/<thread_id>/<run_id>/`;
//! ephemeral threads keep the journal in memory. Every record is flushed before the call that
//! produced it returns to the script. Fields added in stage 2 are optional, so journals of
//! earlier versions still read, and a line a stopped process cut costs only that line.

use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::BufRead;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::PoisonError;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

/// File names inside a run directory.
pub const SCRIPT_FILE: &str = "script.js";
pub const ARGS_FILE: &str = "args.json";
pub const JOURNAL_FILE: &str = "journal.jsonl";
pub const RESULT_FILE: &str = "result.json";

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

    /// What `self` adds on top of `earlier`, field by field and never negative.
    pub fn beyond(self, earlier: TokenCharge) -> TokenCharge {
        TokenCharge {
            charged: self.charged.saturating_sub(earlier.charged),
            cached: self.cached.saturating_sub(earlier.cached),
        }
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
        /// Unix time in milliseconds (stage 2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at_ms: Option<i64>,
        /// The run's token budget (stage 2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        budget_tokens: Option<u64>,
        /// Successful results in the journal of `resume_from` (stage 2, R6.5).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resume_source_results: Option<u64>,
    },
    Phase {
        title: String,
    },
    Log {
        message: String,
    },
    AgentStarted {
        /// Number of the `agent()` call in the script (stage 2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<u64>,
        key: String,
        ordinal: u64,
        label: Option<String>,
        phase: Option<String>,
        thread_id: String,
        agent_path: String,
    },
    AgentFinished {
        /// Number of the `agent()` call in the script (stage 2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        call: Option<u64>,
        key: String,
        ordinal: u64,
        /// The call's label and phase, so a call without `agent_started` (answered from a
        /// journal or settled before its spawn) keeps them in a rebuilt run (R6.7).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phase: Option<String>,
        ok: bool,
        value: Option<Value>,
        reason: Option<String>,
        tokens: TokenCharge,
        /// The value came from the journal of the run this one resumed (R6.3).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        cached: bool,
        /// Where an isolated agent left its changes (R3.8), so a resume that reuses the
        /// answer can report them (R6.3).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worktree: Option<String>,
        /// The same checkpoint as structured worktree details, which a call with
        /// `details: true` returns to the script (R3.13).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        worktree_details: Option<Value>,
    },
    /// A nested `workflow(name)` started (UC12).
    WorkflowStarted {
        call: u64,
        name: String,
        path: String,
        script_sha256: String,
        args_sha256: String,
    },
    /// A nested `workflow(name)` ended; `ok` is false when its body threw (UC12).
    WorkflowFinished {
        call: u64,
        name: String,
        ok: bool,
        error: Option<String>,
    },
    RunFinished {
        status: String,
        result_bytes: usize,
        tokens: TokenCharge,
        error: Option<String>,
        /// Unix time in milliseconds (stage 2).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at_ms: Option<i64>,
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
        std::fs::write(dir.join(SCRIPT_FILE), script)?;
        std::fs::write(dir.join(ARGS_FILE), args_json)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(JOURNAL_FILE))?;
        Ok(Self {
            dir: Some(dir.to_path_buf()),
            state: Mutex::new(JournalState {
                next_seq: 1,
                sink: Sink::File(file),
            }),
        })
    }

    /// R6.4, R6.7: open the journal of an earlier run to read it or append to it; record
    /// numbers continue after its last record.
    pub fn open_existing(dir: &Path) -> io::Result<Self> {
        let path = dir.join(JOURNAL_FILE);
        let (_, last) = journal_bounds(&path)?;
        let mut file = OpenOptions::new().read(true).append(true).open(&path)?;
        // A process that stopped mid-write leaves a cut last line; new records start on
        // their own line.
        let len = file.metadata()?.len();
        if len > 0 {
            let mut last_byte = [0_u8; 1];
            file.seek(SeekFrom::Start(len - 1))?;
            file.read_exact(&mut last_byte)?;
            if last_byte[0] != b'\n' {
                file.write_all(b"\n")?;
            }
        }
        Ok(Self {
            dir: Some(dir.to_path_buf()),
            state: Mutex::new(JournalState {
                next_seq: last.map_or(1, |record| record.seq + 1),
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
                    return read_records(dir);
                }
            }
        };
        Ok(lines
            .iter()
            .filter_map(|line| parse_record(line.as_bytes()))
            .collect())
    }

    /// Persist the run's result next to its journal, so a run rebuilt from disk can show it
    /// (R6.1, R6.7). A process that stops while writing leaves no cut file: the result goes
    /// to a temporary file that then replaces it. Like the journal it is not synced to disk;
    /// [`read_result`] rejects a file that is not JSON. Ephemeral runs keep the result in
    /// memory only.
    pub fn write_result(&self, result_json: &str) -> io::Result<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        let temporary = dir.join(format!("{RESULT_FILE}.tmp"));
        std::fs::write(&temporary, result_json)?;
        std::fs::rename(&temporary, dir.join(RESULT_FILE))
    }
}

/// Records of the journal in `dir`. Lines that do not parse are skipped: a process that
/// stopped mid-write leaves a cut line, possibly inside a multibyte character, and records
/// written after a restart follow it.
pub fn read_records(dir: &Path) -> io::Result<Vec<JournalRecord>> {
    let bytes = std::fs::read(dir.join(JOURNAL_FILE))?;
    Ok(bytes
        .split(|byte| *byte == b'\n')
        .filter_map(parse_record)
        .collect())
}

/// One journal line, or `None` for a blank, cut or foreign line.
fn parse_record(line: &[u8]) -> Option<JournalRecord> {
    if line.trim_ascii().is_empty() {
        return None;
    }
    serde_json::from_slice(line).ok()
}

/// R6.1, R6.7: the persisted result of the run in `dir`, if it is there and is JSON. A cut
/// file, which a crash before atomic writes could leave, is not a result.
pub fn read_result(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(RESULT_FILE)).ok()?;
    serde_json::from_str::<serde::de::IgnoredAny>(&text).ok()?;
    Some(text)
}

/// R6.4: the first and the last readable record of the journal at `path`, read without
/// loading the whole file: journals of large runs hold every agent answer.
pub fn journal_bounds(path: &Path) -> io::Result<(Option<JournalRecord>, Option<JournalRecord>)> {
    let mut file = File::open(path)?;
    let first = {
        let mut reader = io::BufReader::new(&mut file);
        let mut line = Vec::new();
        reader.read_until(b'\n', &mut line)?;
        parse_record(&line)
    };
    let len = file.metadata()?.len();
    let mut window: u64 = 64 * 1024;
    loop {
        let start = len.saturating_sub(window);
        file.seek(SeekFrom::Start(start))?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail)?;
        let mut lines: Vec<&[u8]> = tail
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.trim_ascii().is_empty())
            .collect();
        // The first line of a window that does not start at the beginning may be cut.
        if start > 0 && !lines.is_empty() {
            lines.remove(0);
        }
        let last = lines.iter().rev().find_map(|line| parse_record(line));
        if last.is_some() || start == 0 {
            return Ok((first, last));
        }
        window = window.saturating_mul(4);
    }
}

/// The script and args a persisted run started with (R6.3).
pub fn read_run_files(dir: &Path) -> io::Result<(String, String)> {
    Ok((
        std::fs::read_to_string(dir.join(SCRIPT_FILE))?,
        std::fs::read_to_string(dir.join(ARGS_FILE))?,
    ))
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
