//! Saved workflow scripts: `workflow(name, args)` in a script and the `workflow` tool's `name`
//! (UC12, R12.1–R12.4).

use std::path::PathBuf;

use codex_config::ConfigLayerSource;
use codex_workflow::JournalEvent;
use codex_workflow::SavedScript;
use codex_workflow::build_nested_source;
use codex_workflow::check_args_size;
use codex_workflow::find_saved_script;
use codex_workflow::parse_script;
use codex_workflow::sha256_hex;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_json::json;

use super::run::WorkflowRun;
use crate::config::Config;

/// R12.1: folders searched for `<name>.js`: `workflows` of every enabled project `.codex`
/// folder, nearest to the working directory first, then `$CODEX_HOME/workflows`. Layers of
/// untrusted projects are disabled and skipped, so their scripts never load.
pub(super) fn saved_script_dirs(config: &Config) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = config
        .config_layer_stack
        .layers_high_to_low()
        .filter_map(|layer| match &layer.name {
            ConfigLayerSource::Project { dot_codex_folder } => {
                Some(dot_codex_folder.as_path().join("workflows"))
            }
            _ => None,
        })
        .collect();
    dirs.push(config.codex_home.as_path().join("workflows"));
    dirs
}

/// UC12, R12.3, R12.4: find a saved script for the `workflow` tool's `name`.
pub(super) fn load_saved_script(config: &Config, name: &str) -> Result<SavedScript, String> {
    find_saved_script(&saved_script_dirs(config), name)
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WorkflowCall {
    Load {
        call: u64,
        name: String,
        args_json: String,
    },
    Finished {
        call: u64,
        name: String,
        ok: bool,
        error: Option<String>,
    },
}

/// UC12: serve the `__wf_workflow` host call of a run's cell. `load` finds, checks and
/// compiles a saved script into the function the prelude runs in the same cell (R12.1); a
/// failed lookup or check comes back as an error the prelude throws (R12.4). `finished`
/// records the nested run's outcome.
pub(super) fn handle_workflow_call(
    config: &Config,
    run: &WorkflowRun,
    input: Option<JsonValue>,
) -> Result<JsonValue, String> {
    let call: WorkflowCall = serde_json::from_value(input.unwrap_or(JsonValue::Null))
        .map_err(|error| format!("invalid workflow() call: {error}"))?;
    match call {
        WorkflowCall::Load {
            call,
            name,
            args_json,
        } => Ok(match load_nested(config, run, call, &name, &args_json) {
            Ok(loaded) => loaded,
            Err(message) => {
                run.log_line(&format!("workflow({name:?}) failed to load: {message}"));
                json!({ "ok": false, "error": "load", "message": format!("workflow({name:?}): {message}") })
            }
        }),
        WorkflowCall::Finished {
            call,
            name,
            ok,
            error,
        } => {
            if let Some(error) = &error {
                run.log_line(&format!("workflow {name} failed: {error}"));
            } else {
                run.log_line(&format!("workflow {name} finished"));
            }
            run.journal(JournalEvent::WorkflowFinished {
                call,
                name,
                ok,
                error: error.map(|error| codex_workflow::display_error(&error)),
            });
            Ok(json!({}))
        }
    }
}

fn load_nested(
    config: &Config,
    run: &WorkflowRun,
    call: u64,
    name: &str,
    args_json: &str,
) -> Result<JsonValue, String> {
    let saved = load_saved_script(config, name)?;
    check_args_size(args_json).map_err(|error| error.to_string())?;
    let parsed = parse_script(&saved.source)
        .map_err(|error| format!("saved script {}: {error}", saved.path.display()))?;
    run.journal(JournalEvent::WorkflowStarted {
        call,
        name: name.to_string(),
        path: saved.path.display().to_string(),
        script_sha256: sha256_hex(saved.source.as_bytes()),
        args_sha256: sha256_hex(args_json.as_bytes()),
    });
    run.log_line(&format!(
        "workflow {name} started ({})",
        saved.path.display()
    ));
    Ok(json!({
        "ok": true,
        "name": name,
        "source": build_nested_source(&parsed.body, name),
        "args_json": args_json,
    }))
}
