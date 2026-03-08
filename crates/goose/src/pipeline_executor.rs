//! Pipeline Executor — bridges visual pipeline definitions to DAG dispatch.
//!
//! Converts `Pipeline` nodes/edges into tasks and executes them through
//! the existing `Agent::reply()` infrastructure with DAG-aware scheduling.
//!
//! Adapted from feature/cli-via-goosed pipeline_executor.rs and dispatch.rs,
//! simplified to work with upstream's single-agent architecture.

use crate::agents::types::SessionConfig;
use crate::agents::{Agent, AgentEvent};
use crate::pipeline::{NodeKind, Pipeline, PipelineEdge};
use anyhow::{anyhow, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types — kept from feature/cli-via-goosed pipeline_executor.rs
// ---------------------------------------------------------------------------

/// Status of a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PipelineRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Status of a single node execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Completed,
    Failed,
    Cancelled,
}

/// Result of a single node execution.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NodeResult {
    pub node_id: String,
    pub node_kind: String,
    pub label: String,
    pub status: NodeStatus,
    pub output: String,
    pub duration_ms: u64,
}

/// Full result of a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PipelineRunResult {
    pub run_id: String,
    pub pipeline_id: String,
    pub status: PipelineRunStatus,
    pub node_results: Vec<NodeResult>,
    pub total_duration_ms: u64,
    pub error: Option<String>,
}

/// Events emitted during pipeline execution for SSE streaming.
/// Kept from feature/cli-via-goosed pipeline_executor.rs.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipelineEvent {
    RunStarted {
        run_id: String,
        pipeline_id: String,
        total_nodes: usize,
    },
    NodeStarted {
        run_id: String,
        node_id: String,
        node_kind: String,
        label: String,
    },
    NodeCompleted {
        run_id: String,
        node_id: String,
        status: String,
        output: String,
        duration_ms: u64,
    },
    NodeFailed {
        run_id: String,
        node_id: String,
        error: String,
        duration_ms: u64,
    },
    RunCompleted {
        run_id: String,
        status: String,
        total_duration_ms: u64,
    },
}

// ---------------------------------------------------------------------------
// Pipeline Task — simplified from feature branch's SubTask
// ---------------------------------------------------------------------------

/// A single executable task derived from a pipeline node.
/// Simplified from the feature branch's SubTask + RoutingDecision.
#[derive(Debug, Clone)]
pub struct PipelineTask {
    pub task_id: String,
    pub depends_on: Vec<String>,
    pub prompt: String,
    pub node_kind: String,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Pipeline → Tasks conversion (adapted from feature branch)
// ---------------------------------------------------------------------------

/// Convert pipeline nodes + edges into executable tasks.
/// Adapted from feature/cli-via-goosed `pipeline_to_tasks`.
pub fn pipeline_to_tasks(pipeline: &Pipeline) -> Result<Vec<PipelineTask>> {
    let dep_map = build_dependency_map(&pipeline.edges);

    pipeline
        .nodes
        .iter()
        .map(|node| {
            let depends_on = dep_map.get(&node.id).cloned().unwrap_or_default();

            let config = node.config.as_ref();
            let prompt = build_node_prompt(&node.kind, &node.label, config);

            Ok(PipelineTask {
                task_id: node.id.clone(),
                depends_on,
                prompt,
                node_kind: format!("{:?}", node.kind).to_lowercase(),
                label: node.label.clone(),
            })
        })
        .collect()
}

/// Build a map of node_id → Vec<dependency_node_ids> from edges.
/// Kept from feature/cli-via-goosed pipeline_executor.rs.
fn build_dependency_map(edges: &[PipelineEdge]) -> HashMap<String, Vec<String>> {
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        deps.entry(edge.target.clone())
            .or_default()
            .push(edge.source.clone());
    }
    deps
}

/// Extract a string value from node config.
/// Kept from feature/cli-via-goosed pipeline_executor.rs.
fn config_str(config: Option<&serde_json::Value>, key: &str) -> Option<String> {
    config
        .and_then(|c| c.get(key))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Build the prompt for a pipeline node based on its kind.
/// Adapted from feature branch's `node_to_sub_task`.
fn build_node_prompt(kind: &NodeKind, label: &str, config: Option<&serde_json::Value>) -> String {
    match kind {
        NodeKind::Trigger => {
            config_str(config, "prompt").unwrap_or_else(|| format!("Start: {}", label))
        }
        NodeKind::Agent => {
            let instructions =
                config_str(config, "instructions").unwrap_or_else(|| label.to_string());
            let agent_name = config_str(config, "agent").unwrap_or_else(|| "developer".to_string());
            format!(
                "Acting as {}, complete this task: {}",
                agent_name, instructions
            )
        }
        NodeKind::Tool => {
            let extension =
                config_str(config, "extension").unwrap_or_else(|| "developer".to_string());
            let tool = config_str(config, "tool").unwrap_or_else(|| "shell".to_string());
            let args = config_str(config, "args").unwrap_or_default();
            if args.is_empty() {
                format!("Use the {} tool from the {} extension", tool, extension)
            } else {
                format!(
                    "Use the {} tool from the {} extension with: {}",
                    tool, extension, args
                )
            }
        }
        NodeKind::Condition => {
            let expression = config_str(config, "expression").unwrap_or_else(|| "true".to_string());
            format!(
                "Evaluate this condition and respond with only 'true' or 'false': {}",
                expression
            )
        }
        NodeKind::Transform => {
            let template =
                config_str(config, "template").unwrap_or_else(|| "{{input}}".to_string());
            format!(
                "Apply this transformation template to the input: {}",
                template
            )
        }
        NodeKind::Human => {
            config_str(config, "prompt").unwrap_or_else(|| format!("Human review: {}", label))
        }
        NodeKind::A2a => {
            let task = config_str(config, "task").unwrap_or_else(|| label.to_string());
            format!("A2A delegation: {}", task)
        }
    }
}

// ---------------------------------------------------------------------------
// DAG Scheduler — adapted from feature branch's dispatch_compound_dag
// ---------------------------------------------------------------------------

/// Execute a pipeline as a DAG of tasks.
///
/// Uses upstream's `Agent::reply()` to run each task as a subagent prompt.
/// The DAG scheduler is adapted from feature/cli-via-goosed `dispatch_compound_dag`.
pub async fn execute_pipeline(
    pipeline: &Pipeline,
    run_id: String,
    agent: Arc<Agent>,
    session_id: String,
    max_concurrency: usize,
    cancel_token: Option<CancellationToken>,
    event_tx: Option<broadcast::Sender<PipelineEvent>>,
) -> PipelineRunResult {
    let start = Instant::now();
    let pipeline_id = pipeline
        .name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();

    // Emit run started
    if let Some(tx) = &event_tx {
        let _ = tx.send(PipelineEvent::RunStarted {
            run_id: run_id.clone(),
            pipeline_id: pipeline_id.clone(),
            total_nodes: pipeline.nodes.len(),
        });
    }

    // Convert pipeline to tasks
    let tasks = match pipeline_to_tasks(pipeline) {
        Ok(t) => t,
        Err(e) => {
            let err_msg = format!("Failed to convert pipeline to tasks: {}", e);
            warn!("{}", err_msg);
            if let Some(tx) = &event_tx {
                let _ = tx.send(PipelineEvent::RunCompleted {
                    run_id: run_id.clone(),
                    status: "failed".to_string(),
                    total_duration_ms: start.elapsed().as_millis() as u64,
                });
            }
            return PipelineRunResult {
                run_id,
                pipeline_id,
                status: PipelineRunStatus::Failed,
                node_results: vec![],
                total_duration_ms: start.elapsed().as_millis() as u64,
                error: Some(err_msg),
            };
        }
    };

    // Execute the DAG
    let node_results = execute_dag(
        &tasks,
        agent,
        &session_id,
        &run_id,
        max_concurrency,
        cancel_token,
        &event_tx,
    )
    .await;

    let total_duration_ms = start.elapsed().as_millis() as u64;
    let all_succeeded = node_results
        .iter()
        .all(|r| r.status == NodeStatus::Completed);
    let status = if all_succeeded {
        PipelineRunStatus::Completed
    } else {
        PipelineRunStatus::Failed
    };

    if let Some(tx) = &event_tx {
        let _ = tx.send(PipelineEvent::RunCompleted {
            run_id: run_id.clone(),
            status: format!("{:?}", status).to_lowercase(),
            total_duration_ms,
        });
    }

    info!(
        run_id = %run_id,
        status = ?status,
        duration_ms = total_duration_ms,
        "Pipeline execution complete"
    );

    PipelineRunResult {
        run_id,
        pipeline_id,
        status,
        node_results,
        total_duration_ms,
        error: None,
    }
}

/// DAG scheduler — executes tasks respecting dependencies with bounded concurrency.
/// Adapted from feature/cli-via-goosed `dispatch_compound_dag`.
async fn execute_dag(
    tasks: &[PipelineTask],
    agent: Arc<Agent>,
    session_id: &str,
    run_id: &str,
    max_concurrency: usize,
    cancel_token: Option<CancellationToken>,
    event_tx: &Option<broadcast::Sender<PipelineEvent>>,
) -> Vec<NodeResult> {
    let max_concurrency = max_concurrency.clamp(1, 32);

    // Build task index and dependency tracking
    let task_index: HashMap<&str, usize> = tasks
        .iter()
        .enumerate()
        .map(|(i, t)| (t.task_id.as_str(), i))
        .collect();

    // indegree[i] = number of unfinished dependencies for task i
    let mut indegree: Vec<usize> = vec![0; tasks.len()];
    // dependents[i] = list of task indices that depend on task i
    let mut dependents: Vec<Vec<usize>> = vec![vec![]; tasks.len()];

    for (i, task) in tasks.iter().enumerate() {
        for dep_id in &task.depends_on {
            if let Some(&dep_idx) = task_index.get(dep_id.as_str()) {
                indegree[i] += 1;
                dependents[dep_idx].push(i);
            }
        }
    }

    // Collect initially ready tasks (indegree == 0)
    let mut ready: Vec<usize> = indegree
        .iter()
        .enumerate()
        .filter(|(_, &deg)| deg == 0)
        .map(|(i, _)| i)
        .collect();

    let mut results: Vec<Option<NodeResult>> = vec![None; tasks.len()];
    let mut remaining = tasks.len();
    let mut in_flight = futures::stream::FuturesUnordered::new();

    while remaining > 0 {
        // Check cancellation
        if let Some(ref token) = cancel_token {
            if token.is_cancelled() {
                for i in 0..tasks.len() {
                    if results[i].is_none() {
                        results[i] = Some(NodeResult {
                            node_id: tasks[i].task_id.clone(),
                            node_kind: tasks[i].node_kind.clone(),
                            label: tasks[i].label.clone(),
                            status: NodeStatus::Cancelled,
                            output: String::new(),
                            duration_ms: 0,
                        });
                    }
                }
                break;
            }
        }

        // Launch ready tasks up to concurrency limit
        while !ready.is_empty() && in_flight.len() < max_concurrency {
            let idx = ready.remove(0);
            let task = &tasks[idx];

            if let Some(tx) = event_tx {
                let _ = tx.send(PipelineEvent::NodeStarted {
                    run_id: run_id.to_string(),
                    node_id: task.task_id.clone(),
                    node_kind: task.node_kind.clone(),
                    label: task.label.clone(),
                });
            }

            let agent_clone = agent.clone();
            let prompt = task.prompt.clone();
            let session_id = session_id.to_string();
            let cancel = cancel_token.clone();

            in_flight.push(async move {
                let node_start = Instant::now();
                let result = execute_single_task(&agent_clone, &prompt, &session_id, cancel).await;
                let duration_ms = node_start.elapsed().as_millis() as u64;
                (idx, result, duration_ms)
            });
        }

        // Wait for at least one task to complete
        if let Some((idx, result, duration_ms)) = in_flight.next().await {
            let task = &tasks[idx];
            let node_result = match result {
                Ok(output) => {
                    if let Some(tx) = event_tx {
                        let _ = tx.send(PipelineEvent::NodeCompleted {
                            run_id: run_id.to_string(),
                            node_id: task.task_id.clone(),
                            status: "completed".to_string(),
                            output: output.clone(),
                            duration_ms,
                        });
                    }
                    NodeResult {
                        node_id: task.task_id.clone(),
                        node_kind: task.node_kind.clone(),
                        label: task.label.clone(),
                        status: NodeStatus::Completed,
                        output,
                        duration_ms,
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if let Some(tx) = event_tx {
                        let _ = tx.send(PipelineEvent::NodeFailed {
                            run_id: run_id.to_string(),
                            node_id: task.task_id.clone(),
                            error: error_msg.clone(),
                            duration_ms,
                        });
                    }
                    NodeResult {
                        node_id: task.task_id.clone(),
                        node_kind: task.node_kind.clone(),
                        label: task.label.clone(),
                        status: NodeStatus::Failed,
                        output: error_msg,
                        duration_ms,
                    }
                }
            };

            results[idx] = Some(node_result);
            remaining -= 1;

            // Unlock dependents
            for &dep_idx in &dependents[idx] {
                indegree[dep_idx] -= 1;
                if indegree[dep_idx] == 0 {
                    ready.push(dep_idx);
                }
            }
        } else if ready.is_empty() {
            // No in-flight tasks and nothing ready — deadlock (shouldn't happen with valid DAG)
            warn!("DAG scheduler: no progress possible, breaking");
            break;
        }
    }

    results
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            r.unwrap_or(NodeResult {
                node_id: tasks[i].task_id.clone(),
                node_kind: tasks[i].node_kind.clone(),
                label: tasks[i].label.clone(),
                status: NodeStatus::Cancelled,
                output: String::new(),
                duration_ms: 0,
            })
        })
        .collect()
}

/// Execute a single pipeline task by sending the prompt to the agent.
/// Uses upstream's `Agent::reply()` for full multi-turn tool execution.
async fn execute_single_task(
    agent: &Agent,
    prompt: &str,
    session_id: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<String> {
    use crate::conversation::message::Message;
    use futures::stream::BoxStream;

    let user_message = Message::user().with_text(prompt);
    let session_config = SessionConfig {
        id: session_id.to_string(),
        schedule_id: None,
        max_turns: Some(10),
        retry_config: None,
    };

    let mut stream: BoxStream<'_, anyhow::Result<AgentEvent>> = agent
        .reply(user_message, session_config, cancel_token)
        .await
        .map_err(|e| anyhow!("Agent reply failed: {}", e))?;

    let mut output_texts = Vec::new();
    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(AgentEvent::Message(msg)) => {
                for content in &msg.content {
                    if let Some(text) = content.as_text() {
                        output_texts.push(text.to_string());
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                warn!("Error during task execution: {}", e);
            }
        }
    }

    if output_texts.is_empty() {
        Ok("(no output)".to_string())
    } else {
        Ok(output_texts.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Tests — adapted from feature branch
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Pipeline;

    fn sample_pipeline_yaml() -> &'static str {
        r#"
apiVersion: goose/v1
kind: Pipeline
name: Test Pipeline
description: A test pipeline
nodes:
  - id: trigger1
    kind: trigger
    label: Start
    config:
      prompt: "Begin the workflow"
  - id: agent1
    kind: agent
    label: Analyze Code
    config:
      agent: developer
      instructions: "Review the code for quality"
  - id: condition1
    kind: condition
    label: Tests Pass?
    config:
      expression: "all tests pass"
edges:
  - source: trigger1
    target: agent1
  - source: agent1
    target: condition1
"#
    }

    #[test]
    fn pipeline_to_tasks_basic() {
        let pipeline = Pipeline::from_yaml(sample_pipeline_yaml()).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();

        assert_eq!(tasks.len(), 3);

        // Trigger has no dependencies
        let trigger = tasks.iter().find(|t| t.task_id == "trigger1").unwrap();
        assert!(trigger.depends_on.is_empty());
        assert!(trigger.prompt.contains("Begin the workflow"));

        // Agent depends on trigger
        let agent = tasks.iter().find(|t| t.task_id == "agent1").unwrap();
        assert!(agent.depends_on.contains(&"trigger1".to_string()));
        assert!(agent.prompt.contains("developer"));
        assert!(agent.prompt.contains("Review the code"));

        // Condition depends on agent
        let condition = tasks.iter().find(|t| t.task_id == "condition1").unwrap();
        assert!(condition.depends_on.contains(&"agent1".to_string()));
        assert!(condition.prompt.contains("true' or 'false'"));
    }

    #[test]
    fn pipeline_to_tasks_diamond() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: Diamond
description: Diamond DAG
nodes:
  - id: start
    kind: trigger
    label: Start
  - id: left
    kind: agent
    label: Left Branch
    config:
      instructions: "Left task"
  - id: right
    kind: agent
    label: Right Branch
    config:
      instructions: "Right task"
  - id: merge
    kind: agent
    label: Merge
    config:
      instructions: "Merge results"
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
        let tasks = pipeline_to_tasks(&pipeline).unwrap();

        assert_eq!(tasks.len(), 4);

        let merge = tasks.iter().find(|t| t.task_id == "merge").unwrap();
        assert_eq!(merge.depends_on.len(), 2);
        assert!(merge.depends_on.contains(&"left".to_string()));
        assert!(merge.depends_on.contains(&"right".to_string()));
    }

    #[test]
    fn pipeline_to_tasks_tool_node() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: Tool Test
description: Test tool node conversion
nodes:
  - id: tool1
    kind: tool
    label: Run Tests
    config:
      extension: developer
      tool: shell
      args: "cargo test"
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();

        assert_eq!(tasks.len(), 1);
        let tool = &tasks[0];
        assert!(tool.prompt.contains("shell"));
        assert!(tool.prompt.contains("developer"));
        assert!(tool.prompt.contains("cargo test"));
    }

    #[test]
    fn pipeline_to_tasks_a2a_node() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: A2A Test
description: Test A2A node
nodes:
  - id: a2a1
    kind: a2a
    label: Remote Analysis
    config:
      task: "analyze data remotely"
      agent_card_url: "https://remote.example.com/.well-known/agent.json"
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();

        assert_eq!(tasks.len(), 1);
        let a2a = &tasks[0];
        assert!(a2a.prompt.contains("analyze data remotely"));
    }

    #[test]
    fn pipeline_to_tasks_transform_node() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: Transform Test
description: Test transform
nodes:
  - id: t1
    kind: transform
    label: Format Output
    config:
      template: "Summary: {{input}}"
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();

        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].prompt.contains("Summary: {{input}}"));
    }

    #[test]
    fn config_str_extraction() {
        let config = serde_json::json!({
            "agent": "developer",
            "instructions": "do stuff",
            "count": 42
        });

        assert_eq!(
            config_str(Some(&config), "agent"),
            Some("developer".to_string())
        );
        assert_eq!(
            config_str(Some(&config), "instructions"),
            Some("do stuff".to_string())
        );
        assert_eq!(config_str(Some(&config), "count"), None); // numeric, not string
        assert_eq!(config_str(Some(&config), "missing"), None);
        assert_eq!(config_str(None, "agent"), None);
    }

    #[test]
    fn node_result_serialization() {
        let result = NodeResult {
            node_id: "n1".to_string(),
            node_kind: "agent".to_string(),
            label: "Test".to_string(),
            status: NodeStatus::Completed,
            output: "done".to_string(),
            duration_ms: 500,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: NodeResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.node_id, "n1");
        assert_eq!(parsed.status, NodeStatus::Completed);
    }

    #[test]
    fn pipeline_event_serialization() {
        let event = PipelineEvent::NodeStarted {
            run_id: "run-1".to_string(),
            node_id: "n1".to_string(),
            node_kind: "agent".to_string(),
            label: "Analyze".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"node_started\""));
        assert!(json.contains("\"run_id\":\"run-1\""));
    }

    #[test]
    fn pipeline_run_result_serialization() {
        let result = PipelineRunResult {
            run_id: "run-1".to_string(),
            pipeline_id: "pipe-1".to_string(),
            status: PipelineRunStatus::Completed,
            node_results: vec![],
            total_duration_ms: 1234,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: PipelineRunResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.status, PipelineRunStatus::Completed);
        assert_eq!(parsed.total_duration_ms, 1234);
    }

    #[test]
    fn build_dependency_map_basic() {
        let edges = vec![
            PipelineEdge {
                source: "a".to_string(),
                target: "b".to_string(),
                label: None,
                condition: None,
            },
            PipelineEdge {
                source: "a".to_string(),
                target: "c".to_string(),
                label: None,
                condition: None,
            },
            PipelineEdge {
                source: "b".to_string(),
                target: "d".to_string(),
                label: None,
                condition: None,
            },
            PipelineEdge {
                source: "c".to_string(),
                target: "d".to_string(),
                label: None,
                condition: None,
            },
        ];

        let deps = build_dependency_map(&edges);
        assert_eq!(deps.get("b").unwrap(), &vec!["a".to_string()]);
        assert_eq!(deps.get("c").unwrap(), &vec!["a".to_string()]);
        let d_deps = deps.get("d").unwrap();
        assert_eq!(d_deps.len(), 2);
        assert!(d_deps.contains(&"b".to_string()));
        assert!(d_deps.contains(&"c".to_string()));
        assert!(!deps.contains_key("a")); // root has no deps
    }

    #[test]
    fn build_node_prompt_all_kinds() {
        // Trigger
        let prompt = build_node_prompt(&NodeKind::Trigger, "Start", None);
        assert!(prompt.contains("Start"));

        // Agent with config
        let config = serde_json::json!({"agent": "qa", "instructions": "check quality"});
        let prompt = build_node_prompt(&NodeKind::Agent, "QA Check", Some(&config));
        assert!(prompt.contains("qa"));
        assert!(prompt.contains("check quality"));

        // Tool
        let config = serde_json::json!({"extension": "dev", "tool": "shell", "args": "ls"});
        let prompt = build_node_prompt(&NodeKind::Tool, "List Files", Some(&config));
        assert!(prompt.contains("shell"));
        assert!(prompt.contains("ls"));

        // Condition
        let config = serde_json::json!({"expression": "x > 5"});
        let prompt = build_node_prompt(&NodeKind::Condition, "Check", Some(&config));
        assert!(prompt.contains("x > 5"));
        assert!(prompt.contains("true' or 'false'"));

        // Transform
        let config = serde_json::json!({"template": "{{data}}"});
        let prompt = build_node_prompt(&NodeKind::Transform, "T", Some(&config));
        assert!(prompt.contains("{{data}}"));

        // Human
        let prompt = build_node_prompt(&NodeKind::Human, "Review", None);
        assert!(prompt.contains("Human review"));

        // A2A
        let config = serde_json::json!({"task": "remote work"});
        let prompt = build_node_prompt(&NodeKind::A2a, "Remote", Some(&config));
        assert!(prompt.contains("remote work"));
    }
}
