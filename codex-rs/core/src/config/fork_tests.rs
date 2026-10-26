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
