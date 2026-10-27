use super::*;
use crate::config::Config;
use crate::config::ConfigOverrides;
use codex_config::config_toml::WorkflowsToml;
use core_test_support::TempDirExt;
use pretty_assertions::assert_eq;

async fn load_overshoot(overshoot: f64) -> std::io::Result<Config> {
    let codex_home = tempfile::tempdir()?;
    Config::load_from_base_config_with_overrides(
        ConfigToml {
            workflows: Some(WorkflowsToml {
                budget_hard_overshoot: Some(overshoot),
                ..Default::default()
            }),
            ..Default::default()
        },
        ConfigOverrides::default(),
        codex_home.abs(),
    )
    .await
}

/// R5.4: `budget_hard_overshoot` below 1.0 counts as 1.0, values up to 100 are kept, and a
/// larger or non-finite value is a configuration error instead of a silent cap.
#[tokio::test]
async fn budget_hard_overshoot_above_the_limit_is_rejected() -> anyhow::Result<()> {
    for (overshoot, resolved) in [(0.5, 1.0), (1.2, 1.2), (100.0, 100.0)] {
        let config = load_overshoot(overshoot).await?;
        assert_eq!(config.workflows.budget_hard_overshoot, resolved);
    }
    for overshoot in [150.0, f64::INFINITY, f64::NAN] {
        let error = load_overshoot(overshoot)
            .await
            .expect_err("an overshoot above 100 is rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            error
                .to_string()
                .contains("workflows.budget_hard_overshoot must be a number from 1.0 to 100"),
            "{error}"
        );
    }
    Ok(())
}

const EXEC_EXIT_NOTIFICATIONS_TABLE: &str = "[features.exec_exit_notifications]\nenabled = false\nexec_wait_timeout_ms = 1500\noutput_watch_quiet_ms = 2500\noutput_watch_first_match_ms = 3500\n";

/// Fork-features-experimental R2.2: a boolean toggle of `exec_exit_notifications` (`--enable`,
/// `-c`, a boolean in a higher layer, `/experimental`) sets only `enabled` of its table and keeps
/// `exec_wait_timeout_ms` and the output watch windows. Red without the structured feature path.
#[tokio::test]
async fn exec_exit_notifications_table_survives_boolean_toggles() -> anyhow::Result<()> {
    assert!(codex_config::is_structured_feature_path(&[
        "features",
        "exec_exit_notifications"
    ]));
    let home = tempfile::tempdir()?;
    std::fs::write(
        home.path().join("config.toml"),
        EXEC_EXIT_NOTIFICATIONS_TABLE,
    )?;
    let config = crate::config::ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(vec![(
            "features.exec_exit_notifications".to_string(),
            toml::Value::Boolean(true),
        )])
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await?;
    assert!(config.features.enabled(Feature::ExecExitNotifications));
    assert_eq!(
        config.exec_exit_notifications,
        ExecExitNotificationsConfig {
            exec_wait_timeout_ms: 1500,
            output_watch_quiet_ms: 2500,
            output_watch_first_match_ms: 3500,
        }
    );
    Ok(())
}

/// Fork-features-experimental R2.2: turning `exec_exit_notifications` on and off with
/// `codex features enable/disable` writes only `enabled` and keeps the settings; toggling
/// `workflows` leaves its top-level `[workflows]` table alone.
#[test]
fn feature_toggles_keep_the_settings_tables() -> anyhow::Result<()> {
    let table_at = |text: &str, path: &[&str]| {
        path.iter().try_fold(
            toml::from_str::<toml::Value>(text).expect("parse config.toml"),
            |value, key| value.get(key).cloned(),
        )
    };
    let home = tempfile::tempdir()?;
    let config_path = home.path().join("config.toml");
    let workflows = "[workflows]\nmax_active_runs = 5\n";
    std::fs::write(
        &config_path,
        format!("{EXEC_EXIT_NOTIFICATIONS_TABLE}\n{workflows}"),
    )?;
    let settings = |enabled: bool| {
        EXEC_EXIT_NOTIFICATIONS_TABLE.replace("enabled = false", &format!("enabled = {enabled}"))
    };
    for enabled in [true, false] {
        crate::config::edit::ConfigEditsBuilder::new(home.path())
            .set_feature_enabled("exec_exit_notifications", enabled)
            .set_feature_enabled("workflows", enabled)
            .apply_blocking()?;
        let written = std::fs::read_to_string(&config_path)?;
        let path = ["features", "exec_exit_notifications"];
        assert_eq!(
            table_at(&written, &path),
            table_at(&settings(enabled), &path),
            "{written}"
        );
        assert_eq!(
            table_at(&written, &["workflows"]),
            table_at(workflows, &["workflows"]),
            "{written}"
        );
        assert_eq!(
            table_at(&written, &["features", "workflows"]),
            enabled.then_some(toml::Value::Boolean(true)),
            "{written}"
        );
    }
    Ok(())
}
