//! Runs of earlier processes: `interrupted` marking when a thread loads, runs rebuilt from
//! their journals and the source of `resume_from` (UC6, R6.3, R6.4, R6.7).

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
use codex_workflow::RESULT_FILE;
use codex_workflow::RunStatus;
use codex_workflow::RunTracker;
use codex_workflow::TokenCharge;
use codex_workflow::WorkflowMeta;
use codex_workflow::is_valid_run_id;
use codex_workflow::journal_bounds;
use codex_workflow::parse_script;
use codex_workflow::read_records;
use codex_workflow::read_run_files;
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

/// Fork: hook of `Session::record_initial_history` for a resumed thread (R6.4). Journals of
/// the thread's runs that a stopped process left without `run_finished` get an
/// `interrupted` record; interrupted runs that no later run resumed are registered as
/// terminal runs, so `workflow_status` lists them. No run restarts and no turn starts.
pub(crate) async fn fork_restore_runs(session: &Session, turn_context: &TurnContext) {
    let config = &turn_context.config;
    if !config.features.enabled(Feature::Workflows) || config.ephemeral {
        return;
    }
    let dir = thread_runs_dir(config, session.thread_id);
    if !tokio::fs::try_exists(&dir).await.unwrap_or(false) {
        return;
    }
    let restored = match tokio::task::spawn_blocking(move || restore_interrupted_runs(&dir)).await {
        Ok(restored) => restored,
        Err(error) => {
            warn!(%error, "failed to restore workflow runs");
            return;
        }
    };
    if restored.is_empty() {
        return;
    }
    let registry = WorkflowRegistry::for_session(session);
    for run in restored {
        registry.insert_restored(Arc::new(WorkflowRun::restored(
            run,
            registry.update_sender(),
        )));
    }
}

/// R6.4: mark stopped runs of the thread directory `dir` as interrupted and rebuild the
/// interrupted runs that no other run resumed.
fn restore_interrupted_runs(dir: &Path) -> Vec<RestoredRun> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut interrupted = Vec::new();
    let mut resumed = HashSet::new();
    for entry in entries.flatten() {
        let run_id = entry.file_name().to_string_lossy().to_string();
        if !is_valid_run_id(&run_id) {
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
        let ended_interrupted = match last.map(|record| record.event) {
            Some(JournalEvent::RunFinished { status, .. }) => {
                status == RunStatus::Interrupted.as_str()
            }
            _ => match mark_interrupted(&run_dir) {
                Ok(()) => true,
                Err(error) => {
                    warn!(%run_id, %error, "failed to mark a stopped workflow run as interrupted");
                    false
                }
            },
        };
        if ended_interrupted {
            interrupted.push((run_id, run_dir));
        }
    }
    interrupted
        .into_iter()
        .filter(|(run_id, _)| !resumed.contains(run_id))
        .filter_map(|(run_id, run_dir)| match load_run(&run_id, &run_dir) {
            Ok(run) => Some(run),
            Err(error) => {
                warn!(%run_id, %error, "failed to rebuild a workflow run from its journal");
                None
            }
        })
        .collect()
}

/// R6.4: append `run_finished` with status `interrupted` to the journal of a run whose
/// process stopped, with the tokens its calls were charged.
fn mark_interrupted(run_dir: &Path) -> std::io::Result<()> {
    let records = read_records(run_dir)?;
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
        .map(|_| ())
}

/// R6.7: rebuild a run of an earlier process from its directory. Its meta comes from
/// `script.js`, or, when that cannot be read, its name from the journal.
fn load_run(run_id: &str, run_dir: &Path) -> std::io::Result<RestoredRun> {
    let records = read_records(run_dir)?;
    let (script, args_json) =
        read_run_files(run_dir).unwrap_or_else(|_| (String::new(), "null".to_string()));
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
    let result_json = std::fs::read_to_string(run_dir.join(RESULT_FILE)).ok();
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
        script,
        args_json,
        result_json,
    })
}

/// R6.7: the run `run_id` of this thread: a run of this process, or one of an earlier
/// process rebuilt from its journal and registered. A journal without `run_finished` belongs
/// to a process that stopped (R6.4) and is marked interrupted first.
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
        if !matches!(
            last.map(|record| record.event),
            Some(JournalEvent::RunFinished { .. })
        ) {
            mark_interrupted(&run_dir)?;
        }
        load_run(&owned_id, &run_dir)
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
