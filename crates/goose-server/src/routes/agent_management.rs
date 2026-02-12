use axum::{
    extract::Path,
    routing::{delete, get, post},
    Json, Router,
};
use goose::agent_manager::client::AgentClientManager;
use goose::agent_manager::{NewSessionRequest, SessionId, SessionModeId, SetSessionModeRequest};
use goose::registry::manifest::{RegistryEntryDetail, RegistryEntryKind};
use goose::registry::sources::local::LocalRegistrySource;
use goose::registry::RegistryManager;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use crate::routes::errors::ErrorResponse;

fn acp_manager() -> &'static Arc<Mutex<AgentClientManager>> {
    static INSTANCE: OnceLock<Arc<Mutex<AgentClientManager>>> = OnceLock::new();
    INSTANCE.get_or_init(|| Arc::new(Mutex::new(AgentClientManager::default())))
}

fn default_registry() -> Result<RegistryManager, ErrorResponse> {
    let mut manager = RegistryManager::new();
    let local = LocalRegistrySource::from_default_paths()
        .map_err(|e| ErrorResponse::internal(format!("Registry init failed: {e}")))?;
    manager.add_source(Box::new(local));
    Ok(manager)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ConnectAgentRequest {
    pub name: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ConnectAgentResponse {
    pub agent_id: String,
    pub connected: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    pub working_dir: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PromptAgentRequest {
    pub session_id: String,
    pub text: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PromptAgentResponse {
    pub text: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SetModeAgentRequest {
    pub session_id: String,
    pub mode_id: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AgentListResponse {
    pub agents: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/agents/external/connect",
    request_body = ConnectAgentRequest,
    responses(
        (status = 200, description = "Agent connected", body = ConnectAgentResponse),
        (status = 404, description = "Agent not found"),
        (status = 422, description = "Agent has no distribution")
    ),
    tag = "External Agents"
)]
pub async fn connect_agent(
    Json(req): Json<ConnectAgentRequest>,
) -> Result<Json<ConnectAgentResponse>, ErrorResponse> {
    let registry = default_registry()?;
    let entry = registry
        .get(&req.name, Some(RegistryEntryKind::Agent))
        .await
        .map_err(|e| ErrorResponse::internal(format!("Registry lookup failed: {e}")))?;

    let entry = entry.ok_or_else(|| ErrorResponse::not_found("Agent not found in registry"))?;

    let distribution = match &entry.detail {
        RegistryEntryDetail::Agent(detail) => detail
            .distribution
            .as_ref()
            .ok_or_else(|| ErrorResponse::unprocessable("Agent has no distribution targets"))?,
        _ => {
            return Err(ErrorResponse::unprocessable(
                "Registry entry is not an agent",
            ))
        }
    };

    let mgr = acp_manager().lock().await;
    mgr.connect_with_distribution(req.name.clone(), distribution)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Connection failed: {e}")))?;

    Ok(Json(ConnectAgentResponse {
        agent_id: req.name,
        connected: true,
    }))
}

#[utoipa::path(
    post,
    path = "/agents/external/{agent_id}/session",
    params(("agent_id" = String, Path, description = "Agent identifier")),
    request_body = CreateSessionRequest,
    responses(
        (status = 200, description = "Session created", body = CreateSessionResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "External Agents"
)]
pub async fn create_session(
    Path(agent_id): Path<String>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, ErrorResponse> {
    let cwd = req
        .working_dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let mgr = acp_manager().lock().await;
    let resp = mgr
        .new_session(&agent_id, NewSessionRequest::new(cwd))
        .await
        .map_err(|e| ErrorResponse::internal(format!("Session creation failed: {e}")))?;

    Ok(Json(CreateSessionResponse {
        session_id: resp.session_id.0.to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/agents/external/{agent_id}/prompt",
    params(("agent_id" = String, Path, description = "Agent identifier")),
    request_body = PromptAgentRequest,
    responses(
        (status = 200, description = "Prompt response", body = PromptAgentResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "External Agents"
)]
pub async fn prompt_agent(
    Path(agent_id): Path<String>,
    Json(req): Json<PromptAgentRequest>,
) -> Result<Json<PromptAgentResponse>, ErrorResponse> {
    let session_id = SessionId::from(req.session_id);
    let mgr = acp_manager().lock().await;
    let text = mgr
        .prompt_agent_text(&agent_id, &session_id, &req.text)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Prompt failed: {e}")))?;

    Ok(Json(PromptAgentResponse { text }))
}

#[utoipa::path(
    post,
    path = "/agents/external/{agent_id}/mode",
    params(("agent_id" = String, Path, description = "Agent identifier")),
    request_body = SetModeAgentRequest,
    responses(
        (status = 200, description = "Mode set"),
        (status = 500, description = "Internal server error")
    ),
    tag = "External Agents"
)]
pub async fn set_mode(
    Path(agent_id): Path<String>,
    Json(req): Json<SetModeAgentRequest>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let session_id = SessionId::from(req.session_id);
    let mgr = acp_manager().lock().await;
    mgr.set_mode(
        &agent_id,
        SetSessionModeRequest::new(session_id, SessionModeId::from(req.mode_id)),
    )
    .await
    .map_err(|e| ErrorResponse::internal(format!("Set mode failed: {e}")))?;

    Ok(Json(serde_json::json!({"ok": true})))
}

#[utoipa::path(
    get,
    path = "/agents/external",
    responses((status = 200, description = "List of connected agents", body = AgentListResponse)),
    tag = "External Agents"
)]
pub async fn list_agents() -> Json<AgentListResponse> {
    let mgr = acp_manager().lock().await;
    let agents = mgr.list_agents().await;
    Json(AgentListResponse { agents })
}

#[utoipa::path(
    delete,
    path = "/agents/external/{agent_id}",
    params(("agent_id" = String, Path, description = "Agent identifier")),
    responses(
        (status = 200, description = "Agent disconnected"),
        (status = 500, description = "Internal server error")
    ),
    tag = "External Agents"
)]
pub async fn disconnect_agent(
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, ErrorResponse> {
    let mgr = acp_manager().lock().await;
    mgr.disconnect_agent(&agent_id)
        .await
        .map_err(|e| ErrorResponse::internal(format!("Disconnect failed: {e}")))?;

    Ok(Json(serde_json::json!({"ok": true})))
}

pub fn routes() -> Router {
    Router::new()
        .route("/agents/external/connect", post(connect_agent))
        .route("/agents/external/{agent_id}/session", post(create_session))
        .route("/agents/external/{agent_id}/prompt", post(prompt_agent))
        .route("/agents/external/{agent_id}/mode", post(set_mode))
        .route("/agents/external", get(list_agents))
        .route("/agents/external/{agent_id}", delete(disconnect_agent))
}
