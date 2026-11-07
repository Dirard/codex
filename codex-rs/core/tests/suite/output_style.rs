//! Fork: output styles (`specs/output-styles-2026-11-07.md`): the world-state section
//! `output_style` in requests, `CodexThread::output_styles` / `set_output_style`, resume.

use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use codex_core::TurnInputRequest;
use codex_core::output_style::OutputStyleError;
use codex_core::output_style::OutputStyleSource;
use codex_protocol::config_types::CollaborationMode;
use codex_protocol::config_types::ModeKind;
use codex_protocol::config_types::Settings;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::user_input::UserInput;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::mount_response_sequence;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_completed;
use core_test_support::responses::sse_response;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

const OPEN: &str = "<output_style>";
const DEFAULT_NOTICE: &str = "The default output style is active again.";
const ENABLED: &str = "[features]\noutput_styles = true\n";

fn first(name: &str) -> String {
    format!("{OPEN}\nOutput style: `{name}`.\n")
}

fn replaced(name: &str) -> String {
    format!("{OPEN}\nThe user switched the output style to `{name}`.")
}

fn restated(name: &str) -> String {
    format!("{OPEN}\nOutput style: `{name}`. It stays in effect after the model change.")
}

/// The output style fragments of a request, in order.
fn style_fragments(request: &ResponsesRequest) -> Vec<String> {
    request
        .message_input_texts("developer")
        .into_iter()
        .filter(|text| text.starts_with(OPEN))
        .collect()
}

/// The fragments `request` has beyond those of `previous`.
fn new_style_fragments(previous: &ResponsesRequest, request: &ResponsesRequest) -> Vec<String> {
    let before = style_fragments(previous);
    let after = style_fragments(request);
    assert!(after.starts_with(&before), "earlier fragments must stay");
    after[before.len()..].to_vec()
}

/// A builder whose `config.toml` is the current content of `config` on every build.
fn builder_with_config(config: Arc<Mutex<String>>) -> TestCodexBuilder {
    test_codex().with_pre_build_hook(move |home| {
        let contents = config.lock().expect("config lock").clone();
        std::fs::write(home.join("config.toml"), contents).expect("write config.toml");
    })
}

fn config_with(style: Option<&str>, enabled: bool) -> Arc<Mutex<String>> {
    Arc::new(Mutex::new(config_text(style, enabled)))
}

fn config_text(style: Option<&str>, enabled: bool) -> String {
    let mut config = String::new();
    if let Some(style) = style {
        config.push_str(&format!("output_style = \"{style}\"\n"));
    }
    if enabled {
        config.push_str(ENABLED);
    }
    config
}

fn completed_turns(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| sse_completed(&format!("resp-{index}")))
        .collect()
}

/// R3.1, R3.8: the first request carries the style once in the developer context, and the
/// base instructions match a thread without a style.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_request_announces_the_style_once() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let requests = mount_sse_sequence(&server, completed_turns(2)).await;

    let styled = builder_with_config(config_with(Some("outcome-first"), true))
        .build(&server)
        .await?;
    styled.submit_turn("hello").await?;
    let plain = builder_with_config(config_with(None, true))
        .build(&server)
        .await?;
    plain.submit_turn("hello").await?;

    let requests = requests.requests();
    assert_eq!(requests.len(), 2);
    let fragments = style_fragments(&requests[0]);
    assert_eq!(fragments.len(), 1, "{fragments:?}");
    assert!(
        fragments[0].starts_with(&first("outcome-first")),
        "{}",
        fragments[0]
    );
    assert!(fragments[0].contains("Start with the outcome"));
    assert_eq!(style_fragments(&requests[1]), Vec::<String>::new());
    assert_eq!(
        requests[0].instructions_text(),
        requests[1].instructions_text()
    );
    assert!(!requests[0].instructions_text().contains(OPEN));
    Ok(())
}

/// R3.2, R3.4, R4.2: switching between turns adds one message per change, keeps earlier
/// input, tools and base instructions, and adds nothing while the style stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_styles_appends_one_message_per_change() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let requests = mount_sse_sequence(&server, completed_turns(4)).await;
    let test = builder_with_config(config_with(Some("outcome-first"), true))
        .build(&server)
        .await?;

    test.submit_turn("one").await?;
    let style = test.codex.set_output_style("Explanatory").await?;
    assert_eq!(style.name, "explanatory");
    test.submit_turn("two").await?;
    test.submit_turn("three").await?;
    test.codex.set_output_style("default").await?;
    test.submit_turn("four").await?;

    let requests = requests.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(style_fragments(&requests[0]).len(), 1);
    let added = |index: usize| new_style_fragments(&requests[index - 1], &requests[index]);
    let second = added(1);
    assert_eq!(second.len(), 1, "{second:?}");
    assert!(
        second[0].starts_with(&replaced("explanatory")),
        "{}",
        second[0]
    );
    assert_eq!(added(2), Vec::<String>::new());
    let fourth = added(3);
    assert_eq!(fourth.len(), 1, "{fourth:?}");
    assert!(fourth[0].contains(DEFAULT_NOTICE), "{}", fourth[0]);
    for pair in requests.windows(/*size*/ 2) {
        assert!(
            pair[1].input().starts_with(&pair[0].input()),
            "style changes must be append-only"
        );
        assert_eq!(pair[1].instructions_text(), pair[0].instructions_text());
        assert_eq!(pair[1].body_json()["tools"], pair[0].body_json()["tools"]);
    }
    Ok(())
}

/// R3.9: a style chosen while a turn runs applies from the turn's next model step.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn style_change_during_a_turn_applies_from_the_next_step() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let requests = mount_response_sequence(
        &server,
        vec![
            sse_response(sse(vec![
                ev_function_call("call-1", "unsupported_tool", "{}"),
                ev_completed("resp-1"),
            ]))
            .set_delay(Duration::from_millis(1500)),
            sse_response(sse(vec![
                ev_assistant_message("msg-2", "done"),
                ev_completed("resp-2"),
            ])),
        ],
    )
    .await;
    let test = builder_with_config(config_with(None, true))
        .build(&server)
        .await?;

    test.codex
        .start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text {
            text: "work".into(),
            text_elements: Vec::new(),
        }]))
        .await?;
    tokio::time::timeout(Duration::from_secs(10), async {
        while requests.requests().is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    test.codex.set_output_style("learning").await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let requests = requests.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(style_fragments(&requests[0]), Vec::<String>::new());
    let added = new_style_fragments(&requests[0], &requests[1]);
    assert_eq!(added.len(), 1, "{added:?}");
    assert!(added[0].starts_with(&replaced("learning")), "{}", added[0]);
    Ok(())
}

/// A text turn that runs on `model`, as the TUI sends it after a model change.
fn turn_on_model(test: &TestCodex, text: &str, model: &str) -> TurnInputRequest {
    TurnInputRequest::user_input(vec![UserInput::Text {
        text: text.into(),
        text_elements: Vec::new(),
    }])
    .with_thread_settings(ThreadSettingsOverrides {
        collaboration_mode: Some(CollaborationMode {
            mode: ModeKind::Default,
            settings: Settings {
                model: model.to_string(),
                reasoning_effort: test.config.model_reasoning_effort.clone(),
                developer_instructions: None,
            },
        }),
        ..Default::default()
    })
}

/// R3.3: after a model change an active style is restated after `<model_switch>`; without a
/// style no message is added.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn model_change_restates_an_active_style() -> Result<()> {
    skip_if_no_network!(Ok(()));
    // The override also makes the fixture model the thread's first model.
    const FIRST_MODEL: &str = "gpt-5.6-terra";
    const NEXT_MODEL: &str = "gpt-5.5";
    for style in [Some("outcome-first"), None] {
        let server = start_mock_server().await;
        let requests = mount_sse_sequence(&server, completed_turns(2)).await;
        let mut builder = builder_with_config(config_with(style, true)).with_model_info_override(
            FIRST_MODEL,
            |model| {
                // Keep the request transport and tools as for the next model.
                model.tool_mode = None;
                model.multi_agent_version = None;
                model.use_responses_lite = false;
                model.comp_hash = None;
            },
        );
        let test = builder.build(&server).await?;
        let first_model = test.session_configured.model.clone();
        assert_eq!(first_model, FIRST_MODEL);
        test.codex
            .start_or_steer_turn(turn_on_model(&test, "hello", &first_model))
            .await?;
        wait_for_event(&test.codex, |event| {
            matches!(event, EventMsg::TurnComplete(_))
        })
        .await;
        core_test_support::submit_thread_settings(
            &test.codex,
            ThreadSettingsOverrides {
                model: Some(NEXT_MODEL.to_string()),
                ..Default::default()
            },
        )
        .await?;
        test.codex
            .start_or_steer_turn(turn_on_model(&test, "after the switch", NEXT_MODEL))
            .await?;
        wait_for_event(&test.codex, |event| {
            matches!(event, EventMsg::TurnComplete(_))
        })
        .await;

        let requests = requests.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].body_json()["model"], NEXT_MODEL);
        let added = new_style_fragments(&requests[0], &requests[1]);
        let developer = requests[1].message_input_texts("developer");
        let switch = developer
            .iter()
            .position(|text| text.contains("<model_switch>"))
            .expect("model switch message");
        match style {
            Some(name) => {
                assert_eq!(added.len(), 1, "{added:?}");
                assert!(added[0].starts_with(&restated(name)), "{}", added[0]);
                let restatement = developer
                    .iter()
                    .rposition(|text| text.starts_with(OPEN))
                    .expect("restated style");
                assert!(switch < restatement, "the style follows the model switch");
            }
            None => assert_eq!(added, Vec::<String>::new()),
        }
    }
    Ok(())
}

/// R3.7, R2.2: with the feature off a configured style changes nothing in requests and warns
/// once at startup.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_feature_keeps_requests_unchanged() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let requests = mount_sse_sequence(&server, completed_turns(1)).await;
    let test = builder_with_config(config_with(Some("outcome-first"), false))
        .build(&server)
        .await?;
    test.submit_turn("hello").await?;
    let request = requests.single_request();
    assert!(!request.body_json().to_string().contains(OPEN));
    let warnings = test
        .config
        .startup_warnings
        .iter()
        .filter(|warning| warning.contains("output_style"))
        .count();
    assert_eq!(warnings, 1, "{:?}", test.config.startup_warnings);
    Ok(())
}

/// R3.8: `model_instructions_placement = "instructions"` and `personality = "none"` behave as
/// without a style; the style stays in the developer context.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn placement_and_personality_ignore_the_style() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let requests = mount_sse_sequence(&server, completed_turns(1)).await;
    let config = format!(
        "model_instructions_placement = \"instructions\"\npersonality = \"none\"\n{}",
        config_text(Some("outcome-first"), true)
    );
    let mut builder = builder_with_config(Arc::new(Mutex::new(config))).with_model_info_override(
        "gpt-5.5",
        |model_info| {
            if let Some(messages) = model_info.model_messages.as_mut() {
                messages.instructions_template = Some(
                    "Base instructions\n# Personality\nBaked personality\n# General\nGeneral"
                        .to_string(),
                );
            }
        },
    );
    let test = builder.build(&server).await?;
    test.submit_turn("hello").await?;
    let request = requests.single_request();
    assert_eq!(
        request.body_json()["instructions"].as_str(),
        Some("Base instructions\n# General\nGeneral")
    );
    assert_eq!(style_fragments(&request).len(), 1);
    Ok(())
}

/// R3.7, R3.10, R4.5: after resume the config decides: the same style adds nothing, another
/// style (also over a selection made through the API) one replacement, and a disabled
/// feature one default notice, after which the thread adds nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_follows_the_config() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let requests = mount_sse_sequence(&server, completed_turns(6)).await;
    let mut builder = builder_with_config(config_with(Some("outcome-first"), true));
    let test = builder.build(&server).await?;
    test.submit_turn("one").await?;
    // Pre-build hooks run only on the first build, so later configs are written directly.
    let write_config = |test: &TestCodex, style: Option<&str>, enabled: bool| {
        std::fs::write(
            test.home.path().join("config.toml"),
            config_text(style, enabled),
        )
    };

    let test = builder.restart(&server, &test).await?;
    test.submit_turn("two").await?;

    // A selection through the API lasts only while the thread is loaded.
    test.codex.set_output_style("learning").await?;
    test.submit_turn("three").await?;
    write_config(&test, Some("explanatory"), true)?;
    let test = builder.restart(&server, &test).await?;
    test.submit_turn("four").await?;

    write_config(&test, Some("explanatory"), false)?;
    let test = builder.restart(&server, &test).await?;
    test.submit_turn("five").await?;
    test.submit_turn("six").await?;

    let requests = requests.requests();
    assert_eq!(requests.len(), 6);
    let added = |index: usize| new_style_fragments(&requests[index - 1], &requests[index]);
    assert!(style_fragments(&requests[0])[0].starts_with(&first("outcome-first")));
    assert_eq!(added(1), Vec::<String>::new(), "same style after resume");
    let third = added(2);
    assert_eq!(third.len(), 1);
    assert!(third[0].starts_with(&replaced("learning")), "{}", third[0]);
    let fourth = added(3);
    assert_eq!(fourth.len(), 1);
    assert!(
        fourth[0].starts_with(&replaced("explanatory")),
        "{}",
        fourth[0]
    );
    let fifth = added(4);
    assert_eq!(fifth.len(), 1);
    assert!(fifth[0].contains(DEFAULT_NOTICE), "{}", fifth[0]);
    assert_eq!(added(5), Vec::<String>::new());
    Ok(())
}

/// R4.1–R4.3: the listing names the style in effect, built-in and custom styles and skipped
/// files; unknown names and a disabled feature are refused without changing the selection.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thread_api_lists_and_refuses() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let styles = home.path().join("output-styles");
    std::fs::create_dir_all(&styles)?;
    std::fs::write(
        styles.join("terse.md"),
        "---\ndescription: Short answers.\n---\nBe brief.\n",
    )?;
    std::fs::write(styles.join("broken.md"), "no frontmatter")?;
    let test: TestCodex = builder_with_config(config_with(Some("outcome-first"), true))
        .with_home(Arc::clone(&home))
        .build(&server)
        .await?;

    let listing = test.codex.output_styles().await;
    assert_eq!(listing.current, "outcome-first");
    assert_eq!(
        listing.catalog.names(),
        vec![
            "default",
            "outcome-first",
            "explanatory",
            "learning",
            "terse"
        ]
    );
    assert_eq!(
        listing.catalog.find("terse").map(|style| style.source),
        Some(OutputStyleSource::User)
    );
    assert_eq!(listing.catalog.warnings.len(), 1);
    assert!(listing.catalog.warnings[0].contains("broken.md"));

    let error = test
        .codex
        .set_output_style("missing")
        .await
        .expect_err("unknown style");
    assert_eq!(
        error,
        OutputStyleError::UnknownStyle {
            name: "missing".to_string(),
            available: "default, outcome-first, explanatory, learning, terse".to_string(),
        }
    );
    test.codex.set_output_style("terse").await?;
    assert_eq!(test.codex.output_styles().await.current, "terse");

    let disabled = builder_with_config(config_with(Some("outcome-first"), false))
        .build(&server)
        .await?;
    assert_eq!(
        disabled.codex.set_output_style("learning").await,
        Err(OutputStyleError::FeatureDisabled)
    );
    assert_eq!(disabled.codex.output_styles().await.current, "default");
    Ok(())
}
