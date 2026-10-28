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
    assert_eq!(*config.explorer_profile, ExplorerProfileConfig::default());
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
        *config.explorer_profile,
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

/// R1.5: a boolean toggle (`--enable`, `-c`, `/experimental`) sets only `enabled` of the
/// `[features.explorer_profile]` table and keeps its settings.
#[tokio::test]
async fn profile_table_survives_boolean_toggles() {
    assert!(codex_config::is_structured_feature_path(&[
        "features",
        "explorer_profile"
    ]));

    let home = TempDir::new().expect("create temp dir");
    std::fs::write(
        home.path().join("config.toml"),
        "[features.explorer_profile]\nenabled = false\nmodel = \"cheap-model\"\nreasoning_effort = \"low\"\nmax_result_tokens = 500\n",
    )
    .expect("write config.toml");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(vec![(
            "features.explorer_profile".to_string(),
            TomlValue::Boolean(true),
        )])
        .fallback_cwd(Some(home.path().to_path_buf()))
        .build()
        .await
        .expect("load test config");
    assert!(enabled(&config));
    assert_eq!(
        *config.explorer_profile,
        ExplorerProfileConfig {
            model: Some("cheap-model".to_string()),
            reasoning_effort: Some(ReasoningEffort::Low),
            max_result_tokens: 500,
        }
    );
}

/// R1.6 (review 2026-10-27, item 3): turning the profile off (`codex features disable`, a
/// clear of `features.explorer_profile`) writes `enabled = false` and keeps the settings; a
/// table with only `enabled` is still cleared, as upstream does.
#[test]
fn disabling_the_profile_keeps_its_settings() {
    for (initial, disabled) in [
        (
            "[features.explorer_profile]\nenabled = true\nmodel = \"cheap-model\"\nmax_result_tokens = 500\n",
            "[features.explorer_profile]\nenabled = false\nmodel = \"cheap-model\"\nmax_result_tokens = 500\n",
        ),
        ("[features.explorer_profile]\nenabled = true\n", ""),
        ("[features]\nexplorer_profile = true\n", ""),
    ] {
        let home = TempDir::new().expect("create temp dir");
        let config_path = home.path().join("config.toml");
        std::fs::write(&config_path, initial).expect("write config.toml");
        crate::config::edit::ConfigEditsBuilder::new(home.path())
            .set_feature_enabled("explorer_profile", /*enabled*/ false)
            .apply_blocking()
            .expect("disable explorer_profile");
        let profile_table = |text: &str| {
            toml::from_str::<TomlValue>(text)
                .expect("parse config.toml")
                .get("features")
                .and_then(|features| features.get("explorer_profile"))
                .cloned()
        };
        assert_eq!(
            profile_table(&std::fs::read_to_string(&config_path).expect("read config.toml")),
            profile_table(disabled),
            "{initial}"
        );
    }
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
    let description = role_description(
        &ExplorerProfileConfig {
            model: Some("cheap-model".to_string()),
            reasoning_effort: Some(ReasoningEffort::Low),
            max_result_tokens: 1_500,
        },
        Some("subagent-model"),
    );
    assert!(description.contains("read-only"));
    assert!(description.contains("model `cheap-model` at `low` reasoning effort"));
    assert!(description.contains("Unless you pass `model` or `reasoning_effort`"));
    assert!(description.contains("fork_turns=\"all\""));
    assert!(description.contains("about 1500 tokens"));

    // Review P10: without a profile model the explorer runs on `default_subagent_model`.
    let description = role_description(&ExplorerProfileConfig::default(), Some("subagent-model"));
    assert!(
        description.contains("model `subagent-model` at the model's cheapest reasoning effort"),
        "{description}"
    );
    let description = role_description(&ExplorerProfileConfig::default(), None);
    assert!(
        description
            .contains("your model at the model's cheapest reasoning effort (not below `low`)"),
        "{description}"
    );
}

// Review P4: the role must not promise more than the profile enforces.
#[test]
fn role_texts_describe_the_enforced_limits() {
    let profile = ExplorerProfileConfig::default();
    let description = role_description(&profile, /*default_subagent_model*/ None);
    for promised in [
        "cannot edit workspace files",
        "command allow rules do not lift it",
        "MCP tools that are not annotated read-only",
        "or spawn agents",
        "their messages to other agents share the same cap",
    ] {
        assert!(description.contains(promised), "{promised}: {description}");
    }
    let summary = role_profile_summary(&profile);
    assert!(
        summary.contains("only MCP tools annotated read-only"),
        "{summary}"
    );
    assert!(summary.contains("no spawned agents"), "{summary}");
    assert!(
        summary.contains("messages to other agents capped"),
        "{summary}"
    );
    let instructions = developer_instructions(&profile);
    assert!(instructions.contains("Only MCP tools annotated read-only are available to you"));
    assert!(instructions.contains("You cannot spawn agents"));
    // Review 2026-10-27, item 4: base instructions and MCP server descriptions still mention
    // tools the explorer does not have.
    assert!(instructions.contains(
        "A tool that is not in your tool list is unavailable to you even where other instructions or tool descriptions mention it"
    ));
    assert!(instructions.contains("including any worktree you run in"));
    assert!(instructions.contains("Messages you send to other agents share one budget"));
}

#[tokio::test]
async fn sandbox_bypass_and_tool_denials_apply_only_to_explorers() {
    let (_home, enabled_config) =
        config_with(&[("features.explorer_profile", TomlValue::Boolean(true))]).await;
    let (_home, disabled) = config_with(&[]).await;
    let explorer = explorer_source(Some("explorer"));
    let worker = explorer_source(Some("worker"));

    assert!(forbids_sandbox_bypass(&enabled_config, &explorer));
    assert!(!forbids_sandbox_bypass(&enabled_config, &worker));
    assert!(!forbids_sandbox_bypass(&disabled, &explorer));

    let read_only = rmcp::model::ToolAnnotations::new().read_only(true);
    let not_destructive = rmcp::model::ToolAnnotations::new()
        .read_only(false)
        .destructive(false);
    assert_eq!(
        mcp_tool_denial(&enabled_config, &explorer, "srv", "peek", Some(&read_only)),
        None
    );
    for annotations in [Some(&not_destructive), None] {
        let denial = mcp_tool_denial(&enabled_config, &explorer, "srv", "poke", annotations)
            .expect("an explorer cannot call a tool that is not annotated read-only");
        assert!(
            denial.contains("`srv/poke`") && denial.contains("readOnlyHint"),
            "{denial}"
        );
        assert_eq!(
            mcp_tool_denial(&enabled_config, &worker, "srv", "poke", annotations),
            None
        );
        assert_eq!(
            mcp_tool_denial(&disabled, &explorer, "srv", "poke", annotations),
            None
        );
    }
    assert!(dynamic_tool_denial(&enabled_config, &explorer, "client_echo").is_some());
    assert_eq!(
        dynamic_tool_denial(&enabled_config, &worker, "client_echo"),
        None
    );
    assert!(skips_default_isolation(&enabled_config, Some("explorer")));
    assert!(!skips_default_isolation(&enabled_config, Some("worker")));
    assert!(!skips_default_isolation(&disabled, Some("explorer")));
}

// Review 2026-10-27 P1 (R2.10): every spawn path (V1 and V2 `spawn_agent`, full-history forks,
// workflow children) builds the child config with `build_agent_spawn_config`, which refuses a
// spawn from an explorer before anything else; other children and a disabled profile spawn.
#[tokio::test]
async fn explorer_spawn_config_is_refused() {
    for (explorer_profile, role, refused) in [
        (true, "explorer", true),
        (true, "worker", false),
        (false, "explorer", false),
    ] {
        let (_session, mut turn) = crate::session::tests::make_session_and_context().await;
        let mut config = (*turn.config).clone();
        config
            .features
            .set_enabled(Feature::ExplorerProfile, explorer_profile)
            .expect("test config should allow feature update");
        turn.config = std::sync::Arc::new(config);
        turn.session_source = explorer_source(Some(role));
        let step_context =
            crate::session::step_context::StepContext::for_test(std::sync::Arc::new(turn));
        let base_instructions = codex_protocol::models::BaseInstructions {
            text: "base".to_string(),
            provenance: None,
        };
        let result = crate::agent::child_config::build_agent_spawn_config(
            &base_instructions,
            step_context.as_ref(),
        );
        let case = format!("profile={explorer_profile} role={role}");
        match result {
            Err(err) => {
                assert!(refused, "{case}: {err}");
                assert!(
                    err.contains("explorer_profile: a read-only explorer cannot spawn agents"),
                    "{case}: {err}"
                );
            }
            Ok(_) => assert!(!refused, "{case}: an explorer built a spawn config"),
        }
    }
}

#[test]
fn message_budget_caps_then_refuses() {
    let budget = ExplorerMessageBudget::new(200);
    assert_eq!(
        budget.take("short note".to_string(), 200),
        Ok("short note".to_string())
    );

    let long = "message line\n".repeat(2_000);
    let capped = budget
        .take(long.clone(), 200)
        .expect("a long message is capped to the remaining budget");
    assert!(capped.len() < long.len() / 4, "{}", capped.len());
    assert!(capped.starts_with("message line"));
    assert!(
        capped.contains("explorer_profile: this message was truncated to about"),
        "{capped}"
    );

    let refused = budget
        .take("one more".to_string(), 200)
        .expect_err("the spent budget refuses further messages");
    assert!(refused.contains("about 200 tokens"), "{refused}");
    assert!(refused.contains("final answer"), "{refused}");
}

#[tokio::test]
async fn message_resume_config_keeps_authority_without_explorer_locks() {
    let (_home, mut config) = config_with(&[
        ("features.explorer_profile", TomlValue::Boolean(true)),
        (
            "sandbox_mode",
            TomlValue::String("danger-full-access".to_string()),
        ),
        (
            "developer_instructions",
            TomlValue::String("Parent rules.".to_string()),
        ),
    ])
    .await;
    enforce_read_only_session_config(&mut config).expect("enforce explorer profile");
    assert!(
        config
            .permissions
            .approval_policy
            .can_set(&AskForApproval::OnRequest)
            .is_err(),
        "the explorer itself stays locked"
    );

    release_session_locks_for_message_resume(&mut config).expect("release explorer locks");

    // The reloaded agent keeps the explorer's authority...
    assert_eq!(
        config.permissions.permission_profile(),
        &PermissionProfile::read_only()
    );
    assert_eq!(
        config.permissions.approval_policy.value(),
        AskForApproval::Never
    );
    // ...without its locks or instructions (review P9).
    assert!(
        config
            .permissions
            .approval_policy
            .can_set(&AskForApproval::OnRequest)
            .is_ok()
    );
    assert!(
        config
            .permissions
            .can_set_permission_profile(&PermissionProfile::Disabled)
            .is_ok()
    );
    assert_eq!(
        config.developer_instructions.as_deref(),
        Some("Parent rules.")
    );

    // Instructions that were only the explorer block disappear entirely.
    let (_home, mut bare) =
        config_with(&[("features.explorer_profile", TomlValue::Boolean(true))]).await;
    append_developer_instructions(&mut bare);
    append_developer_instructions(&mut bare);
    assert_eq!(
        bare.developer_instructions
            .as_deref()
            .map(|text| text.matches("# Explorer profile").count()),
        Some(1)
    );
    strip_developer_instructions(&mut bare);
    assert_eq!(bare.developer_instructions, None);
}

#[tokio::test]
async fn message_resume_config_keeps_requirement_constraints() {
    let (_home, mut config) =
        config_with(&[("features.explorer_profile", TomlValue::Boolean(true))]).await;
    // Requirements that forbid `on-request` must still bind the reloaded agent.
    let mut requirements = config.config_layer_stack.requirements().clone();
    requirements.approval_policy.value = Constrained::new(AskForApproval::Never, |candidate| {
        if *candidate == AskForApproval::OnRequest {
            Err(ConstraintError::InvalidValue {
                field_name: "approval_policy",
                candidate: format!("{candidate}"),
                allowed: "[never, untrusted]".to_string(),
                requirement_source: RequirementSource::Unknown,
            })
        } else {
            Ok(())
        }
    })
    .expect("requirement constraint");
    config.config_layer_stack = codex_config::ConfigLayerStack::new(
        config
            .config_layer_stack
            .all_layers_low_to_high()
            .cloned()
            .collect(),
        requirements,
        config.config_layer_stack.requirements_toml().clone(),
    )
    .expect("replace test requirements");
    enforce_read_only_session_config(&mut config).expect("enforce explorer profile");
    release_session_locks_for_message_resume(&mut config).expect("release explorer locks");
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
            .approval_policy
            .can_set(&AskForApproval::UnlessTrusted)
            .is_ok()
    );
}
