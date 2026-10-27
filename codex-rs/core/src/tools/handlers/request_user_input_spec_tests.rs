use super::*;
use codex_features::Feature;
use codex_features::Features;
use codex_protocol::config_types::ModeKind;
use codex_protocol::request_user_input::RequestUserInputQuestion;
use codex_protocol::request_user_input::RequestUserInputQuestionOption;
use codex_tools::JsonSchema;
use codex_tools::request_user_input_available_modes;
use pretty_assertions::assert_eq;
use std::collections::BTreeMap;

fn default_mode_enabled_available_modes() -> Vec<ModeKind> {
    let mut features = Features::with_defaults();
    features.enable(Feature::DefaultModeRequestUserInput);
    request_user_input_available_modes(&features)
}

fn default_available_modes() -> Vec<ModeKind> {
    request_user_input_available_modes(&Features::with_defaults())
}

#[test]
fn request_user_input_tool_includes_questions_schema() {
    assert_eq!(
        create_request_user_input_tool("Ask the user to choose.".to_string()),
        ToolSpec::Function(ResponsesApiTool {
            name: "request_user_input".to_string(),
            description: "Ask the user to choose.".to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(BTreeMap::from([
                (
                    "questions".to_string(),
                    JsonSchema::array(
                        JsonSchema::object(
                            BTreeMap::from([
                                (
                                    "header".to_string(),
                                    JsonSchema::string(Some(
                                        "Short header label shown in the UI (12 or fewer chars)."
                                            .to_string(),
                                    )),
                                ),
                                (
                                    "id".to_string(),
                                    JsonSchema::string(Some(
                                        "Stable identifier for mapping answers (snake_case)."
                                            .to_string(),
                                    )),
                                ),
                                (
                                    "options".to_string(),
                                    JsonSchema::array(
                                        JsonSchema::object(
                                            BTreeMap::from([
                                                (
                                                    "description".to_string(),
                                                    JsonSchema::string(Some(
                                                        "One short sentence explaining impact/tradeoff if selected."
                                                            .to_string(),
                                                    )),
                                                ),
                                                (
                                                    "label".to_string(),
                                                    JsonSchema::string(Some(
                                                        "User-facing label (1-5 words)."
                                                            .to_string(),
                                                    )),
                                                ),
                                            ]),
                                            Some(vec![
                                                "label".to_string(),
                                                "description".to_string(),
                                            ]),
                                            Some(false.into()),
                                        ),
                                        Some(
                                            "Provide 2-3 mutually exclusive choices. Put the recommended option first and suffix its label with \"(Recommended)\". Do not include an \"Other\" option in this list; the client will add a free-form \"Other\" option automatically."
                                                .to_string(),
                                        ),
                                    ),
                                ),
                                (
                                    "question".to_string(),
                                    JsonSchema::string(Some(
                                        "Single-sentence prompt shown to the user.".to_string(),
                                    )),
                                ),
                            ]),
                            Some(vec![
                                "id".to_string(),
                                "header".to_string(),
                                "question".to_string(),
                                "options".to_string(),
                            ]),
                            Some(false.into()),
                        ),
                        Some(
                            "Questions to show the user. Prefer 1 and do not exceed 3".to_string(),
                        ),
                    ),
                ),
            ]),
            Some(vec!["questions".to_string()]),
            Some(false.into())),
            output_schema: None,
        })
    );
}

#[test]
fn normalize_request_user_input_tool_args_sets_other_on_every_question() {
    let args = RequestUserInputToolArgs {
        questions: vec![RequestUserInputQuestion {
            id: "confirm".to_string(),
            header: "Confirm".to_string(),
            question: "Proceed?".to_string(),
            is_other: false,
            is_secret: false,
            options: Some(vec![RequestUserInputQuestionOption {
                label: "Yes (Recommended)".to_string(),
                description: "Continue.".to_string(),
            }]),
        }],
    };

    assert_eq!(
        normalize_request_user_input_tool_args(args.clone()),
        Ok(RequestUserInputToolArgs {
            questions: vec![RequestUserInputQuestion {
                is_other: true,
                ..args.questions[0].clone()
            }],
        })
    );
}

#[test]
fn normalize_request_user_input_tool_args_rejects_missing_options() {
    let args = RequestUserInputToolArgs {
        questions: vec![RequestUserInputQuestion {
            id: "confirm".to_string(),
            header: "Confirm".to_string(),
            question: "Proceed?".to_string(),
            is_other: false,
            is_secret: false,
            options: None,
        }],
    };

    assert_eq!(
        normalize_request_user_input_tool_args(args),
        Err("request_user_input requires non-empty options for every question".to_string())
    );
}

#[test]
fn request_user_input_unavailable_messages_respect_default_mode_feature_flag() {
    assert_eq!(
        request_user_input_unavailable_message(ModeKind::Plan, &default_available_modes()),
        None
    );
    assert_eq!(
        request_user_input_unavailable_message(ModeKind::Default, &default_available_modes()),
        Some("request_user_input is unavailable in Default mode".to_string())
    );
    assert_eq!(
        request_user_input_unavailable_message(
            ModeKind::Default,
            &default_mode_enabled_available_modes()
        ),
        None
    );
}

#[test]
fn request_user_input_tool_description_mentions_available_modes() {
    assert_eq!(
        request_user_input_tool_description(&default_available_modes()),
        "Request user input for one to three short questions and wait for the response. This tool is only available in Plan mode.".to_string()
    );
    // Fork: Default mode adds guidance on when to ask (specs/explorer-and-questions-2026-10-25.md).
    assert_eq!(
        request_user_input_tool_description(&default_mode_enabled_available_modes()),
        format!(
            "Request user input for one to three short questions and wait for the response. This tool is only available in Default or Plan mode.\n{DEFAULT_MODE_GUIDANCE}"
        )
    );
    assert_eq!(
        request_user_input_tool_description(&[ModeKind::Default]),
        format!(
            "Request user input for one to three short questions and wait for the response. This tool is only available in Default mode.\n{DEFAULT_MODE_GUIDANCE}"
        )
    );
}

#[test]
fn default_mode_guidance_limits_questions_to_optional_forks() {
    // Mirrors the upstream Default-mode template: the tool is for optional questions only.
    assert!(DEFAULT_MODE_GUIDANCE.contains("ask only optional questions"));
    assert!(DEFAULT_MODE_GUIDANCE.contains("Do not ask for permission"));
    assert!(DEFAULT_MODE_GUIDANCE.contains("if no answer arrives"));
}

// Review P5: an irreversible decision is not an optional question. Neither text may let the
// model take such an action on its recommended option when nobody answered.
#[test]
fn default_mode_texts_keep_irreversible_actions_with_the_user() {
    for text in [DEFAULT_MODE_GUIDANCE, DEFAULT_MODE_NO_ANSWER_MESSAGE] {
        assert!(text.contains("irreversible or destructive"), "{text}");
        assert!(text.contains("in plain text"), "{text}");
    }
    assert!(
        DEFAULT_MODE_GUIDANCE.contains("do not take such an action until the user answers"),
        "{DEFAULT_MODE_GUIDANCE}"
    );
    assert!(
        DEFAULT_MODE_NO_ANSWER_MESSAGE.contains("do not take it without the user"),
        "{DEFAULT_MODE_NO_ANSWER_MESSAGE}"
    );
    assert!(
        !DEFAULT_MODE_GUIDANCE.contains("irreversible or costly decision the user should own"),
        "irreversible decisions are no longer a reason to ask with this tool"
    );
}

// Tails review 2026-10-27, P3-11: the guidance is added whether or not the model catalog lists
// the async question tool, so it names no such tool; it still steers Default-mode questions
// to this one.
#[test]
fn default_mode_guidance_names_no_tool_the_catalog_may_lack() {
    assert!(
        !DEFAULT_MODE_GUIDANCE.contains("request_user_input_async"),
        "{DEFAULT_MODE_GUIDANCE}"
    );
    assert!(
        DEFAULT_MODE_GUIDANCE.contains(
            "To ask in Default mode, use this tool rather than an asynchronous question tool"
        ),
        "{DEFAULT_MODE_GUIDANCE}"
    );
}

// Review P6: the description is shared by every mode, so the guidance names Default mode and
// leaves Plan mode's own question rules untouched.
#[test]
fn default_mode_guidance_does_not_change_plan_mode() {
    assert!(DEFAULT_MODE_GUIDANCE.starts_with("In Default mode"));
    assert!(DEFAULT_MODE_GUIDANCE.ends_with("Plan mode keeps its own rules for this tool."));
    assert_eq!(
        DEFAULT_MODE_GUIDANCE.matches("Plan mode").count(),
        1,
        "the only mention of Plan mode leaves it unchanged"
    );
}

#[test]
fn request_user_input_modes_for_session_exclude_sub_agents() {
    use codex_protocol::ThreadId;
    use codex_protocol::protocol::SessionSource;
    use codex_protocol::protocol::SubAgentSource;

    let mut features = Features::with_defaults();
    features.enable(Feature::DefaultModeRequestUserInput);
    let subagent = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: ThreadId::new(),
        depth: 1,
        agent_path: None,
        agent_nickname: None,
        agent_role: None,
    });

    assert_eq!(
        request_user_input_modes_for_session(&features, &SessionSource::Cli),
        default_mode_enabled_available_modes()
    );
    // `codex exec` turns the feature off for its own sessions instead.
    assert_eq!(
        request_user_input_modes_for_session(&features, &SessionSource::Exec),
        default_mode_enabled_available_modes()
    );
    assert_eq!(
        request_user_input_modes_for_session(&features, &subagent),
        default_available_modes()
    );
    assert_eq!(
        request_user_input_modes_for_session(&Features::with_defaults(), &SessionSource::Cli),
        default_available_modes()
    );
}
