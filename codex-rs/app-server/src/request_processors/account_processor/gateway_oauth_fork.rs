//! Fork: a gateway OAuth login is admitted before its request waits in a serialization
//! queue, so `account/gatewayOAuth/cancel` sent after the login request always finds it
//! (`docs/fork/04-app-server-protocol.md`, section 1.4). A child module of `gateway_oauth`.

use super::*;
use crate::message_processor::ConnectionSessionState;

/// The login slot taken by [`AccountRequestProcessor::admit_gateway_oauth_login`].
pub(crate) struct GatewayOAuthLoginAdmission {
    pub(super) cancel: CancellationToken,
    pub(super) guard: LoginGuard,
}

/// The admitted login; it resolves to the `account/gatewayOAuth/login` response payload.
pub(crate) type GatewayOAuthLoginFuture =
    futures::future::BoxFuture<'static, Result<Option<ClientResponsePayload>, JSONRPCErrorError>>;

impl AccountRequestProcessor {
    /// Admits `account/gatewayOAuth/login` before the request is queued and returns the
    /// login to run in its place; other requests return `None`.
    pub(crate) fn fork_admit_gateway_oauth_login(
        self: &Arc<Self>,
        request: &ClientRequest,
        connection_id: ConnectionId,
        session: &ConnectionSessionState,
    ) -> Result<Option<GatewayOAuthLoginFuture>, JSONRPCErrorError> {
        let ClientRequest::GatewayOAuthLogin { .. } = request else {
            return Ok(None);
        };
        if session
            .opted_out_notification_methods()
            .contains("account/gatewayOAuth/changed")
        {
            return Err(invalid_request(
                "Gateway login requires account/gatewayOAuth/changed notifications",
            ));
        }
        let admission = self.admit_gateway_oauth_login(connection_id, &session.rpc_gate)?;
        let account_processor = Arc::clone(self);
        Ok(Some(Box::pin(async move {
            account_processor
                .gateway_oauth_login(connection_id, admission)
                .await
                .map(|response| Some(response.into()))
        })))
    }
}
