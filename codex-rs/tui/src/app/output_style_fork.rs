//! Fork: app side of `/output-style` (`specs/output-styles-2026-11-07.md`, UC5).
//!
//! `thread/outputStyle/*` requests and the config write run in background tasks, like the
//! `/workflows` requests; their answers come back as `AppEvent::OutputStyle` and go to the
//! chat widget. The style is set on the session's primary thread, not on a subagent being
//! watched (R5.2, R5.3).

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadOutputStyleListParams;
use codex_app_server_protocol::ThreadOutputStyleListResponse;
use codex_app_server_protocol::ThreadOutputStyleSetParams;
use codex_app_server_protocol::ThreadOutputStyleSetResponse;
use codex_protocol::ThreadId;
use uuid::Uuid;

use super::App;
use crate::app_event::AppEvent;
use crate::app_server_session::AppServerSession;
use crate::chatwidget::output_style::OutputStyleEvent;
use crate::chatwidget::output_style::OutputStyleSaved;
use crate::chatwidget::output_style::default_style_edit;

impl App {
    /// UC5: route an `/output-style` event.
    pub(super) fn handle_output_style_event(
        &mut self,
        app_server: &AppServerSession,
        event: OutputStyleEvent,
    ) {
        match event {
            OutputStyleEvent::OpenPicker => {
                if let Some(thread_id) = self.output_style_thread_id() {
                    self.spawn_output_style_list(app_server, thread_id);
                }
            }
            OutputStyleEvent::Listed(Ok(response)) => {
                self.chat_widget.show_output_style_picker(response);
            }
            OutputStyleEvent::Listed(Err(error)) => {
                self.chat_widget
                    .add_error_message(format!("Failed to list output styles: {error}"));
            }
            OutputStyleEvent::Select(name) => {
                if let Some(thread_id) = self.output_style_thread_id() {
                    self.spawn_output_style_set(app_server, thread_id, name);
                }
            }
            OutputStyleEvent::Selected(result) => self.chat_widget.on_output_style_selected(result),
        }
    }

    /// R5.6: the primary thread, or an info message while there is none yet.
    fn output_style_thread_id(&mut self) -> Option<ThreadId> {
        if self.primary_thread_id.is_none() {
            self.chat_widget.add_info_message(
                "Output styles are not available yet.".to_string(),
                Some("Wait for the session to start, then run /output-style again.".to_string()),
            );
        }
        self.primary_thread_id
    }

    fn spawn_output_style_list(&self, app_server: &AppServerSession, thread_id: ThreadId) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let result = request_handle
                .request_typed::<ThreadOutputStyleListResponse>(
                    ClientRequest::ThreadOutputStyleList {
                        request_id: RequestId::String(Uuid::new_v4().to_string()),
                        params: ThreadOutputStyleListParams {
                            thread_id: thread_id.to_string(),
                        },
                    },
                )
                .await
                .map_err(|error| error.to_string());
            app_event_tx.send(AppEvent::OutputStyle(OutputStyleEvent::Listed(result)));
        });
    }

    /// R5.3: sets the style on the thread, then saves its name as `output_style` in the active
    /// user config; nothing is written when the server refuses the style or when it is a
    /// project style (review O-2, [`default_style_edit`]).
    fn spawn_output_style_set(
        &self,
        app_server: &AppServerSession,
        thread_id: ThreadId,
        name: String,
    ) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            let set = request_handle
                .request_typed::<ThreadOutputStyleSetResponse>(
                    ClientRequest::ThreadOutputStyleSet {
                        request_id: RequestId::String(Uuid::new_v4().to_string()),
                        params: ThreadOutputStyleSetParams {
                            thread_id: thread_id.to_string(),
                            name,
                        },
                    },
                )
                .await
                .map_err(|error| error.to_string());
            let result = match set {
                Ok(response) => {
                    let write = match default_style_edit(&response.style) {
                        Some(edit) => Some(
                            crate::config_update::write_config_batch(request_handle, vec![edit])
                                .await
                                .map(|response| response.status)
                                .map_err(|error| crate::config_update::format_config_error(&error)),
                        ),
                        None => None,
                    };
                    Ok(OutputStyleSaved {
                        name: response.style.name,
                        write,
                    })
                }
                Err(error) => Err(error),
            };
            app_event_tx.send(AppEvent::OutputStyle(OutputStyleEvent::Selected(result)));
        });
    }
}
