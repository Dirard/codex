use super::*;
use codex_tools::JsonSchemaPrimitiveType;
use codex_tools::JsonSchemaType;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn spawn_agent_tool_v2_requires_task_name() {
    let tool = create_spawn_agent_tool_v2(
        SpawnAgentToolOptions {
            agent_type_description: "role help".to_string(),
            expose_agent_type: true,
            hide_agent_type_model_reasoning: false,
            expose_spawn_agent_model_overrides: true,
            usage_hint_text: None,
            ..Default::default()
        },
        /*description_override*/ None,
    );

    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        output_schema,
        ..
    }) = tool
    else {
        panic!("spawn_agent should be a function tool");
    };
    assert_eq!(
        parameters.schema_type,
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
    let properties = parameters
        .properties
        .as_ref()
        .expect("spawn_agent should use object params");
    assert!(description.contains("Spawns an agent to work on the specified task."));
    assert!(description.contains("The spawned agent will have the same tools as you"));
    assert!(!description.contains("max_concurrent_threads_per_session"));
    assert!(description.contains(SPAWN_AGENT_INHERITED_MODEL_GUIDANCE));
    assert!(properties.contains_key("task_name"));
    assert!(properties.contains_key("message"));
    assert_eq!(
        properties
            .get("message")
            .and_then(|schema| schema.encrypted),
        Some(true)
    );
    assert!(properties.contains_key("fork_turns"));
    assert!(!properties.contains_key("items"));
    assert!(!properties.contains_key("fork_context"));
    assert_eq!(
        properties
            .get("model")
            .and_then(|schema| schema.description.as_deref()),
        Some(SPAWN_AGENT_MODEL_OVERRIDE_DESCRIPTION)
    );
    assert_eq!(
        properties
            .get("reasoning_effort")
            .and_then(|schema| schema.description.as_deref()),
        Some("Reasoning effort override for the new agent. Omit to inherit the parent effort.")
    );
    assert!(!properties.contains_key("service_tier"));
    assert_eq!(
        parameters.required.as_deref(),
        Some(["task_name".to_string(), "message".to_string()].as_slice())
    );
    assert_eq!(
        output_schema.expect("spawn_agent output schema").to_value()["required"],
        json!(["task_name", "nickname"])
    );
}

#[test]
fn spawn_agent_catalog_description_preserves_generated_context() {
    let options = SpawnAgentToolOptions {
        agent_type_description: "Available agent roles: explorer".to_string(),
        expose_spawn_agent_model_overrides: true,
        usage_hint_text: Some("Local usage hint.".to_string()),
        ..Default::default()
    };
    let ToolSpec::Function(default_tool) =
        create_spawn_agent_tool_v2(options.clone(), /*description_override*/ None)
    else {
        panic!("spawn_agent should be a function tool");
    };
    let ToolSpec::Function(mut configured_tool) =
        create_spawn_agent_tool_v2(options, Some("Catalog spawning guidance."))
    else {
        panic!("spawn_agent should be a function tool");
    };
    assert!(
        configured_tool
            .description
            .contains("Catalog spawning guidance.")
    );
    assert!(
        configured_tool
            .description
            .contains(SPAWN_AGENT_INHERITED_MODEL_GUIDANCE)
    );
    assert!(configured_tool.description.ends_with("Local usage hint."));
    assert!(
        !configured_tool
            .description
            .contains("Spawns an agent to work on the specified task.")
    );
    configured_tool.description = default_tool.description.clone();
    assert_eq!(configured_tool, default_tool);
}

#[test]
fn spawn_agent_tool_v2_fork_turns_lists_supported_values() {
    let ToolSpec::Function(ResponsesApiTool { parameters, .. }) = create_spawn_agent_tool_v2(
        SpawnAgentToolOptions {
            available_models: Vec::new(),
            agent_type_description: "role help".to_string(),
            expose_agent_type: true,
            hide_agent_type_model_reasoning: false,
            expose_spawn_agent_model_overrides: true,
            multi_agent_version: MultiAgentVersion::V2,
            model_catalog_in_context: false,
            usage_hint_text: None,
            fork: Default::default(),
        },
        /*description_override*/ None,
    ) else {
        panic!("spawn_agent should be a function tool");
    };
    let fork_description = parameters
        .properties
        .as_ref()
        .and_then(|properties| properties.get("fork_turns"))
        .and_then(|schema| schema.description.as_deref())
        .expect("fork_turns description");
    assert!(fork_description.contains("use `none` to start without parent history"));
    assert!(fork_description.contains("Only `all` and `none` are supported"));
    assert!(!fork_description.contains("positive integer"));
}

#[test]
fn spawn_agent_tool_v1_keeps_legacy_fork_context_field() {
    let tool = create_spawn_agent_tool_v1(SpawnAgentToolOptions {
        agent_type_description: "role help".to_string(),
        expose_agent_type: true,
        hide_agent_type_model_reasoning: false,
        expose_spawn_agent_model_overrides: true,
        usage_hint_text: None,
        ..Default::default()
    });

    let ToolSpec::Namespace(namespace) = tool else {
        panic!("spawn_agent v1 should be a namespace tool");
    };
    assert_eq!(namespace.name, MULTI_AGENT_V1_NAMESPACE);
    let Some(ResponsesApiNamespaceTool::Function(ResponsesApiTool { parameters, .. })) =
        namespace.tools.first()
    else {
        panic!("spawn_agent should be a namespace function tool");
    };
    assert_eq!(
        parameters.schema_type.clone(),
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
    let properties = parameters
        .properties
        .as_ref()
        .expect("spawn_agent should use object params");

    assert!(properties.contains_key("fork_context"));
    assert!(!properties.contains_key("fork_turns"));
    assert_eq!(
        properties.get("agent_type"),
        Some(&JsonSchema::string(Some(format!(
            "{SPAWN_AGENT_TYPE_OVERRIDE_DESCRIPTION_V1}\nrole help"
        ))))
    );
    assert_eq!(
        properties
            .get("message")
            .and_then(|schema| schema.encrypted),
        None
    );
    assert_eq!(
        properties
            .get("model")
            .and_then(|schema| schema.description.as_deref()),
        Some(SPAWN_AGENT_MODEL_OVERRIDE_DESCRIPTION)
    );
    assert!(!properties.contains_key("service_tier"));
}

#[test_case::test_case(false; "inline_catalog")]
#[test_case::test_case(true; "context_catalog")]
fn spawn_agent_tool_keeps_model_controls_when_spawn_metadata_is_hidden(
    model_catalog_in_context: bool,
) {
    let tool = create_spawn_agent_tool_v2(
        SpawnAgentToolOptions {
            agent_type_description: "role help".to_string(),
            expose_agent_type: false,
            hide_agent_type_model_reasoning: true,
            expose_spawn_agent_model_overrides: true,
            model_catalog_in_context,
            usage_hint_text: None,
            ..Default::default()
        },
        /*description_override*/ None,
    );

    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        ..
    }) = tool
    else {
        panic!("spawn_agent should be a function tool");
    };
    let properties = parameters
        .properties
        .as_ref()
        .expect("spawn_agent should use object params");

    assert!(!properties.contains_key("agent_type"));
    assert!(properties.contains_key("model"));
    assert!(properties.contains_key("reasoning_effort"));
    assert!(!properties.contains_key("service_tier"));
    assert!(!description.contains(SPAWN_AGENT_INHERITED_MODEL_GUIDANCE));
    assert_eq!(
        description.contains(SPAWN_AGENT_INHERITED_MODEL_GUIDANCE_V2),
        model_catalog_in_context
    );
}

#[test]
fn spawn_agent_tool_hides_model_controls_without_override_exposure() {
    let tool = create_spawn_agent_tool_v2(
        SpawnAgentToolOptions {
            agent_type_description: "role help".to_string(),
            expose_agent_type: false,
            hide_agent_type_model_reasoning: true,
            expose_spawn_agent_model_overrides: false,
            usage_hint_text: None,
            ..Default::default()
        },
        Some(""),
    );

    let ToolSpec::Function(ResponsesApiTool {
        description,
        parameters,
        ..
    }) = tool
    else {
        panic!("spawn_agent should be a function tool");
    };
    let properties = parameters
        .properties
        .as_ref()
        .expect("spawn_agent should use object params");

    for property in ["agent_type", "model", "reasoning_effort", "service_tier"] {
        assert!(!properties.contains_key(property));
    }
    assert!(!description.contains(SPAWN_AGENT_INHERITED_MODEL_GUIDANCE));
    assert!(!description.contains("Available model overrides"));
}

#[test]
fn send_message_tool_requires_message_and_has_no_output_schema() {
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = create_send_message_tool()
    else {
        panic!("send_message should be a function tool");
    };
    assert_eq!(
        parameters.schema_type,
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
    let properties = parameters
        .properties
        .as_ref()
        .expect("send_message should use object params");
    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("message"));
    assert_eq!(
        properties
            .get("message")
            .and_then(|schema| schema.encrypted),
        Some(true)
    );
    assert!(!properties.contains_key("interrupt"));
    assert!(!properties.contains_key("items"));
    assert_eq!(
        properties
            .get("target")
            .and_then(|schema| schema.description.as_deref()),
        Some("Relative or canonical task name to message (from spawn_agent).")
    );
    assert_eq!(
        parameters.required.as_deref(),
        Some(["target".to_string(), "message".to_string()].as_slice())
    );
    assert_eq!(output_schema, None);
}

#[test]
fn followup_task_tool_requires_message_and_has_no_output_schema() {
    let ToolSpec::Function(ResponsesApiTool {
        name,
        description,
        parameters,
        output_schema,
        ..
    }) = create_followup_task_tool()
    else {
        panic!("followup_task should be a function tool");
    };
    assert_eq!(name, "followup_task");
    assert_eq!(
        description,
        "Send a follow-up task to an existing non-root target agent and trigger a turn if it is idle. If the target is already running, deliver the task promptly at message boundaries while sampling, or after the pending tool call completes."
    );
    assert_eq!(
        parameters.schema_type,
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
    let properties = parameters
        .properties
        .as_ref()
        .expect("followup_task should use object params");
    assert!(properties.contains_key("target"));
    assert!(properties.contains_key("message"));
    assert_eq!(
        properties
            .get("message")
            .and_then(|schema| schema.encrypted),
        Some(true)
    );
    assert!(!properties.contains_key("items"));
    assert_eq!(
        parameters.required.as_deref(),
        Some(["target".to_string(), "message".to_string()].as_slice())
    );
    assert_eq!(output_schema, None);
}

#[test]
fn wait_agent_tool_v2_uses_timeout_only_summary_output() {
    let ToolSpec::Namespace(namespace) = create_wait_agent_tool_v1(WaitAgentTimeoutOptions {
        default_timeout_ms: 1_800_000,
        min_timeout_ms: 10_000,
        max_timeout_ms: 3_600_000,
    }) else {
        panic!("wait_agent v1 should be a namespace tool");
    };
    let [ResponsesApiNamespaceTool::Function(v1)] = namespace.tools.as_slice() else {
        panic!("wait_agent v1 should contain function tools");
    };
    assert_eq!(v1.name, "wait_agent");

    let ToolSpec::Function(ResponsesApiTool {
        name,
        description,
        parameters,
        output_schema,
        ..
    }) = create_wait_agent_tool_v2(
        WaitAgentTimeoutOptions {
            default_timeout_ms: 1_800_000,
            min_timeout_ms: 10_000,
            max_timeout_ms: 3_600_000,
        },
        WaitAgentForkOptions::default(),
    )
    else {
        panic!("wait_agent should be a function tool");
    };
    assert_eq!(name, "wait_agent");
    assert_eq!(
        parameters.schema_type,
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
    let properties = parameters
        .properties
        .as_ref()
        .expect("wait_agent should use object params");
    assert!(!properties.contains_key("targets"));
    assert!(properties.contains_key("timeout_ms"));
    // Fork tool guidance UC2: the result is a short status, the answers arrive as messages.
    assert!(description.contains(
        "Does not return the content: the result is a short status (completed, interrupted by new user input, no active agents, or a timeout snapshot)"
    ));
    assert!(description.contains("final answers arrive as separate messages right after it"));
    assert!(!description.contains("summary of which agents have updates"));
    assert!(description.contains("For normal background work, omit `timeout_ms`"));
    assert!(description.contains("Passing `0` performs one immediate check"));
    assert_eq!(
        properties
            .get("timeout_ms")
            .and_then(|schema| schema.description.as_deref()),
        Some(
            "Absolute deadline in milliseconds. Omit for the 1800000 ms default; pass 0 for one immediate check. Positive values below 10000 are clamped and values above 3600000 are rejected. Prefer longer waits (minutes) to avoid busy polling."
        )
    );
    assert_eq!(parameters.required.as_ref(), None);
    assert_eq!(
        output_schema.expect("wait output schema").to_value()["properties"]["message"]["description"],
        json!(
            "Brief wait summary without the agent's final content, including any timeout adjustment."
        )
    );
}

#[test]
fn list_agents_tool_includes_path_prefix_and_agent_fields() {
    let ToolSpec::Function(ResponsesApiTool {
        parameters,
        output_schema,
        ..
    }) = create_list_agents_tool()
    else {
        panic!("list_agents should be a function tool");
    };
    assert_eq!(
        parameters.schema_type,
        Some(JsonSchemaType::Single(JsonSchemaPrimitiveType::Object))
    );
    let properties = parameters
        .properties
        .as_ref()
        .expect("list_agents should use object params");
    assert!(properties.contains_key("path_prefix"));
    assert_eq!(
        properties
            .get("path_prefix")
            .and_then(|schema| schema.description.as_deref()),
        Some("Task-path prefix filter without a trailing slash. Omit to list all live agents.")
    );
    assert_eq!(
        output_schema.expect("list_agents output schema").to_value()["properties"]["agents"]["items"]
            ["required"],
        json!(["agent_name", "agent_status"])
    );
}

#[test]
fn list_agents_tool_status_schema_includes_interrupted() {
    let ToolSpec::Function(ResponsesApiTool { output_schema, .. }) = create_list_agents_tool()
    else {
        panic!("list_agents should be a function tool");
    };

    assert_eq!(
        output_schema.expect("list_agents output schema").to_value()["properties"]["agents"]["items"]
            ["properties"]["agent_status"]["allOf"][0]["oneOf"][0]["enum"],
        json!([
            "pending_init",
            "running",
            "interrupted",
            "shutdown",
            "not_found"
        ])
    );
}

/// Fork: `isolation` and the `worktree` result field exist only with the feature enabled (R1.1).
#[test]
fn spawn_agent_tool_v2_exposes_isolation_only_when_enabled() {
    let tool = |worktree_isolation, hide_agent_type_model_reasoning| {
        let ToolSpec::Function(tool) = create_spawn_agent_tool_v2(
            SpawnAgentToolOptions {
                multi_agent_version: MultiAgentVersion::V2,
                hide_agent_type_model_reasoning,
                fork: SpawnAgentForkOptions {
                    worktree_isolation,
                    ..Default::default()
                },
                ..Default::default()
            },
            /*description_override*/ None,
        ) else {
            panic!("spawn_agent should be a function tool");
        };
        tool
    };
    let has_isolation = |tool: &ResponsesApiTool| {
        tool.parameters
            .properties
            .as_ref()
            .is_some_and(|properties| properties.contains_key("isolation"))
    };
    let has_worktree_output = |tool: &ResponsesApiTool| {
        tool.output_schema
            .as_ref()
            .map(codex_tools::ToolOutputSchema::to_value)
            .is_some_and(|schema| schema["properties"].get("worktree").is_some())
    };

    let disabled = tool(false, false);
    assert!(!has_isolation(&disabled));
    assert!(!has_worktree_output(&disabled));

    for hide_metadata in [false, true] {
        let enabled = tool(true, hide_metadata);
        assert!(has_isolation(&enabled));
        assert!(has_worktree_output(&enabled));
        let isolation = enabled
            .parameters
            .properties
            .as_ref()
            .and_then(|properties| properties.get("isolation"))
            .and_then(|schema| serde_json::to_value(schema).ok())
            .expect("isolation schema");
        assert_eq!(isolation["enum"], serde_json::json!(["none", "worktree"]));
        // R4.10, R7.2: the description names the tool first and the block's git command only
        // as the fallback for what the tool refuses.
        let description = isolation["description"].as_str().unwrap_or_default();
        let tool = description.find("apply_agent_worktree");
        let fallback = description.find("`fallback` git command");
        assert!(
            tool.is_some() && fallback.is_some() && tool < fallback,
            "{isolation}"
        );
        assert!(!description.contains("git diff-tree"), "{isolation}");
    }
}

fn wait_agent_description(fork_options: WaitAgentForkOptions) -> String {
    let ToolSpec::Function(ResponsesApiTool { description, .. }) =
        create_wait_agent_tool_v2(WaitAgentTimeoutOptions::default(), fork_options)
    else {
        panic!("wait_agent should be a function tool");
    };
    description
}

fn spawn_agent_tool(
    fork: SpawnAgentForkOptions,
    description_override: Option<&str>,
) -> ResponsesApiTool {
    let ToolSpec::Function(tool) = create_spawn_agent_tool_v2(
        SpawnAgentToolOptions {
            agent_type_description: "Available roles:\nexplorer: {\nRead-only.\n}".to_string(),
            multi_agent_version: MultiAgentVersion::V2,
            fork,
            ..Default::default()
        },
        description_override,
    ) else {
        panic!("spawn_agent should be a function tool");
    };
    tool
}

/// Fork tool guidance UC2, R2.2: agent results do not start a turn, so `wait_agent` tells the
/// model to keep its turn open while agents run; with `wake_on_agent_messages` they do.
#[test]
fn wait_agent_tool_v2_says_whether_agent_results_start_a_turn() {
    let waits = wait_agent_description(WaitAgentForkOptions::default());
    assert!(
        waits.contains("While agents you started are running, do not end your turn: their answers do not start a new turn by themselves"),
        "{waits}"
    );
    assert!(waits.contains("Call wait_agent to collect them"), "{waits}");
    assert!(!waits.contains("background terminal"), "{waits}");
    assert!(!waits.contains("workflow"), "{waits}");

    let wakes = wait_agent_description(WaitAgentForkOptions {
        agent_results_wake: true,
        ..Default::default()
    });
    assert!(
        wakes.contains("their answers start a new turn when they arrive (not in Plan mode or after the user interrupts), so you may end your turn instead of waiting idly"),
        "{wakes}"
    );
    assert!(!wakes.contains("do not end your turn"), "{wakes}");
}

/// Fork tool guidance UC2, R1.4, R2.3: terminals, agents and workflow runs read as one text:
/// terminals may end the turn, agents may not, and workflow runs are not waited for.
#[test]
fn wait_agent_tool_v2_orders_terminals_agents_and_workflow_runs() {
    let description = wait_agent_description(WaitAgentForkOptions {
        exit_notifications: true,
        workflows: true,
        agent_results_wake: false,
    });
    let terminals = description
        .find(" It also returns when a background terminal started with exec_command notify_on_exit or notify_on_output exits")
        .expect("terminal sentence");
    let end_turn = description
        .find("To wait only for terminals, end your turn instead: their notifications start a new turn.")
        .expect("terminals may end the turn");
    let agents = description
        .find("do not end your turn")
        .expect("agents keep the turn open");
    let workflows = description
        .find("It also returns early when a workflow run finishes (its notification follows), but it does not wait for workflow runs")
        .expect("workflow sentence");
    assert!(
        terminals < end_turn && end_turn < agents && agents < workflows,
        "{description}"
    );
    // Report A, 3.5: the parenthesis with the output watch reads as a sentence now.
    assert!(
        description.contains("for a terminal with only an output watch, until its first matching line (awaited a few minutes after your turn ends), then while matching lines keep coming"),
        "{description}"
    );
}

/// A catalog `wait_agent` text keeps the upstream wording without a notifying fork feature and
/// gets the terminal sentence first, then the agent rule, with one.
#[test]
fn wait_agent_catalog_guidance_follows_notifying_features() {
    assert_eq!(
        wait_agent_catalog_guidance(WaitAgentForkOptions::default()),
        ""
    );
    let wakes = wait_agent_catalog_guidance(WaitAgentForkOptions {
        agent_results_wake: true,
        ..Default::default()
    });
    assert_eq!(wakes, "");

    let terminals = wait_agent_catalog_guidance(WaitAgentForkOptions {
        exit_notifications: true,
        ..Default::default()
    });
    assert!(
        terminals.starts_with(" It also returns when a background terminal started with exec_command notify_on_exit or notify_on_output exits"),
        "{terminals}"
    );
    assert!(terminals.contains("do not end your turn"), "{terminals}");

    let workflows = wait_agent_catalog_guidance(WaitAgentForkOptions {
        workflows: true,
        ..Default::default()
    });
    assert!(workflows.contains("do not end your turn"), "{workflows}");
    assert!(workflows.contains("workflow run finishes"), "{workflows}");
}

/// Fork tool guidance UC2, R2.1, R2.3: with a fork feature `spawn_agent` says what spawned
/// agents lack and how to delegate; without one it stays as upstream composed it.
#[test]
fn spawn_agent_tool_v2_fork_guidance_follows_features() {
    let upstream = spawn_agent_tool(SpawnAgentForkOptions::default(), None).description;
    assert!(!upstream.contains("When you delegate"), "{upstream}");
    assert!(upstream.contains(
        "The spawned agent will have the same tools as you and the ability to spawn its own subagents."
    ));

    let explorer = spawn_agent_tool(
        SpawnAgentForkOptions {
            explorer_profile: true,
            wait_agent: true,
            ..Default::default()
        },
        None,
    )
    .description;
    assert!(explorer.contains(
        "The spawned agent will have the same tools as you and the ability to spawn its own subagents; an `explorer` gets only read-only tools and cannot spawn."
    ), "{explorer}");
    for fragment in [
        "When you delegate: an agent costs a full model run and returns only its final answer",
        "Start independent agents in the same response so they run in parallel",
        "With `fork_turns=\"none\"` the message is the agent's only context",
        "Collect their results with `wait_agent` before you end your turn.",
    ] {
        assert!(explorer.contains(fragment), "{fragment}: {explorer}");
    }
    assert!(!explorer.contains("workflow"), "{explorer}");

    let workflows = spawn_agent_tool(
        SpawnAgentForkOptions {
            workflows: true,
            wait_agent: true,
            ..Default::default()
        },
        None,
    )
    .description;
    assert!(workflows.contains(
        "The spawned agent will have the same tools as you except `workflow`, and the ability to spawn its own subagents."
    ), "{workflows}");
    assert!(
        workflows.contains("use the `workflow` tool instead"),
        "{workflows}"
    );
    assert!(
        !workflows.contains("Spawned agents cannot start workflows."),
        "{workflows}"
    );

    // A catalog description has no tool sentence to correct, so the guidance names the gap.
    let catalog = spawn_agent_tool(
        SpawnAgentForkOptions {
            workflows: true,
            agent_results_wake: true,
            wait_agent: true,
            ..Default::default()
        },
        Some("Catalog spawn."),
    )
    .description;
    assert!(catalog.contains("Catalog spawn."), "{catalog}");
    assert!(catalog.contains("When you delegate"), "{catalog}");
    assert!(
        catalog.contains("Spawned agents cannot start workflows."),
        "{catalog}"
    );
    // Results that wake the caller need no reminder to collect them before the turn ends.
    assert!(!catalog.contains("before you end your turn"), "{catalog}");
}

/// Fork tool guidance UC2, R2.1 (report B, P1 #1): the explorer profile drops the upstream
/// "Omit unless explicitly asked" that kept the model from choosing `explorer`.
#[test]
fn spawn_agent_tool_v2_agent_type_invites_a_role_with_the_explorer_profile() {
    let agent_type = |explorer_profile| {
        spawn_agent_tool(
            SpawnAgentForkOptions {
                explorer_profile,
                ..Default::default()
            },
            None,
        )
        .parameters
        .properties
        .as_ref()
        .and_then(|properties| properties.get("agent_type"))
        .and_then(|schema| schema.description.clone())
        .expect("agent_type description")
    };
    let upstream = agent_type(false);
    assert!(
        upstream
            .starts_with("Agent type override for the new agent. Omit unless explicitly asked.")
    );

    let profiled = agent_type(true);
    assert!(
        profiled.starts_with("Role of the new agent: pick the role below whose description fits the task, such as `explorer` for read-only questions about the code, or omit it for a `default` agent. The selected role applies"),
        "{profiled}"
    );
    assert!(
        !profiled.contains("Omit unless explicitly asked"),
        "{profiled}"
    );
    assert!(profiled.contains("explorer: {"), "{profiled}");
}

/// Fork tool guidance UC2, R2.1 (report B, P2 #4): `isolation` says when a worktree pays off,
/// what it costs and that its edits stay invisible until applied.
#[test]
fn spawn_agent_tool_v2_isolation_names_its_cost() {
    let isolation = spawn_agent_tool(
        SpawnAgentForkOptions {
            worktree_isolation: true,
            ..Default::default()
        },
        None,
    )
    .parameters
    .properties
    .as_ref()
    .and_then(|properties| properties.get("isolation"))
    .and_then(|schema| schema.description.clone())
    .expect("isolation description");
    for fragment in [
        "ignored files such as build output and dependencies are not, so builds there start cold",
        "its edits stay invisible to you and other agents until you apply them",
        "Use `worktree` only for an agent that edits files while you or other agents edit the same checkout, never for `explorer` agents",
        "Edits to the same file can still conflict on apply",
    ] {
        assert!(isolation.contains(fragment), "{fragment}: {isolation}");
    }
    assert!(!isolation.contains("without conflicts"), "{isolation}");
}
