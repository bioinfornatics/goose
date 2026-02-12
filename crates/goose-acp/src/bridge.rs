//! Bridge: implements the agent-client-protocol Agent trait
//! by delegating to GooseAcpAgent methods.
//!
//! This lives on a !Send LocalSet. Notifications and permission requests
//! are forwarded through channels set up in serve().

use std::sync::Arc;

use agent_client_protocol_schema::{
    AuthenticateRequest, AuthenticateResponse, CancelNotification, Error, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, Result, SetSessionModeRequest,
    SetSessionModeResponse, SetSessionModelRequest, SetSessionModelResponse,
};

use crate::server::GooseAcpAgent;

/// Bridge that implements the `agent_client_protocol::Agent` trait.
///
/// Lives on a LocalSet (not Send). Dispatches incoming ACP requests
/// to the Send-safe GooseAcpAgent.
pub struct AcpBridge {
    pub agent: Arc<GooseAcpAgent>,
}

impl AcpBridge {
    pub fn new(agent: Arc<GooseAcpAgent>) -> Self {
        Self { agent }
    }
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Agent for AcpBridge {
    async fn initialize(&self, args: InitializeRequest) -> Result<InitializeResponse> {
        self.agent.on_initialize(args).await
    }

    async fn authenticate(&self, _args: AuthenticateRequest) -> Result<AuthenticateResponse> {
        Err(Error::method_not_found())
    }

    async fn new_session(&self, args: NewSessionRequest) -> Result<NewSessionResponse> {
        self.agent.on_new_session(args).await
    }

    async fn load_session(&self, args: LoadSessionRequest) -> Result<LoadSessionResponse> {
        self.agent.on_load_session(args).await
    }

    async fn prompt(&self, args: PromptRequest) -> Result<PromptResponse> {
        self.agent.on_prompt(args).await
    }

    async fn cancel(&self, args: CancelNotification) -> Result<()> {
        let _ = self.agent.on_cancel(args).await;
        Ok(())
    }

    async fn set_session_mode(
        &self,
        args: SetSessionModeRequest,
    ) -> Result<SetSessionModeResponse> {
        self.agent
            .on_set_mode(&args.session_id.0, &args.mode_id.0)
            .await
    }

    async fn set_session_model(
        &self,
        args: SetSessionModelRequest,
    ) -> Result<SetSessionModelResponse> {
        self.agent
            .on_set_model(&args.session_id.0, &args.model_id.0)
            .await
    }
}
