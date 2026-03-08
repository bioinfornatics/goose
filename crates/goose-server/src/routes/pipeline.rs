use axum::extract::State;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::state::AppState;

use goose::pipeline::{
    delete_pipeline, list_pipelines, load_pipeline, save_pipeline, Pipeline, PipelineManifest,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SavePipelineRequest {
    pub pipeline: Pipeline,
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SavePipelineResponse {
    pub id: String,
    pub file_path: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[utoipa::path(
    get,
    path = "/pipelines/list",
    responses(
        (status = 200, description = "List all saved pipelines", body = Vec<PipelineManifest>),
        (status = 500, description = "Internal error")
    ),
    operation_id = "listPipelines"
)]
async fn list_pipelines_handler() -> Result<Json<Vec<PipelineManifest>>, StatusCode> {
    match list_pipelines() {
        Ok(manifests) => Ok(Json(manifests)),
        Err(e) => {
            tracing::error!("Failed to list pipelines: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[utoipa::path(
    get,
    path = "/pipelines/{id}",
    params(
        ("id" = String, Path, description = "Pipeline ID (filename without extension)")
    ),
    responses(
        (status = 200, description = "Pipeline loaded", body = Pipeline),
        (status = 404, description = "Pipeline not found")
    ),
    operation_id = "getPipeline"
)]
async fn get_pipeline_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Pipeline>, StatusCode> {
    match load_pipeline(&id) {
        Ok((pipeline, _)) => Ok(Json(pipeline)),
        Err(e) => {
            tracing::error!("Pipeline not found: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

#[utoipa::path(
    post,
    path = "/pipelines/save",
    request_body = SavePipelineRequest,
    responses(
        (status = 200, description = "Pipeline saved", body = SavePipelineResponse),
        (status = 400, description = "Invalid pipeline")
    ),
    operation_id = "savePipeline"
)]
async fn save_pipeline_handler(
    Json(request): Json<SavePipelineRequest>,
) -> Result<Json<SavePipelineResponse>, (StatusCode, String)> {
    if let Err(e) = request.pipeline.validate() {
        return Err((StatusCode::BAD_REQUEST, e.to_string()));
    }

    let file_path = request.id.map(|id| {
        let dir = goose::pipeline::get_pipeline_dir();
        dir.join(format!("{}.yaml", id))
    });

    match save_pipeline(&request.pipeline, file_path) {
        Ok(path) => {
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            Ok(Json(SavePipelineResponse {
                id,
                file_path: path.to_string_lossy().to_string(),
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[utoipa::path(
    put,
    path = "/pipelines/{id}",
    request_body = Pipeline,
    responses(
        (status = 200, description = "Pipeline updated", body = SavePipelineResponse),
        (status = 400, description = "Invalid pipeline"),
        (status = 404, description = "Pipeline not found")
    ),
    operation_id = "updatePipeline"
)]
async fn update_pipeline_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(pipeline): Json<Pipeline>,
) -> Result<Json<SavePipelineResponse>, (StatusCode, String)> {
    if let Err(e) = pipeline.validate() {
        return Err((StatusCode::BAD_REQUEST, e.to_string()));
    }

    let dir = goose::pipeline::get_pipeline_dir();
    let path = dir.join(format!("{}.yaml", id));
    if !path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Pipeline '{}' not found", id),
        ));
    }

    match save_pipeline(&pipeline, Some(path)) {
        Ok(path) => Ok(Json(SavePipelineResponse {
            id,
            file_path: path.to_string_lossy().to_string(),
        })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[utoipa::path(
    delete,
    path = "/pipelines/{id}",
    params(
        ("id" = String, Path, description = "Pipeline ID to delete")
    ),
    responses(
        (status = 200, description = "Pipeline deleted"),
        (status = 404, description = "Pipeline not found")
    ),
    operation_id = "deletePipeline"
)]
async fn delete_pipeline_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    match delete_pipeline(&id) {
        Ok(()) => Ok(StatusCode::OK),
        Err(e) => Err((StatusCode::NOT_FOUND, e.to_string())),
    }
}

#[utoipa::path(
    post,
    path = "/pipelines/validate",
    request_body = Pipeline,
    responses(
        (status = 200, description = "Validation result", body = ValidatePipelineResponse)
    ),
    operation_id = "validatePipeline"
)]
async fn validate_pipeline_handler(
    Json(pipeline): Json<Pipeline>,
) -> Json<ValidatePipelineResponse> {
    match pipeline.validate() {
        Ok(warnings) => Json(ValidatePipelineResponse {
            valid: true,
            warnings,
            error: None,
        }),
        Err(e) => Json(ValidatePipelineResponse {
            valid: false,
            warnings: vec![],
            error: Some(e.to_string()),
        }),
    }
}

/// Request body for executing a pipeline.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ExecutePipelineRequest {
    /// Maximum number of concurrent tasks (default: 4)
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    /// Optional session ID for agent context
    pub session_id: Option<String>,
}

fn default_max_concurrency() -> usize {
    4
}

#[utoipa::path(
    post,
    path = "/pipelines/{id}/run",
    request_body = ExecutePipelineRequest,
    responses(
        (status = 200, description = "Pipeline execution started, returns SSE stream"),
        (status = 404, description = "Pipeline not found"),
        (status = 400, description = "Invalid pipeline")
    ),
    operation_id = "executePipeline"
)]
async fn execute_pipeline_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<ExecutePipelineRequest>,
) -> Result<
    axum::response::Sse<
        impl futures::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
    >,
    (StatusCode, String),
> {
    use axum::response::sse;
    use goose::agents::dispatch::AgentReplyDispatcher;
    use goose::pipeline_executor::{execute_pipeline, PipelineEvent};

    // Load the pipeline
    let pipeline = match goose::pipeline::load_pipeline(&id) {
        Ok((p, _path)) => p,
        Err(e) => return Err((StatusCode::NOT_FOUND, e.to_string())),
    };

    // Validate
    if let Err(e) = pipeline.validate() {
        return Err((StatusCode::BAD_REQUEST, format!("Pipeline invalid: {}", e)));
    }

    let run_id = format!("run-{}", uuid::Uuid::new_v4());
    let max_concurrency = req.max_concurrency.clamp(1, 32);

    // Get session ID (use provided or create a new one)
    let session_id = req
        .session_id
        .unwrap_or_else(|| format!("pipeline-{}", uuid::Uuid::new_v4()));

    // Get or create the agent for this session
    let agent = match state.get_agent(session_id.clone()).await {
        Ok(a) => a,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get agent: {}", e),
            ))
        }
    };

    // Restore provider so the agent can make LLM calls
    let session = match state.session_manager().get_session(&session_id, true).await {
        Ok(s) => s,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get session: {}", e),
            ))
        }
    };
    if let Err(e) = agent.restore_provider_from_session(&session).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to restore provider: {}", e),
        ));
    }

    // Create the dispatcher (uses the full agent for multi-turn tool execution)
    let dispatcher = AgentReplyDispatcher::new(agent, session_id.clone());

    // Create event channel for SSE streaming
    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel::<PipelineEvent>(256);
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Spawn the pipeline execution
    let exec_pipeline = pipeline.clone();
    let exec_run_id = run_id.clone();
    let exec_cancel = cancel_token.clone();
    tokio::spawn(async move {
        execute_pipeline(
            &exec_pipeline,
            exec_run_id,
            dispatcher,
            max_concurrency,
            Some(exec_cancel),
            Some(event_tx),
        )
        .await;
    });

    // Build SSE stream from pipeline events
    let stream = async_stream::stream! {
        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    let event_type = match &event {
                        PipelineEvent::RunStarted { .. } => "run_started",
                        PipelineEvent::NodeStarted { .. } => "node_started",
                        PipelineEvent::NodeCompleted { .. } => "node_completed",
                        PipelineEvent::NodeFailed { .. } => "node_failed",
                        PipelineEvent::RunCompleted { .. } => "run_completed",
                    };
                    yield Ok(sse::Event::default().event(event_type).data(json));

                    // Stop streaming after run_completed
                    if matches!(event, PipelineEvent::RunCompleted { .. }) {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    };

    Ok(axum::response::Sse::new(stream).keep_alive(
        sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("ping"),
    ))
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/pipelines/list", get(list_pipelines_handler))
        .route("/pipelines/{id}", get(get_pipeline_handler))
        .route("/pipelines/save", post(save_pipeline_handler))
        .route("/pipelines/{id}", put(update_pipeline_handler))
        .route("/pipelines/{id}", delete(delete_pipeline_handler))
        .route("/pipelines/validate", post(validate_pipeline_handler))
        .route("/pipelines/{id}/run", post(execute_pipeline_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use goose::pipeline::Pipeline;
    use goose::pipeline_executor::pipeline_to_tasks;

    #[test]
    fn test_execute_request_defaults() {
        let json = r#"{}"#;
        let req: ExecutePipelineRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.max_concurrency, 4);
        assert!(req.session_id.is_none());
    }

    #[test]
    fn test_execute_request_custom_concurrency() {
        let json = r#"{"max_concurrency": 8, "session_id": "sess-123"}"#;
        let req: ExecutePipelineRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.max_concurrency, 8);
        assert_eq!(req.session_id.unwrap(), "sess-123");
    }

    #[test]
    fn test_execute_request_concurrency_clamp() {
        // Verify the clamping logic matches what the handler does
        let max_c: usize = 100;
        let clamped = max_c.clamp(1, 32);
        assert_eq!(clamped, 32);

        let max_c: usize = 0;
        let clamped = max_c.clamp(1, 32);
        assert_eq!(clamped, 1);
    }

    #[test]
    fn test_pipeline_validate_before_execute() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: invalid-pipeline
nodes:
  - id: agent1
    kind: agent
    label: Test Agent
    config:
      agent: developer
      prompt: test
edges:
  - source: nonexistent
    target: agent1
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let result = pipeline.validate();
        assert!(
            result.is_err(),
            "Pipeline with dangling edge should fail validation"
        );
    }

    #[test]
    fn test_valid_pipeline_produces_tasks() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: test-pipeline
nodes:
  - id: trigger1
    kind: trigger
    label: Start
    config:
      event: manual
  - id: agent1
    kind: agent
    label: Analyze
    config:
      agent: developer
      mode: write
      prompt: analyze the code
  - id: agent2
    kind: agent
    label: Test
    config:
      agent: qa
      mode: review
      prompt: write tests
edges:
  - source: trigger1
    target: agent1
  - source: agent1
    target: agent2
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        assert!(pipeline.validate().is_ok());

        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        // trigger + 2 agents = 3 tasks
        assert_eq!(tasks.len(), 3);

        // agent1 depends on trigger1
        let agent1_task = tasks.iter().find(|(t, _)| t.task_id == "agent1").unwrap();
        assert!(agent1_task.0.depends_on.contains(&"trigger1".to_string()));
        assert_eq!(agent1_task.0.routing.agent_name, "developer");
        assert_eq!(agent1_task.0.routing.mode_slug, "write");

        // agent2 depends on agent1
        let agent2_task = tasks.iter().find(|(t, _)| t.task_id == "agent2").unwrap();
        assert!(agent2_task.0.depends_on.contains(&"agent1".to_string()));
        assert_eq!(agent2_task.0.routing.agent_name, "qa");
        assert_eq!(agent2_task.0.routing.mode_slug, "review");
    }

    #[test]
    fn test_a2a_node_produces_url() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: a2a-test
nodes:
  - id: remote1
    kind: a2a
    label: Remote Agent
    config:
      agent_card_url: "https://remote.example.com/.well-known/agent.json"
      task: analyze remotely
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);

        let (task, a2a_url) = &tasks[0];
        assert!(a2a_url.is_some());
        assert!(a2a_url.as_ref().unwrap().contains("remote.example.com"));
        assert!(task.sub_task_description.contains("analyze remotely"));
    }

    #[test]
    fn test_pipeline_with_cycle_fails_validation() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: cycle-test
nodes:
  - id: a
    kind: agent
    label: A
    config:
      agent: developer
      prompt: a
  - id: b
    kind: agent
    label: B
    config:
      agent: developer
      prompt: b
edges:
  - source: a
    target: b
  - source: b
    target: a
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let result = pipeline.validate();
        assert!(
            result.is_err(),
            "Pipeline with cycle should fail validation"
        );
    }

    #[test]
    fn test_diamond_pipeline_tasks_have_correct_deps() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: diamond
nodes:
  - id: start
    kind: trigger
    label: Start
    config:
      event: manual
  - id: left
    kind: agent
    label: Left
    config:
      agent: developer
      prompt: left path
  - id: right
    kind: agent
    label: Right
    config:
      agent: qa
      prompt: right path
  - id: merge
    kind: agent
    label: Merge
    config:
      agent: developer
      mode: review
      prompt: merge results
edges:
  - source: start
    target: left
  - source: start
    target: right
  - source: left
    target: merge
  - source: right
    target: merge
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        assert!(pipeline.validate().is_ok());

        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 4); // start + left + right + merge

        let merge_task = tasks.iter().find(|(t, _)| t.task_id == "merge").unwrap();
        assert!(merge_task.0.depends_on.contains(&"left".to_string()));
        assert!(merge_task.0.depends_on.contains(&"right".to_string()));
        assert_eq!(merge_task.0.depends_on.len(), 2);
    }

    #[test]
    fn test_save_pipeline_request_serialization() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: ser-test
nodes: []
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let req = SavePipelineRequest {
            pipeline,
            id: Some("ser-test-id".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ser-test"));
        assert!(json.contains("ser-test-id"));

        let parsed: SavePipelineRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.pipeline.name, "ser-test");
        assert_eq!(parsed.id.unwrap(), "ser-test-id");
    }
}
