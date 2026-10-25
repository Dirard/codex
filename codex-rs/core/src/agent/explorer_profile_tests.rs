use super::*;
use crate::config::ConfigBuilder;
use codex_protocol::ThreadId;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_protocol::protocol::SubAgentSource;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use toml::Value as TomlValue;

async fn config_with(overrides: &[(&str, TomlValue)]) -> (TempDir, Config) {
    let home = TempDir::new().expect("create temp dir");
    let home_path = home.path().to_path_buf();
    let config = ConfigBuilder::default()
        .codex_home(home_path.clone())
        .cli_overrides(
            overrides
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
        )
        .fallback_cwd(Some(home_path))
        .build()
        .await
        .expect("load test config");
    (home, config)
}

fn explorer_source(role: Option<&str>) -> SessionSource {
    SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: ThreadId::new(),
        depth: 1,
        agent_path: None,
        agent_nickname: None,
        agent_role: role.map(str::to_string),
    })
}

fn efforts(efforts: &[ReasoningEffort]) -> Vec<ReasoningEffortPreset> {
    efforts
        .iter()
        .map(|effort| ReasoningEffortPreset {
            effort: effort.clone(),
            description: String::new(),
        })
        .collect()
}

#[tokio::test]
async fn profile_config_defaults_and_rejects_tiny_result_limit() {
    let (_home, config) =
        config_with(&[("features.explorer_profile", TomlValue::Boolean(true))]).await;
    assert!(enabled(&config));
    assert_eq!(config.explorer_profile, ExplorerProfileConfig::default());
    assert_eq!(config.explorer_profile.max_result_tokens, 2_000);

    let (_home, config) = config_with(&[
        (
            "features.explorer_profile.enabled",
            TomlValue::Boolean(true),
        ),
        (
            "features.explorer_profile.model",
            TomlValue::String("cheap-model".to_string()),
        ),
        (
            "features.explorer_profile.reasoning_effort",
            TomlValue::String("low".to_string()),
        ),
        (
            "features.explorer_profile.max_result_tokens",
            TomlValue::Integer(500),
        ),
    ])
    .await;
    assert!(enabled(&config));
    assert_eq!(
        config.explorer_profile,
        ExplorerProfileConfig {
            model: Some("cheap-model".to_string()),
            reasoning_effort: Some(ReasoningEffort::Low),
            max_result_tokens: 500,
        }
    );

    let home = TempDir::new().expect("create temp dir");
    let err = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(vec![(
            "features.explorer_profile.max_result_tokens".to_string(),
            TomlValue::Integer(100),
        )])
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await
        .expect_err("a limit below 200 tokens is rejected");
    assert!(
        err.to_string()
            .contains("features.explorer_profile.max_result_tokens must be at least 200"),
        "{err}"
    );
}

#[tokio::test]
async fn profile_applies_only_to_explorer_role_when_enabled() {
    let (_home, disabled) = config_with(&[]).await;
    let (_home, enabled_config) =
        config_with(&[("features.explorer_profile", TomlValue::Boolean(true))]).await;

    assert!(!applies_to_role(&disabled, Some("explorer")));
    assert!(applies_to_role(&enabled_config, Some("explorer")));
    assert!(applies_to_role(&enabled_config, Some(" explorer ")));
    assert!(!applies_to_role(&enabled_config, Some("worker")));
    assert!(!applies_to_role(&enabled_config, None));

    assert!(applies_to_source(
        &enabled_config,
        &explorer_source(Some("explorer"))
    ));
    assert!(!applies_to_source(&enabled_config, &explorer_source(None)));
    assert!(!applies_to_source(&enabled_config, &SessionSource::Exec));
    assert!(!applies_to_source(
        &disabled,
        &explorer_source(Some("explorer"))
    ));
    assert_eq!(
        result_max_tokens(&enabled_config, &explorer_source(Some("explorer"))),
        Some(2_000)
    );
    assert_eq!(
        result_max_tokens(&disabled, &explorer_source(Some("explorer"))),
        None
    );
}

#[test]
fn read_only_ceiling_removes_writes_and_keeps_denied_reads() {
    assert_eq!(
        read_only_ceiling(&PermissionProfile::Disabled),
        PermissionProfile::read_only()
    );
    assert_eq!(
        read_only_ceiling(&PermissionProfile::External {
            network: NetworkSandboxPolicy::Enabled,
        }),
        PermissionProfile::read_only()
    );

    let workspace_write = PermissionProfile::workspace_write_with(
        &[],
        NetworkSandboxPolicy::Enabled,
        /*exclude_tmpdir_env_var*/ false,
        /*exclude_slash_tmp*/ false,
    );
    let ceiling = read_only_ceiling(&workspace_write);
    let file_system = ceiling.file_system_sandbox_policy();
    assert!(
        file_system
            .entries
            .iter()
            .all(|entry| entry.access != FileSystemAccessMode::Write),
        "{file_system:?}"
    );
    assert_eq!(
        ceiling.network_sandbox_policy(),
        NetworkSandboxPolicy::Restricted
    );
}

#[tokio::test]
async fn enforce_read_only_locks_sandbox_and_approval() {
    let (_home, mut config) = config_with(&[
        ("features.explorer_profile", TomlValue::Boolean(true)),
        (
            "sandbox_mode",
            TomlValue::String("danger-full-access".to_string()),
        ),
        (
            "approval_policy",
            TomlValue::String("on-request".to_string()),
        ),
        (
            "developer_instructions",
            TomlValue::String("Parent rules.".to_string()),
        ),
    ])
    .await;

    enforce_read_only_session_config(&mut config).expect("enforce explorer profile");

    assert_eq!(
        config.permissions.permission_profile(),
        &PermissionProfile::read_only()
    );
    assert_eq!(
        config.permissions.approval_policy.value(),
        AskForApproval::Never
    );
    assert!(
        config
            .permissions
            .approval_policy
            .can_set(&AskForApproval::OnRequest)
            .is_err()
    );
    assert!(
        config
            .permissions
            .can_set_permission_profile(&PermissionProfile::Disabled)
            .is_err()
    );
    assert!(!config.features.enabled(Feature::RequestPermissionsTool));
    let instructions = config
        .developer_instructions
        .clone()
        .expect("developer instructions");
    assert!(instructions.starts_with("Parent rules.\n\n# Explorer profile\n"));
    assert!(instructions.contains("under about 2000 tokens"));

    // A second application, for example on reload, does not repeat the instructions.
    enforce_read_only_session_config(&mut config).expect("enforce explorer profile again");
    assert_eq!(config.developer_instructions, Some(instructions));
}

#[tokio::test]
async fn enforce_read_only_rejects_requirements_without_never() {
    let (_home, mut config) =
        config_with(&[("features.explorer_profile", TomlValue::Boolean(true))]).await;
    config.permissions.approval_policy = Constrained::new(AskForApproval::OnRequest, |candidate| {
        if *candidate == AskForApproval::Never {
            Err(ConstraintError::InvalidValue {
                field_name: "approval_policy",
                candidate: format!("{candidate}"),
                allowed: "[on-request]".to_string(),
                requirement_source: RequirementSource::Unknown,
            })
        } else {
            Ok(())
        }
    })
    .expect("constrained approval policy");

    let err = enforce_read_only_session_config(&mut config)
        .expect_err("requirements that disallow never reject the explorer");
    assert!(
        err.to_string()
            .contains("explorer_profile requires approval_policy `never`"),
        "{err}"
    );
}

#[test]
fn clamp_environment_selection_narrows_ready_owner_config() {
    let mut profile = PermissionProfileSnapshot::legacy(PermissionProfile::Disabled);
    clamp_environment_permission_profile(&mut profile);
    assert_eq!(
        profile.permission_profile(),
        &PermissionProfile::read_only()
    );
}

#[test]
fn cheapest_reasoning_effort_prefers_low_floor() {
    use ReasoningEffort::*;
    assert_eq!(
        cheapest_reasoning_effort(&efforts(&[Low, Medium])),
        Some(Low)
    );
    assert_eq!(
        cheapest_reasoning_effort(&efforts(&[Minimal, Low])),
        Some(Low)
    );
    assert_eq!(
        cheapest_reasoning_effort(&efforts(&[Medium, High])),
        Some(Medium)
    );
    assert_eq!(
        cheapest_reasoning_effort(&efforts(&[Max, Ultra, Low, XHigh])),
        Some(Low)
    );
    assert_eq!(
        cheapest_reasoning_effort(&efforts(&[None, Minimal])),
        Some(None)
    );
    assert_eq!(
        cheapest_reasoning_effort(&efforts(&[Custom("turbo".to_string()), High])),
        Some(High)
    );
    assert_eq!(cheapest_reasoning_effort(&[]), Option::None);
}

#[test]
fn cap_result_keeps_short_answers_and_marks_truncation() {
    assert_eq!(cap_result("short answer", 200), "short answer");

    let long = "conclusion line\n".repeat(2_000);
    let capped = cap_result(&long, 200);
    assert!(capped.len() < long.len() / 4, "{}", capped.len());
    assert!(capped.starts_with("conclusion line"));
    assert!(capped.contains("tokens truncated"), "{capped}");
    assert!(capped.ends_with(
        "Ask this explorer a narrower follow-up question for the omitted part instead of re-reading the code yourself.]"
    ));
}

#[test]
fn role_description_names_profile_settings() {
    let description = role_description(&ExplorerProfileConfig {
        model: Some("cheap-model".to_string()),
        reasoning_effort: Some(ReasoningEffort::Low),
        max_result_tokens: 1_500,
    });
    assert!(description.contains("read-only"));
    assert!(description.contains("model `cheap-model` at `low` reasoning effort"));
    assert!(description.contains("fork_turns=\"all\""));
    assert!(description.contains("about 1500 tokens"));

    let description = role_description(&ExplorerProfileConfig::default());
    assert!(description.contains("your model at its cheapest reasoning effort"));
}
