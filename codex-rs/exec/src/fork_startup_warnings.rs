//! Fork (live recheck 2026-10-27, R4): every startup warning of the configuration reached
//! `codex exec` twice, once as the `configWarning` the in-process app-server sends when it
//! starts and once as the `warning` the session sends for the same
//! `Config::startup_warnings` (upstream's "duplicate startup warnings" TODO in
//! `should_process_notification`). The TUI drops the repeat (`WarningDisplayState`); exec now
//! does the same, so such a warning, for example that worktree isolation is off under the
//! `collaboration` namespace, is reported once.

use codex_app_server_protocol::ServerNotification;

/// Whether `notification` is a session warning that repeats one of `startup_warnings`, which
/// exec has already reported as `configWarning`s.
pub(crate) fn repeats_startup_warning(
    notification: &ServerNotification,
    startup_warnings: &[String],
) -> bool {
    matches!(
        notification,
        ServerNotification::Warning(warning) if startup_warnings.contains(&warning.message)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::ConfigWarningNotification;
    use codex_app_server_protocol::WarningNotification;

    #[test]
    fn only_a_session_warning_that_repeats_a_startup_warning_is_dropped() {
        let startup = vec!["isolation is off".to_string()];
        let warning = |message: &str| {
            ServerNotification::Warning(WarningNotification {
                thread_id: Some("thread".to_string()),
                message: message.to_string(),
            })
        };
        assert!(repeats_startup_warning(
            &warning("isolation is off"),
            &startup
        ));
        assert!(!repeats_startup_warning(
            &warning("something else"),
            &startup
        ));
        assert!(!repeats_startup_warning(&warning("isolation is off"), &[]));
        let config_warning = ServerNotification::ConfigWarning(ConfigWarningNotification {
            summary: "isolation is off".to_string(),
            details: None,
            path: None,
            range: None,
        });
        assert!(!repeats_startup_warning(&config_warning, &startup));
    }
}
