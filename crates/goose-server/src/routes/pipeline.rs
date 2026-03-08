use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use goose::pipeline::{
    delete_pipeline as fs_delete, generate_pipeline_id, get_pipeline as fs_get,
    list_pipelines as fs_list, save_pipeline as fs_save, Pipeline, PipelineManifest,
};
use serde::{Deserialize, Serialize};
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
    if pipeline.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PipelineErrorResponse {
                error: "pipeline name is required".to_string(),
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
    if let Err(e) = fs_get(&id) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(PipelineErrorResponse {
                error: e.to_string(),
            }),
        ));
    }
    let mut pipeline = req.pipeline;
    if pipeline.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(PipelineErrorResponse {
                error: "pipeline name is required".to_string(),
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
        .with_state(state)
}
