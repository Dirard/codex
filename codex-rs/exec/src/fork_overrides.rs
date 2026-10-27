//! Fork: feature overrides that only `codex exec` applies
//! (`specs/explorer-and-questions-2026-10-25.md`).
//!
//! `exec_exit_notifications` keeps its configured value: with it enabled, exec waits for
//! background terminal notifications before it exits
//! (`specs/background-exec-notifications-2026-10-25.md`, R1.4, UC7).

use codex_utils_cli::CliConfigOverrides;

/// Parses the `-c` overrides of `codex exec`. The exec-only feature overrides come last,
/// so they win over the user's `-c` and `--enable` flags.
pub(crate) fn exec_cli_kv_overrides(
    config_overrides: CliConfigOverrides,
) -> Result<Vec<(String, codex_config::TomlValue)>, String> {
    let mut cli_kv_overrides = config_overrides.parse_overrides()?;
    // exec has no client that can answer questions, so Default mode never offers
    // `request_user_input` here (specs/explorer-and-questions-2026-10-25.md).
    cli_kv_overrides.push((
        "features.default_mode_request_user_input".to_string(),
        codex_config::TomlValue::Boolean(false),
    ));
    // Nor any other question tool: exec rejects `requestUserInput` and never sends the later
    // user message that would answer `request_user_input_async` (combined live run D2).
    cli_kv_overrides.push((
        "tools.experimental_request_user_input.enabled".to_string(),
        codex_config::TomlValue::Boolean(false),
    ));
    Ok(cli_kv_overrides)
}
