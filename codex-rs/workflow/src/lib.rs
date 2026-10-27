//! Fork: pure logic of dynamic workflows (`specs/dynamic-workflow-2026-10-25.md`).
//!
//! The crate has no dependency on `codex-core`: it validates workflow scripts, generates
//! the Code Mode cell that runs them, checks and validates JSON schemas of `agent()` calls,
//! keys and journals calls, tracks run snapshots and renders the completion notification.

mod index;
mod journal;
mod key;
mod literal;
mod meta;
mod notification;
mod prelude;
mod resume;
mod saved;
mod scan;
mod schema;
mod snapshot;

pub use index::INDEX_FILE;
pub use index::append_index;
pub use index::index_entry;
pub use index::read_index;
pub use journal::ARGS_FILE;
pub use journal::JOURNAL_FILE;
pub use journal::Journal;
pub use journal::JournalEvent;
pub use journal::JournalRecord;
pub use journal::RESULT_FILE;
pub use journal::SCRIPT_FILE;
pub use journal::TokenCharge;
pub use journal::journal_bounds;
pub use journal::read_records;
pub use journal::read_result;
pub use journal::read_run_files;
pub use key::KEYED_OPTIONS;
pub use key::call_key;
pub use key::canonical_json;
pub use key::sha256_hex;
pub use meta::MAX_ARGS_BYTES;
pub use meta::MAX_PHASE_TITLE_CHARS;
pub use meta::MAX_PHASES;
pub use meta::MAX_SCRIPT_BYTES;
pub use meta::ParsedScript;
pub use meta::ScriptError;
pub use meta::ScriptRule;
pub use meta::WorkflowMeta;
pub use meta::check_args_size;
pub use meta::is_valid_name;
pub use meta::parse_script;
pub use notification::NOTIFICATION_CLOSE_TAG;
pub use notification::NOTIFICATION_OPEN_TAG;
pub use notification::NotificationSummary;
pub use notification::format_duration;
pub use notification::parse_notification_fragment;
pub use notification::render_notification;
pub use notification::render_notification_with_boundary;
pub use prelude::HOST_TOOLS;
pub use prelude::MAX_FAN_OUT;
pub use prelude::MAX_RESULT_BYTES;
pub use prelude::TOOL_AGENT;
pub use prelude::TOOL_BUDGET;
pub use prelude::TOOL_EVENT;
pub use prelude::TOOL_WORKFLOW;
pub use prelude::build_cell_source;
pub use prelude::build_nested_source;
pub use prelude::display_error;
pub use resume::CachedAnswer;
pub use resume::JournalCache;
pub use saved::SavedScript;
pub use saved::find_saved_script;
pub use schema::check_strict_compatible;
pub use schema::is_schema_format_rejection;
pub use schema::native_rejected_follow_up;
pub use schema::parse_final_message;
pub use schema::prompt_schema_suffix;
pub use schema::validate_value;
pub use snapshot::AgentCounts;
pub use snapshot::AgentEntry;
pub use snapshot::AgentState;
pub use snapshot::LOG_TAIL_LINES;
pub use snapshot::MAX_LOG_CHARS;
pub use snapshot::PhaseSnapshot;
pub use snapshot::ResumeStats;
pub use snapshot::RunSnapshot;
pub use snapshot::RunStatus;
pub use snapshot::RunTracker;
pub use snapshot::truncate_chars;

/// Developer instruction given to every workflow agent (UC3, R3.4).
pub const WORKFLOW_AGENT_INSTRUCTIONS: &str = "You are a workflow agent started by an orchestration script. Work autonomously: no user is available to answer questions. Your final message is returned verbatim to the script. When a JSON schema is required, your final message must be only that JSON object. Do not spawn other agents.";

/// Why an `agent()` call resolved to null (UC3, R3.7).
pub mod null_reason {
    pub const SPAWN_FAILED: &str = "spawn_failed";
    pub const ERRORED: &str = "errored";
    pub const INTERRUPTED: &str = "interrupted";
    pub const SHUTDOWN: &str = "shutdown";
    pub const TIMEOUT: &str = "timeout";
    pub const CANCELLED: &str = "cancelled";
    pub const BUDGET_EXHAUSTED: &str = "budget_exhausted";
    pub const AGENT_LIMIT: &str = "agent_limit";
    pub const CAPACITY: &str = "capacity";
    pub const SCHEMA_INVALID: &str = "schema_invalid";
    pub const ISOLATION_UNAVAILABLE: &str = "isolation_unavailable";
    pub const GUARDIAN_DENIED: &str = "guardian_denied";
}

/// Whether `id` has the form of a run id, `wf-` and eight lowercase hex digits. Ids from the
/// model are checked before they name a directory (R6.3, R6.7).
pub fn is_valid_run_id(id: &str) -> bool {
    id.strip_prefix("wf-").is_some_and(|hex| {
        hex.len() == 8
            && hex
                .chars()
                .all(|ch| ch.is_ascii_digit() || ('a'..='f').contains(&ch))
    })
}

/// Agent path segment of a workflow child: `wf_<run>_<seq>_<label>` with the label reduced
/// to lowercase `[a-z0-9_]` and at most 24 characters (UC3, R3.9).
pub fn child_agent_name(run_id: &str, seq: u64, label: Option<&str>) -> String {
    let run = run_id
        .strip_prefix("wf-")
        .unwrap_or(run_id)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    let mut name = format!("wf_{run}_{seq}");
    if let Some(label) = label {
        let mut sanitized = String::new();
        for ch in label.chars() {
            let ch = if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            };
            if !(ch == '_' && sanitized.ends_with('_')) {
                sanitized.push(ch);
            }
        }
        let sanitized: String = sanitized.trim_matches('_').chars().take(24).collect();
        let sanitized = sanitized.trim_end_matches('_');
        if !sanitized.is_empty() {
            name.push('_');
            name.push_str(sanitized);
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// R3.9: child names carry the run, the call number and a sanitized label.
    #[test]
    fn child_names_are_sanitized() {
        assert_eq!(
            child_agent_name("wf-1A2b3c4d", 7, Some("find src/a.py")),
            "wf_1a2b3c4d_7_find_src_a_py"
        );
        assert_eq!(
            child_agent_name(
                "wf-1a2b3c4d",
                1,
                Some("  Verify   src/very/long/path/name.rs:42 ")
            ),
            "wf_1a2b3c4d_1_verify_src_very_long_pat"
        );
        assert_eq!(
            child_agent_name("wf-1a2b3c4d", 2, Some("***")),
            "wf_1a2b3c4d_2"
        );
        assert_eq!(child_agent_name("wf-1a2b3c4d", 3, None), "wf_1a2b3c4d_3");
    }

    /// R6.3, R6.7: only `wf-` and eight lowercase hex digits name a run directory.
    #[test]
    fn run_ids_are_checked() {
        assert!(is_valid_run_id("wf-0a1b2c3d"));
        for bad in [
            "wf-0A1B2C3D",
            "wf-0a1b2c3",
            "wf-0a1b2c3d0",
            "wf-../../x",
            "0a1b2c3d",
            "",
        ] {
            assert!(!is_valid_run_id(bad), "{bad}");
        }
    }
}
