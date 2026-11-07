//! Fork: the chat widget side of `/output-style` (`specs/output-styles-2026-11-07.md`, UC5).
//!
//! The command asks the app for the styles of the primary thread; the app runs the
//! `thread/outputStyle/*` requests in the background and hands the answers back here, where
//! the picker opens and the outcome is reported.

use codex_app_server_protocol::ConfigEdit;
use codex_app_server_protocol::OutputStyleInfo;
use codex_app_server_protocol::OutputStyleSource;
use codex_app_server_protocol::ThreadOutputStyleListResponse;
use codex_app_server_protocol::WriteStatus;
use codex_features::Feature;

use super::ChatWidget;
use crate::app_event::AppEvent;
use crate::bottom_pane::SelectionItem;
use crate::bottom_pane::SelectionViewParams;
use crate::bottom_pane::popup_consts::standard_popup_hint_line;

/// View id of the `/output-style` picker.
pub(crate) const OUTPUT_STYLE_VIEW_ID: &str = "output-style-picker";

/// Payload of `AppEvent::OutputStyle`: user actions and the app-server answers.
#[derive(Debug)]
pub(crate) enum OutputStyleEvent {
    /// `/output-style` without a name: list the styles of the primary thread (R5.2).
    OpenPicker,
    /// The answer of `thread/outputStyle/list`.
    Listed(Result<ThreadOutputStyleListResponse, String>),
    /// The user chose a style in the picker or with `/output-style <name>` (R5.3).
    Select(String),
    /// The outcome of `thread/outputStyle/set` and, after it succeeded, of saving the name to
    /// the user config.
    Selected(Result<OutputStyleSaved, String>),
}

/// A style the primary thread now uses, and how saving it as the default went.
#[derive(Debug)]
pub(crate) struct OutputStyleSaved {
    pub(crate) name: String,
    /// `None` when the style is not saved as the default: a project style (review O-2).
    pub(crate) write: Option<Result<WriteStatus, String>>,
}

/// R5.3, review O-2: the config edit that saves a selected style as the user's default, or
/// `None` for a project style. Clients can write only the user config (`config/batchWrite`
/// refuses other files), and there a project style's name would be unknown in every other
/// project, in `codex exec` and in IDE threads.
pub(crate) fn default_style_edit(style: &OutputStyleInfo) -> Option<ConfigEdit> {
    match style.source {
        OutputStyleSource::Builtin | OutputStyleSource::User => {
            Some(crate::config_update::replace_config_value(
                "output_style",
                serde_json::json!(style.name),
            ))
        }
        OutputStyleSource::Project => None,
    }
}

impl ChatWidget {
    /// R5.1: `/output-style` exists only while `features.output_styles` is enabled.
    pub(crate) fn fork_sync_output_style_command(&mut self) {
        self.bottom_pane
            .set_output_style_command_enabled(self.config.features.enabled(Feature::OutputStyles));
    }

    /// R5.1, review O-1: a toggle of `features.output_styles` shows or hides `/output-style`.
    /// A running thread keeps the features it started with, so a toggle that changes the
    /// value while a thread is shown says that it applies to new threads.
    pub(crate) fn fork_on_output_styles_toggled(&mut self, was_enabled: bool) {
        self.fork_sync_output_style_command();
        let enabled = self.config.features.enabled(Feature::OutputStyles);
        if enabled == was_enabled || self.thread_id.is_none() {
            return;
        }
        if enabled {
            self.add_info_message(
                "Output styles take effect in new threads.".to_string(),
                Some("Start a new thread with /new to use /output-style.".to_string()),
            );
        } else {
            self.add_info_message(
                "Output styles are off for new threads.".to_string(),
                Some(
                    "This thread keeps its current output style until you start a new thread."
                        .to_string(),
                ),
            );
        }
    }

    /// R5.2: `/output-style` asks the app for the styles of the primary thread.
    pub(super) fn open_output_style_picker(&mut self) {
        if !self.config.features.enabled(Feature::OutputStyles) {
            return;
        }
        self.app_event_tx
            .send(AppEvent::OutputStyle(OutputStyleEvent::OpenPicker));
    }

    /// R5.3: `/output-style <name>` selects without the picker.
    pub(super) fn select_output_style_by_name(&mut self, name: String) {
        if !self.config.features.enabled(Feature::OutputStyles) {
            return;
        }
        self.app_event_tx
            .send(AppEvent::OutputStyle(OutputStyleEvent::Select(name)));
    }

    /// R5.2, R5.5: the picker over a list answer, after one warning for skipped files.
    pub(crate) fn show_output_style_picker(&mut self, response: ThreadOutputStyleListResponse) {
        if !response.warnings.is_empty() {
            self.add_warning_message(format!(
                "Some output styles were skipped:\n{}",
                response.warnings.join("\n")
            ));
        }
        let current = response.current;
        let items: Vec<SelectionItem> = response
            .styles
            .into_iter()
            .map(|style| {
                let tag = match style.source {
                    OutputStyleSource::Builtin => "",
                    OutputStyleSource::User => " (user)",
                    OutputStyleSource::Project => " (project)",
                };
                let name = style.name;
                let selected = name.clone();
                SelectionItem {
                    is_current: name.eq_ignore_ascii_case(&current),
                    name: format!("{name}{tag}"),
                    description: Some(style.description),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::OutputStyle(OutputStyleEvent::Select(
                            selected.clone(),
                        )));
                    })],
                    dismiss_on_select: true,
                    search_value: Some(name),
                    ..Default::default()
                }
            })
            .collect();
        let initial_selected_idx = items.iter().position(|item| item.is_current);
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(OUTPUT_STYLE_VIEW_ID),
            title: Some("Output style".to_string()),
            subtitle: Some("Choose how Codex communicates in this thread.".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items,
            initial_selected_idx,
            ..SelectionViewParams::picker()
        });
    }

    /// R5.3: reports a selection: set and saved, saved but overridden, set but not saved, set
    /// for this thread only (a project style, review O-2), or refused by the server.
    pub(crate) fn on_output_style_selected(&mut self, result: Result<OutputStyleSaved, String>) {
        match result {
            Ok(OutputStyleSaved { name, write }) => match write {
                None => self.add_info_message(
                    format!("Output style set to {name} for this thread"),
                    Some(format!(
                        "Project styles are not saved as your default. To use it whenever you work in this project, add output_style = \"{name}\" to the project's .codex/config.toml."
                    )),
                ),
                Some(Ok(status)) => {
                    self.add_info_message(format!("Output style set to {name}"), /*hint*/ None);
                    if status == WriteStatus::OkOverridden {
                        self.add_warning_message(
                            "Saved the output style, but a higher-priority configuration layer overrides the saved value."
                                .to_string(),
                        );
                    }
                }
                Some(Err(error)) => self.add_error_message(format!(
                    "This thread now uses the output style {name}, but saving it as the default failed: {error}"
                )),
            },
            Err(error) => {
                self.add_error_message(format!("Failed to set the output style: {error}"));
            }
        }
    }
}
