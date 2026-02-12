//! Bridge module: implements the agent-client-protocol Agent trait
//! by delegating to GooseAcpAgent methods.

use std::cell::OnceCell;
use std::rc::Rc;
use std::sync::Arc;

// Import Result type alias from schema (Result<T> = std::result::Result<T, Error>)
use agent_client_protocol_schema::{
    AuthenticateRequest, AuthenticateResponse, CancelNotification, Error, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, NewSessionRequest,
    NewSessionResponse, PromptRequest, PromptResponse, Result, SetSessionModelRequest,
    SetSessionModelResponse,
};

use crate::server::GooseAcpAgent;

/// Bridge that implements the Agent trait from agent-client-protocol.
pub struct AcpBridge {
    pub agent: Arc<GooseAcpAgent>,
    client: OnceCell<Rc<agent_client_protocol::AgentSideConnection>>,
}

impl AcpBridge {
    pub fn new(agent: Arc<GooseAcpAgent>) -> Self {
        Self {
            agent,
            client: OnceCell::new(),
        }
    }

    pub fn set_client(&self, client: Rc<agent_client_protocol::AgentSideConnection>) {
        let _ = self.client.set(client);
    }

    #[allow(dead_code)]
    pub fn client(&self) -> &agent_client_protocol::AgentSideConnection {
        self.client.get().expect("AcpBridge: client not set")
    }
}

// Use the EXACT same async_trait attribute as the Agent trait definition
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

    async fn load_session(&self, _args: LoadSessionRequest) -> Result<LoadSessionResponse> {
        // TODO: Refactor on_load_session to accept &dyn Client
        Err(Error::method_not_found())
    }

    async fn prompt(&self, _args: PromptRequest) -> Result<PromptResponse> {
        // TODO: Refactor on_prompt to accept &dyn Client
        Err(Error::method_not_found())
    }

    async fn cancel(&self, args: CancelNotification) -> Result<()> {
        let _ = self.agent.on_cancel(args).await;
        Ok(())
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
