pub mod client;
pub mod spawner;

// Re-export commonly used ACP schema types for downstream crates
pub use agent_client_protocol_schema::{
    NewSessionRequest, NewSessionResponse, SessionId, SessionModeId, SetSessionModeRequest,
    SetSessionModeResponse,
};
