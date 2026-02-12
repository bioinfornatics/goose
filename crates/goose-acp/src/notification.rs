use agent_client_protocol_schema::{
    RequestPermissionRequest, RequestPermissionResponse, SessionNotification,
};

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
