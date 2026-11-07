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
            workflows: Some(Box::new(WorkflowsToml {
                budget_hard_overshoot: Some(overshoot),
                ..Default::default()
            })),
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

/// Workflows ultra mode: `[workflows] ultra_mode` is on by default, also with a
/// `[workflows]` table that does not name it, and `false` turns it off.
#[tokio::test]
async fn workflows_ultra_mode_defaults_to_true() -> anyhow::Result<()> {
    assert!(WorkflowsConfig::default().ultra_mode);
    for (workflows, expected) in [
        (None, true),
        (Some(WorkflowsToml::default()), true),
        (
            Some(WorkflowsToml {
                ultra_mode: Some(false),
                ..Default::default()
            }),
            false,
        ),
    ] {
        let codex_home = tempfile::tempdir()?;
        let config = Config::load_from_base_config_with_overrides(
            ConfigToml {
                workflows: workflows.map(Box::new),
                ..Default::default()
            },
            ConfigOverrides::default(),
            codex_home.abs(),
        )
        .await?;
        assert_eq!(config.workflows.ultra_mode, expected);
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
        *config.exec_exit_notifications,
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

/// Scheduled-wakeups R1.1, R1.5: defaults apply without a table and with a table that only sets
/// `enabled`; a table with settings survives a boolean toggle; each value outside its range is
/// a configuration error that names the key and the range.
#[tokio::test]
async fn scheduled_wakeups_settings_resolve_and_reject_out_of_range_values() -> anyhow::Result<()> {
    use codex_features::ScheduledWakeupsConfigToml;

    let resolve = |config: ScheduledWakeupsConfigToml| {
        let mut features = FeaturesToml::default();
        features.scheduled_wakeups = Some(FeatureToml::Config(config));
        ScheduledWakeupsConfig::from_features_toml(Some(&features))
    };
    assert_eq!(
        ScheduledWakeupsConfig::from_features_toml(None)?,
        ScheduledWakeupsConfig {
            min_delay_seconds: 60,
            max_delay_seconds: 3600,
            max_wakeups: 50,
            max_duration_minutes: 720,
            exec_wait_timeout_ms: 3_600_000,
        }
    );
    assert_eq!(
        resolve(ScheduledWakeupsConfigToml {
            enabled: Some(true),
            ..Default::default()
        })?,
        ScheduledWakeupsConfig::default()
    );
    for (config, key, range) in [
        (
            ScheduledWakeupsConfigToml {
                min_delay_seconds: Some(0),
                ..Default::default()
            },
            "min_delay_seconds",
            "from 1 to max_delay_seconds (3600)",
        ),
        (
            ScheduledWakeupsConfigToml {
                min_delay_seconds: Some(120),
                max_delay_seconds: Some(60),
                ..Default::default()
            },
            "min_delay_seconds",
            "from 1 to max_delay_seconds (60)",
        ),
        (
            ScheduledWakeupsConfigToml {
                max_delay_seconds: Some(86_401),
                ..Default::default()
            },
            "max_delay_seconds",
            "from min_delay_seconds (60) to 86400",
        ),
        (
            ScheduledWakeupsConfigToml {
                max_wakeups: Some(0),
                ..Default::default()
            },
            "max_wakeups",
            "from 1 to 1000",
        ),
        (
            ScheduledWakeupsConfigToml {
                max_wakeups: Some(1001),
                ..Default::default()
            },
            "max_wakeups",
            "from 1 to 1000",
        ),
        (
            ScheduledWakeupsConfigToml {
                max_duration_minutes: Some(10_081),
                ..Default::default()
            },
            "max_duration_minutes",
            "from 1 to 10080",
        ),
        (
            ScheduledWakeupsConfigToml {
                exec_wait_timeout_ms: Some(999),
                ..Default::default()
            },
            "exec_wait_timeout_ms",
            "at least 1000",
        ),
    ] {
        let error = resolve(config).expect_err("an out-of-range value is rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            format!("features.scheduled_wakeups.{key} must be {range}")
        );
    }

    let home = tempfile::tempdir()?;
    std::fs::write(
        home.path().join("config.toml"),
        "[features.scheduled_wakeups]\nenabled = false\nmin_delay_seconds = 5\n",
    )?;
    let config = crate::config::ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(vec![(
            "features.scheduled_wakeups".to_string(),
            toml::Value::Boolean(true),
        )])
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await?;
    assert!(config.features.enabled(Feature::ScheduledWakeups));
    assert_eq!(config.scheduled_wakeups.min_delay_seconds, 5);
    Ok(())
}

/// Loads `config.toml` with `text` and the CLI overrides `overrides` from a fresh home.
async fn load_config_toml(
    text: &str,
    overrides: Vec<(String, toml::Value)>,
) -> std::io::Result<Config> {
    let home = tempfile::tempdir()?;
    std::fs::write(home.path().join("config.toml"), text)?;
    crate::config::ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(overrides)
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await
}

/// Tool-output-spill R6.2, R6.3: the limits default to 8 MiB per file, 256 MiB per thread,
/// 2 GiB in all and 7 days; configured values are kept, and each limit below its minimum is a
/// configuration error that names its key.
#[tokio::test]
async fn tool_output_spill_limits_resolve_and_reject_small_values() -> anyhow::Result<()> {
    let config = load_config_toml("", Vec::new()).await?;
    assert!(!config.features.enabled(Feature::ToolOutputSpill));
    assert_eq!(
        *config.tool_output_spill,
        ToolOutputSpillConfig {
            max_file_bytes: 8 * 1024 * 1024,
            max_thread_bytes: 256 * 1024 * 1024,
            max_total_bytes: 2 * 1024 * 1024 * 1024,
            max_age_days: 7,
        }
    );

    let config = load_config_toml(
        "[features.tool_output_spill]\nenabled = true\nmax_file_bytes = 65536\nmax_thread_bytes = 131072\nmax_total_bytes = 262144\nmax_age_days = 2\n",
        Vec::new(),
    )
    .await?;
    assert!(config.features.enabled(Feature::ToolOutputSpill));
    assert_eq!(
        *config.tool_output_spill,
        ToolOutputSpillConfig {
            max_file_bytes: 65_536,
            max_thread_bytes: 131_072,
            max_total_bytes: 262_144,
            max_age_days: 2,
        }
    );

    for (table, message) in [
        (
            "max_file_bytes = 65535",
            "features.tool_output_spill.max_file_bytes must be at least 65536",
        ),
        (
            "max_file_bytes = 131072\nmax_thread_bytes = 65536",
            "features.tool_output_spill.max_thread_bytes must be at least max_file_bytes (131072)",
        ),
        (
            "max_thread_bytes = 9000000\nmax_total_bytes = 8388608",
            "features.tool_output_spill.max_total_bytes must be at least max_thread_bytes (9000000)",
        ),
        (
            "max_age_days = 0",
            "features.tool_output_spill.max_age_days must be at least 1",
        ),
    ] {
        let error = load_config_toml(
            &format!("[features.tool_output_spill]\nenabled = true\n{table}\n"),
            Vec::new(),
        )
        .await
        .expect_err(table);
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{table}");
        assert!(error.to_string().contains(message), "{error}");
    }
    Ok(())
}

const TOOL_OUTPUT_SPILL_TABLE: &str =
    "[features.tool_output_spill]\nenabled = false\nmax_file_bytes = 131072\nmax_age_days = 3\n";

/// Tool-output-spill R6.4: turning `tool_output_spill` on from the command line or with
/// `codex features enable/disable` (as `/experimental` does) changes only `enabled` and keeps
/// the limits of its table.
#[tokio::test]
async fn tool_output_spill_table_survives_toggles() -> anyhow::Result<()> {
    assert!(codex_config::is_structured_feature_path(&[
        "features",
        "tool_output_spill"
    ]));
    let config = load_config_toml(
        TOOL_OUTPUT_SPILL_TABLE,
        vec![(
            "features.tool_output_spill".to_string(),
            toml::Value::Boolean(true),
        )],
    )
    .await?;
    assert!(config.features.enabled(Feature::ToolOutputSpill));
    assert_eq!(config.tool_output_spill.max_file_bytes, 131_072);
    assert_eq!(config.tool_output_spill.max_age_days, 3);

    let home = tempfile::tempdir()?;
    let config_path = home.path().join("config.toml");
    std::fs::write(&config_path, TOOL_OUTPUT_SPILL_TABLE)?;
    for enabled in [true, false] {
        crate::config::edit::ConfigEditsBuilder::new(home.path())
            .set_feature_enabled("tool_output_spill", enabled)
            .apply_blocking()?;
        let written: toml::Value = toml::from_str(&std::fs::read_to_string(&config_path)?)?;
        let expected: toml::Value = toml::from_str(
            &TOOL_OUTPUT_SPILL_TABLE.replace("enabled = false", &format!("enabled = {enabled}")),
        )?;
        assert_eq!(written, expected);
    }
    Ok(())
}
