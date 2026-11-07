//! Fork: app side of `/loop` (`specs/scheduled-wakeups-2026-11-07.md`, UC9).
//!
//! `thread/loop/start`, `thread/wakeup/cancel` and `thread/wakeup/read` run in background
//! tasks; their answers come back as `AppEvent::Wakeups` and go to the chat widget, which
//! shows them.

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadLoopStartParams;
use codex_app_server_protocol::ThreadLoopStartResponse;
use codex_app_server_protocol::ThreadWakeupCancelParams;
use codex_app_server_protocol::ThreadWakeupCancelResponse;
use codex_app_server_protocol::ThreadWakeupReadParams;
use codex_app_server_protocol::ThreadWakeupReadResponse;
use uuid::Uuid;

use super::App;
use crate::app_event::AppEvent;
use crate::app_server_session::AppServerSession;
use crate::wakeups::WakeupsEvent;

impl App {
    /// R9.1, R9.4: `/loop` acts on the shown thread; answers go to the chat widget.
    pub(super) fn handle_wakeups_event(
        &mut self,
        app_server: &AppServerSession,
        event: WakeupsEvent,
    ) {
        let request_handle = app_server.request_handle();
        let app_event_tx = self.app_event_tx.clone();
        let thread_id = self.current_displayed_thread_id();
        let request_id = || RequestId::String(Uuid::new_v4().to_string());
        match event {
            WakeupsEvent::StartLoop { interval_s, prompt } => {
                let Some(thread_id) = thread_id else {
                    self.chat_widget
                        .add_error_message("Start a conversation before /loop.".to_string());
                    return;
                };
                let request = ClientRequest::ThreadLoopStart {
                    request_id: request_id(),
                    params: ThreadLoopStartParams {
                        thread_id: thread_id.to_string(),
                        prompt,
                        interval_seconds: interval_s,
                        max_iterations: None,
                    },
                };
                tokio::spawn(async move {
                    let result = request_handle
                        .request_typed::<ThreadLoopStartResponse>(request)
                        .await
                        .map(|response| response.state)
                        .map_err(|err| err.to_string());
                    app_event_tx.send(AppEvent::Wakeups(WakeupsEvent::LoopStarted {
                        thread_id,
                        result,
                    }));
                });
            }
            WakeupsEvent::Stop => {
                let Some(thread_id) = thread_id else {
                    return;
                };
                let request = ClientRequest::ThreadWakeupCancel {
                    request_id: request_id(),
                    params: ThreadWakeupCancelParams {
                        thread_id: thread_id.to_string(),
                    },
                };
                tokio::spawn(async move {
                    let result = request_handle
                        .request_typed::<ThreadWakeupCancelResponse>(request)
                        .await
                        .map(|response| (response.cancelled, response.state))
                        .map_err(|err| err.to_string());
                    app_event_tx.send(AppEvent::Wakeups(WakeupsEvent::Cancelled {
                        thread_id,
                        result,
                    }));
                });
            }
            WakeupsEvent::Show => {
                let Some(thread_id) = thread_id else {
                    self.chat_widget.add_info_message(
                        "No scheduled wakeup or /loop yet.".to_string(),
                        /*hint*/ None,
                    );
                    return;
                };
                let request = ClientRequest::ThreadWakeupRead {
                    request_id: request_id(),
                    params: ThreadWakeupReadParams {
                        thread_id: thread_id.to_string(),
                    },
                };
                tokio::spawn(async move {
                    let result = request_handle
                        .request_typed::<ThreadWakeupReadResponse>(request)
                        .await
                        .map(|response| (response.available, response.state))
                        .map_err(|err| err.to_string());
                    app_event_tx.send(AppEvent::Wakeups(WakeupsEvent::Loaded {
                        thread_id,
                        show: true,
                        result,
                    }));
                });
            }
            WakeupsEvent::LoopStarted { thread_id, result } => {
                self.chat_widget.on_loop_started(thread_id, result);
            }
            WakeupsEvent::Cancelled { thread_id, result } => {
                self.chat_widget.on_wakeup_cancelled(thread_id, result);
            }
            WakeupsEvent::Loaded {
                thread_id,
                show,
                result,
            } => self.chat_widget.on_wakeup_loaded(thread_id, show, result),
        }
    }
}
