//! Pipeline Executor — bridges visual pipeline definitions to DAG dispatch.
//!
//! Converts `Pipeline` nodes/edges into `SubTask`s and executes them
//! through the existing `dispatch_compound_dag` infrastructure.

use crate::agents::dispatch::{
    dispatch_compound_dag, AgentReplyDispatcher, DispatchEvent, DispatchStatus,
};
use crate::agents::intent_router::RoutingDecision;
use crate::agents::orchestrator_agent::SubTask;
use crate::pipeline::{NodeKind, Pipeline, PipelineEdge};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Status of a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Result of a single node execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: String,
    pub node_kind: String,
    pub label: String,
    pub status: DispatchStatus,
    pub output: String,
    pub duration_ms: u64,
}

/// Full result of a pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRunResult {
    pub run_id: String,
    pub pipeline_id: String,
    pub status: PipelineRunStatus,
    pub node_results: Vec<NodeResult>,
    pub total_duration_ms: u64,
    pub error: Option<String>,
}

/// Events emitted during pipeline execution for SSE streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Convert a pipeline into a list of (SubTask, Option<A2A_URL>) pairs
/// suitable for `dispatch_compound_dag`.
pub fn pipeline_to_tasks(pipeline: &Pipeline) -> Result<Vec<(SubTask, Option<String>)>> {
    let edge_map = build_dependency_map(&pipeline.edges);
    let mut tasks = Vec::with_capacity(pipeline.nodes.len());

    for node in &pipeline.nodes {
        let depends_on = edge_map.get(node.id.as_str()).cloned().unwrap_or_default();

        let (sub_task, a2a_url) = node_to_sub_task(node, depends_on)?;
        tasks.push((sub_task, a2a_url));
    }

    Ok(tasks)
}

/// Build a map: target_node_id → [source_node_ids] from edges.
fn build_dependency_map(edges: &[PipelineEdge]) -> HashMap<String, Vec<String>> {
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        deps.entry(edge.target.clone())
            .or_default()
            .push(edge.source.clone());
    }
    deps
}

/// Convert a single PipelineNode into a SubTask + optional A2A URL.
fn node_to_sub_task(
    node: &crate::pipeline::PipelineNode,
    depends_on: Vec<String>,
) -> Result<(SubTask, Option<String>)> {
    let config = &node.config;

    match node.kind {
        NodeKind::Agent => {
            let agent_name = config_str(config, "agent").unwrap_or_else(|| "Developer".to_string());
            let mode_slug = config_str(config, "mode").unwrap_or_else(|| "write".to_string());
            let prompt = config_str(config, "prompt").unwrap_or_else(|| node.label.clone());

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: agent_name.clone(),
                    mode_slug,
                    confidence: 1.0,
                    reasoning: format!("Pipeline node: {}", node.label),
                },
                sub_task_description: prompt,
            };
            Ok((sub_task, None))
        }

        NodeKind::Tool => {
            let extension =
                config_str(config, "extension").unwrap_or_else(|| "developer".to_string());
            let tool = config_str(config, "tool").unwrap_or_else(|| "shell".to_string());
            let args = config_str(config, "args").unwrap_or_default();

            let description = if args.is_empty() {
                format!("Use the {} tool from the {} extension", tool, extension)
            } else {
                format!(
                    "Use the {} tool from the {} extension with: {}",
                    tool, extension, args
                )
            };

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: "Developer".to_string(),
                    mode_slug: "write".to_string(),
                    confidence: 1.0,
                    reasoning: format!("Pipeline tool node: {}", node.label),
                },
                sub_task_description: description,
            };
            Ok((sub_task, None))
        }

        NodeKind::A2a => {
            let agent_card_url = config_str(config, "agent_card_url")
                .ok_or_else(|| anyhow!("A2A node '{}' missing agent_card_url", node.id))?;
            let task = config_str(config, "task").unwrap_or_else(|| node.label.clone());

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: "A2A".to_string(),
                    mode_slug: "default".to_string(),
                    confidence: 1.0,
                    reasoning: format!("A2A delegation to {}", agent_card_url),
                },
                sub_task_description: task,
            };
            Ok((sub_task, Some(agent_card_url)))
        }

        NodeKind::Condition => {
            let expression = config_str(config, "expression").unwrap_or_else(|| "true".to_string());

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: "Developer".to_string(),
                    mode_slug: "ask".to_string(),
                    confidence: 1.0,
                    reasoning: format!("Evaluate condition: {}", expression),
                },
                sub_task_description: format!(
                    "Evaluate this condition and respond with only 'true' or 'false': {}",
                    expression
                ),
            };
            Ok((sub_task, None))
        }

        NodeKind::Transform => {
            let template =
                config_str(config, "template").unwrap_or_else(|| "{{input}}".to_string());

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: "Developer".to_string(),
                    mode_slug: "write".to_string(),
                    confidence: 1.0,
                    reasoning: format!("Transform: {}", node.label),
                },
                sub_task_description: format!(
                    "Apply this transformation template to the input: {}",
                    template
                ),
            };
            Ok((sub_task, None))
        }

        NodeKind::Human => {
            let prompt = config_str(config, "prompt")
                .unwrap_or_else(|| "Waiting for human input".to_string());

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: "Developer".to_string(),
                    mode_slug: "ask".to_string(),
                    confidence: 1.0,
                    reasoning: format!("Human checkpoint: {}", node.label),
                },
                sub_task_description: format!(
                    "This is a human review checkpoint. Present to the user: {}",
                    prompt
                ),
            };
            Ok((sub_task, None))
        }

        NodeKind::Trigger => {
            let event_type = config_str(config, "event").unwrap_or_else(|| "manual".to_string());

            let sub_task = SubTask {
                task_id: node.id.clone(),
                depends_on,
                routing: RoutingDecision {
                    agent_name: "Developer".to_string(),
                    mode_slug: "ask".to_string(),
                    confidence: 1.0,
                    reasoning: format!("Trigger: {}", event_type),
                },
                sub_task_description: format!(
                    "Pipeline triggered by '{}' event. Acknowledge and pass through.",
                    event_type
                ),
            };
            Ok((sub_task, None))
        }
    }
}

/// Extract a string value from a node's config map.
fn config_str(config: &HashMap<String, serde_json::Value>, key: &str) -> Option<String> {
    config.get(key).map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    })
}

/// Execute a pipeline using the DAG dispatcher.
///
/// Returns a stream of `PipelineEvent`s for real-time progress and
/// the final `PipelineRunResult`.
pub async fn execute_pipeline(
    pipeline: &Pipeline,
    run_id: String,
    dispatcher: AgentReplyDispatcher,
    max_concurrency: usize,
    cancel_token: Option<tokio_util::sync::CancellationToken>,
    event_tx: Option<broadcast::Sender<PipelineEvent>>,
) -> PipelineRunResult {
    let start = std::time::Instant::now();
    let pipeline_id = pipeline.name.clone();

    info!(run_id = %run_id, pipeline_id = %pipeline_id, "Starting pipeline execution");

    // Emit run started event
    if let Some(tx) = &event_tx {
        let _ = tx.send(PipelineEvent::RunStarted {
            run_id: run_id.clone(),
            pipeline_id: pipeline_id.clone(),
            total_nodes: pipeline.nodes.len(),
        });
    }

    // Convert pipeline to tasks
    let tasks = match pipeline_to_tasks(pipeline) {
        Ok(tasks) => tasks,
        Err(e) => {
            let error_msg = format!("Failed to convert pipeline to tasks: {}", e);
            warn!(run_id = %run_id, error = %error_msg);
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
                error: Some(error_msg),
            };
        }
    };

    // Build node_id → index map for correlating dispatch results
    let _node_id_to_index: HashMap<String, usize> = pipeline
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();

    // Create a dispatch event bridge: forward DispatchEvents to PipelineEvents
    let (dispatch_tx, mut dispatch_rx) = broadcast::channel::<DispatchEvent>(256);
    let bridge_event_tx = event_tx.clone();
    let bridge_run_id = run_id.clone();
    let bridge_nodes: Vec<_> = pipeline
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                format!("{:?}", n.kind).to_lowercase(),
                n.label.clone(),
            )
        })
        .collect();

    let bridge_handle = tokio::spawn(async move {
        while let Ok(event) = dispatch_rx.recv().await {
            if let Some(tx) = &bridge_event_tx {
                match event {
                    DispatchEvent::Started { task_index, .. } => {
                        if let Some((id, kind, label)) = bridge_nodes.get(task_index) {
                            let _ = tx.send(PipelineEvent::NodeStarted {
                                run_id: bridge_run_id.clone(),
                                node_id: id.clone(),
                                node_kind: kind.clone(),
                                label: label.clone(),
                            });
                        }
                    }
                    DispatchEvent::Completed { task_index, result } => {
                        if let Some((id, _, _)) = bridge_nodes.get(task_index) {
                            let _ = tx.send(PipelineEvent::NodeCompleted {
                                run_id: bridge_run_id.clone(),
                                node_id: id.clone(),
                                status: format!("{:?}", result.status).to_lowercase(),
                                output: result.output.clone(),
                                duration_ms: result.duration_ms,
                            });
                        }
                    }
                    DispatchEvent::Failed { task_index, error } => {
                        if let Some((id, _, _)) = bridge_nodes.get(task_index) {
                            let _ = tx.send(PipelineEvent::NodeFailed {
                                run_id: bridge_run_id.clone(),
                                node_id: id.clone(),
                                error,
                                duration_ms: 0,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    // Execute the DAG
    let dispatch_results = dispatch_compound_dag(
        &dispatcher,
        &tasks,
        max_concurrency,
        Some(dispatch_tx),
        cancel_token,
    )
    .await;

    // Convert dispatch results to node results
    let mut node_results = Vec::with_capacity(dispatch_results.len());
    let mut all_succeeded = true;

    for (i, result) in dispatch_results.into_iter().enumerate() {
        if let Some(node) = pipeline.nodes.get(i) {
            let node_result = NodeResult {
                node_id: node.id.clone(),
                node_kind: format!("{:?}", node.kind).to_lowercase(),
                label: node.label.clone(),
                status: result.status.clone(),
                output: result.output.clone(),
                duration_ms: result.duration_ms,
            };
            if result.status != DispatchStatus::Completed {
                all_succeeded = false;
            }
            node_results.push(node_result);
        }
    }

    let total_duration_ms = start.elapsed().as_millis() as u64;
    let status = if all_succeeded {
        PipelineRunStatus::Completed
    } else {
        PipelineRunStatus::Failed
    };

    // Emit run completed
    if let Some(tx) = &event_tx {
        let _ = tx.send(PipelineEvent::RunCompleted {
            run_id: run_id.clone(),
            status: format!("{:?}", status).to_lowercase(),
            total_duration_ms,
        });
    }

    // Clean up bridge task
    bridge_handle.abort();

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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pipeline() -> Pipeline {
        Pipeline::from_yaml(
            r#"
apiVersion: goose/v1
kind: Pipeline
name: test-pipeline
description: Test pipeline
version: "1.0"
nodes:
  - id: trigger1
    kind: trigger
    label: Start
    config:
      event: manual
  - id: agent1
    kind: agent
    label: Analyze Code
    config:
      agent: Developer
      mode: ask
      prompt: "Analyze the main.rs file"
  - id: agent2
    kind: agent
    label: Write Tests
    config:
      agent: Developer
      mode: write
      prompt: "Write unit tests for main.rs"
  - id: condition1
    kind: condition
    label: Tests Pass?
    config:
      expression: "all tests passed"
  - id: a2a1
    kind: a2a
    label: External Review
    config:
      agent_card_url: "http://localhost:8080/.well-known/agent.json"
      task: "Review the code changes"
edges:
  - source: trigger1
    target: agent1
  - source: agent1
    target: agent2
  - source: agent1
    target: condition1
  - source: condition1
    target: a2a1
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_pipeline_to_tasks_basic() {
        let pipeline = sample_pipeline();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();

        assert_eq!(tasks.len(), 5);

        // trigger1 has no dependencies
        assert!(tasks[0].0.depends_on.is_empty());
        assert_eq!(tasks[0].0.task_id, "trigger1");
        assert!(tasks[0].1.is_none()); // no A2A URL

        // agent1 depends on trigger1
        assert_eq!(tasks[1].0.task_id, "agent1");
        assert_eq!(tasks[1].0.depends_on, vec!["trigger1"]);
        assert_eq!(tasks[1].0.routing.agent_name, "Developer");
        assert_eq!(tasks[1].0.routing.mode_slug, "ask");

        // agent2 depends on agent1
        assert_eq!(tasks[2].0.task_id, "agent2");
        assert_eq!(tasks[2].0.depends_on, vec!["agent1"]);
        assert_eq!(tasks[2].0.routing.mode_slug, "write");

        // condition1 depends on agent1
        assert_eq!(tasks[3].0.task_id, "condition1");
        assert_eq!(tasks[3].0.depends_on, vec!["agent1"]);

        // a2a1 depends on condition1 and has an A2A URL
        assert_eq!(tasks[4].0.task_id, "a2a1");
        assert_eq!(tasks[4].0.depends_on, vec!["condition1"]);
        assert!(tasks[4].1.is_some());
        assert_eq!(
            tasks[4].1.as_ref().unwrap(),
            "http://localhost:8080/.well-known/agent.json"
        );
    }

    #[test]
    fn test_pipeline_to_tasks_parallel() {
        let pipeline = Pipeline::from_yaml(
            r#"
apiVersion: goose/v1
kind: Pipeline
name: parallel-test
description: Test parallel execution
version: "1.0"
nodes:
  - id: start
    kind: trigger
    label: Start
    config:
      event: manual
  - id: branch_a
    kind: agent
    label: Branch A
    config:
      agent: Developer
      mode: write
      prompt: "Task A"
  - id: branch_b
    kind: agent
    label: Branch B
    config:
      agent: QA
      mode: review
      prompt: "Task B"
  - id: merge
    kind: transform
    label: Merge Results
    config:
      template: "Combine results from A and B"
edges:
  - source: start
    target: branch_a
  - source: start
    target: branch_b
  - source: branch_a
    target: merge
  - source: branch_b
    target: merge
"#,
        )
        .unwrap();

        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 4);

        // branch_a and branch_b both depend on start
        assert_eq!(tasks[1].0.depends_on, vec!["start"]);
        assert_eq!(tasks[2].0.depends_on, vec!["start"]);

        // merge depends on both branches
        let merge_deps = &tasks[3].0.depends_on;
        assert_eq!(merge_deps.len(), 2);
        assert!(merge_deps.contains(&"branch_a".to_string()));
        assert!(merge_deps.contains(&"branch_b".to_string()));
    }

    #[test]
    fn test_pipeline_to_tasks_tool_node() {
        let pipeline = Pipeline::from_yaml(
            r#"
apiVersion: goose/v1
kind: Pipeline
name: tool-test
version: "1.0"
nodes:
  - id: tool1
    kind: tool
    label: Run Tests
    config:
      extension: developer
      tool: shell
      args: "cargo test"
edges: []
"#,
        )
        .unwrap();

        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].0.sub_task_description.contains("shell"));
        assert!(tasks[0].0.sub_task_description.contains("cargo test"));
    }

    #[test]
    fn test_pipeline_to_tasks_a2a_missing_url() {
        let pipeline = Pipeline::from_yaml(
            r#"
apiVersion: goose/v1
kind: Pipeline
name: a2a-error-test
version: "1.0"
nodes:
  - id: bad_a2a
    kind: a2a
    label: Bad A2A
    config: {}
edges: []
"#,
        )
        .unwrap();

        let result = pipeline_to_tasks(&pipeline);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing agent_card_url"));
    }

    #[test]
    fn test_config_str_extraction() {
        let mut config = HashMap::new();
        config.insert(
            "agent".to_string(),
            serde_json::Value::String("Developer".to_string()),
        );
        config.insert("max_turns".to_string(), serde_json::json!(5));

        assert_eq!(config_str(&config, "agent"), Some("Developer".to_string()));
        assert_eq!(config_str(&config, "max_turns"), Some("5".to_string()));
        assert_eq!(config_str(&config, "missing"), None);
    }

    #[test]
    fn test_node_result_serialization() {
        let result = NodeResult {
            node_id: "node1".to_string(),
            node_kind: "agent".to_string(),
            label: "Test".to_string(),
            status: DispatchStatus::Completed,
            output: "Done".to_string(),
            duration_ms: 1234,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"node_id\":\"node1\""));
        assert!(json.contains("\"status\":\"Completed\""));
    }

    #[test]
    fn test_pipeline_event_serialization() {
        let event = PipelineEvent::NodeStarted {
            run_id: "run-1".to_string(),
            node_id: "node-1".to_string(),
            node_kind: "agent".to_string(),
            label: "Analyze".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"node_started\""));
        assert!(json.contains("\"node_id\":\"node-1\""));
    }

    #[test]
    fn test_pipeline_run_result_serialization() {
        let result = PipelineRunResult {
            run_id: "run-1".to_string(),
            pipeline_id: "pipeline-1".to_string(),
            status: PipelineRunStatus::Completed,
            node_results: vec![],
            total_duration_ms: 5000,
            error: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"completed\""));
        assert!(json.contains("\"total_duration_ms\":5000"));
    }

    #[test]
    fn test_build_dependency_map() {
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

        let map = build_dependency_map(&edges);
        assert_eq!(map.get("b").unwrap(), &vec!["a".to_string()]);
        assert_eq!(map.get("c").unwrap(), &vec!["a".to_string()]);
        let d_deps = map.get("d").unwrap();
        assert_eq!(d_deps.len(), 2);
        assert!(d_deps.contains(&"b".to_string()));
        assert!(d_deps.contains(&"c".to_string()));
        assert!(!map.contains_key("a")); // no deps for root
    }

    // ── Additional edge-case unit tests ──────────────────────────────────

    #[test]
    fn test_pipeline_to_tasks_empty_pipeline() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: empty
nodes: []
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_pipeline_to_tasks_trigger_becomes_task() {
        // Trigger nodes become lightweight entry-point tasks in the DAG
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: trigger-only
nodes:
  - id: trigger1
    kind: trigger
    label: Start
    config:
      event: manual
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].0.task_id, "trigger1");
    }

    #[test]
    fn test_pipeline_to_tasks_condition_node() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: condition-test
nodes:
  - id: check1
    kind: condition
    label: Check Quality
    config:
      expression: "score > 80"
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);
        let (sub_task, a2a_url) = &tasks[0];
        assert_eq!(sub_task.task_id, "check1");
        assert!(sub_task.sub_task_description.contains("score > 80"));
        assert!(a2a_url.is_none());
    }

    #[test]
    fn test_pipeline_to_tasks_transform_node() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: transform-test
nodes:
  - id: t1
    kind: transform
    label: Extract Summary
    config:
      template: "Summarize: {{input}}"
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0]
            .0
            .sub_task_description
            .contains("Summarize: {{input}}"));
    }

    #[test]
    fn test_pipeline_to_tasks_human_node() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: human-test
nodes:
  - id: h1
    kind: human
    label: Review Code
    config:
      prompt: "Please review the changes"
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0]
            .0
            .sub_task_description
            .contains("Please review the changes"));
    }

    #[test]
    fn test_pipeline_to_tasks_chain_dependencies() {
        // A → B → C linear chain
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: chain
nodes:
  - id: a
    kind: agent
    label: Step A
    config:
      agent: developer
      mode: ask
      prompt: First step
  - id: b
    kind: agent
    label: Step B
    config:
      agent: developer
      mode: write
      prompt: Second step
  - id: c
    kind: agent
    label: Step C
    config:
      agent: developer
      mode: review
      prompt: Third step
edges:
  - source: a
    target: b
  - source: b
    target: c
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 3);

        let task_a = tasks.iter().find(|(t, _)| t.task_id == "a").unwrap();
        let task_b = tasks.iter().find(|(t, _)| t.task_id == "b").unwrap();
        let task_c = tasks.iter().find(|(t, _)| t.task_id == "c").unwrap();

        assert!(task_a.0.depends_on.is_empty());
        assert_eq!(task_b.0.depends_on, vec!["a"]);
        assert_eq!(task_c.0.depends_on, vec!["b"]);
    }

    #[test]
    fn test_pipeline_to_tasks_diamond_dependencies() {
        // Diamond: A → B, A → C, B → D, C → D
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: diamond
nodes:
  - id: a
    kind: agent
    label: Root
    config:
      agent: developer
      prompt: start
  - id: b
    kind: agent
    label: Left
    config:
      agent: developer
      prompt: left branch
  - id: c
    kind: agent
    label: Right
    config:
      agent: developer
      prompt: right branch
  - id: d
    kind: agent
    label: Join
    config:
      agent: developer
      prompt: merge results
edges:
  - source: a
    target: b
  - source: a
    target: c
  - source: b
    target: d
  - source: c
    target: d
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 4);

        let task_d = tasks.iter().find(|(t, _)| t.task_id == "d").unwrap();
        assert_eq!(task_d.0.depends_on.len(), 2);
        assert!(task_d.0.depends_on.contains(&"b".to_string()));
        assert!(task_d.0.depends_on.contains(&"c".to_string()));
    }

    #[test]
    fn test_pipeline_to_tasks_agent_reasoning_effort() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: effort-test
nodes:
  - id: a1
    kind: agent
    label: Deep Analysis
    config:
      agent: developer
      mode: debug
      prompt: analyze bug
      reasoning_effort: high
edges: []
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let tasks = pipeline_to_tasks(&pipeline).unwrap();
        assert_eq!(tasks.len(), 1);
        // Prompt is used as the task description
        let desc = &tasks[0].0.sub_task_description;
        assert!(desc.contains("analyze bug"));
        assert_eq!(tasks[0].0.routing.mode_slug, "debug");
    }

    #[test]
    fn test_config_str_various_types() {
        let mut config = HashMap::new();
        config.insert("string_val".to_string(), serde_json::json!("hello"));
        config.insert("number_val".to_string(), serde_json::json!(42));
        config.insert("bool_val".to_string(), serde_json::json!(true));
        config.insert("null_val".to_string(), serde_json::json!(null));

        assert_eq!(config_str(&config, "string_val"), Some("hello".to_string()));
        assert_eq!(config_str(&config, "number_val"), Some("42".to_string()));
        assert_eq!(config_str(&config, "bool_val"), Some("true".to_string()));
        assert_eq!(config_str(&config, "null_val"), Some("null".to_string()));
        assert_eq!(config_str(&config, "missing"), None);
    }

    #[test]
    fn test_pipeline_event_all_variants() {
        // Test all event variants serialize correctly
        let events = vec![
            PipelineEvent::RunStarted {
                run_id: "r1".into(),
                pipeline_id: "p1".into(),
                total_nodes: 5,
            },
            PipelineEvent::NodeStarted {
                run_id: "r1".into(),
                node_id: "n1".into(),
                node_kind: "agent".into(),
                label: "Test".into(),
            },
            PipelineEvent::NodeCompleted {
                run_id: "r1".into(),
                node_id: "n1".into(),
                status: "Completed".into(),
                output: "result".into(),
                duration_ms: 1000,
            },
            PipelineEvent::NodeFailed {
                run_id: "r1".into(),
                node_id: "n1".into(),
                error: "oops".into(),
                duration_ms: 500,
            },
            PipelineEvent::RunCompleted {
                run_id: "r1".into(),
                status: "completed".into(),
                total_duration_ms: 5000,
            },
        ];

        for event in &events {
            let json = serde_json::to_string(event).unwrap();
            assert!(
                json.contains("\"run_id\":\"r1\""),
                "event should have run_id"
            );
            // Round-trip
            let _: serde_json::Value = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_pipeline_run_status_serialization() {
        assert_eq!(
            serde_json::to_string(&PipelineRunStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&PipelineRunStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&PipelineRunStatus::Failed).unwrap(),
            "\"failed\""
        );
        assert_eq!(
            serde_json::to_string(&PipelineRunStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
    }

    #[test]
    fn test_build_dependency_map_empty() {
        let edges: Vec<PipelineEdge> = vec![];
        let map = build_dependency_map(&edges);
        assert!(map.is_empty());
    }

    #[test]
    fn test_build_dependency_map_multiple_sources() {
        // Multiple edges into one target
        let edges = vec![
            PipelineEdge {
                source: "x".into(),
                target: "z".into(),
                label: None,
                condition: None,
            },
            PipelineEdge {
                source: "y".into(),
                target: "z".into(),
                label: None,
                condition: None,
            },
        ];
        let map = build_dependency_map(&edges);
        let z_deps = map.get("z").unwrap();
        assert_eq!(z_deps.len(), 2);
        assert!(z_deps.contains(&"x".to_string()));
        assert!(z_deps.contains(&"y".to_string()));
    }
}
