//! Fork: explorer profile end to end (specs/explorer-and-questions-2026-10-25.md).

use anyhow::Result;
use codex_core::CodexThread;
use codex_core::TurnInputRequest;
use codex_features::Feature;
use codex_models_manager::bundled_models_response;
use codex_protocol::models::PermissionProfile;
use codex_protocol::models::PermissionProfileSnapshot;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EnvironmentConfigState;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::ResponseMock;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_apply_patch_custom_tool_call;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_exec_command_call;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::environment_config_for_selection;
use core_test_support::test_codex::local_selections;
use core_test_support::test_codex::test_codex;
use core_test_support::test_codex::turn_permission_fields;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use test_case::test_case;
use tokio::time::Instant;
use tokio::time::sleep;
use wiremock::MockServer;

const MULTI_AGENT_V2_NAMESPACE: &str = "collaboration";
const SPAWN_CALL_ID: &str = "spawn-explorer-1";
const PARENT_PROMPT: &str = "parent: ask an explorer";
const PARENT_SEED_PROMPT: &str = "parent seed: remember the secret word pelican";
const PARENT_CHECK_PROMPT: &str = "parent: check the explorer result";
const CHILD_PROMPT: &str = "explorer: inspect the repository";
const PARENT_MODEL: &str = "gpt-5.5";
const PROFILE_MODEL: &str = "gpt-5.6-luna";

fn decoded_body(req: &wiremock::Request) -> String {
    let is_zstd = req
        .headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|entry| entry.trim() == "zstd"));
    let body = if is_zstd {
        zstd::stream::decode_all(std::io::Cursor::new(&req.body)).unwrap_or_default()
    } else {
        req.body.clone()
    };
    String::from_utf8(body).unwrap_or_default()
}

fn is_child_request(req: &wiremock::Request) -> bool {
    req.headers.contains_key("x-codex-parent-thread-id")
}

fn parent_request_containing(
    text: &'static str,
) -> impl Fn(&wiremock::Request) -> bool + Send + Sync + 'static {
    move |req| !is_child_request(req) && decoded_body(req).contains(text)
}

fn child_request_containing(
    text: &'static str,
) -> impl Fn(&wiremock::Request) -> bool + Send + Sync + 'static {
    move |req| is_child_request(req) && decoded_body(req).contains(text)
}

fn child_request_without(
    text: &'static str,
    absent: &'static [&'static str],
) -> impl Fn(&wiremock::Request) -> bool + Send + Sync + 'static {
    move |req| {
        let body = decoded_body(req);
        is_child_request(req) && body.contains(text) && absent.iter().all(|a| !body.contains(a))
    }
}

async fn mount_parent_spawn(server: &MockServer, spawn_args: Value) -> Result<ResponseMock> {
    let spawn_args = serde_json::to_string(&spawn_args)?;
    let first = mount_sse_once_match(
        server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_PROMPT)(req)
                && !decoded_body(req).contains(SPAWN_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-parent-1"),
            ev_function_call_with_namespace(
                SPAWN_CALL_ID,
                MULTI_AGENT_V2_NAMESPACE,
                "spawn_agent",
                &spawn_args,
            ),
            ev_completed("resp-parent-1"),
        ]),
    )
    .await;
    mount_sse_once_match(
        server,
        parent_request_containing(SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-parent-2"),
            ev_assistant_message("msg-parent-2", "parent waits"),
            ev_completed("resp-parent-2"),
        ]),
    )
    .await;
    Ok(first)
}

fn build_test(
    explorer_profile: bool,
    configure: impl FnOnce(&mut codex_core::config::Config) + Send + 'static,
) -> core_test_support::test_codex::TestCodexBuilder {
    test_codex().with_config(move |config| {
        for feature in [Feature::Collab, Feature::MultiAgentV2] {
            config
                .features
                .enable(feature)
                .expect("test config should allow feature update");
        }
        if explorer_profile {
            config
                .features
                .enable(Feature::ExplorerProfile)
                .expect("test config should allow feature update");
        }
        config.model_catalog =
            Some(bundled_models_response().expect("bundled models.json should parse"));
        config.model = Some(PARENT_MODEL.to_string());
        config.model_reasoning_effort = Some(ReasoningEffort::XHigh);
        configure(config);
    })
}

async fn wait_for_child(test: &TestCodex) -> Result<Arc<CodexThread>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let ids = test.thread_manager.list_thread_ids().await;
        if let Some(thread_id) = ids
            .iter()
            .find(|id| **id != test.session_configured.thread_id)
        {
            return test
                .thread_manager
                .get_thread(*thread_id)
                .await
                .map_err(anyhow::Error::from);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for the explorer thread");
        }
        sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_for_request(mock: &ResponseMock) -> Result<ResponsesRequest> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(request) = mock.requests().into_iter().next() {
            return Ok(request);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for a request");
        }
        sleep(Duration::from_millis(10)).await;
    }
}

fn find_tool<'a>(value: &'a Value, name: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if map.get("name").and_then(Value::as_str) == Some(name)
                && map.contains_key("parameters")
            {
                return Some(value);
            }
            map.values().find_map(|value| find_tool(value, name))
        }
        Value::Array(items) => items.iter().find_map(|value| find_tool(value, name)),
        _ => None,
    }
}

/// Names of every tool in a tool list, including the functions inside namespaces.
fn tool_names(tools: &Value) -> Vec<String> {
    let mut names = Vec::new();
    for tool in tools.as_array().into_iter().flatten() {
        if let Some(name) = tool.get("name").and_then(Value::as_str) {
            names.push(name.to_string());
        }
        if let Some(nested) = tool.get("tools") {
            names.extend(tool_names(nested));
        }
    }
    names
}

/// Names of the tools a request offers the model directly (R2.9).
fn offered_tool_names(request: &ResponsesRequest) -> Vec<String> {
    tool_names(&request.body_json()["tools"])
}

fn spawn_parameter_description(request: &ResponsesRequest, parameter: &str) -> Option<String> {
    let body = request.body_json();
    let tool = find_tool(&body["tools"], "spawn_agent")?;
    tool["parameters"]["properties"][parameter]["description"]
        .as_str()
        .map(str::to_string)
}

#[test_case(true; "profile_enabled")]
#[test_case(false; "profile_disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_cannot_write_files(explorer_profile: bool) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    let patch =
        "*** Begin Patch\n*** Add File: explorer-patch.txt\n+written by explorer\n*** End Patch";
    let child_first = mount_sse_once_match(
        &server,
        child_request_without(
            CHILD_PROMPT,
            &["explorer-patch-call", "explorer-shell-call"],
        ),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_apply_patch_custom_tool_call("explorer-patch-call", patch),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let child_second = mount_sse_once_match(
        &server,
        child_request_without("explorer-patch-call", &["explorer-shell-call"]),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_exec_command_call("explorer-shell-call", "printf written > explorer-shell.txt"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;
    let child_last = mount_sse_once_match(
        &server,
        child_request_containing("explorer-shell-call"),
        sse(vec![
            ev_response_created("resp-child-3"),
            ev_assistant_message("msg-child-3", "explorer done"),
            ev_completed("resp-child-3"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |_| {}).build(&server).await?;
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::OnRequest,
        PermissionProfile::Disabled,
    )
    .await?;
    let child = wait_for_child(&test).await?;
    wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let patch_file = test.cwd.path().join("explorer-patch.txt");
    let shell_file = test.cwd.path().join("explorer-shell.txt");
    let child_config = child.config_snapshot().await;
    let patch_output = child_second
        .single_request()
        .custom_tool_call_output_content_and_success("explorer-patch-call")
        .and_then(|(content, _)| content)
        .unwrap_or_default();
    let shell_output = child_last
        .single_request()
        .function_call_output_text("explorer-shell-call")
        .unwrap_or_default();
    let child_developer_text = child_first
        .single_request()
        .message_input_text_groups("developer")
        .concat()
        .join("\n");
    let child_tools = offered_tool_names(&child_first.single_request());

    // R2.9: the explorer does not see `apply_patch`; a call that names it is still refused.
    assert_eq!(
        child_tools.iter().any(|name| name == "apply_patch"),
        !explorer_profile,
        "{child_tools:?}"
    );
    assert!(
        child_tools.iter().any(|name| name == "exec_command"),
        "{child_tools:?}"
    );
    if explorer_profile {
        assert!(!patch_file.exists(), "explorer wrote with apply_patch");
        assert!(!shell_file.exists(), "explorer wrote with a shell command");
        assert!(
            patch_output.contains("read-only sandbox"),
            "apply_patch output: {patch_output}"
        );
        assert!(
            shell_output.contains("Process exited with code") && !shell_output.contains("code 0"),
            "shell output: {shell_output}"
        );
        assert_eq!(child_config.approval_policy, AskForApproval::Never);
        assert!(
            child_config
                .permission_profile
                .file_system_sandbox_policy()
                .entries
                .iter()
                .all(|entry| entry.access
                    != codex_protocol::permissions::FileSystemAccessMode::Write),
            "{:?}",
            child_config.permission_profile
        );
        assert!(
            child_developer_text.contains("# Explorer profile"),
            "{child_developer_text}"
        );
    } else {
        assert!(patch_file.exists(), "control: apply_patch should write");
        assert!(
            shell_file.exists(),
            "control: shell should write: {shell_output}"
        );
        assert_eq!(child_config.approval_policy, AskForApproval::OnRequest);
        assert!(!child_developer_text.contains("# Explorer profile"));
    }
    Ok(())
}

/// Writes a user exec-policy rule file; the parent loads it and children inherit its policy.
fn write_allow_rule(config: &codex_core::config::Config, pattern: &[&str]) {
    let rules_dir = config.codex_home.join("rules");
    std::fs::create_dir_all(&rules_dir).expect("create rules directory");
    let pattern = pattern
        .iter()
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(", ");
    std::fs::write(
        rules_dir.join("explorer-test.rules"),
        format!("prefix_rule(pattern=[{pattern}], decision=\"allow\")\n"),
    )
    .expect("write allow rule");
}

// Review P1: an exec-policy `allow` rule used to run an explorer's command outside its
// read-only sandbox. The control case proves the rule is real and bypasses a read-only
// sandbox for an ordinary child, so the explorer case cannot pass for lack of rules.
#[test_case(true, PermissionProfile::Disabled, false; "explorer_keeps_read_only_sandbox")]
#[test_case(false, PermissionProfile::read_only(), true; "control_rule_bypasses_read_only_child")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_allow_rule_cannot_bypass_sandbox(
    explorer_profile: bool,
    parent_profile: PermissionProfile,
    expect_written: bool,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    mount_sse_once_match(
        &server,
        child_request_without(CHILD_PROMPT, &["explorer-rule-call"]),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_exec_command_call("explorer-rule-call", "touch explorer-rule.txt"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let child_last = mount_sse_once_match(
        &server,
        child_request_containing("explorer-rule-call"),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_assistant_message("msg-child-2", "explorer done"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |config| {
        write_allow_rule(config, &["touch"]);
    })
    .build(&server)
    .await?;
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::Never,
        parent_profile,
    )
    .await?;
    let child = wait_for_child(&test).await?;
    wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let output = child_last
        .single_request()
        .function_call_output_text("explorer-rule-call")
        .unwrap_or_default();
    assert_eq!(
        test.cwd.path().join("explorer-rule.txt").exists(),
        expect_written,
        "allow-rule command output: {output}"
    );
    if !expect_written {
        assert!(
            output.contains("Process exited with code") && !output.contains("code 0"),
            "{output}"
        );
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_cannot_request_escalation() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    let escalated = json!({
        "cmd": "printf escalated > explorer-escalated.txt",
        "sandbox_permissions": "require_escalated",
        "justification": "explorer wants to write",
    });
    mount_sse_once_match(
        &server,
        child_request_without(CHILD_PROMPT, &["explorer-escalate-call"]),
        sse(vec![
            ev_response_created("resp-child-1"),
            core_test_support::responses::ev_exec_command_call_with_args(
                "explorer-escalate-call",
                &escalated,
            ),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let child_last = mount_sse_once_match(
        &server,
        child_request_containing("explorer-escalate-call"),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_assistant_message("msg-child-2", "explorer done"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;

    let test = build_test(/*explorer_profile*/ true, |_| {})
        .build(&server)
        .await?;
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::OnRequest,
        PermissionProfile::Disabled,
    )
    .await?;
    let child = wait_for_child(&test).await?;
    let event = wait_for_event(&child, |event| {
        matches!(
            event,
            EventMsg::ExecApprovalRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;
    assert!(
        matches!(event, EventMsg::TurnComplete(_)),
        "explorer must not ask for escalation: {event:?}"
    );
    let output = child_last
        .single_request()
        .function_call_output_text("explorer-escalate-call")
        .unwrap_or_default();
    assert!(
        output.contains("you cannot ask for escalated permissions"),
        "{output}"
    );
    assert!(!test.cwd.path().join("explorer-escalated.txt").exists());
    Ok(())
}

enum ModelScenario {
    ProfileDisabled,
    ParentModelCheapestEffort,
    ConfiguredModel,
    ExplicitEffort,
}

#[test_case(ModelScenario::ProfileDisabled, PARENT_MODEL, "parent"; "profile_disabled_inherits")]
#[test_case(ModelScenario::ParentModelCheapestEffort, PARENT_MODEL, "low"; "parent_model_cheapest_effort")]
#[test_case(ModelScenario::ConfiguredModel, PROFILE_MODEL, "low"; "configured_model")]
#[test_case(ModelScenario::ExplicitEffort, PARENT_MODEL, "high"; "explicit_effort_wins")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_uses_cheap_model_and_effort(
    scenario: ModelScenario,
    expected_model: &str,
    expected_effort: &str,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mut spawn_args = json!({
        "message": CHILD_PROMPT,
        "task_name": "scout",
        "agent_type": "explorer",
    });
    if matches!(scenario, ModelScenario::ExplicitEffort) {
        spawn_args["reasoning_effort"] = json!("high");
    }
    let parent_first = mount_parent_spawn(&server, spawn_args).await?;
    let child = mount_sse_once_match(
        &server,
        child_request_containing(CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_assistant_message("msg-child-1", "explorer done"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;

    let explorer_profile = !matches!(scenario, ModelScenario::ProfileDisabled);
    let configured_model = matches!(scenario, ModelScenario::ConfiguredModel);
    let test = build_test(explorer_profile, move |config| {
        if configured_model {
            config.explorer_profile.model = Some(PROFILE_MODEL.to_string());
        }
    })
    .build(&server)
    .await?;
    test.submit_turn(PARENT_PROMPT).await?;

    let request = wait_for_request(&child).await?;
    let body = request.body_json();
    assert_eq!(body["model"], expected_model);
    let expected_effort = match expected_effort {
        // Without the profile the child inherits the parent's effective effort.
        "parent" => parent_first.single_request().body_json()["reasoning"]["effort"].clone(),
        effort => json!(effort),
    };
    assert_eq!(body["reasoning"]["effort"], expected_effort);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_rejects_unknown_configured_model() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "task_name": "scout",
        "agent_type": "explorer",
    }))?;
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_PROMPT)(req)
                && !decoded_body(req).contains(SPAWN_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-parent-1"),
            ev_function_call_with_namespace(
                SPAWN_CALL_ID,
                MULTI_AGENT_V2_NAMESPACE,
                "spawn_agent",
                &spawn_args,
            ),
            ev_completed("resp-parent-1"),
        ]),
    )
    .await;
    let parent_followup = mount_sse_once_match(
        &server,
        parent_request_containing(SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-parent-2"),
            ev_assistant_message("msg-parent-2", "parent done"),
            ev_completed("resp-parent-2"),
        ]),
    )
    .await;

    let test = build_test(/*explorer_profile*/ true, |config| {
        config.explorer_profile.model = Some("no-such-model".to_string());
    })
    .build(&server)
    .await?;
    test.submit_turn(PARENT_PROMPT).await?;

    let output = parent_followup
        .single_request()
        .function_call_output_text(SPAWN_CALL_ID)
        .unwrap_or_default();
    assert!(
        output.contains("features.explorer_profile.model"),
        "{output}"
    );
    assert!(output.contains("no-such-model"), "{output}");
    Ok(())
}

// Review P10: an unsupported configured effort must name the config key, like the model.
#[test_case(None; "parent_model")]
#[test_case(Some(PROFILE_MODEL); "configured_model")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_rejects_unsupported_configured_effort(
    configured_model: Option<&'static str>,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let spawn_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "task_name": "scout",
        "agent_type": "explorer",
    }))?;
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_PROMPT)(req)
                && !decoded_body(req).contains(SPAWN_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-parent-1"),
            ev_function_call_with_namespace(
                SPAWN_CALL_ID,
                MULTI_AGENT_V2_NAMESPACE,
                "spawn_agent",
                &spawn_args,
            ),
            ev_completed("resp-parent-1"),
        ]),
    )
    .await;
    let parent_followup = mount_sse_once_match(
        &server,
        parent_request_containing(SPAWN_CALL_ID),
        sse(vec![
            ev_response_created("resp-parent-2"),
            ev_assistant_message("msg-parent-2", "parent done"),
            ev_completed("resp-parent-2"),
        ]),
    )
    .await;

    // `gpt-5.5` stops at `xhigh` and `gpt-5.6-luna` at `max`; neither supports `ultra`.
    let test = build_test(/*explorer_profile*/ true, move |config| {
        config.explorer_profile.model = configured_model.map(str::to_string);
        config.explorer_profile.reasoning_effort = Some(ReasoningEffort::Ultra);
    })
    .build(&server)
    .await?;
    test.submit_turn(PARENT_PROMPT).await?;

    let output = parent_followup
        .single_request()
        .function_call_output_text(SPAWN_CALL_ID)
        .unwrap_or_default();
    assert!(
        output.contains("features.explorer_profile.reasoning_effort"),
        "{output}"
    );
    assert!(output.contains("ultra"), "{output}");
    Ok(())
}

#[test_case(true, None, "explorer", false; "explorer_defaults_to_fresh")]
#[test_case(true, Some("all"), "explorer", true; "explorer_explicit_all_forks")]
#[test_case(true, None, "worker", true; "worker_keeps_full_history")]
#[test_case(false, None, "explorer", true; "profile_disabled_keeps_full_history")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_fork_turns_default(
    explorer_profile: bool,
    fork_turns: Option<&str>,
    agent_type: &str,
    expect_parent_history: bool,
) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_sse_once_match(
        &server,
        parent_request_containing(PARENT_SEED_PROMPT),
        sse(vec![
            ev_response_created("resp-seed-1"),
            ev_assistant_message("msg-seed-1", "seeded"),
            ev_completed("resp-seed-1"),
        ]),
    )
    .await;
    let mut spawn_args = json!({
        "message": CHILD_PROMPT,
        "task_name": "scout",
        "agent_type": agent_type,
    });
    if let Some(fork_turns) = fork_turns {
        spawn_args["fork_turns"] = json!(fork_turns);
    }
    let parent_first = mount_parent_spawn(&server, spawn_args).await?;
    let child = mount_sse_once_match(
        &server,
        child_request_containing(CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_assistant_message("msg-child-1", "explorer done"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |_| {}).build(&server).await?;
    test.submit_turn(PARENT_SEED_PROMPT).await?;
    test.submit_turn(PARENT_PROMPT).await?;

    let request = wait_for_request(&child).await?;
    let body = request.body_json().to_string();
    assert_eq!(
        body.contains(PARENT_SEED_PROMPT),
        expect_parent_history,
        "child request: {body}"
    );
    // Review P2: a full-history fork reuses the parent's context, and still has to carry the
    // explorer instructions exactly once (result limit, `path:line`, no dumps).
    let developer_text = request
        .message_input_text_groups("developer")
        .concat()
        .join("\n");
    let expect_explorer_block = explorer_profile && agent_type == "explorer";
    assert_eq!(
        developer_text.matches("# Explorer profile").count(),
        usize::from(expect_explorer_block),
        "child developer text: {developer_text}"
    );
    if expect_explorer_block {
        for required in [
            "`path:line`",
            "Do not dump whole files",
            "under about 2000 tokens",
        ] {
            assert!(
                developer_text.contains(required),
                "missing {required}: {developer_text}"
            );
        }
    }

    let fork_turns_description =
        spawn_parameter_description(&parent_first.single_request(), "fork_turns")
            .unwrap_or_default();
    assert_eq!(
        fork_turns_description.contains("`none` for `explorer` agents"),
        explorer_profile,
        "{fork_turns_description}"
    );
    Ok(())
}

#[test_case(true; "profile_enabled")]
#[test_case(false; "profile_disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_result_is_capped_for_parent(explorer_profile: bool) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    let long_answer = (1..=5_000)
        .map(|line| format!("finding-{line:05}: see src/lib.rs:{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    mount_sse_once_match(
        &server,
        child_request_containing(CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_assistant_message("msg-child-1", &long_answer),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    // Mail reaches the parent through `wait_agent`, like the upstream completion tests.
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_CHECK_PROMPT)(req)
                && !decoded_body(req).contains("Message Type: FINAL_ANSWER")
        },
        sse(vec![
            ev_response_created("resp-parent-3"),
            ev_function_call_with_namespace(
                "wait-explorer-call",
                MULTI_AGENT_V2_NAMESPACE,
                "wait_agent",
                "{}",
            ),
            ev_completed("resp-parent-3"),
        ]),
    )
    .await;
    let parent_check = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_CHECK_PROMPT)(req)
                && decoded_body(req).contains("Message Type: FINAL_ANSWER")
        },
        sse(vec![
            ev_response_created("resp-parent-4"),
            ev_assistant_message("msg-parent-4", "checked"),
            ev_completed("resp-parent-4"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |_| {}).build(&server).await?;
    test.submit_turn(PARENT_PROMPT).await?;
    let child = wait_for_child(&test).await?;
    let completed =
        wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;
    let EventMsg::TurnComplete(completed) = completed else {
        unreachable!("wait_for_event returns the matched event");
    };
    assert_eq!(
        completed.last_agent_message.as_deref(),
        Some(long_answer.as_str()),
        "the explorer keeps its full answer"
    );
    test.submit_turn(PARENT_CHECK_PROMPT).await?;

    let body = parent_check.single_request().body_json().to_string();
    assert!(
        body.contains("finding-00001"),
        "the head of the answer is kept"
    );
    if explorer_profile {
        assert!(
            !body.contains("finding-02500"),
            "the middle of the answer is truncated"
        );
        assert!(
            body.contains("explorer_profile: this result was truncated to about 2000 tokens"),
            "the truncation is marked"
        );
    } else {
        assert!(body.contains("finding-02500"));
        assert!(!body.contains("explorer_profile: this result was truncated"));
    }
    Ok(())
}

// Review P3: `send_message` used to carry an explorer dump to its parent uncapped. Messages
// to other agents share one `max_result_tokens` budget per explorer turn.
#[test_case(true; "profile_enabled")]
#[test_case(false; "profile_disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_messages_are_capped_for_other_agents(explorer_profile: bool) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    let dump = (1..=3_000)
        .map(|line| format!("dump-{line:05}: src/lib.rs:{line} fn item_{line}()"))
        .collect::<Vec<_>>()
        .join("\n");
    let dump_args = serde_json::to_string(&json!({ "target": "/root", "message": dump }))?;
    let second_args = serde_json::to_string(&json!({
        "target": "/root",
        "message": "second-note-after-the-dump",
    }))?;
    mount_sse_once_match(
        &server,
        child_request_without(CHILD_PROMPT, &["explorer-dump-call", "explorer-note-call"]),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_function_call_with_namespace(
                "explorer-dump-call",
                MULTI_AGENT_V2_NAMESPACE,
                "send_message",
                &dump_args,
            ),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let child_second = mount_sse_once_match(
        &server,
        child_request_without("explorer-dump-call", &["explorer-note-call"]),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_function_call_with_namespace(
                "explorer-note-call",
                MULTI_AGENT_V2_NAMESPACE,
                "send_message",
                &second_args,
            ),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;
    let child_last = mount_sse_once_match(
        &server,
        child_request_containing("explorer-note-call"),
        sse(vec![
            ev_response_created("resp-child-3"),
            ev_assistant_message("msg-child-3", "explorer done"),
            ev_completed("resp-child-3"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_CHECK_PROMPT)(req)
                && !decoded_body(req).contains("Message Type: FINAL_ANSWER")
        },
        sse(vec![
            ev_response_created("resp-parent-3"),
            ev_function_call_with_namespace(
                "wait-explorer-call",
                MULTI_AGENT_V2_NAMESPACE,
                "wait_agent",
                "{}",
            ),
            ev_completed("resp-parent-3"),
        ]),
    )
    .await;
    let parent_check = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_CHECK_PROMPT)(req)
                && decoded_body(req).contains("Message Type: FINAL_ANSWER")
        },
        sse(vec![
            ev_response_created("resp-parent-4"),
            ev_assistant_message("msg-parent-4", "checked"),
            ev_completed("resp-parent-4"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |config| {
        // Children get collaboration tools only on a model that supports MultiAgentV2 children.
        config.model = Some("gpt-5.6-sol".to_string());
        config.explorer_profile.max_result_tokens = 500;
    })
    .build(&server)
    .await?;
    test.submit_turn(PARENT_PROMPT).await?;
    let child = wait_for_child(&test).await?;
    wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;
    test.submit_turn(PARENT_CHECK_PROMPT).await?;

    let body = parent_check.single_request().body_json().to_string();
    let second_output = child_last
        .single_request()
        .function_call_output_text("explorer-note-call")
        .unwrap_or_default();
    let dump_output = child_second
        .single_request()
        .function_call_output_text("explorer-dump-call")
        .unwrap_or_default();
    assert!(
        body.contains("dump-00001"),
        "the head of the message arrives; send_message output: {dump_output}"
    );
    if explorer_profile {
        assert!(
            !body.contains("dump-01500"),
            "the middle of the message is truncated"
        );
        assert!(
            body.contains("explorer_profile: this message was truncated to about 500 tokens"),
            "the truncation is marked"
        );
        assert!(
            !body.contains("second-note-after-the-dump"),
            "a message past the budget is not delivered"
        );
        assert!(
            second_output.contains("explorer_profile") && second_output.contains("final answer"),
            "the explorer is told to use its final answer: {second_output}"
        );
    } else {
        assert!(body.contains("dump-01500"));
        assert!(body.contains("second-note-after-the-dump"));
        assert!(!body.contains("explorer_profile: this message was truncated"));
    }
    Ok(())
}

#[test_case(true; "profile_enabled")]
#[test_case(false; "profile_disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_role_is_offered_with_profile(explorer_profile: bool) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let parent = mount_sse_once_match(
        &server,
        parent_request_containing(PARENT_PROMPT),
        sse(vec![
            ev_response_created("resp-parent-1"),
            ev_assistant_message("msg-parent-1", "no spawn"),
            ev_completed("resp-parent-1"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |_| {}).build(&server).await?;
    test.submit_turn(PARENT_PROMPT).await?;

    let description = spawn_parameter_description(&parent.single_request(), "agent_type");
    if explorer_profile {
        let description = description.expect("agent_type is exposed with the profile");
        assert!(description.contains("explorer: {"), "{description}");
        assert!(
            description.contains("Explorers are read-only"),
            "{description}"
        );
    } else {
        assert_eq!(description, None);
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_clamps_owner_provided_environment() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    mount_sse_once_match(
        &server,
        child_request_without(CHILD_PROMPT, &["explorer-owner-call"]),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_exec_command_call("explorer-owner-call", "printf owner > explorer-owner.txt"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        child_request_containing("explorer-owner-call"),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_assistant_message("msg-child-2", "explorer done"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;

    let test = build_test(/*explorer_profile*/ true, |_| {})
        .build(&server)
        .await?;
    // The parent's owner (for example an app-server client) supplies full access directly.
    let mut environments = local_selections(test.config.cwd.clone());
    let selection = environments
        .environments
        .first_mut()
        .expect("local environment selection");
    let mut owner_config = environment_config_for_selection(&test.config, selection);
    owner_config.permission_profile =
        PermissionProfileSnapshot::legacy(PermissionProfile::Disabled);
    selection.config = EnvironmentConfigState::Ready(owner_config);
    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, test.cwd.path());
    test.codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: PARENT_PROMPT.to_string(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                environments: Some(environments.into_requests()),
                approval_policy: Some(AskForApproval::OnRequest),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                ..Default::default()
            }),
        )
        .await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    let child = wait_for_child(&test).await?;
    wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    assert!(
        !test.cwd.path().join("explorer-owner.txt").exists(),
        "explorer wrote through the owner-provided environment"
    );
    let child_environments = child.config_snapshot().await.environments;
    let EnvironmentConfigState::Ready(child_config) = &child_environments.environments[0].config
    else {
        panic!("explorer environment should be ready: {child_environments:?}");
    };
    assert_eq!(
        child_config.permission_profile.permission_profile(),
        &PermissionProfile::read_only()
    );

    // A later owner update is narrowed the same way.
    let child_selection = child_environments.environments[0].clone();
    let mut wider_config = child_config.clone();
    wider_config.permission_profile =
        PermissionProfileSnapshot::legacy(PermissionProfile::Disabled);
    child
        .environment_ready(&child_selection, wider_config)
        .await?;
    let updated = child.config_snapshot().await.environments;
    let EnvironmentConfigState::Ready(updated_config) = &updated.environments[0].config else {
        panic!("explorer environment should stay ready: {updated:?}");
    };
    assert_eq!(
        updated_config.permission_profile.permission_profile(),
        &PermissionProfile::read_only()
    );

    // Review P8: an owner that changes the explorer thread's own settings with a ready
    // full-access environment is narrowed as well.
    let mut owner_update = local_selections(test.config.cwd.clone());
    let owner_selection = owner_update
        .environments
        .first_mut()
        .expect("local environment selection");
    let mut owner_update_config = environment_config_for_selection(&test.config, owner_selection);
    owner_update_config.permission_profile =
        PermissionProfileSnapshot::legacy(PermissionProfile::Disabled);
    owner_selection.config = EnvironmentConfigState::Ready(owner_update_config);
    core_test_support::submit_thread_settings(
        &child,
        ThreadSettingsOverrides {
            environments: Some(owner_update.into_requests()),
            ..Default::default()
        },
    )
    .await?;
    let settled = child.config_snapshot().await.environments;
    let EnvironmentConfigState::Ready(settled_config) = &settled.environments[0].config else {
        panic!("explorer environment should stay ready: {settled:?}");
    };
    assert_eq!(
        settled_config.permission_profile.permission_profile(),
        &PermissionProfile::read_only()
    );
    Ok(())
}

fn git_in(cwd: &std::path::Path, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn init_git_checkout(cwd: &std::path::Path) -> Result<()> {
    git_in(cwd, &["init", "--quiet", "-b", "main"])?;
    git_in(cwd, &["config", "user.name", "Codex Test"])?;
    git_in(cwd, &["config", "user.email", "codex@example.invalid"])?;
    std::fs::write(cwd.join("a.txt"), "base\n")?;
    git_in(cwd, &["add", "."])?;
    git_in(
        cwd,
        &["commit", "--quiet", "--no-gpg-sign", "-m", "initial"],
    )
}

// Review P7: with `default_isolation = "worktree"` every explorer got a worktree it never
// writes to, and under a full-access parent its note said the parent checkout was writable.
// R9.3: the worktree note of an explorer says it is read-only and gives no edit instructions,
// also when a full-history fork carries the note in the copied history.
#[test_case(None, None, false; "default_isolation_skips_explorers")]
#[test_case(Some("worktree"), None, true; "explicit_worktree_note_is_read_only")]
#[test_case(Some("worktree"), Some("all"), true; "full_fork_worktree_note_is_read_only")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_worktree_isolation_stays_read_only(
    isolation: Option<&str>,
    fork_turns: Option<&str>,
    expect_worktree: bool,
) -> Result<()> {
    skip_if_no_network!(Ok(()));
    if cfg!(windows) {
        return Ok(());
    }

    let server = start_mock_server().await;
    let mut spawn_args = json!({
        "message": CHILD_PROMPT,
        "task_name": "scout",
        "agent_type": "explorer",
    });
    if let Some(isolation) = isolation {
        spawn_args["isolation"] = json!(isolation);
    }
    if let Some(fork_turns) = fork_turns {
        spawn_args["fork_turns"] = json!(fork_turns);
    }
    mount_parent_spawn(&server, spawn_args).await?;
    let child = mount_sse_once_match(
        &server,
        child_request_containing(CHILD_PROMPT),
        sse(vec![
            ev_response_created("resp-child-1"),
            ev_assistant_message("msg-child-1", "explorer done"),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;

    let test = build_test(/*explorer_profile*/ true, |config| {
        config
            .features
            .enable(Feature::AgentWorktreeIsolation)
            .expect("test config should allow feature update");
        config.agent_worktree_isolation.default_isolation =
            codex_features::AgentIsolation::Worktree;
        config.workspace_roots = vec![config.cwd.clone()];
    })
    .with_workspace_setup(|cwd, _fs| async move { init_git_checkout(cwd.as_path()) })
    .build(&server)
    .await?;
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::Never,
        PermissionProfile::Disabled,
    )
    .await?;

    let request = wait_for_request(&child).await?;
    let developer_text = request
        .message_input_text_groups("developer")
        .concat()
        .join("\n");
    let child_thread = wait_for_child(&test).await?;
    let child_cwd = child_thread.config_snapshot().await.cwd().clone();
    assert_eq!(
        developer_text.contains("isolated git worktree"),
        expect_worktree,
        "{developer_text}"
    );
    assert_eq!(
        child_cwd != test.config.cwd,
        expect_worktree,
        "child cwd {}",
        child_cwd.display()
    );
    if expect_worktree {
        assert!(
            developer_text.contains("are read-only for you"),
            "{developer_text}"
        );
        assert!(
            developer_text.contains("you are a read-only explorer"),
            "{developer_text}"
        );
        for edit_instruction in ["still writable", "Keep all edits", "records your changes"] {
            assert!(
                !developer_text.contains(edit_instruction),
                "{edit_instruction}: {developer_text}"
            );
        }
    }
    Ok(())
}

/// MCP fixture for review P4: `peek` is annotated read-only; `poke` (not destructive, closed
/// world) and `plain` (no annotations, server auto-approves) write the file named in `path`.
const MCP_PROBE_SERVER: &str = r#"
import json
import sys

def send(message):
    print(json.dumps({"jsonrpc": "2.0", **message}), flush=True)

schema = {"type": "object", "properties": {"path": {"type": "string"}}}
tools = [
    {"name": "peek", "inputSchema": schema, "annotations": {"readOnlyHint": True}},
    {"name": "poke", "inputSchema": schema,
     "annotations": {"readOnlyHint": False, "destructiveHint": False, "openWorldHint": False}},
    {"name": "plain", "inputSchema": schema},
]
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": request["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "probe", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": tools}
    elif method == "tools/call":
        name = request["params"]["name"]
        path = (request["params"].get("arguments") or {}).get("path")
        if name != "peek" and path:
            with open(path, "w") as handle:
                handle.write(name)
        result = {"content": [{"type": "text", "text": "probe-" + name + "-ran"}]}
    elif method == "resources/list":
        result = {"resources": []}
    elif method == "resources/templates/list":
        result = {"resourceTemplates": []}
    elif "id" not in request:
        continue
    else:
        result = {}
    send({"id": request["id"], "result": result})
"#;

const EXPLORER_TOOLS_PROMPT: &str = "explorer: call the probe tools";
const PROBE_SEARCH_CALL_ID: &str = "explorer-probe-search";

/// Names of the tools a `tool_search` call returned, including namespace children.
fn searched_tool_names(request: &ResponsesRequest, call_id: &str) -> Vec<String> {
    tool_names(&request.tool_search_output(call_id)["tools"])
}

// Review P4: MCP tools that need no approval (auto-approved servers, non-destructive hints)
// ran outside the sandbox and could write. Explorers may only call tools annotated
// `readOnlyHint: true`; client-defined dynamic tools carry no annotations and are refused.
// R2.9: the explorer does not see those tools either, neither in its tool list nor in
// `tool_search` (MCP tools are deferred for models with tool search).
#[test_case(true; "profile_enabled")]
#[test_case(false; "profile_disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_calls_only_read_only_mcp_tools(explorer_profile: bool) -> Result<()> {
    use codex_core::StartThreadOptions;
    use codex_protocol::dynamic_tools::DynamicToolFunctionSpec;
    use codex_protocol::dynamic_tools::DynamicToolSpec;
    use codex_protocol::protocol::SessionSource;
    use codex_protocol::protocol::SubAgentSource;

    skip_if_no_network!(Ok(()));
    core_test_support::skip_if_wine_exec!(
        Ok(()),
        "the MCP fixture requires a host Python interpreter"
    );

    let server = start_mock_server().await;
    let test = build_test(explorer_profile, |_| {}).build(&server).await?;
    let poke_path = test.cwd.path().join("explorer-mcp-poke.txt");
    let plain_path = test.cwd.path().join("explorer-mcp-plain.txt");
    let path_args = |path: &std::path::Path| {
        serde_json::to_string(&json!({ "path": path.display().to_string() }))
    };
    let mut calls = vec![
        ev_response_created("resp-explorer-1"),
        ev_function_call_with_namespace(
            "mcp-poke-call",
            "mcp__probe",
            "poke",
            &path_args(&poke_path)?,
        ),
        ev_function_call_with_namespace(
            "mcp-plain-call",
            "mcp__probe",
            "plain",
            &path_args(&plain_path)?,
        ),
        ev_function_call_with_namespace("mcp-peek-call", "mcp__probe", "peek", "{}"),
    ];
    // Without the profile the client would have to answer the dynamic tool; the control only
    // checks that the MCP writes happen.
    if explorer_profile {
        calls.push(core_test_support::responses::ev_function_call(
            "dynamic-echo-call",
            "client_echo",
            &json!({ "message": "hi" }).to_string(),
        ));
    }
    calls.push(ev_completed("resp-explorer-1"));
    let search = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            let body = decoded_body(req);
            body.contains(EXPLORER_TOOLS_PROMPT) && !body.contains(PROBE_SEARCH_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-explorer-0"),
            core_test_support::responses::ev_tool_search_call(
                PROBE_SEARCH_CALL_ID,
                &json!({ "query": "probe peek poke plain", "limit": 8 }),
            ),
            ev_completed("resp-explorer-0"),
        ]),
    )
    .await;
    let after_search = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            let body = decoded_body(req);
            body.contains(PROBE_SEARCH_CALL_ID) && !body.contains("mcp-poke-call")
        },
        sse(calls),
    )
    .await;
    let follow_up = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| decoded_body(req).contains("mcp-poke-call"),
        sse(vec![
            ev_response_created("resp-explorer-2"),
            ev_assistant_message("msg-explorer-2", "explorer done"),
            ev_completed("resp-explorer-2"),
        ]),
    )
    .await;

    let mut config = test.config.clone();
    config.mcp_servers.set(serde_json::from_value(json!({
        "probe": {
            "command": if cfg!(windows) { "python" } else { "python3" },
            "args": ["-u", "-c", MCP_PROBE_SERVER],
            "default_tools_approval_mode": "approve"
        }
    }))?)?;
    let session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: test.session_configured.thread_id,
        depth: 1,
        agent_path: None,
        agent_nickname: None,
        agent_role: Some("explorer".to_string()),
    });
    let thread = test
        .thread_manager
        .start_thread(StartThreadOptions {
            session_source: Some(session_source),
            environments: Some(vec![test.executor_environment().request()]),
            dynamic_tools: vec![DynamicToolSpec::Function(DynamicToolFunctionSpec {
                name: "client_echo".to_string(),
                description: "Echo a message through the client.".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {"message": {"type": "string"}},
                }),
                defer_loading: false,
            })],
            ..StartThreadOptions::new(config)
        })
        .await?
        .thread;
    core_test_support::wait_for_mcp_server(&thread, "probe").await?;
    thread
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: EXPLORER_TOOLS_PROMPT.to_string(),
            text_elements: Vec::new(),
        }]))
        .await?;
    let event = wait_for_event(&thread, |event| {
        matches!(
            event,
            EventMsg::DynamicToolCallRequest(_) | EventMsg::TurnComplete(_)
        )
    })
    .await;
    assert!(
        matches!(event, EventMsg::TurnComplete(_)),
        "the explorer must not reach the client tool: {event:?}"
    );

    // R2.9: the tool list and `tool_search` offer only what the explorer may call.
    let offered = offered_tool_names(&search.single_request());
    assert_eq!(
        offered.iter().any(|name| name == "client_echo"),
        !explorer_profile,
        "{offered:?}"
    );
    let searched = searched_tool_names(&after_search.single_request(), PROBE_SEARCH_CALL_ID);
    assert!(searched.iter().any(|name| name == "peek"), "{searched:?}");
    for hidden in ["poke", "plain"] {
        assert_eq!(
            searched.iter().any(|name| name == hidden),
            !explorer_profile,
            "{hidden}: {searched:?}"
        );
    }

    let request = follow_up.single_request();
    // MCP results arrive as content items, so compare the serialized output.
    let output = |call_id: &str| request.function_call_output(call_id)["output"].to_string();
    assert!(
        output("mcp-peek-call").contains("probe-peek-ran"),
        "{}",
        output("mcp-peek-call")
    );
    if explorer_profile {
        assert!(!poke_path.exists(), "explorer ran a non read-only MCP tool");
        assert!(!plain_path.exists(), "explorer ran an unannotated MCP tool");
        for call_id in ["mcp-poke-call", "mcp-plain-call"] {
            let text = output(call_id);
            assert!(
                text.contains("explorer_profile") && text.contains("readOnlyHint"),
                "{call_id}: {text}"
            );
        }
        let dynamic = output("dynamic-echo-call");
        assert!(dynamic.contains("explorer_profile"), "{dynamic}");
    } else {
        assert!(poke_path.exists(), "control: poke should write");
        assert!(plain_path.exists(), "control: plain should write");
    }
    Ok(())
}

const SPAWNED_SEARCH_CALL_ID: &str = "spawned-explorer-probe-search";

// R2.9 on the real spawn path: an explorer that a parent spawns (not a thread started with an
// explorer source) finds the parent's read-only MCP tool through `tool_search`, and only it.
#[test_case(true; "profile_enabled")]
#[test_case(false; "profile_disabled")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_explorer_finds_only_read_only_mcp_tools(explorer_profile: bool) -> Result<()> {
    skip_if_no_network!(Ok(()));
    core_test_support::skip_if_wine_exec!(
        Ok(()),
        "the MCP fixture requires a host Python interpreter"
    );

    let server = start_mock_server().await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
            "fork_turns": "none",
        }),
    )
    .await?;
    mount_sse_once_match(
        &server,
        child_request_without(CHILD_PROMPT, &[SPAWNED_SEARCH_CALL_ID]),
        sse(vec![
            ev_response_created("resp-child-1"),
            core_test_support::responses::ev_tool_search_call(
                SPAWNED_SEARCH_CALL_ID,
                &json!({ "query": "probe peek poke plain", "limit": 8 }),
            ),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let after_search = mount_sse_once_match(
        &server,
        child_request_containing(SPAWNED_SEARCH_CALL_ID),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_assistant_message("msg-child-2", "explorer done"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;

    let test = build_test(explorer_profile, |config| {
        config
            .mcp_servers
            .set(
                serde_json::from_value(json!({
                    "probe": {
                        "command": if cfg!(windows) { "python" } else { "python3" },
                        "args": ["-u", "-c", MCP_PROBE_SERVER],
                        "default_tools_approval_mode": "approve"
                    }
                }))
                .expect("probe MCP server config"),
            )
            .expect("test config should accept the probe MCP server");
    })
    .build(&server)
    .await?;
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::Never,
        PermissionProfile::Disabled,
    )
    .await?;
    let child = wait_for_child(&test).await?;
    wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let request = wait_for_request(&after_search).await?;
    let searched = searched_tool_names(&request, SPAWNED_SEARCH_CALL_ID);
    assert!(searched.iter().any(|name| name == "peek"), "{searched:?}");
    for hidden in ["poke", "plain"] {
        assert_eq!(
            searched.iter().any(|name| name == hidden),
            !explorer_profile,
            "{hidden}: {searched:?}"
        );
    }
    Ok(())
}

/// MCP fixture whose first start (the parent's, which fills the catalog cache) is quick and
/// whose later starts take `argv[2]` seconds: a spawned explorer that started it would wait.
const SLOW_PROBE_SERVER: &str = r#"
import json
import os
import sys
import time

marker = sys.argv[1]
if os.path.exists(marker):
    time.sleep(float(sys.argv[2]))
else:
    open(marker, "w").close()

def send(message):
    print(json.dumps({"jsonrpc": "2.0", **message}), flush=True)

schema = {"type": "object", "properties": {"path": {"type": "string"}}}
tools = [
    {"name": "peek", "inputSchema": schema, "annotations": {"readOnlyHint": True}},
    {"name": "plain", "inputSchema": schema},
]
for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": request["params"]["protocolVersion"],
                  "capabilities": {"tools": {}},
                  "serverInfo": {"name": "probe", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": tools}
    elif method == "tools/call":
        result = {"content": [{"type": "text", "text": "probe-ran"}]}
    elif method == "resources/list":
        result = {"resources": []}
    elif method == "resources/templates/list":
        result = {"resourceTemplates": []}
    elif "id" not in request:
        continue
    else:
        result = {}
    send({"id": request["id"], "result": result})
"#;

const SLOW_SEARCH_CALL_ID: &str = "slow-explorer-probe-search";

// Review 2026-10-27 P2 (R2.9): a spawned explorer took its MCP catalog from the cache without
// annotations and therefore started and waited for every MCP server on its first step (8 s for
// a slow server, `startup_timeout_sec` for a hanging one). The cache now keeps the server's
// `readOnlyHint` as a visibility hint, so the explorer starts at once and still finds only the
// read-only tool.
#[test_case(8, None; "slow_server")]
#[test_case(1000, Some(4); "hanging_server")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spawned_explorer_does_not_wait_for_mcp_startup(
    later_start_secs: u64,
    startup_timeout_sec: Option<u64>,
) -> Result<()> {
    skip_if_no_network!(Ok(()));
    core_test_support::skip_if_wine_exec!(
        Ok(()),
        "the MCP fixture requires a host Python interpreter"
    );

    let server = start_mock_server().await;
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            let body = decoded_body(req);
            !is_child_request(req)
                && body.contains(PARENT_SEED_PROMPT)
                && !body.contains("seed-peek-call")
        },
        sse(vec![
            ev_response_created("resp-seed-1"),
            ev_function_call_with_namespace("seed-peek-call", "mcp__probe", "peek", "{}"),
            ev_completed("resp-seed-1"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            let body = decoded_body(req);
            !is_child_request(req)
                && body.contains("seed-peek-call")
                && !body.contains(PARENT_PROMPT)
        },
        sse(vec![
            ev_response_created("resp-seed-2"),
            ev_assistant_message("msg-seed-2", "seeded"),
            ev_completed("resp-seed-2"),
        ]),
    )
    .await;
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
            "fork_turns": "none",
        }),
    )
    .await?;
    let first = mount_sse_once_match(
        &server,
        child_request_without(CHILD_PROMPT, &[SLOW_SEARCH_CALL_ID]),
        sse(vec![
            ev_response_created("resp-child-1"),
            core_test_support::responses::ev_tool_search_call(
                SLOW_SEARCH_CALL_ID,
                &json!({ "query": "probe peek plain", "limit": 8 }),
            ),
            ev_completed("resp-child-1"),
        ]),
    )
    .await;
    let after_search = mount_sse_once_match(
        &server,
        child_request_containing(SLOW_SEARCH_CALL_ID),
        sse(vec![
            ev_response_created("resp-child-2"),
            ev_assistant_message("msg-child-2", "explorer done"),
            ev_completed("resp-child-2"),
        ]),
    )
    .await;

    let marker_dir = tempfile::tempdir()?;
    let marker = marker_dir.path().join("started").display().to_string();
    let test = build_test(/*explorer_profile*/ true, move |config| {
        let mut probe = json!({
            "command": if cfg!(windows) { "python" } else { "python3" },
            "args": ["-u", "-c", SLOW_PROBE_SERVER, marker, later_start_secs.to_string()],
            "default_tools_approval_mode": "approve"
        });
        if let Some(timeout) = startup_timeout_sec {
            probe["startup_timeout_sec"] = json!(timeout);
        }
        config
            .mcp_servers
            .set(
                serde_json::from_value(json!({ "probe": probe })).expect("probe MCP server config"),
            )
            .expect("test config should accept the probe MCP server");
    })
    .build(&server)
    .await?;
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_SEED_PROMPT,
        AskForApproval::Never,
        PermissionProfile::Disabled,
    )
    .await?;
    let started = Instant::now();
    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::Never,
        PermissionProfile::Disabled,
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(60);
    while first.requests().is_empty() {
        anyhow::ensure!(Instant::now() < deadline, "no explorer request");
        sleep(Duration::from_millis(20)).await;
    }
    let first_request_after = started.elapsed();
    let request = wait_for_request(&after_search).await?;
    let searched = searched_tool_names(&request, SLOW_SEARCH_CALL_ID);
    assert!(searched.iter().any(|name| name == "peek"), "{searched:?}");
    assert!(!searched.iter().any(|name| name == "plain"), "{searched:?}");
    assert!(
        first_request_after < Duration::from_secs(3),
        "the explorer waited {first_request_after:?} for MCP startup"
    );
    Ok(())
}

const GRANDCHILD_PROMPT: &str = "grandchild: touch the review file";
const GRAND_SPAWN_CALL_ID: &str = "explorer-grand-spawn";
const GRANDCHILD_RULE_CALL_ID: &str = "grandchild-rule-call";

/// The thread that sent a request: its `x-codex-parent-thread-id`, `None` for the root.
fn parent_thread_header(req: &wiremock::Request) -> Option<String> {
    req.headers
        .get("x-codex-parent-thread-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// Whether a request declares the `spawn_agent` tool. Responses Lite models (all V2 models)
/// carry their tool declarations in the input, so this looks for a declaration (an object
/// with that `name` and `parameters`) anywhere in the body.
fn offers_spawn_agent(request: &ResponsesRequest) -> bool {
    find_tool(&request.body_json(), "spawn_agent").is_some()
}

// Review 2026-10-27 P1 (R2.10): an explorer on a V2 model (`gpt-6-*`) could spawn an ordinary
// child. The child inherited the read-only sandbox and approval `never`, but not the explorer's
// rules, so an exec-policy allow rule ran its command outside the sandbox. An explorer spawns
// no agents at all, whatever the spawn asks for; the control (profile off) shows that the
// grandchild's allow-rule command really writes.
#[test_case(true, json!({"task_name": "writer", "fork_turns": "none"}); "fresh_default_child")]
#[test_case(true, json!({"task_name": "writer", "agent_type": "worker", "fork_turns": "none"}); "worker_child")]
#[test_case(true, json!({"task_name": "writer", "fork_turns": "all"}); "full_history_fork")]
#[test_case(true, json!({"task_name": "writer", "agent_type": "explorer"}); "explorer_child")]
#[test_case(false, json!({"task_name": "writer", "fork_turns": "none"}); "control_child_writes")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explorer_cannot_spawn_agents(explorer_profile: bool, spawn: Value) -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let test = build_test(explorer_profile, |config| {
        write_allow_rule(config, &["touch"]);
        // A V2 model gives every child `spawn_agent` (models.json `multi_agent_version`).
        config.model = Some("gpt-6-astra".to_string());
        config.explorer_profile.model = Some("gpt-6-astra".to_string());
    })
    .build(&server)
    .await?;
    let root = test.session_configured.thread_id.to_string();
    mount_parent_spawn(
        &server,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "scout",
            "agent_type": "explorer",
        }),
    )
    .await?;
    let mut spawn = spawn;
    spawn["message"] = json!(GRANDCHILD_PROMPT);
    let spawn = serde_json::to_string(&spawn)?;
    let explorer_root = root.clone();
    let explorer_first = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_thread_header(req).as_deref() == Some(explorer_root.as_str())
                && !decoded_body(req).contains(GRAND_SPAWN_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-explorer-1"),
            ev_function_call_with_namespace(
                GRAND_SPAWN_CALL_ID,
                MULTI_AGENT_V2_NAMESPACE,
                "spawn_agent",
                &spawn,
            ),
            ev_completed("resp-explorer-1"),
        ]),
    )
    .await;
    let explorer_root = root.clone();
    let explorer_last = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_thread_header(req).as_deref() == Some(explorer_root.as_str())
                && decoded_body(req).contains(GRAND_SPAWN_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-explorer-2"),
            ev_assistant_message("msg-explorer-2", "explorer done"),
            ev_completed("resp-explorer-2"),
        ]),
    )
    .await;
    let grandchild_root = root.clone();
    let grandchild_first = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_thread_header(req).is_some_and(|parent| parent != grandchild_root)
                && !decoded_body(req).contains(GRANDCHILD_RULE_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-grandchild-1"),
            ev_exec_command_call(GRANDCHILD_RULE_CALL_ID, "touch grandchild-rule.txt"),
            ev_completed("resp-grandchild-1"),
        ]),
    )
    .await;
    let grandchild_root = root;
    let grandchild_last = mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_thread_header(req).is_some_and(|parent| parent != grandchild_root)
                && decoded_body(req).contains(GRANDCHILD_RULE_CALL_ID)
        },
        sse(vec![
            ev_response_created("resp-grandchild-2"),
            ev_assistant_message("msg-grandchild-2", "grandchild done"),
            ev_completed("resp-grandchild-2"),
        ]),
    )
    .await;

    test.submit_turn_with_approval_and_permission_profile(
        PARENT_PROMPT,
        AskForApproval::Never,
        PermissionProfile::Disabled,
    )
    .await?;
    let explorer_request = wait_for_request(&explorer_first).await?;
    let spawn_output = wait_for_request(&explorer_last)
        .await?
        .function_call_output_text(GRAND_SPAWN_CALL_ID)
        .unwrap_or_default();
    let refused = spawn_output.contains("explorer_profile: a read-only explorer cannot spawn");
    if !refused {
        // The grandchild started: wait until its allow-rule command ran.
        let grandchild = wait_for_request(&grandchild_last).await?;
        eprintln!(
            "grandchild command output: {}",
            grandchild
                .function_call_output_text(GRANDCHILD_RULE_CALL_ID)
                .unwrap_or_default()
        );
    }
    let written = test.cwd.path().join("grandchild-rule.txt").exists();
    assert_eq!(
        written, !explorer_profile,
        "file written through a child of the explorer; spawn output: {spawn_output}"
    );
    assert_eq!(refused, explorer_profile, "spawn output: {spawn_output}");
    assert_eq!(
        offers_spawn_agent(&explorer_request),
        !explorer_profile,
        "{}",
        explorer_request.body_json()
    );
    if explorer_profile {
        assert!(
            grandchild_first.requests().is_empty(),
            "a child of the explorer reached the model"
        );
    }
    Ok(())
}

/// Explorer fan-out (Decision log 2026-10-27): an explorer does not spawn agents, so the
/// parent fans out explorers itself. One parent step with two `spawn_agent` calls starts two
/// explorers, each with the explorer profile, a read-only sandbox and no `spawn_agent`, and
/// each answers its own task.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parent_fans_out_explorers_in_one_step() -> Result<()> {
    skip_if_no_network!(Ok(()));

    const SCOUT_A: &str = "explorer A: map the parser";
    const SCOUT_B: &str = "explorer B: map the renderer";
    let server = start_mock_server().await;
    let spawn = |message: &str, task_name: &str| {
        json!({ "message": message, "task_name": task_name, "agent_type": "explorer" }).to_string()
    };
    mount_sse_once_match(
        &server,
        move |req: &wiremock::Request| {
            parent_request_containing(PARENT_PROMPT)(req)
                && !decoded_body(req).contains("spawn-scout-a")
        },
        sse(vec![
            ev_response_created("resp-parent-1"),
            ev_function_call_with_namespace(
                "spawn-scout-a",
                MULTI_AGENT_V2_NAMESPACE,
                "spawn_agent",
                &spawn(SCOUT_A, "scout_a"),
            ),
            ev_function_call_with_namespace(
                "spawn-scout-b",
                MULTI_AGENT_V2_NAMESPACE,
                "spawn_agent",
                &spawn(SCOUT_B, "scout_b"),
            ),
            ev_completed("resp-parent-1"),
        ]),
    )
    .await;
    mount_sse_once_match(
        &server,
        parent_request_containing("spawn-scout-b"),
        sse(vec![
            ev_response_created("resp-parent-2"),
            ev_assistant_message("msg-parent-2", "parent waits"),
            ev_completed("resp-parent-2"),
        ]),
    )
    .await;
    let mut explorers = Vec::new();
    for (scout, id) in [(SCOUT_A, "a"), (SCOUT_B, "b")] {
        explorers.push((
            scout,
            mount_sse_once_match(
                &server,
                child_request_containing(scout),
                sse(vec![
                    ev_response_created(&format!("resp-scout-{id}")),
                    ev_assistant_message(&format!("msg-scout-{id}"), &format!("map {id} done")),
                    ev_completed(&format!("resp-scout-{id}")),
                ]),
            )
            .await,
        ));
    }

    let test = build_test(/*explorer_profile*/ true, |_| {})
        .build(&server)
        .await?;
    test.submit_turn(PARENT_PROMPT).await?;

    let deadline = Instant::now() + Duration::from_secs(20);
    let children = loop {
        let children: Vec<_> = test
            .thread_manager
            .list_thread_ids()
            .await
            .into_iter()
            .filter(|id| *id != test.session_configured.thread_id)
            .collect();
        if children.len() == 2 {
            break children;
        }
        assert!(Instant::now() < deadline, "explorers: {children:?}");
        sleep(Duration::from_millis(20)).await;
    };
    let mut answers = Vec::new();
    for child_id in children {
        let child = test.thread_manager.get_thread(child_id).await?;
        let completed =
            wait_for_event(&child, |event| matches!(event, EventMsg::TurnComplete(_))).await;
        let EventMsg::TurnComplete(completed) = completed else {
            unreachable!("wait_for_event returns the matched event");
        };
        answers.push(completed.last_agent_message.unwrap_or_default());
        let child_config = child.config_snapshot().await;
        assert!(
            child_config
                .permission_profile
                .file_system_sandbox_policy()
                .entries
                .iter()
                .all(|entry| entry.access
                    != codex_protocol::permissions::FileSystemAccessMode::Write),
            "{:?}",
            child_config.permission_profile
        );
        assert_eq!(child_config.approval_policy, AskForApproval::Never);
    }
    answers.sort();
    assert_eq!(answers, vec!["map a done", "map b done"]);
    for (scout, explorer) in explorers {
        let request = wait_for_request(&explorer).await?;
        let developer_text = request
            .message_input_text_groups("developer")
            .concat()
            .join("\n");
        assert!(
            developer_text.contains("# Explorer profile"),
            "{scout}: {developer_text}"
        );
        assert!(!offers_spawn_agent(&request), "{}", request.body_json());
    }
    Ok(())
}
