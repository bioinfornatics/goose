use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use bytes::Bytes;
use futures::Stream;
use goose::pipeline::{
    delete_pipeline as fs_delete, generate_pipeline_id, get_pipeline as fs_get,
    list_pipelines as fs_list, save_pipeline as fs_save, Pipeline, PipelineManifest,
};
use goose::pipeline_executor::{execute_pipeline, PipelineEvent};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use crate::state::AppState;

// ── Request / Response types ───────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct SavePipelineRequest {
    pub pipeline: Pipeline,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SavePipelineResponse {
    pub id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PipelineListResponse {
    pub pipelines: Vec<PipelineManifest>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PipelineResponse {
    pub pipeline: Pipeline,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ValidateResponse {
    pub valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PipelineErrorResponse {
    pub error: String,
}

// ── Handlers ───────────────────────────────────────────────────────────

#[utoipa::path(
    get,
    path = "/pipelines",
    responses(
        (status = 200, description = "List all pipelines", body = PipelineListResponse),
    ),
    tag = "pipeline"
)]
pub async fn list_pipelines(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<PipelineListResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    match fs_list() {
        Ok(pipelines) => Ok(Json(PipelineListResponse { pipelines })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 200, description = "Pipeline found", body = PipelineResponse),
        (status = 404, description = "Pipeline not found", body = PipelineErrorResponse),
    ),
    tag = "pipeline"
)]
pub async fn get_pipeline(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<PipelineResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    match fs_get(&id) {
        Ok(pipeline) => Ok(Json(PipelineResponse { pipeline })),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((
                status,
                Json(PipelineErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

#[utoipa::path(
    post,
    path = "/pipelines",
    request_body = SavePipelineRequest,
    responses(
        (status = 201, description = "Pipeline created", body = SavePipelineResponse),
        (status = 400, description = "Validation failed", body = PipelineErrorResponse),
    ),
    tag = "pipeline"
)]
pub async fn create_pipeline(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<SavePipelineRequest>,
) -> Result<(StatusCode, Json<SavePipelineResponse>), (StatusCode, Json<PipelineErrorResponse>)> {
    let mut pipeline = req.pipeline;
    if let Err(e) = pipeline.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        ));
    }
    let id = generate_pipeline_id(&pipeline.name);
    match fs_save(&id, &mut pipeline) {
        Ok(saved_id) => Ok((
            StatusCode::CREATED,
            Json(SavePipelineResponse { id: saved_id }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

#[utoipa::path(
    put,
    path = "/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = SavePipelineRequest,
    responses(
        (status = 200, description = "Pipeline updated", body = SavePipelineResponse),
        (status = 400, description = "Validation failed", body = PipelineErrorResponse),
        (status = 404, description = "Pipeline not found", body = PipelineErrorResponse),
    ),
    tag = "pipeline"
)]
pub async fn update_pipeline(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SavePipelineRequest>,
) -> Result<Json<SavePipelineResponse>, (StatusCode, Json<PipelineErrorResponse>)> {
    // Verify the pipeline exists
    if let Err(e) = fs_get(&id) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        ));
    }
    let mut pipeline = req.pipeline;
    if let Err(e) = pipeline.validate() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        ));
    }
    match fs_save(&id, &mut pipeline) {
        Ok(saved_id) => Ok(Json(SavePipelineResponse { id: saved_id })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

#[utoipa::path(
    delete,
    path = "/pipelines/{id}",
    params(("id" = String, Path, description = "Pipeline ID")),
    responses(
        (status = 204, description = "Pipeline deleted"),
        (status = 404, description = "Pipeline not found", body = PipelineErrorResponse),
    ),
    tag = "pipeline"
)]
pub async fn delete_pipeline(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<PipelineErrorResponse>)> {
    match fs_delete(&id) {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            Err((
                status,
                Json(PipelineErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    }
}

#[utoipa::path(
    post,
    path = "/pipelines/validate",
    request_body = SavePipelineRequest,
    responses(
        (status = 200, description = "Validation result", body = ValidateResponse),
    ),
    tag = "pipeline"
)]
pub async fn validate_pipeline(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<SavePipelineRequest>,
) -> Json<ValidateResponse> {
    match req.pipeline.validate() {
        Ok(()) => Json(ValidateResponse {
            valid: true,
            errors: None,
        }),
        Err(e) => Json(ValidateResponse {
            valid: false,
            errors: Some(vec![e.to_string()]),
        }),
    }
}

// ── Execution types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct ExecutePipelineRequest {
    #[serde(default = "default_session_id")]
    pub session_id: String,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
}

fn default_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_max_concurrency() -> usize {
    4
}

// SSE response type (follows upstream reply.rs pattern)
pub struct PipelineSseResponse {
    rx: ReceiverStream<String>,
}

impl PipelineSseResponse {
    fn new(rx: ReceiverStream<String>) -> Self {
        Self { rx }
    }
}

impl Stream for PipelineSseResponse {
    type Item = Result<Bytes, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx)
            .poll_next(cx)
            .map(|opt| opt.map(|s| Ok(Bytes::from(s))))
    }
}

impl axum::response::IntoResponse for PipelineSseResponse {
    fn into_response(self) -> axum::response::Response {
        let body = axum::body::Body::from_stream(self);
        http::Response::builder()
            .header("Content-Type", "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("Connection", "keep-alive")
            .body(body)
            .unwrap()
    }
}

fn format_sse_event(event_type: &str, data: &str) -> String {
    format!("event: {event_type}\ndata: {data}\n\n")
}

// ── Execution handler ──────────────────────────────────────────────────

#[utoipa::path(
    post,
    path = "/pipelines/{id}/run",
    params(("id" = String, Path, description = "Pipeline ID")),
    request_body = ExecutePipelineRequest,
    responses(
        (status = 200, description = "Streaming pipeline execution events",
         content_type = "text/event-stream"),
        (status = 404, description = "Pipeline not found", body = PipelineErrorResponse),
    ),
    tag = "pipeline"
)]
pub async fn execute_pipeline_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(request): Json<ExecutePipelineRequest>,
) -> Result<PipelineSseResponse, (StatusCode, Json<PipelineErrorResponse>)> {
    let pipeline = match fs_get(&id) {
        Ok(p) => p,
        Err(e) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(PipelineErrorResponse {
                    error: e.to_string(),
                }),
            ));
        }
    };

    let session_id = request.session_id;
    let max_concurrency = request.max_concurrency.clamp(1, 8);
    let run_id = uuid::Uuid::new_v4().to_string();

    let (tx, rx) = mpsc::channel::<String>(100);
    let stream = ReceiverStream::new(rx);
    let cancel_token = CancellationToken::new();

    let task_cancel = cancel_token.clone();

    drop(tokio::spawn(async move {
        // Get or create an agent for this session
        let agent = match state.get_agent(session_id.clone()).await {
            Ok(agent) => agent,
            Err(e) => {
                tracing::error!("Failed to get agent for pipeline execution: {}", e);
                let _ = tx
                    .send(format_sse_event(
                        "error",
                        &serde_json::json!({"error": e.to_string()}).to_string(),
                    ))
                    .await;
                return;
            }
        };

        // Create an event broadcast channel
        let (event_tx, mut event_rx) = tokio::sync::broadcast::channel::<PipelineEvent>(64);

        // Spawn a bridge task: forward PipelineEvents to SSE
        let bridge_tx = tx.clone();
        let bridge_cancel = task_cancel.clone();
        let bridge_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = bridge_cancel.cancelled() => break,
                    result = event_rx.recv() => {
                        match result {
                            Ok(event) => {
                                let data = serde_json::to_string(&event)
                                    .unwrap_or_default();
                                let event_type = match &event {
                                    PipelineEvent::RunStarted { .. } => "run_started",
                                    PipelineEvent::NodeStarted { .. } => "node_started",
                                    PipelineEvent::NodeCompleted { .. } => "node_completed",
                                    PipelineEvent::NodeFailed { .. } => "node_failed",
                                    PipelineEvent::RunCompleted { .. } => "run_completed",
                                };
                                if bridge_tx.send(format_sse_event(event_type, &data)).await.is_err() {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        }
                    }
                }
            }
        });

        // Execute the pipeline
        let result = execute_pipeline(
            &pipeline,
            run_id,
            agent,
            session_id,
            max_concurrency,
            Some(task_cancel.clone()),
            Some(event_tx),
        )
        .await;

        // Send the final result
        if let Ok(json) = serde_json::to_string(&result) {
            let _ = tx.send(format_sse_event("result", &json)).await;
        }

        // Clean up
        task_cancel.cancel();
        let _ = bridge_handle.await;
    }));

    Ok(PipelineSseResponse::new(stream))
}

// ── Router ─────────────────────────────────────────────────────────────

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/pipelines", get(list_pipelines).post(create_pipeline))
        .route("/pipelines/validate", post(validate_pipeline))
        .route(
            "/pipelines/{id}",
            get(get_pipeline)
                .put(update_pipeline)
                .delete(delete_pipeline),
        )
        .route("/pipelines/{id}/run", post(execute_pipeline_handler))
        .with_state(state)
}
