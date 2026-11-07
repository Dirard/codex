//! Fork: `thread/outputStyle/*` requests over the output style of a loaded thread
//! (`specs/output-styles-2026-11-07.md`, UC4 R4.4).

use std::sync::Arc;

use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ClientResponsePayload;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::OutputStyleInfo;
use codex_app_server_protocol::OutputStyleSource;
use codex_app_server_protocol::ThreadOutputStyleListParams;
use codex_app_server_protocol::ThreadOutputStyleListResponse;
use codex_app_server_protocol::ThreadOutputStyleSetParams;
use codex_app_server_protocol::ThreadOutputStyleSetResponse;
use codex_core::CodexThread;
use codex_core::ThreadManager;
use codex_core::output_style;
use codex_protocol::ThreadId;

use crate::error_code::internal_error;
use crate::error_code::invalid_request;

pub(crate) struct ThreadOutputStyleRequestProcessor {
    thread_manager: Arc<ThreadManager>,
}

impl ThreadOutputStyleRequestProcessor {
    pub(crate) fn new(thread_manager: Arc<ThreadManager>) -> Self {
        Self { thread_manager }
    }

    /// Handles a `thread/outputStyle/*` request routed here by `MessageProcessor`.
    pub(crate) async fn handle(
        &self,
        request: ClientRequest,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        match request {
            ClientRequest::ThreadOutputStyleList { params, .. } => self
                .list(params)
                .await
                .map(|response| Some(response.into())),
            ClientRequest::ThreadOutputStyleSet { params, .. } => {
                self.set(params).await.map(|response| Some(response.into()))
            }
            other => Err(internal_error(format!(
                "{} is not a thread/outputStyle request",
                other.method_name()
            ))),
        }
    }

    /// R4.1: the style in effect and the catalog of the thread's config.
    async fn list(
        &self,
        params: ThreadOutputStyleListParams,
    ) -> Result<ThreadOutputStyleListResponse, JSONRPCErrorError> {
        let thread = self.loaded_thread(&params.thread_id).await?;
        let listing = thread.output_styles().await;
        Ok(ThreadOutputStyleListResponse {
            current: listing.current,
            styles: listing
                .catalog
                .styles
                .iter()
                .map(|style| style_info(style))
                .collect(),
            warnings: listing.catalog.warnings,
        })
    }

    /// R4.2, R4.3: selects a style for the thread; refusals are `invalid_request`.
    async fn set(
        &self,
        params: ThreadOutputStyleSetParams,
    ) -> Result<ThreadOutputStyleSetResponse, JSONRPCErrorError> {
        let thread = self.loaded_thread(&params.thread_id).await?;
        let style = thread
            .set_output_style(&params.name)
            .await
            .map_err(|error| invalid_request(error.to_string()))?;
        Ok(ThreadOutputStyleSetResponse {
            style: style_info(&style),
        })
    }

    /// The selection lives with the thread's session, so only a loaded thread has one.
    async fn loaded_thread(
        &self,
        raw_thread_id: &str,
    ) -> Result<Arc<CodexThread>, JSONRPCErrorError> {
        let thread_id = ThreadId::from_string(raw_thread_id)
            .map_err(|error| invalid_request(format!("invalid thread id: {error}")))?;
        self.thread_manager
            .get_thread(thread_id)
            .await
            .map_err(|_| invalid_request(format!("thread not loaded: {thread_id}")))
    }
}

fn style_info(style: &output_style::OutputStyle) -> OutputStyleInfo {
    OutputStyleInfo {
        name: style.name.clone(),
        description: style.description.clone(),
        source: match style.source {
            output_style::OutputStyleSource::Builtin => OutputStyleSource::Builtin,
            output_style::OutputStyleSource::User => OutputStyleSource::User,
            output_style::OutputStyleSource::Project => OutputStyleSource::Project,
        },
        path: style.path.clone(),
    }
}
