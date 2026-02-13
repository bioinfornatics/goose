//! ACP-compatible /runs endpoint for standard run lifecycle.
//!
//! Implements the Agent Communication Protocol run management:
//! - POST /runs — create a new run (sync, async, or streaming)
//! - GET /runs/{run_id} — get run status
//! - POST /runs/{run_id} — resume a paused/awaiting run

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::debug;

use crate::state::AppState;

/// Run status per ACP spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Created,
    InProgress,
    Awaiting,
    Completed,
    Failed,
    Cancelled,
}

/// Run mode per ACP spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Sync,
    Async,
    Stream,
}

/// A message in the ACP run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMessage {
    pub role: String,
    pub content: String,
}

/// Request to create a new run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCreateRequest {
    pub agent_name: Option<String>,
    pub session_id: Option<String>,
    pub messages: Vec<RunMessage>,
    #[serde(default)]
    pub mode: RunMode,
    pub metadata: Option<serde_json::Value>,
}

/// Request to resume an awaiting run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResumeRequest {
    pub messages: Vec<RunMessage>,
}

/// A run object per ACP spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    pub run_id: String,
    pub status: RunStatus,
    pub agent_name: Option<String>,
    pub session_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<RunMessage>,
    pub output: Vec<RunMessage>,
    pub metadata: Option<serde_json::Value>,
}

/// In-memory run store (swap for persistent backend later).
#[derive(Debug, Default, Clone)]
pub struct RunStore {
    runs: Arc<Mutex<HashMap<String, Run>>>,
}

impl RunStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn create(&self, run: Run) {
        self.runs.lock().await.insert(run.run_id.clone(), run);
    }

    pub async fn get(&self, run_id: &str) -> Option<Run> {
        self.runs.lock().await.get(run_id).cloned()
    }

    pub async fn update_status(&self, run_id: &str, status: RunStatus) {
        if let Some(run) = self.runs.lock().await.get_mut(run_id) {
            run.status = status;
            run.updated_at = now_iso();
        }
    }

    pub async fn append_output(&self, run_id: &str, message: RunMessage) {
        if let Some(run) = self.runs.lock().await.get_mut(run_id) {
            run.output.push(message);
            run.updated_at = now_iso();
        }
    }

    pub async fn list(&self, limit: usize, offset: usize) -> Vec<Run> {
        let runs = self.runs.lock().await;
        runs.values().skip(offset).take(limit).cloned().collect()
    }
}

fn now_iso() -> String {
    let duration = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    let secs = duration.as_secs();
    // Simple ISO-ish timestamp without external dependency
    format!("{secs}")
}

fn generate_run_id() -> String {
    use std::time::UNIX_EPOCH;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis();
    format!("run_{ts:x}")
}

/// Configure ACP /runs routes.
pub fn routes(state: Arc<crate::state::AppState>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/runs", post(create_run).get(list_runs))
        .route("/runs/{run_id}", get(get_run).post(resume_run))
        .with_state((*state).clone())
}

/// POST /runs — create a new run.
pub async fn create_run(
    State(state): State<AppState>,
    Json(req): Json<RunCreateRequest>,
) -> impl IntoResponse {
    let run_id = generate_run_id();
    let now = now_iso();

    let run = Run {
        run_id: run_id.clone(),
        status: RunStatus::Created,
        agent_name: req.agent_name.clone(),
        session_id: req.session_id.clone(),
        created_at: now.clone(),
        updated_at: now,
        messages: req.messages.clone(),
        output: Vec::new(),
        metadata: req.metadata.clone(),
    };

    let store = state.run_store();
    store.create(run).await;

    debug!(run_id = %run_id, mode = ?req.mode, "ACP run created");

    match req.mode {
        RunMode::Stream => {
            // Return SSE stream
            store.update_status(&run_id, RunStatus::InProgress).await;

            let stream = create_run_stream(state, run_id.clone(), req).await;
            Sse::new(stream)
                .keep_alive(KeepAlive::default())
                .into_response()
        }
        RunMode::Async => {
            // Return 202 Accepted with run_id, process in background
            store.update_status(&run_id, RunStatus::InProgress).await;

            let bg_state = state.clone();
            let bg_run_id = run_id.clone();
            let bg_req = req.clone();
            tokio::spawn(async move {
                process_run(bg_state, bg_run_id, bg_req).await;
            });

            (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({
                    "run_id": run_id,
                    "status": "in_progress"
                })),
            )
                .into_response()
        }
        RunMode::Sync => {
            // Process synchronously and return result
            store.update_status(&run_id, RunStatus::InProgress).await;
            process_run(state.clone(), run_id.clone(), req).await;

            let run = store.get(&run_id).await;
            match run {
                Some(r) => Json(r).into_response(),
                None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            }
        }
    }
}

/// GET /runs/{run_id} — get run status.
pub async fn get_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> impl IntoResponse {
    let store = state.run_store();

    match store.get(&run_id).await {
        Some(run) => Json(run).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "run not found"})),
        )
            .into_response(),
    }
}

/// POST /runs/{run_id} — resume an awaiting run.
pub async fn resume_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
    Json(req): Json<RunResumeRequest>,
) -> impl IntoResponse {
    let store = state.run_store();

    let run = match store.get(&run_id).await {
        Some(r) => r,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "run not found"})),
            )
                .into_response()
        }
    };

    if run.status != RunStatus::Awaiting {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "run is not in awaiting state",
                "current_status": run.status
            })),
        )
            .into_response();
    }

    store.update_status(&run_id, RunStatus::InProgress).await;

    // Build a new request from original + resume messages
    let mut all_messages = run.messages.clone();
    all_messages.extend(req.messages);

    let resume_req = RunCreateRequest {
        agent_name: run.agent_name,
        session_id: run.session_id,
        messages: all_messages,
        mode: RunMode::Sync,
        metadata: run.metadata,
    };

    process_run(state.clone(), run_id.clone(), resume_req).await;

    match store.get(&run_id).await {
        Some(r) => Json(r).into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// GET /runs — list runs.
pub async fn list_runs(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.run_store();
    let runs = store.list(100, 0).await;
    Json(serde_json::json!({
        "runs": runs,
        "total": runs.len()
    }))
}

async fn process_run(state: AppState, run_id: String, req: RunCreateRequest) {
    let store = state.run_store();

    let user_text = req
        .messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if user_text.is_empty() {
        store.update_status(&run_id, RunStatus::Failed).await;
        store
            .append_output(
                &run_id,
                RunMessage {
                    role: "agent".to_string(),
                    content: "No user message provided".to_string(),
                },
            )
            .await;
        return;
    }

    // For now, create a simple response acknowledging the run
    // Full integration with Agent.reply() will be added when
    // the orchestrator is fully wired
    store
        .append_output(
            &run_id,
            RunMessage {
                role: "agent".to_string(),
                content: format!("Run {run_id} processed: {user_text}"),
            },
        )
        .await;

    store.update_status(&run_id, RunStatus::Completed).await;
}

async fn create_run_stream(
    state: AppState,
    run_id: String,
    req: RunCreateRequest,
) -> impl Stream<Item = Result<SseEvent, std::convert::Infallible>> {
    let store = state.run_store();
    let state_clone = state.clone();

    // Process the run
    process_run(state_clone, run_id.clone(), req).await;

    let run = store.get(&run_id).await;

    let events: Vec<Result<SseEvent, std::convert::Infallible>> = match run {
        Some(r) => {
            let mut evts = Vec::new();

            evts.push(Ok(SseEvent::default()
                .event("run.status")
                .json_data(serde_json::json!({
                    "run_id": r.run_id,
                    "status": "in_progress"
                }))
                .unwrap_or_else(|_| SseEvent::default())));

            for msg in &r.output {
                evts.push(Ok(SseEvent::default()
                    .event("run.message")
                    .json_data(serde_json::json!({
                        "run_id": r.run_id,
                        "message": msg
                    }))
                    .unwrap_or_else(|_| SseEvent::default())));
            }

            evts.push(Ok(SseEvent::default()
                .event("run.completed")
                .json_data(serde_json::json!({
                    "run_id": r.run_id,
                    "status": "completed"
                }))
                .unwrap_or_else(|_| SseEvent::default())));

            evts
        }
        None => {
            vec![Ok(SseEvent::default()
                .event("run.error")
                .json_data(serde_json::json!({"error": "run processing failed"}))
                .unwrap_or_else(|_| SseEvent::default()))]
        }
    };

    stream::iter(events)
}
