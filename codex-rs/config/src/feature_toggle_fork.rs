//! Fork: turning off a fork feature keeps its settings table
//! (`specs/explorer-and-questions-2026-10-25.md`, R1.6; review 2026-10-27, item 3).

use crate::is_structured_feature_path;

/// Fork features configured by a `[features.<key>]` table with settings besides `enabled`.
const FORK_FEATURE_TABLES: [&str; 3] = [
    "explorer_profile",
    "agent_worktree_isolation",
    "exec_exit_notifications",
];

/// Whether clearing the structured feature path `path` (as `/experimental` and
/// `codex features disable` do to turn a default-off feature off) should write
/// `enabled = false` instead: the path names a fork feature whose table holds settings besides
/// `enabled` (`table_keys`), and clearing it would lose them. Upstream features keep the upstream
/// rule (only `network_proxy` with broker credentials keeps its table).
pub fn fork_feature_clear_keeps_table<S: AsRef<str>, K: AsRef<str>>(
    path: &[S],
    table_keys: impl IntoIterator<Item = K>,
) -> bool {
    is_structured_feature_path(path)
        && path
            .last()
            .is_some_and(|feature| FORK_FEATURE_TABLES.contains(&feature.as_ref()))
        && table_keys.into_iter().any(|key| key.as_ref() != "enabled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fork_feature_tables_with_settings_are_kept() {
        let settings = ["enabled", "model"];
        assert!(fork_feature_clear_keeps_table(
            &["features", "explorer_profile"],
            settings
        ));
        assert!(fork_feature_clear_keeps_table(
            &["profiles", "work", "features", "explorer_profile"],
            settings
        ));
        assert!(!fork_feature_clear_keeps_table(
            &["features", "explorer_profile"],
            ["enabled"]
        ));
        assert!(!fork_feature_clear_keeps_table(
            &["features", "code_mode"],
            settings
        ));
        // `workflows` keeps its settings in the top-level `[workflows]` table, which a toggle of
        // `features.workflows` never touches; it has no feature table to keep.
        assert!(!fork_feature_clear_keeps_table(
            &["features", "workflows"],
            settings
        ));
    }

    /// Fork-features-experimental R2.2: every fork feature with a settings table is a structured
    /// feature, so a boolean toggle sets only its `enabled` and turning it off keeps the table.
    #[test]
    fn every_fork_feature_table_is_structured() {
        for feature in FORK_FEATURE_TABLES {
            assert!(
                is_structured_feature_path(&["features", feature]),
                "{feature}"
            );
            assert!(
                fork_feature_clear_keeps_table(&["features", feature], ["enabled", "setting"]),
                "{feature}"
            );
        }
    }
}
