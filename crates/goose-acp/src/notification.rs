use agent_client_protocol_schema::{
    RequestPermissionRequest, RequestPermissionResponse, SessionNotification,
};

/// Trait for sending notifications and permission requests to the client.
///
/// Implementations must be Send + Sync because GooseAcpAgent is shared
/// across tokio tasks. The channel-based AcpNotificationSender satisfies
/// this by forwarding calls through mpsc channels to a !Send LocalSet.
#[async_trait::async_trait]
pub trait NotificationSender: Send + Sync {
    async fn send_session_notification(
        &self,
        notification: SessionNotification,
    ) -> std::result::Result<(), agent_client_protocol_schema::Error>;

    async fn send_permission_request(
        &self,
        request: RequestPermissionRequest,
    ) -> std::result::Result<RequestPermissionResponse, agent_client_protocol_schema::Error>;
}

// --- sacp-based sender (to be removed in Step 7) ---

use sacp::{AgentToClient, JrConnectionCx};

pub struct SacpNotificationSender {
    cx: JrConnectionCx<AgentToClient>,
}

impl SacpNotificationSender {
    pub fn new(cx: JrConnectionCx<AgentToClient>) -> Self {
        Self { cx }
    }
}

#[async_trait::async_trait]
impl NotificationSender for SacpNotificationSender {
    async fn send_session_notification(
        &self,
        notification: SessionNotification,
    ) -> std::result::Result<(), agent_client_protocol_schema::Error> {
        self.cx
            .send_notification(notification)
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())
    }

    async fn send_permission_request(
        &self,
        request: RequestPermissionRequest,
    ) -> std::result::Result<RequestPermissionResponse, agent_client_protocol_schema::Error> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self
            .cx
            .send_request(request)
            .on_receiving_result(move |result| async move {
                let _ = tx.send(result);
                Ok(())
            });
        rx.await
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())?
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())
    }
}

// --- Channel-based sender for AgentSideConnection ---

/// A message sent from the Send world to the !Send LocalSet.
pub(crate) enum ClientCommand {
    Notify {
        notification: SessionNotification,
        reply: tokio::sync::oneshot::Sender<
            std::result::Result<(), agent_client_protocol_schema::Error>,
        >,
    },
    RequestPermission {
        request: RequestPermissionRequest,
        reply: tokio::sync::oneshot::Sender<
            std::result::Result<RequestPermissionResponse, agent_client_protocol_schema::Error>,
        >,
    },
}

/// Send + Sync notification sender that forwards through an mpsc channel
/// to a !Send AgentSideConnection running on a LocalSet.
pub struct AcpNotificationSender {
    tx: tokio::sync::mpsc::UnboundedSender<ClientCommand>,
}

impl AcpNotificationSender {
    pub(crate) fn new(tx: tokio::sync::mpsc::UnboundedSender<ClientCommand>) -> Self {
        Self { tx }
    }
}

#[async_trait::async_trait]
impl NotificationSender for AcpNotificationSender {
    async fn send_session_notification(
        &self,
        notification: SessionNotification,
    ) -> std::result::Result<(), agent_client_protocol_schema::Error> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ClientCommand::Notify {
                notification,
                reply: reply_tx,
            })
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())?;
        reply_rx
            .await
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())?
    }

    async fn send_permission_request(
        &self,
        request: RequestPermissionRequest,
    ) -> std::result::Result<RequestPermissionResponse, agent_client_protocol_schema::Error> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ClientCommand::RequestPermission {
                request,
                reply: reply_tx,
            })
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())?;
        reply_rx
            .await
            .map_err(|_| agent_client_protocol_schema::Error::internal_error())?
    }
}
