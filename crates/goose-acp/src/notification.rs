use agent_client_protocol_schema::{
    RequestPermissionRequest, RequestPermissionResponse, SessionNotification,
};
use async_trait::async_trait;

#[async_trait]
pub trait NotificationSender: Send + Sync {
    async fn send_notification(&self, notification: SessionNotification) -> anyhow::Result<()>;
    async fn request_permission(
        &self,
        request: RequestPermissionRequest,
    ) -> anyhow::Result<RequestPermissionResponse>;
}

// --- Channel-based sender for bridging Send → !Send AgentSideConnection ---

pub(crate) enum ClientCommand {
    Notify {
        notification: SessionNotification,
        reply: tokio::sync::oneshot::Sender<agent_client_protocol_schema::Result<()>>,
    },
    RequestPermission {
        request: RequestPermissionRequest,
        reply: tokio::sync::oneshot::Sender<
            agent_client_protocol_schema::Result<RequestPermissionResponse>,
        >,
    },
}

pub(crate) struct AcpNotificationSender {
    tx: tokio::sync::mpsc::UnboundedSender<ClientCommand>,
}

impl AcpNotificationSender {
    pub(crate) fn new(tx: tokio::sync::mpsc::UnboundedSender<ClientCommand>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl NotificationSender for AcpNotificationSender {
    async fn send_notification(&self, notification: SessionNotification) -> anyhow::Result<()> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ClientCommand::Notify {
                notification,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("ACP notification channel closed"))?;
        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("ACP notification reply dropped"))?
            .map_err(|e| anyhow::anyhow!("ACP notification error: {e}"))
    }

    async fn request_permission(
        &self,
        request: RequestPermissionRequest,
    ) -> anyhow::Result<RequestPermissionResponse> {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(ClientCommand::RequestPermission {
                request,
                reply: reply_tx,
            })
            .map_err(|_| anyhow::anyhow!("ACP permission channel closed"))?;
        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("ACP permission reply dropped"))?
            .map_err(|e| anyhow::anyhow!("ACP permission error: {e}"))
    }
}
