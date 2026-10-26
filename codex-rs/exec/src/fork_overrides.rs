//! Fork: feature overrides that only `codex exec` applies
//! (`specs/background-exec-notifications-2026-10-25.md`,
//! `specs/explorer-and-questions-2026-10-25.md`).

use codex_utils_cli::CliConfigOverrides;

/// Config override that keeps exit notifications off in `codex exec`.
const EXEC_EXIT_NOTIFICATIONS_DISABLED_OVERRIDE: &str = "features.exec_exit_notifications=false";

/// Parses the `-c` overrides of `codex exec`. The exec-only feature overrides come last,
/// so they win over the user's `-c` and `--enable` flags.
pub(crate) fn exec_cli_kv_overrides(
    mut config_overrides: CliConfigOverrides,
) -> Result<Vec<(String, codex_config::TomlValue)>, String> {
    // `codex exec` ends its session after the first turn, which kills background
    // processes, so it never offers `exec_command` exit notifications that could not arrive.
    config_overrides
        .raw_overrides
        .push(EXEC_EXIT_NOTIFICATIONS_DISABLED_OVERRIDE.to_string());
    let mut cli_kv_overrides = config_overrides.parse_overrides()?;
    // exec has no client that can answer questions, so Default mode never offers
    // `request_user_input` here (specs/explorer-and-questions-2026-10-25.md).
    cli_kv_overrides.push((
        "features.default_mode_request_user_input".to_string(),
        codex_config::TomlValue::Boolean(false),
    ));
    Ok(cli_kv_overrides)
}
