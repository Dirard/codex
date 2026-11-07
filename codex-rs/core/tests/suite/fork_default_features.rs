//! Fork: the fork's features that are on by default
//! (`specs/fork-features-default-on-2026-11-07.md`).
//!
//! These tests load configurations with the production defaults: through `ConfigBuilder`
//! directly, or through `TestCodexBuilder` with
//! [`core_test_support::fork_defaults::use_production_feature_defaults`], not through the
//! upstream-test defaults of `load_default_config_for_test` (UC6), which they also check.

use anyhow::Result;
use codex_config::LoaderOverrides;
use codex_core::config::Config;
use codex_core::config::ConfigBuilder;
use codex_features::Feature;
use codex_utils_absolute_path::AbsolutePathBuf;
use core_test_support::context_snapshot;
use core_test_support::context_snapshot::ContextSnapshotOptions;
use core_test_support::fork_defaults::use_production_feature_defaults;
use core_test_support::load_default_config_for_test;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use pretty_assertions::assert_eq;
use serde_json::Value;
use tempfile::TempDir;

/// The four fork features that are on by default (subagent worktrees not on Windows).
const DEFAULT_ON: [Feature; 4] = [
    Feature::ExecExitNotifications,
    Feature::ExplorerProfile,
    Feature::AgentWorktreeIsolation,
    Feature::Workflows,
];

/// The start of the startup warning that worktree isolation is off under `collaboration`.
const RESERVED_NAMESPACE_WARNING: &str =
    "Subagent worktrees (`features.agent_worktree_isolation`) are off for this session";

fn on_by_default(feature: Feature) -> bool {
    feature != Feature::AgentWorktreeIsolation || !cfg!(windows)
}

/// Loads `config.toml` with `text` from a fresh `CODEX_HOME` with the production defaults.
async fn load_production(
    home: &TempDir,
    text: &str,
    loader_overrides: LoaderOverrides,
    cli_overrides: Vec<(String, toml::Value)>,
) -> std::io::Result<Config> {
    std::fs::write(home.path().join("config.toml"), text)?;
    ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .loader_overrides(loader_overrides)
        .cli_overrides(cli_overrides)
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await
}

fn isolation_warnings(config: &Config) -> Vec<&String> {
    config
        .startup_warnings
        .iter()
        .filter(|warning| warning.starts_with(RESERVED_NAMESPACE_WARNING))
        .collect()
}

/// R1.1, R1.2, R3.1: a `CODEX_HOME` whose config names no feature turns the four features on,
/// keeps questions outside Plan mode off and puts the MultiAgentV2 tools under `agents`.
#[tokio::test]
async fn production_defaults_enable_four_fork_features() -> Result<()> {
    let home = TempDir::new()?;
    let config = load_production(
        &home,
        "",
        LoaderOverrides::without_managed_config_for_tests(),
        Vec::new(),
    )
    .await?;
    for feature in DEFAULT_ON {
        assert_eq!(
            config.features.enabled(feature),
            on_by_default(feature),
            "{feature:?}"
        );
    }
    assert!(
        !config
            .features
            .enabled(Feature::DefaultModeRequestUserInput)
    );
    assert_eq!(
        config.multi_agent_v2.tool_namespace.as_deref(),
        Some(if cfg!(windows) {
            "collaboration"
        } else {
            "agents"
        })
    );
    assert_eq!(isolation_warnings(&config), Vec::<&String>::new());
    Ok(())
}

/// R1.4, R1.5: an explicit `false` in any layer wins over the new default (`[features]`, the
/// `enabled` of a settings table, `-c`, a profile); a settings table without `enabled` keeps
/// the feature on with its settings.
#[tokio::test]
async fn explicit_false_keeps_a_fork_feature_off() -> Result<()> {
    let home = TempDir::new()?;
    let profile = home.path().join("work.config.toml");
    std::fs::write(&profile, "[features]\nagent_worktree_isolation = false\n")?;
    let mut loader_overrides = LoaderOverrides::without_managed_config_for_tests();
    loader_overrides.user_config_path = Some(AbsolutePathBuf::from_absolute_path(&profile)?);
    loader_overrides.user_config_profile = Some("work".parse()?);
    let config = load_production(
        &home,
        "[features]\nworkflows = false\n\n[features.exec_exit_notifications]\nenabled = false\nexec_wait_timeout_ms = 1000\n",
        loader_overrides,
        vec![(
            "features.explorer_profile".to_string(),
            toml::Value::Boolean(false),
        )],
    )
    .await?;
    for feature in DEFAULT_ON {
        assert!(!config.features.enabled(feature), "{feature:?}");
    }
    assert_eq!(config.exec_exit_notifications.exec_wait_timeout_ms, 1000);
    assert_eq!(
        config.multi_agent_v2.tool_namespace.as_deref(),
        Some("collaboration")
    );

    let home = TempDir::new()?;
    let config = load_production(
        &home,
        "[features.exec_exit_notifications]\nexec_wait_timeout_ms = 1000\n",
        LoaderOverrides::without_managed_config_for_tests(),
        Vec::new(),
    )
    .await?;
    assert!(config.features.enabled(Feature::ExecExitNotifications));
    assert_eq!(config.exec_exit_notifications.exec_wait_timeout_ms, 1000);
    Ok(())
}

/// R3.2, R3.3 through the config loader: the reserved namespace turns default-on isolation off
/// without a warning, and isolation a layer turned on off with the warning.
#[tokio::test]
async fn reserved_namespace_warns_only_for_configured_isolation() -> Result<()> {
    let reserved =
        "[features.multi_agent_v2]\nenabled = true\ntool_namespace = \"collaboration\"\n";
    for (text, warnings) in [
        (reserved.to_string(), 0),
        (
            format!("{reserved}\n[features.agent_worktree_isolation]\nenabled = true\n"),
            1,
        ),
    ] {
        let home = TempDir::new()?;
        let config = load_production(
            &home,
            &text,
            LoaderOverrides::without_managed_config_for_tests(),
            Vec::new(),
        )
        .await?;
        assert!(
            !config.features.enabled(Feature::AgentWorktreeIsolation),
            "{text}"
        );
        assert_eq!(isolation_warnings(&config).len(), warnings, "{text}");
        assert_eq!(
            config.multi_agent_v2.tool_namespace.as_deref(),
            Some("collaboration")
        );
    }
    Ok(())
}

/// R6.1, R6.2: the core integration harness keeps the default-on fork features off and the
/// namespace `collaboration` unless the test sets a feature in its config.
#[tokio::test]
async fn upstream_test_harness_keeps_fork_features_off() -> Result<()> {
    let home = TempDir::new()?;
    std::fs::write(
        home.path().join("config.toml"),
        "[features.multi_agent_v2]\nenabled = true\n",
    )?;
    let config = load_default_config_for_test(&home).await;
    for feature in codex_features::fork_default_on_features() {
        assert!(!config.features.enabled(feature), "{feature:?}");
    }
    assert_eq!(
        config.multi_agent_v2.tool_namespace.as_deref(),
        Some("collaboration")
    );

    let home = TempDir::new()?;
    std::fs::write(
        home.path().join("config.toml"),
        "[features]\nworkflows = true\n",
    )?;
    let config = load_default_config_for_test(&home).await;
    for feature in codex_features::fork_default_on_features() {
        assert_eq!(
            config.features.enabled(feature),
            feature == Feature::Workflows,
            "{feature:?}"
        );
    }
    Ok(())
}

/// Every tool name of a request, namespaced tools as `<namespace>.<name>`.
fn tool_names(tools: &[Value]) -> Vec<String> {
    tools
        .iter()
        .flat_map(|tool| {
            let name = tool["name"].as_str().unwrap_or_default().to_string();
            match tool["tools"].as_array() {
                Some(children) => tool_names(children)
                    .into_iter()
                    .map(|child| format!("{name}.{child}"))
                    .collect(),
                None => vec![name],
            }
        })
        .collect()
}

/// The tools of a request, wherever the request puts them.
fn request_tools(body: &Value) -> Vec<Value> {
    let mut tools = body["tools"].as_array().cloned().unwrap_or_default();
    for item in body["input"].as_array().into_iter().flatten() {
        if item["type"] == "additional_tools"
            && let Some(more) = item["tools"].as_array()
        {
            tools.extend(more.iter().cloned());
        }
    }
    tools
}

/// R7.1, R7.2: the root's first request with the production defaults and a config that names no
/// fork feature (only `multi_agent_v2`, which the fork does not own) offers the MultiAgentV2
/// tools under `agents` with `isolation`, explorers and `apply_agent_worktree`, the notification
/// parameters of `exec_command` and `write_stdin`, the workflow tools, and `request_user_input`
/// for Plan mode only. The snapshot shows any later growth of the fork's tool text.
#[cfg(not(target_os = "windows"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_root_tool_list_with_fork_defaults() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let mock = mount_sse_sequence(
        &server,
        vec![sse(vec![
            ev_response_created("resp-1"),
            ev_assistant_message("msg-1", "Done."),
            ev_completed("resp-1"),
        ])],
    )
    .await;
    let test = test_codex()
        .with_pre_build_hook(|home| {
            use_production_feature_defaults(home);
            std::fs::write(
                home.join("config.toml"),
                "[features]\nmulti_agent_v2 = true\n",
            )
            .expect("write config.toml");
        })
        .with_config(|config| {
            // The default depends on the host's CPUs and is part of the `workflow` description.
            config.workflows.max_concurrent_agents = 4;
        })
        .build(&server)
        .await?;
    for feature in DEFAULT_ON {
        assert!(test.config.features.enabled(feature), "{feature:?}");
    }

    test.submit_turn("hi").await?;
    let requests = mock.requests();
    assert_eq!(requests.len(), 1);
    let body = requests[0].body_json();
    let tools = request_tools(&body);
    let names = tool_names(&tools);
    for name in [
        "agents.spawn_agent",
        "agents.apply_agent_worktree",
        "agents.wait_agent",
        "exec_command",
        "write_stdin",
        "workflow",
        "workflow_status",
        "workflow_cancel",
        "request_user_input",
    ] {
        assert!(names.iter().any(|tool| tool == name), "{name} in {names:?}");
    }
    assert!(
        !names.iter().any(|tool| tool.starts_with("collaboration.")),
        "{names:?}"
    );
    let text = Value::Array(tools).to_string();
    for needle in [
        "\"isolation\"",
        "explorer",
        "notify_on_exit",
        "notify_on_output",
        "stop_output_watch",
        "This tool is only available in Plan mode.",
    ] {
        assert!(text.contains(needle), "{needle}");
    }
    insta::assert_snapshot!(context_snapshot::format_labeled_requests_snapshot(
        "The root's first request with the fork's default-on features and a config that names none of them.",
        &[("first root request", &requests[0])],
        &ContextSnapshotOptions::default()
            .rewrite_known_segments()
            .include_request_settings(),
    ));
    Ok(())
}
