//! Runs of earlier processes: `interrupted` marking when a thread loads, the run index of
//! the thread, runs rebuilt from their journals and the source of `resume_from` (UC6, R6.3,
//! R6.4, R6.7, R6.8).

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use codex_features::Feature;
use codex_protocol::ThreadId;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_text;
use codex_workflow::JOURNAL_FILE;
use codex_workflow::Journal;
use codex_workflow::JournalCache;
use codex_workflow::JournalEvent;
use codex_workflow::JournalRecord;
use codex_workflow::RunSnapshot;
use codex_workflow::RunStatus;
use codex_workflow::RunTracker;
use codex_workflow::SCRIPT_FILE;
use codex_workflow::TokenCharge;
use codex_workflow::WorkflowMeta;
use codex_workflow::append_index;
use codex_workflow::index_entry;
use codex_workflow::is_valid_run_id;
use codex_workflow::journal_bounds;
use codex_workflow::parse_script;
use codex_workflow::read_index;
use codex_workflow::read_records;
use codex_workflow::read_result;
use tracing::warn;

use super::now_ms;
use super::registry::WorkflowRegistry;
use super::run::RestoredRun;
use super::run::WorkflowRun;
use crate::config::Config;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;

/// Error of runs interrupted because their process stopped (R6.4).
const STOPPED_ERROR: &str = "the Codex process stopped while the run was active";
/// Preview size of a restored run's result, as in the completion notification (R7.1).
const RESTORED_PREVIEW_TOKENS: usize = 2_000;

/// `$CODEX_HOME/workflows/<thread_id>`: the journals of a thread's runs (R6.1).
pub(super) fn thread_runs_dir(config: &Config, thread_id: ThreadId) -> PathBuf {
    config
        .codex_home
        .as_path()
        .join("workflows")
        .join(thread_id.to_string())
}

/// Fork: hook of `Session::record_initial_history` for a resumed thread (R6.4, R6.8).
/// Journals of the thread's runs that a stopped process left without `run_finished` get an
/// `interrupted` record; interrupted runs that no later run resumed are registered as
/// terminal runs, and the index entries of every finished run are kept as the thread's past
/// runs, so `workflow_status` and `thread/workflow/list` list them. No run restarts and no
/// turn starts.
pub(crate) async fn fork_restore_runs(session: &Session, turn_context: &TurnContext) {
    let config = &turn_context.config;
    if !config.features.enabled(Feature::Workflows) || config.ephemeral {
        return;
    }
    let dir = thread_runs_dir(config, session.thread_id);
    if !tokio::fs::try_exists(&dir).await.unwrap_or(false) {
        return;
    }
    let RestoredRuns { interrupted, past } =
        match tokio::task::spawn_blocking(move || restore_runs(&dir)).await {
            Ok(restored) => restored,
            Err(error) => {
                warn!(%error, "failed to restore workflow runs");
                return;
            }
        };
    if interrupted.is_empty() && past.is_empty() {
        return;
    }
    let registry = WorkflowRegistry::for_session(session);
    registry.set_past(past);
    for run in interrupted {
        registry.insert_restored(Arc::new(WorkflowRun::restored(
            run,
            registry.update_sender(),
        )));
    }
}

/// What the load of a thread finds in its workflow directory (R6.4, R6.8).
struct RestoredRuns {
    /// Interrupted runs that no other run resumed, rebuilt to be registered.
    interrupted: Vec<RestoredRun>,
    /// Index entries of the finished runs whose directories exist.
    past: Vec<RunSnapshot>,
}

/// R6.4, R6.8: read the thread directory `dir`. Runs its index lists are finished, and their
/// journals are not read. For other runs only the first and last journal records are read:
/// a stopped run is marked interrupted, and every run found finished or marked is rebuilt
/// once to add its index entry. Interrupted runs that no other run resumed are rebuilt to
/// be registered.
fn restore_runs(dir: &Path) -> RestoredRuns {
    let mut index = read_index(dir).unwrap_or_else(|error| {
        warn!(%error, "failed to read the workflow run index");
        Vec::new()
    });
    let indexed: HashSet<String> = index.iter().map(|entry| entry.run_id.clone()).collect();
    let mut resumed: HashSet<String> = index
        .iter()
        .filter_map(|entry| entry.resume.as_ref().map(|resume| resume.from.clone()))
        .collect();
    let mut present = HashSet::new();
    let mut loaded: HashMap<String, RestoredRun> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return RestoredRuns {
            interrupted: Vec::new(),
            past: Vec::new(),
        };
    };
    for entry in entries.flatten() {
        let run_id = entry.file_name().to_string_lossy().to_string();
        if !is_valid_run_id(&run_id) {
            continue;
        }
        present.insert(run_id.clone());
        if indexed.contains(&run_id) {
            continue;
        }
        let run_dir = entry.path();
        let Ok((first, last)) = journal_bounds(&run_dir.join(JOURNAL_FILE)) else {
            continue;
        };
        if let Some(JournalRecord {
            event:
                JournalEvent::RunStarted {
                    resume_from: Some(source),
                    ..
                },
            ..
        }) = &first
        {
            resumed.insert(source.clone());
        }
        let finished = match last.map(|record| record.event) {
            Some(JournalEvent::RunFinished { .. }) => true,
            _ => match mark_interrupted(&run_dir) {
                Ok(_) => true,
                Err(error) => {
                    warn!(%run_id, %error, "failed to mark a stopped workflow run as interrupted");
                    false
                }
            },
        };
        if !finished {
            continue;
        }
        // A journal of a version before the index, or of a process that stopped between
        // `run_finished` and the index line, is indexed now.
        match load_run(&run_id, &run_dir) {
            Ok(run) => {
                let snapshot = run.tracker.snapshot(/*include_agents*/ false);
                if let Err(error) = append_index(dir, &snapshot) {
                    warn!(%run_id, %error, "failed to index a workflow run");
                }
                index.push(index_entry(&snapshot));
                loaded.insert(run_id, run);
            }
            Err(error) => {
                warn!(%run_id, %error, "failed to rebuild a workflow run from its journal");
            }
        }
    }
    index.retain(|entry| present.contains(&entry.run_id));
    let interrupted = index
        .iter()
        .filter(|entry| entry.status == RunStatus::Interrupted && !resumed.contains(&entry.run_id))
        .filter_map(|entry| {
            let run_id = entry.run_id.as_str();
            match loaded.remove(run_id) {
                Some(run) => Some(run),
                None => load_run(run_id, &dir.join(run_id))
                    .map_err(|error| {
                        warn!(%run_id, %error, "failed to rebuild a workflow run from its journal");
                    })
                    .ok(),
            }
        })
        .collect();
    RestoredRuns {
        interrupted,
        past: index,
    }
}

/// Serializes the marking of stopped runs in this process: two lookups of one run, such as
/// `workflow_status` and `workflow {resume_from}` in one model response, would otherwise both
/// see a journal without `run_finished` and both append one. Other processes cannot reach a
/// thread's journals while this one owns the thread (its writer lock, R6.4 and Q5).
static MARKING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// R6.4: append `run_finished` with status `interrupted` to the journal of a run whose
/// process stopped, with the tokens its calls were charged. A journal that already ends with
/// `run_finished` is left as it is, so concurrent lookups mark a run once. Returns whether
/// this call appended the record.
fn mark_interrupted(run_dir: &Path) -> std::io::Result<bool> {
    let _marking = MARKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let records = read_records(run_dir)?;
    if matches!(
        records.last().map(|record| &record.event),
        Some(JournalEvent::RunFinished { .. })
    ) {
        return Ok(false);
    }
    let mut tokens = TokenCharge::default();
    for record in &records {
        if let JournalEvent::AgentFinished { tokens: call, .. } = &record.event {
            tokens.add(*call);
        }
    }
    Journal::open_existing(run_dir)?
        .append(JournalEvent::RunFinished {
            status: RunStatus::Interrupted.as_str().to_string(),
            result_bytes: 0,
            tokens,
            error: Some(STOPPED_ERROR.to_string()),
            finished_at_ms: Some(now_ms()),
        })
        .map(|_| true)
}

/// R6.7: rebuild a run of an earlier process from its directory. Its meta comes from
/// `script.js`, or, when that cannot be read, its name from the journal. The script and args
/// stay on disk; `resume_from` reads them when it needs them (R6.3).
fn load_run(run_id: &str, run_dir: &Path) -> std::io::Result<RestoredRun> {
    let records = read_records(run_dir)?;
    let script = std::fs::read_to_string(run_dir.join(SCRIPT_FILE)).unwrap_or_default();
    let meta = parse_script(&script)
        .map(|parsed| parsed.meta)
        .unwrap_or_else(|_| {
            let name = records
                .iter()
                .find_map(|record| match &record.event {
                    JournalEvent::RunStarted { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| run_id.to_string());
            WorkflowMeta {
                name,
                description: String::new(),
                phases: Vec::new(),
            }
        });
    let result_json = read_result(run_dir);
    let preview = result_json
        .as_deref()
        .map(|json| truncate_text(json, TruncationPolicy::Tokens(RESTORED_PREVIEW_TOKENS)));
    let modified_ms = std::fs::metadata(run_dir.join(JOURNAL_FILE))
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        });
    let tracker = RunTracker::replay(
        run_id.to_string(),
        meta.description.clone(),
        &meta.phases,
        &records,
        modified_ms,
        preview,
    );
    Ok(RestoredRun {
        id: run_id.to_string(),
        meta,
        tracker,
        journal: Journal::open_existing(run_dir)?,
        result_json,
    })
}

/// R6.7: the run `run_id` of this thread: a run of this process, or one of an earlier
/// process rebuilt from its journal and registered. A journal without `run_finished` belongs
/// to a process that stopped (R6.4) and is marked interrupted first; the run then gets its
/// index entry (R6.8).
pub(super) async fn find_or_restore_run(
    session: &Session,
    config: &Config,
    run_id: &str,
) -> Result<Arc<WorkflowRun>, String> {
    let unknown = || format!("unknown workflow run `{run_id}` in this thread");
    if let Some(run) = WorkflowRegistry::existing(session).and_then(|registry| registry.get(run_id))
    {
        return Ok(run);
    }
    if config.ephemeral || !is_valid_run_id(run_id) {
        return Err(unknown());
    }
    let run_dir = thread_runs_dir(config, session.thread_id).join(run_id);
    if !tokio::fs::try_exists(run_dir.join(JOURNAL_FILE))
        .await
        .unwrap_or(false)
    {
        return Err(unknown());
    }
    let owned_id = run_id.to_string();
    let restored = tokio::task::spawn_blocking(move || {
        let (_, last) = journal_bounds(&run_dir.join(JOURNAL_FILE))?;
        // A second lookup of the same run that raced this one finds it marked.
        let marked = !matches!(
            last.map(|record| record.event),
            Some(JournalEvent::RunFinished { .. })
        ) && mark_interrupted(&run_dir)?;
        let run = load_run(&owned_id, &run_dir)?;
        if marked
            && let Some(thread_dir) = run_dir.parent()
            && let Err(error) = append_index(thread_dir, &run.tracker.snapshot(false))
        {
            warn!(run_id = %owned_id, %error, "failed to index a workflow run");
        }
        Ok::<_, std::io::Error>(run)
    })
    .await
    .map_err(|error| format!("failed to read workflow run `{run_id}`: {error}"))?
    .map_err(|error| format!("failed to read workflow run `{run_id}`: {error}"))?;
    let registry = WorkflowRegistry::for_session(session);
    Ok(registry.insert_restored(Arc::new(WorkflowRun::restored(
        restored,
        registry.update_sender(),
    ))))
}

/// The run `resume_from` names and the cache of its successful answers (R6.3).
pub(super) struct ResumeSource {
    pub(super) run: Arc<WorkflowRun>,
    pub(super) cache: JournalCache,
}

/// R6.3: find the source run of `resume_from`; it must be terminal.
pub(super) async fn resume_source(
    session: &Session,
    config: &Config,
    run_id: &str,
) -> Result<ResumeSource, String> {
    let run = find_or_restore_run(session, config, run_id).await?;
    if !run.status().is_terminal() {
        return Err(format!(
            "workflow run `{run_id}` is still running; cancel it with workflow_cancel or wait for its notification before resuming it"
        ));
    }
    let records = run
        .journal_records()
        .map_err(|error| format!("failed to read the journal of `{run_id}`: {error}"))?;
    Ok(ResumeSource {
        cache: JournalCache::from_records(run_id, &records),
        run,
    })
}

#[cfg(test)]
#[path = "resume_tests.rs"]
mod tests;
