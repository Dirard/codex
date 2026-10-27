#![allow(clippy::unwrap_used)]

use codex_core::TurnInputRequest;
use core_test_support::test_codex::local_requests;
use std::collections::HashMap;

use codex_features::Feature;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::Settings;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::request_user_input::RequestUserInputAnswer;
use codex_protocol::request_user_input::RequestUserInputResponse;
use codex_protocol::user_input::UserInput;
use core_test_support::TempDirExt;
use core_test_support::responses;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_completed_with_tokens;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::test_codex;
use core_test_support::test_codex::turn_permission_fields;
use core_test_support::wait_for_event;
use core_test_support::wait_for_event_match;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tokio::time::Duration;
use tokio::time::timeout;

fn call_output(req: &ResponsesRequest, call_id: &str) -> String {
    let raw = req.function_call_output(call_id);
    assert_eq!(
        raw.get("call_id").and_then(Value::as_str),
        Some(call_id),
        "mismatched call_id in function_call_output"
    );
    let (content_opt, _success) = req
        .function_call_output_content_and_success(call_id)
        .expect("function_call_output present");
    content_opt.expect("function_call_output content present")
}

fn call_output_content_and_success(
    req: &ResponsesRequest,
    call_id: &str,
) -> (String, Option<bool>) {
    let raw = req.function_call_output(call_id);
    assert_eq!(
        raw.get("call_id").and_then(Value::as_str),
        Some(call_id),
        "mismatched call_id in function_call_output"
    );
    let (content_opt, success) = req
        .function_call_output_content_and_success(call_id)
        .expect("function_call_output present");
    let content = content_opt.expect("function_call_output content present");
    (content, success)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_round_trip_resolves_pending() -> anyhow::Result<()> {
    request_user_input_round_trip_for_mode(ModeKind::Plan).await
}

async fn request_user_input_round_trip_for_mode(mode: ModeKind) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    let builder = test_codex();
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = builder
        .with_config(move |config| {
            if mode == ModeKind::Default {
                config
                    .features
                    .enable(Feature::DefaultModeRequestUserInput)
                    .expect("test config should allow feature update");
            }
        })
        .build(&server)
        .await?;

    let call_id = "user-input-call";
    let expected_is_blocking = mode == ModeKind::Plan;
    let request_args = json!({
        "questions": [{
            "id": "confirm_path",
            "header": "Confirm",
            "question": "Proceed with the plan?",
            "options": [{
                "label": "Yes (Recommended)",
                "description": "Continue the current plan."
            }, {
                "label": "No",
                "description": "Stop and revisit the approach."
            }]
        }]
    });
    let request_args = request_args.to_string();

    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_function_call(call_id, "request_user_input", &request_args),
        ev_rate_limits(),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once(&server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "thanks"),
        ev_completed("resp-2"),
    ]);
    let second_mock = responses::mount_sse_once(&server, second_response).await;

    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, cwd.path());

    codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "please confirm".into(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                environments: Some(local_requests(cwd.abs())),
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                collaboration_mode: Some(CollaborationMode {
                    mode,
                    settings: Settings {
                        model: session_configured.model.clone(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            }),
        )
        .await?;

    let request = wait_for_event_match(&codex, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    assert_eq!(request.call_id, call_id);
    assert_eq!(request.questions.len(), 1);
    assert_eq!(request.is_blocking, expected_is_blocking);
    assert_eq!(request.auto_resolution_ms, None);
    assert_eq!(request.questions[0].is_other, true);
    assert!(
        timeout(Duration::from_millis(200), async {
            loop {
                let event = codex
                    .next_event()
                    .await
                    .expect("event stream should stay open");
                if matches!(event.msg, EventMsg::TokenCount(_)) {
                    return;
                }
            }
        })
        .await
        .is_err(),
        "TokenCount should wait until request_user_input resolves"
    );

    let mut answers = HashMap::new();
    answers.insert(
        "confirm_path".to_string(),
        RequestUserInputAnswer {
            answers: vec!["yes".to_string()],
        },
    );
    let response = RequestUserInputResponse { answers };
    codex
        .submit(Op::UserInputAnswer {
            id: request.turn_id.clone(),
            response,
        })
        .await?;

    wait_for_event(&codex, |event| matches!(event, EventMsg::TokenCount(_))).await;
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = second_mock.single_request();
    let output_text = call_output(&req, call_id);
    let output_json: Value = serde_json::from_str(&output_text)?;
    assert_eq!(
        output_json,
        json!({
            "answers": {
                "confirm_path": { "answers": ["yes"] }
            }
        })
    );

    Ok(())
}

fn ev_rate_limits() -> Value {
    json!({
        "type": "codex.rate_limits",
        "plan_type": "plus",
        "rate_limits": {
            "allowed": true,
            "limit_reached": false,
            "primary": {
                "used_percent": 42,
                "window_minutes": 60,
                "reset_at": 1700000000
            },
            "secondary": null
        },
        "code_review_rate_limits": null,
        "credits": null,
        "promo": null
    })
}

#[test_case::test_case("response.completed"; "completed")]
#[test_case::test_case("response.incomplete"; "incomplete")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_interrupt_emits_deferred_token_count(
    terminal_type: &str,
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = test_codex().build(&server).await?;

    let call_id = "user-input-interrupt";
    let request_args = json!({
        "questions": [{
            "id": "confirm_path",
            "header": "Confirm",
            "question": "Proceed with the plan?",
            "options": [{
                "label": "Yes (Recommended)",
                "description": "Continue the current plan."
            }, {
                "label": "No",
                "description": "Stop and revisit the approach."
            }]
        }]
    })
    .to_string();

    let mut terminal = ev_completed_with_tokens("resp-interrupt", /*total_tokens*/ 77);
    terminal["type"] = json!(terminal_type);
    terminal["response"]["incomplete_details"] = json!({"reason": "max_output_tokens"});
    let response = sse(vec![
        ev_response_created("resp-interrupt"),
        ev_function_call(call_id, "request_user_input", &request_args),
        terminal,
    ]);
    responses::mount_sse_once(&server, response).await;

    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, cwd.path());
    codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "please confirm".into(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                environments: Some(local_requests(cwd.abs())),
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                collaboration_mode: Some(CollaborationMode {
                    mode: ModeKind::Plan,
                    settings: Settings {
                        model: session_configured.model,
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            }),
        )
        .await?;

    let request = wait_for_event_match(&codex, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;

    codex.submit(Op::Interrupt).await?;

    let token_count = wait_for_event_match(&codex, |event| match event {
        EventMsg::TokenCount(token_count) => Some(token_count.clone()),
        _ => None,
    })
    .await;
    assert_eq!(
        token_count
            .info
            .map(|info| info.total_token_usage.total_tokens),
        Some(77)
    );
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnAborted(_))).await;

    assert_eq!(request.call_id, call_id);
    Ok(())
}

async fn assert_request_user_input_rejected<F>(mode_name: &str, build_mode: F) -> anyhow::Result<()>
where
    F: FnOnce(String) -> CollaborationMode,
{
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;

    let mut builder = test_codex();
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = builder.build(&server).await?;

    let mode_slug = mode_name.to_lowercase().replace(' ', "-");
    let call_id = format!("user-input-{mode_slug}-call");
    let request_args = json!({
        "questions": [{
            "id": "confirm_path",
            "header": "Confirm",
            "question": "Proceed with the plan?",
            "options": [{
                "label": "Yes (Recommended)",
                "description": "Continue the current plan."
            }, {
                "label": "No",
                "description": "Stop and revisit the approach."
            }]
        }]
    })
    .to_string();

    let first_response = sse(vec![
        ev_response_created("resp-1"),
        ev_function_call(&call_id, "request_user_input", &request_args),
        ev_completed("resp-1"),
    ]);
    responses::mount_sse_once(&server, first_response).await;

    let second_response = sse(vec![
        ev_assistant_message("msg-1", "thanks"),
        ev_completed("resp-2"),
    ]);
    let second_mock = responses::mount_sse_once(&server, second_response).await;

    let session_model = session_configured.model.clone();
    let collaboration_mode = build_mode(session_model.clone());
    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, cwd.path());

    codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "please confirm".into(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                environments: Some(local_requests(cwd.abs())),
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                collaboration_mode: Some(collaboration_mode),
                ..Default::default()
            }),
        )
        .await?;

    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = second_mock.single_request();
    let (output, success) = call_output_content_and_success(&req, &call_id);
    assert_eq!(success, None);
    assert_eq!(
        output,
        format!("request_user_input is unavailable in {mode_name} mode")
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_rejected_in_default_mode_by_default() -> anyhow::Result<()> {
    assert_request_user_input_rejected("Default", |model| CollaborationMode {
        mode: ModeKind::Default,
        settings: Settings {
            model,
            reasoning_effort: None,
            developer_instructions: None,
        },
    })
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_round_trip_in_default_mode_with_feature() -> anyhow::Result<()> {
    request_user_input_round_trip_for_mode(ModeKind::Default).await
}

/// Fork: resolves one request with `answers` and returns the tool output the model receives.
async fn request_user_input_output_for_answers(
    mode: ModeKind,
    answers: HashMap<String, RequestUserInputAnswer>,
) -> anyhow::Result<(String, Option<bool>)> {
    let server = start_mock_server().await;
    let TestCodex {
        codex,
        cwd,
        session_configured,
        ..
    } = test_codex()
        .with_config(|config| {
            config
                .features
                .enable(Feature::DefaultModeRequestUserInput)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await?;

    let call_id = "user-input-no-answer";
    let request_args = json!({
        "questions": [{
            "id": "storage",
            "header": "Storage",
            "question": "Which storage backend should the cache use?",
            "options": [{
                "label": "SQLite (Recommended)",
                "description": "Persistent and already a dependency."
            }, {
                "label": "In memory",
                "description": "Fast but lost on restart."
            }]
        }]
    })
    .to_string();
    responses::mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("resp-1"),
            ev_function_call(call_id, "request_user_input", &request_args),
            ev_completed("resp-1"),
        ]),
    )
    .await;
    let second_mock = responses::mount_sse_once(
        &server,
        sse(vec![
            ev_assistant_message("msg-1", "continuing"),
            ev_completed("resp-2"),
        ]),
    )
    .await;

    let (sandbox_policy, permission_profile) =
        turn_permission_fields(PermissionProfile::Disabled, cwd.path());
    codex
        .start_or_steer_turn(
            TurnInputRequest::user_input(vec![UserInput::Text {
                text: "add a cache".into(),
                text_elements: Vec::new(),
            }])
            .with_thread_settings(ThreadSettingsOverrides {
                environments: Some(local_requests(cwd.abs())),
                approval_policy: Some(AskForApproval::Never),
                sandbox_policy: Some(sandbox_policy),
                permission_profile,
                collaboration_mode: Some(CollaborationMode {
                    mode,
                    settings: Settings {
                        model: session_configured.model.clone(),
                        reasoning_effort: None,
                        developer_instructions: None,
                    },
                }),
                ..Default::default()
            }),
        )
        .await?;
    let request = wait_for_event_match(&codex, |event| match event {
        EventMsg::RequestUserInput(request) => Some(request.clone()),
        _ => None,
    })
    .await;
    codex
        .submit(Op::UserInputAnswer {
            id: request.turn_id.clone(),
            response: RequestUserInputResponse { answers },
        })
        .await?;
    wait_for_event(&codex, |event| matches!(event, EventMsg::TurnComplete(_))).await;

    let req = second_mock.single_request();
    Ok(call_output_content_and_success(&req, call_id))
}

#[test_case::test_case(HashMap::new(); "no answers")]
#[test_case::test_case(HashMap::from([(
    "storage".to_string(),
    RequestUserInputAnswer { answers: vec!["  ".to_string()] },
)]); "blank answer")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_default_mode_without_answer_tells_model_to_continue(
    answers: HashMap<String, RequestUserInputAnswer>,
) -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let (output, _success) =
        request_user_input_output_for_answers(ModeKind::Default, answers).await?;
    assert!(
        output.starts_with("No answer was received"),
        "Default-mode output without an answer: {output}"
    );
    assert!(output.contains("recommended option"), "{output}");
    assert!(output.contains("Do not ask again"), "{output}");
    // Review P5: no answer never authorizes an irreversible action.
    assert!(
        output.contains("irreversible or destructive")
            && output.contains("do not take it without the user"),
        "{output}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_default_mode_answer_is_returned_as_json() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let (output, _success) = request_user_input_output_for_answers(
        ModeKind::Default,
        HashMap::from([(
            "storage".to_string(),
            RequestUserInputAnswer {
                answers: vec!["SQLite (Recommended)".to_string()],
            },
        )]),
    )
    .await?;
    let output: Value = serde_json::from_str(&output)?;
    assert_eq!(
        output,
        json!({"answers": {"storage": {"answers": ["SQLite (Recommended)"]}}})
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn request_user_input_plan_mode_empty_answers_stay_json() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let (output, _success) =
        request_user_input_output_for_answers(ModeKind::Plan, HashMap::new()).await?;
    let output: Value = serde_json::from_str(&output)?;
    assert_eq!(output, json!({"answers": {}}));
    Ok(())
}

/// Fork, combined live run 2026-10-27 D2: with Default-mode questions the model also keeps
/// the upstream `request_user_input_async` of its catalog (upstream offers both under this
/// flag), and it asked through the async tool, whose answer never came. The description of
/// `request_user_input` now tells the model to ask with it in Default mode.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_mode_questions_point_the_model_at_the_blocking_tool() -> anyhow::Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let mock = responses::mount_sse_once(
        &server,
        sse(vec![ev_response_created("resp-1"), ev_completed("resp-1")]),
    )
    .await;
    let test = test_codex()
        .with_model_info_override("gpt-5.2", |model| {
            model
                .experimental_supported_tools
                .push("request_user_input_async".to_string());
        })
        .with_config(|config| {
            config
                .features
                .enable(Feature::DefaultModeRequestUserInput)
                .expect("test config should allow feature update");
        })
        .build(&server)
        .await?;
    test.submit_turn("Pick an approach.").await?;

    let body = mock.single_request().body_json();
    let tools = body["tools"].as_array().expect("request tools");
    let description = |name: &str| {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .and_then(|tool| tool["description"].as_str())
            .map(str::to_string)
    };
    assert!(
        description("request_user_input_async").is_some(),
        "{tools:?}"
    );
    let blocking = description("request_user_input").expect("request_user_input");
    assert!(
        blocking.contains(
            "To ask in Default mode, use this tool rather than an asynchronous question tool"
        ),
        "{blocking}"
    );
    // Only some catalogs list the async tool, so the shared guidance names none (P3-11).
    assert!(!blocking.contains("request_user_input_async"), "{blocking}");
    Ok(())
}
