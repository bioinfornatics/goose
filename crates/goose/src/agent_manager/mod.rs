pub mod client;
pub mod health;
pub mod spawner;
pub mod task;

pub use health::{AgentHealth, AgentState, AgentStatus};
pub use task::{TaskManager, TaskState, TaskStatus};

// Re-export commonly used ACP schema types for downstream crates
pub use agent_client_protocol_schema::{
    NewSessionRequest, NewSessionResponse, SessionId, SessionModeId, SetSessionModeRequest,
    SetSessionModeResponse,
};
