//! Fork: the fork's features that are on by default
//! (`specs/fork-features-default-on-2026-11-07.md`).

use std::time::Duration;

use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_app_server_protocol::ConfigBatchWriteParams;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::ConfigWriteResponse;
use codex_app_server_protocol::ExperimentalFeature;
use codex_app_server_protocol::ExperimentalFeatureListParams;
use codex_app_server_protocol::ExperimentalFeatureListResponse;
use codex_app_server_protocol::ExperimentalFeatureStage;
use codex_app_server_protocol::MergeStrategy;
use codex_features::Feature;
use pretty_assertions::assert_eq;
use serde::de::DeserializeOwned;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// The four features that are on by default, and `default_mode_request_user_input`, which is not.
const FORK_FEATURES: [Feature; 5] = [
    Feature::ExecExitNotifications,
    Feature::ExplorerProfile,
    Feature::AgentWorktreeIsolation,
    Feature::Workflows,
    Feature::DefaultModeRequestUserInput,
];

async fn read_response<T: DeserializeOwned>(mcp: &mut TestAppServer, request_id: i64) -> Result<T> {
    timeout(DEFAULT_TIMEOUT, mcp.read_response(request_id)).await?
}

async fn list_features(mcp: &mut TestAppServer) -> Result<Vec<ExperimentalFeature>> {
    let request_id = mcp
        .send_experimental_feature_list_request(ExperimentalFeatureListParams::default())
        .await?;
    let response = read_response::<ExperimentalFeatureListResponse>(mcp, request_id).await?;
    Ok(response.data)
}

fn fork_feature(features: &[ExperimentalFeature], feature: Feature) -> &ExperimentalFeature {
    features
        .iter()
        .find(|entry| entry.name == feature.key())
        .unwrap_or_else(|| panic!("{feature:?} is listed"))
}

/// R2.1: with a `config.toml` that names no feature, `experimentalFeature/list` reports the four
/// features as `beta`, on and on by default (subagent worktrees not on Windows), and
/// `default_mode_request_user_input` as off by default. Without the upstream-test hook of UC6.
#[tokio::test]
async fn experimental_feature_list_reports_the_fork_defaults() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized_with_timeout(DEFAULT_TIMEOUT)
        .await?;

    let features = list_features(&mut mcp).await?;
    for feature in FORK_FEATURES {
        let entry = fork_feature(&features, feature);
        let on_by_default = feature.default_enabled();
        assert_eq!(
            on_by_default,
            match feature {
                Feature::DefaultModeRequestUserInput => false,
                Feature::AgentWorktreeIsolation => !cfg!(windows),
                _ => true,
            },
            "{feature:?}"
        );
        assert_eq!(entry.stage, ExperimentalFeatureStage::Beta, "{feature:?}");
        assert_eq!(entry.default_enabled, on_by_default, "{feature:?}");
        assert_eq!(entry.enabled, on_by_default, "{feature:?}");
    }
    Ok(())
}

/// R6.1: `MockResponsesConfig::write` keeps every default-on fork feature off unless the test
/// sets it, while `experimentalFeature/list` still reports the production defaults. R2.4, R2.5: a
/// client's `config/batchWrite` of `false` turns a default-on feature off and writes the value.
#[tokio::test]
async fn mock_responses_config_keeps_fork_features_off_unless_set() -> Result<()> {
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new("http://127.0.0.1:9")
        .enable_feature(Feature::Workflows)
        .write(codex_home.path())?;
    let mut mcp = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .without_auto_env()
        .build_initialized_with_timeout(DEFAULT_TIMEOUT)
        .await?;

    let features = list_features(&mut mcp).await?;
    for feature in codex_features::fork_default_on_features() {
        let entry = fork_feature(&features, feature);
        assert!(entry.default_enabled, "{feature:?}");
        assert_eq!(entry.enabled, feature == Feature::Workflows, "{feature:?}");
    }

    let request_id = mcp
        .send_config_batch_write_request(ConfigBatchWriteParams {
            edits: vec![ConfigEdit {
                key_path: "features.workflows".to_string(),
                value: json!(false),
                merge_strategy: MergeStrategy::Replace,
            }],
            file_path: None,
            expected_version: None,
            reload_user_config: false,
        })
        .await?;
    read_response::<ConfigWriteResponse>(&mut mcp, request_id).await?;
    let written = std::fs::read_to_string(codex_home.path().join("config.toml"))?;
    let written: toml::Value = toml::from_str(&written)?;
    assert_eq!(
        written["features"]["workflows"],
        toml::Value::Boolean(false)
    );
    let features = list_features(&mut mcp).await?;
    assert!(!fork_feature(&features, Feature::Workflows).enabled);
    Ok(())
}
