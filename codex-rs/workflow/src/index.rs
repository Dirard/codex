//! Run index of a thread (UC6, R6.8).
//!
//! `runs.jsonl` sits in `$CODEX_HOME/workflows/<thread_id>/` next to the run directories and
//! holds one summary line per finished run, so a thread loaded by a later process lists its
//! earlier runs without reading their journals. Only the process that owns the thread writes
//! it (the thread's writer lock, R6.4). The index is a cache of the journals: a line may
//! repeat, and a run whose line is missing is indexed again from its journal at the next
//! load.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::sync::PoisonError;

use crate::is_valid_run_id;
use crate::meta::MAX_DESCRIPTION_CHARS;
use crate::meta::MAX_PHASE_TITLE_CHARS;
use crate::snapshot::MAX_LOG_CHARS;
use crate::snapshot::RunSnapshot;
use crate::snapshot::truncate_chars;

/// File name of the run index inside a thread's workflow directory.
pub const INDEX_FILE: &str = "runs.jsonl";

/// Serializes appends of this process, so concurrent lines never interleave.
static APPENDING: Mutex<()> = Mutex::new(());

/// R6.8: the index line of a finished run: its summary snapshot without the per-call list,
/// the log tail and the result preview, which `workflow_status` and `thread/workflow/read`
/// rebuild from the journal when a run is opened by id (R6.7).
pub fn index_entry(snapshot: &RunSnapshot) -> RunSnapshot {
    RunSnapshot {
        agent_list: Vec::new(),
        logs_tail: Vec::new(),
        result_preview: None,
        ..snapshot.clone()
    }
}

/// R6.8: append the index line of a finished run to the index of `thread_dir`.
pub fn append_index(thread_dir: &Path, snapshot: &RunSnapshot) -> io::Result<()> {
    let mut line = serde_json::to_string(&index_entry(snapshot)).map_err(io::Error::other)?;
    line.push('\n');
    let _appending = APPENDING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(thread_dir.join(INDEX_FILE))?;
    file.write_all(line.as_bytes())?;
    file.flush()
}

/// Longest run name of an index line, as `meta.name` allows.
const MAX_NAME_CHARS: usize = 64;
/// Most phases and null reasons an index line keeps.
const MAX_LISTED: usize = 256;
/// Longest null reason of an index line.
const MAX_REASON_CHARS: usize = 64;

/// R6.8, P3-11: an index line as the journal could have produced it, or `None`. Only a
/// finished run is indexed; the per-call list, log tail and result preview are not kept, and
/// free text is cut to the limits the run itself had, so a hand-edited or damaged line cannot
/// list a running run or carry more than a journal would.
fn checked_entry(entry: RunSnapshot) -> Option<RunSnapshot> {
    if !is_valid_run_id(&entry.run_id) || !entry.status.is_terminal() {
        return None;
    }
    let mut entry = index_entry(&entry);
    entry.name = truncate_chars(&entry.name, MAX_NAME_CHARS);
    entry.description = truncate_chars(&entry.description, MAX_DESCRIPTION_CHARS);
    entry.error = entry
        .error
        .map(|error| truncate_chars(&error, MAX_LOG_CHARS));
    entry.current_phase = entry
        .current_phase
        .map(|phase| truncate_chars(&phase, MAX_PHASE_TITLE_CHARS));
    entry.phases.truncate(MAX_LISTED);
    for phase in &mut entry.phases {
        phase.title = truncate_chars(&phase.title, MAX_PHASE_TITLE_CHARS);
    }
    entry.null_reasons = std::mem::take(&mut entry.null_reasons)
        .into_iter()
        .take(MAX_LISTED)
        .map(|(reason, count)| (truncate_chars(&reason, MAX_REASON_CHARS), count))
        .collect();
    if entry
        .resume
        .as_ref()
        .is_some_and(|resume| !is_valid_run_id(&resume.from))
    {
        entry.resume = None;
    }
    Some(entry)
}

/// R6.8: the indexed runs of `thread_dir` in the order they first appear, each with its last
/// line. Lines that do not parse, such as one a stopped process cut, lines with an invalid
/// run id and lines of a run that has not finished are skipped (P3-11); a missing index is
/// empty.
pub fn read_index(thread_dir: &Path) -> io::Result<Vec<RunSnapshot>> {
    let bytes = match std::fs::read(thread_dir.join(INDEX_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut entries: Vec<RunSnapshot> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        let Some(entry) = serde_json::from_slice::<RunSnapshot>(line)
            .ok()
            .and_then(checked_entry)
        else {
            continue;
        };
        match positions.get(&entry.run_id) {
            Some(&position) => entries[position] = entry,
            None => {
                positions.insert(entry.run_id.clone(), entries.len());
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
