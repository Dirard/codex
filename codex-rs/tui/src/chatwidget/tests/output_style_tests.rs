//! Fork: `/output-style` (`specs/output-styles-2026-11-07.md`, UC5).

use super::*;
use crate::chatwidget::output_style::OutputStyleEvent;
use crate::chatwidget::output_style::OutputStyleSaved;
use crate::chatwidget::output_style::default_style_edit;
use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::MergeStrategy;
use codex_app_server_protocol::OutputStyleInfo;
use codex_app_server_protocol::OutputStyleSource;
use codex_app_server_protocol::ThreadOutputStyleListResponse;
use codex_app_server_protocol::WriteStatus;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;

fn app_events(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> Vec<AppEvent> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn selected_names(events: &[AppEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| match event {
            AppEvent::OutputStyle(OutputStyleEvent::Select(name)) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// The inserted history in transcript form (warnings show only there and in `/warnings`), as
/// one line with single spaces so that wrapped words still match.
fn history_text(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> String {
    drain_insert_history_transcript(rx)
        .iter()
        .flat_map(|lines| lines.iter())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn style(name: &str, description: &str, source: OutputStyleSource) -> OutputStyleInfo {
    let path = match source {
        OutputStyleSource::Builtin => None,
        OutputStyleSource::User | OutputStyleSource::Project => Some(
            AbsolutePathBuf::from_absolute_path(if cfg!(windows) {
                "C:\\styles\\style.md"
            } else {
                "/styles/style.md"
            })
            .expect("absolute path"),
        ),
    };
    OutputStyleInfo {
        name: name.to_string(),
        description: description.to_string(),
        source,
        path,
    }
}

fn listing(warnings: Vec<String>) -> ThreadOutputStyleListResponse {
    ThreadOutputStyleListResponse {
        current: "outcome-first".to_string(),
        styles: vec![
            style(
                "default",
                "Codex's standard communication; no extra instructions.",
                OutputStyleSource::Builtin,
            ),
            style(
                "outcome-first",
                "Outcome first, short updates, plain words; written for a teammate who was not watching.",
                OutputStyleSource::Builtin,
            ),
            style(
                "explanatory",
                "Adds short insights about codebase-specific choices while doing the task.",
                OutputStyleSource::Builtin,
            ),
            style(
                "learning",
                "Asks you to write small key pieces of code (TODO(human)) and explains choices.",
                OutputStyleSource::Builtin,
            ),
            style("terse", "Short answers.", OutputStyleSource::User),
            style("team", "Our team's voice.", OutputStyleSource::Project),
        ],
        warnings,
    }
}

async fn output_style_chat() -> (ChatWidget, tokio::sync::mpsc::UnboundedReceiver<AppEvent>) {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    drain_insert_history(&mut rx);
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ true);
    app_events(&mut rx);
    (chat, rx)
}

/// R5.1, R5.2: `/output-style` exists only with `features.output_styles` and asks the app
/// for the list.
#[tokio::test]
async fn output_style_command_follows_the_feature() {
    let (mut chat, mut rx) = output_style_chat().await;
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ false);
    chat.bottom_pane
        .set_composer_text("/output-s".to_string(), Vec::new(), Vec::new());
    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(!popup.contains("/output-style"), "{popup}");

    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ true);
    chat.bottom_pane
        .set_composer_text("/output-s".to_string(), Vec::new(), Vec::new());
    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(
        popup.contains("/output-style") && popup.contains("choose how Codex communicates"),
        "{popup}"
    );
    chat.bottom_pane
        .set_composer_text("/output-style".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app_events(&mut rx)
            .iter()
            .any(|event| matches!(event, AppEvent::OutputStyle(OutputStyleEvent::OpenPicker)))
    );
}

/// R5.2: the picker lists every style with its description, tags custom ones and marks the
/// current one; choosing a row asks the app to select it.
#[tokio::test]
async fn output_style_picker_lists_and_selects() {
    let (mut chat, mut rx) = output_style_chat().await;
    chat.show_output_style_picker(listing(Vec::new()));
    assert_chatwidget_snapshot!(
        "output_style_picker",
        render_bottom_popup(&chat, /*width*/ 100)
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(selected_names(&app_events(&mut rx)), vec!["explanatory"]);
}

/// R5.5: the warnings of a list answer are shown once, as one warning.
#[tokio::test]
async fn list_warnings_are_shown_once() {
    let (mut chat, mut rx) = output_style_chat().await;
    chat.show_output_style_picker(listing(vec![
        "skipped output style file /a.md: missing YAML frontmatter delimited by ---".to_string(),
        "skipped output style file /b.md: the style text after the frontmatter is empty"
            .to_string(),
    ]));
    let text = history_text(&mut rx);
    assert_eq!(
        text.matches("Some output styles were skipped").count(),
        1,
        "{text}"
    );
    assert!(text.contains("/a.md") && text.contains("/b.md"), "{text}");
}

/// R5.3: `/output-style <name>` selects without the picker.
#[tokio::test]
async fn inline_name_selects_without_the_picker() {
    let (mut chat, mut rx) = output_style_chat().await;
    chat.bottom_pane.set_composer_text(
        "/output-style learning".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    let events = app_events(&mut rx);
    assert_eq!(selected_names(&events), vec!["learning"]);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AppEvent::OutputStyle(OutputStyleEvent::OpenPicker)))
    );
}

/// R5.3: the outcome of a selection: saved, saved but overridden, set but not saved, set for
/// this thread only (a project style, review O-2), refused.
#[tokio::test]
async fn selection_outcomes_are_reported() {
    let (mut chat, mut rx) = output_style_chat().await;
    chat.on_output_style_selected(Ok(OutputStyleSaved {
        name: "learning".to_string(),
        write: Some(Ok(WriteStatus::Ok)),
    }));
    let text = history_text(&mut rx);
    assert!(text.contains("Output style set to learning"), "{text}");
    assert!(!text.contains("overrides"), "{text}");

    chat.on_output_style_selected(Ok(OutputStyleSaved {
        name: "learning".to_string(),
        write: Some(Ok(WriteStatus::OkOverridden)),
    }));
    let text = history_text(&mut rx);
    assert!(text.contains("Output style set to learning"), "{text}");
    assert!(
        text.contains("a higher-priority configuration layer overrides the saved value"),
        "{text}"
    );

    chat.on_output_style_selected(Ok(OutputStyleSaved {
        name: "learning".to_string(),
        write: Some(Err("permission denied".to_string())),
    }));
    let text = history_text(&mut rx);
    assert!(
        text.contains("This thread now uses the output style learning, but saving it as the default failed: permission denied"),
        "{text}"
    );

    chat.on_output_style_selected(Ok(OutputStyleSaved {
        name: "team".to_string(),
        write: None,
    }));
    let text = history_text(&mut rx);
    assert!(
        text.contains("Output style set to team for this thread")
            && text.contains("add output_style = \"team\" to the project's .codex/config.toml"),
        "{text}"
    );

    chat.on_output_style_selected(Err("unknown output style `nope`".to_string()));
    let text = history_text(&mut rx);
    assert!(
        text.contains("Failed to set the output style: unknown output style `nope`"),
        "{text}"
    );
}

/// Review O-1: a running thread keeps the features it started with, so turning output styles
/// on or off during a session says what that means for this thread; without a thread, or
/// when the value does not change, nothing is said.
#[tokio::test]
async fn toggling_the_feature_mid_session_points_to_a_new_thread() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    drain_insert_history(&mut rx);
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ true);
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ false);
    assert_eq!(history_text(&mut rx), "");

    chat.thread_id = Some(ThreadId::new());
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ true);
    let text = history_text(&mut rx);
    assert!(
        text.contains("Output styles take effect in new threads.") && text.contains("/new"),
        "{text}"
    );
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ true);
    assert_eq!(history_text(&mut rx), "");
    chat.set_feature_enabled(Feature::OutputStyles, /*enabled*/ false);
    let text = history_text(&mut rx);
    assert!(
        text.contains("This thread keeps its current output style"),
        "{text}"
    );
}

/// Review O-2: built-in and user styles are saved as `output_style` in the user config; a
/// project style is not, since its name is unknown outside that project.
#[test]
fn project_styles_are_not_saved_as_the_default() {
    for source in [OutputStyleSource::Builtin, OutputStyleSource::User] {
        assert_eq!(
            default_style_edit(&style("terse", "Short answers.", source)),
            Some(ConfigEdit {
                key_path: "output_style".to_string(),
                value: serde_json::json!("terse"),
                merge_strategy: MergeStrategy::Replace,
            }),
            "{source:?}"
        );
    }
    assert_eq!(
        default_style_edit(&style(
            "team",
            "Our team's voice.",
            OutputStyleSource::Project
        )),
        None
    );
}
