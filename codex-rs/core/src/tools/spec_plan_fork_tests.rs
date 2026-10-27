//! Fork: tool plan hooks of `spec_plan_fork.rs`.

use super::*;
use pretty_assertions::assert_eq;

/// R3: the stub of a direct-only tool does not take the Code Mode name of a nested tool, so a
/// dynamic `request-user-input` stays callable as `tools.request_user_input`.
#[tokio::test]
async fn direct_only_stub_does_not_shadow_a_nested_tool() {
    let plan = probe_with(
        |turn| set_features(turn, &[Feature::CodeMode, Feature::CodeModeOnly]),
        ToolPlanInputs {
            dynamic_tools: vec![dynamic_tool(
                /*namespace*/ None,
                "request-user-input",
                /*defer_loading*/ false,
            )],
            ..ToolPlanInputs::default()
        },
    )
    .await;

    assert_eq!(
        plan.exposure("request_user_input"),
        ToolExposure::DirectModelOnly
    );
    assert_eq!(
        plan.code_mode_tool_names.get("request_user_input"),
        Some(&ToolName::plain("request-user-input"))
    );
    // The direct tool stays visible to the model; `exec` declares the dynamic tool.
    plan.assert_visible_contains(&["request_user_input"]);
    let ToolSpec::Freeform(exec) = plan.visible_spec(codex_code_mode::PUBLIC_TOOL_NAME) else {
        panic!("expected code mode exec tool");
    };
    assert!(exec.description.contains(
        "### `request_user_input` (`request-user-input`)\nrequest-user-input dynamic tool"
    ));
    assert!(exec.description.contains("request_user_input(args:"));
}

/// Explorer-and-questions R2.9: the `multi_env_copy_file` extension tool, stubbed for the plan.
struct CopyFileStub;

impl<'call> ToolExecutor<ExtensionToolCall<'call>> for CopyFileStub {
    fn tool_name(&self) -> ToolName {
        ToolName::plain("multi_env_copy_file")
    }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: "multi_env_copy_file".to_string(),
            description: "Copy a file between two environments.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: codex_tools::JsonSchema::default(),
            output_schema: None,
        })
    }

    fn handle<'a>(&'a self, _call: ExtensionToolCall<'call>) -> codex_tools::ToolExecutorFuture<'a>
    where
        'call: 'a,
    {
        Box::pin(async { panic!("spec planning should not execute extension tools") })
    }
}

fn annotated_mcp_tool(name: &str, read_only_hint: Option<bool>) -> ToolInfo {
    let mut info = mcp_tool("probe", "mcp__probe", name);
    info.tool.annotations =
        read_only_hint.map(|read_only| rmcp::model::ToolAnnotations::new().read_only(read_only));
    info
}

fn explorer_source(role: &str) -> SessionSource {
    SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: ThreadId::new(),
        depth: 1,
        agent_path: None,
        agent_nickname: None,
        agent_role: Some(role.to_string()),
    })
}

/// The plan of a child with `role`, `apply_patch`, three MCP tools (`peek` annotated
/// read-only, `poke` annotated writing, `plain` without annotations), a client tool and the
/// `multi_env_copy_file` extension, after the explorer hook ran.
async fn explorer_plan(explorer_profile: bool, role: &str, code_mode: bool) -> ToolPlanProbe {
    explorer_plan_with(
        explorer_profile,
        role,
        code_mode,
        &[],
        std::collections::HashSet::new(),
    )
    .await
}

/// [`explorer_plan`] with `extra_mcp_tools` and the `(server, tool)` pairs that the catalog
/// cache marks read-only.
async fn explorer_plan_with(
    explorer_profile: bool,
    role: &str,
    code_mode: bool,
    extra_mcp_tools: &[ToolInfo],
    cached_read_only_tools: std::collections::HashSet<(String, String)>,
) -> ToolPlanProbe {
    let (_session, mut turn) = make_session_and_context().await;
    set_feature(&mut turn, Feature::ExplorerProfile, explorer_profile);
    if code_mode {
        set_features(&mut turn, &[Feature::CodeMode]);
    }
    update_turn_settings_for_test(&mut turn, |settings| {
        Arc::make_mut(&mut settings.model_info).apply_patch_tool_type =
            Some(ApplyPatchToolType::Freeform);
    });
    turn.session_source = explorer_source(role);
    let mut mcp_tools = vec![
        annotated_mcp_tool("peek", Some(true)),
        annotated_mcp_tool("poke", Some(false)),
        annotated_mcp_tool("plain", /*read_only_hint*/ None),
    ];
    mcp_tools.extend_from_slice(extra_mcp_tools);
    let mcp = codex_mcp::McpBinding::empty(mcp_config_for_test(&turn.config));
    let model_info = turn.model_info();
    let mut registry = build_core_tool_registry(
        &turn,
        model_info,
        &turn.initial_environments,
        &mcp,
        /*tool_suggest_candidates*/ None,
        /*wait_for_environment_tool_config*/ None,
    );
    let runtimes = mcp_tools
        .iter()
        .map(|info| {
            let runtime: Arc<dyn CoreToolRuntime> =
                Arc::new(McpHandler::new(info.clone()).expect("MCP tool spec should build"));
            RegisteredTool {
                runtime,
                exposure: ToolExposure::Direct,
                model_spec: None,
            }
        })
        .collect();
    let hosted_specs = append_source_tools(
        &turn,
        model_info,
        &mut registry,
        runtimes,
        vec![Arc::new(CopyFileStub) as Arc<dyn for<'call> ToolExecutor<ExtensionToolCall<'call>>>],
        &[dynamic_tool(
            /*namespace*/ None,
            "client_echo",
            /*defer_loading*/ false,
        )],
    );
    crate::agent::explorer_profile::hide_unavailable_tools(
        &turn.config,
        &turn.session_source,
        mcp_tools.iter(),
        || cached_read_only_tools,
        &mut registry,
    );
    ToolPlanProbe::from_router(ToolRouter::from_registry(
        &turn,
        model_info,
        registry,
        hosted_specs,
        &Default::default(),
    ))
}

fn explorer_hidden_tools() -> [ToolName; 5] {
    [
        ToolName::plain("apply_patch"),
        ToolName::plain("multi_env_copy_file"),
        ToolName::plain("client_echo"),
        ToolName::namespaced("mcp__probe", "poke"),
        ToolName::namespaced("mcp__probe", "plain"),
    ]
}

/// R2.9: an explorer sees neither `apply_patch`, `multi_env_copy_file`, MCP tools without
/// `readOnlyHint: true` nor client tools, in its tool list or in Code Mode; they stay
/// registered, so a call still reaches the handler's refusal. Other children see them all.
#[tokio::test]
async fn explorer_hides_tools_it_cannot_call() {
    for (explorer_profile, role, hidden) in [
        (true, "explorer", true),
        (false, "explorer", false),
        (true, "worker", false),
    ] {
        let plan = explorer_plan(explorer_profile, role, /*code_mode*/ false).await;
        let case = format!("profile={explorer_profile} role={role}");
        for name in explorer_hidden_tools() {
            let exposure = plan.exposure(&name.to_string());
            assert_eq!(exposure == ToolExposure::Hidden, hidden, "{case}: {name:?}");
        }
        assert_eq!(
            plan.exposure(&ToolName::namespaced("mcp__probe", "peek").to_string()),
            ToolExposure::Direct,
            "{case}"
        );
        plan.assert_visible_contains(&["exec_command", "mcp__probe"]);
        assert_eq!(
            plan.visible_names.iter().any(|name| name == "apply_patch"),
            !hidden,
            "{case}: {:?}",
            plan.visible_names
        );
        assert_eq!(
            plan.visible_names
                .iter()
                .any(|name| name == "multi_env_copy_file" || name == "client_echo"),
            !hidden,
            "{case}: {:?}",
            plan.visible_names
        );
        let expected_mcp: &[&str] = if hidden {
            &["peek"]
        } else {
            &["peek", "plain", "poke"]
        };
        let mut mcp_functions = plan.namespace_function_names("mcp__probe").to_vec();
        mcp_functions.sort();
        assert_eq!(mcp_functions, expected_mcp, "{case}");
    }
}

/// R2.9, review 2026-10-27 P2: a tool from the catalog cache has no annotations; the explorer
/// shows it when the cache keeps the server's `readOnlyHint: true` for it, and live
/// annotations win over the cached hint.
#[tokio::test]
async fn explorer_shows_cached_read_only_tools() {
    let cached = std::collections::HashSet::from([
        ("probe".to_string(), "cached_peek".to_string()),
        ("probe".to_string(), "poke".to_string()),
    ]);
    let plan = explorer_plan_with(
        /*explorer_profile*/ true,
        "explorer",
        /*code_mode*/ false,
        &[
            annotated_mcp_tool("cached_peek", /*read_only_hint*/ None),
            annotated_mcp_tool("cached_poke", /*read_only_hint*/ None),
        ],
        cached,
    )
    .await;
    let mut mcp_functions = plan.namespace_function_names("mcp__probe").to_vec();
    mcp_functions.sort();
    assert_eq!(mcp_functions, ["cached_peek", "peek"]);
    for hidden in ["cached_poke", "poke", "plain"] {
        assert_eq!(
            plan.exposure(&ToolName::namespaced("mcp__probe", hidden).to_string()),
            ToolExposure::Hidden,
            "{hidden}"
        );
    }
}

/// R2.9: hidden tools stay out of the nested Code Mode tools too.
#[tokio::test]
async fn explorer_hides_tools_from_code_mode() {
    for (explorer_profile, hidden) in [(true, true), (false, false)] {
        let plan = explorer_plan(explorer_profile, "explorer", /*code_mode*/ true).await;
        let nested = plan.code_mode_tool_names.values().collect::<Vec<_>>();
        assert!(
            nested.contains(&&ToolName::namespaced("mcp__probe", "peek")),
            "{nested:?}"
        );
        for name in explorer_hidden_tools() {
            assert_eq!(
                nested.contains(&&name),
                !hidden,
                "profile={explorer_profile}: {name:?} in {nested:?}"
            );
        }
    }
}

/// R2.9: `build_tool_router` runs the explorer hook, so an explorer's real tool plan has no
/// `apply_patch` and no client tools.
#[tokio::test]
async fn explorer_tool_router_hides_write_and_client_tools() {
    for (explorer_profile, hidden) in [(true, true), (false, false)] {
        let (session, mut turn) = make_session_and_context().await;
        set_feature(&mut turn, Feature::ExplorerProfile, explorer_profile);
        update_turn_settings_for_test(&mut turn, |settings| {
            Arc::make_mut(&mut settings.model_info).apply_patch_tool_type =
                Some(ApplyPatchToolType::Freeform);
        });
        turn.session_source = explorer_source("explorer");
        // R2.10: with multi-agent V2 the plan registers `spawn_agent`.
        turn.multi_agent_version = MultiAgentVersion::V2;
        let dynamic_tools = vec![dynamic_tool(
            /*namespace*/ None,
            "client_echo",
            /*defer_loading*/ false,
        )];
        let turn = Arc::new(turn);
        let step_context = StepContext::for_test(Arc::clone(&turn));
        let router = crate::tools::spec_plan::build_tool_router(
            &session,
            step_context.turn.as_ref(),
            &step_context.settings.model_info,
            &step_context.environments,
            &step_context.mcp,
            /*apps_enabled*/ false,
            &turn.extension_data,
            /*tool_suggest_candidates*/ None,
            &dynamic_tools,
        )
        .expect("build explorer tool router");
        let plan = ToolPlanProbe::from_router(router);
        plan.assert_visible_contains(&["exec_command"]);
        plan.assert_registered_contains(&["apply_patch", "client_echo"]);
        for name in ["apply_patch", "client_echo"] {
            assert_eq!(
                plan.visible_names.iter().any(|visible| visible == name),
                !hidden,
                "profile={explorer_profile}: {name} in {:?}",
                plan.visible_names
            );
        }
        // R2.10: an explorer does not see `spawn_agent`; a call still reaches the refusal.
        let spawn_agent = plan
            .registered_names
            .iter()
            .find(|name| name.ends_with("spawn_agent"))
            .unwrap_or_else(|| panic!("V2 registers spawn_agent: {:?}", plan.registered_names));
        assert_eq!(
            plan.exposure(spawn_agent) == ToolExposure::Hidden,
            hidden,
            "profile={explorer_profile}: {spawn_agent}"
        );
    }
}

/// Agent-worktree-isolation R11.1: `apply_agent_worktree` sits beside the MultiAgentV2 tools
/// only while worktree isolation is enabled, and a read-only explorer does not get it.
#[tokio::test]
async fn apply_agent_worktree_is_offered_only_with_isolation() {
    let apply = ToolName::namespaced("agents", "apply_agent_worktree").to_string();
    for (isolation, explorer, offered) in [
        (true, false, true),
        (false, false, false),
        (true, true, false),
    ] {
        let plan = probe(|turn| {
            set_feature(turn, Feature::MultiAgentV2, /*enabled*/ true);
            set_feature(turn, Feature::AgentWorktreeIsolation, isolation);
            set_feature(turn, Feature::ExplorerProfile, explorer);
            update_config(turn, |config| {
                config.multi_agent_v2.tool_namespace = Some("agents".to_string());
            });
            if explorer {
                turn.session_source = explorer_source("explorer");
            }
        })
        .await;
        assert_eq!(
            plan.registered_names.contains(&apply),
            offered,
            "isolation={isolation} explorer={explorer}: {:?}",
            plan.registered_names
        );
        assert_eq!(
            plan.namespace_function_names("agents")
                .iter()
                .any(|name| name == "apply_agent_worktree"),
            offered
        );
    }
}
