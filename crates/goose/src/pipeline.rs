use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

use crate::config::paths::Paths;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    #[serde(default = "default_api_version")]
    pub api_version: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub nodes: Vec<PipelineNode>,
    #[serde(default)]
    pub edges: Vec<PipelineEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<PipelineLayout>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

fn default_api_version() -> String {
    "goose/v1".to_string()
}

fn default_kind() -> String {
    "Pipeline".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineNode {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<NodePosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Trigger,
    Agent,
    Tool,
    Condition,
    Transform,
    Human,
    A2a,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineEdge {
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct Viewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PipelineManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub node_count: usize,
    pub edge_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Parsing & serialization
// ---------------------------------------------------------------------------

impl Pipeline {
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let pipeline: Self = serde_yaml::from_str(yaml)?;
        Ok(pipeline)
    }

    pub fn to_yaml(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(self)?;
        Ok(yaml)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let pipeline: Self = serde_json::from_str(json)?;
        Ok(pipeline)
    }

    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string_pretty(self)?;
        Ok(json)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        match path.extension().and_then(|e| e.to_str()) {
            Some("json") => Self::from_json(&content),
            _ => Self::from_yaml(&content),
        }
    }
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

impl Pipeline {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("pipeline name is required");
        }
        if self.nodes.is_empty() {
            bail!("pipeline must have at least one node");
        }

        let node_ids: HashSet<&str> = self.nodes.iter().map(|n| n.id.as_str()).collect();

        // Check for duplicate node ids
        if node_ids.len() != self.nodes.len() {
            bail!("duplicate node ids detected");
        }

        // Validate edges reference existing nodes
        for edge in &self.edges {
            if !node_ids.contains(edge.source.as_str()) {
                bail!("edge source '{}' not found in nodes", edge.source);
            }
            if !node_ids.contains(edge.target.as_str()) {
                bail!("edge target '{}' not found in nodes", edge.target);
            }
        }

        // Cycle detection
        if self.has_cycle() {
            bail!("pipeline contains a cycle");
        }

        Ok(())
    }

    fn has_cycle(&self) -> bool {
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
        for node in &self.nodes {
            adjacency.entry(node.id.as_str()).or_default();
        }
        for edge in &self.edges {
            adjacency
                .entry(edge.source.as_str())
                .or_default()
                .push(edge.target.as_str());
        }

        let mut visited: HashSet<&str> = HashSet::new();
        let mut in_stack: HashSet<&str> = HashSet::new();

        for node in adjacency.keys() {
            if !visited.contains(node)
                && Self::dfs_cycle(node, &adjacency, &mut visited, &mut in_stack)
            {
                return true;
            }
        }
        false
    }

    fn dfs_cycle<'a>(
        node: &'a str,
        adjacency: &HashMap<&'a str, Vec<&'a str>>,
        visited: &mut HashSet<&'a str>,
        in_stack: &mut HashSet<&'a str>,
    ) -> bool {
        visited.insert(node);
        in_stack.insert(node);

        if let Some(neighbors) = adjacency.get(node) {
            for neighbor in neighbors {
                if !visited.contains(neighbor) {
                    if Self::dfs_cycle(neighbor, adjacency, visited, in_stack) {
                        return true;
                    }
                } else if in_stack.contains(neighbor) {
                    return true;
                }
            }
        }

        in_stack.remove(node);
        false
    }
}

// ---------------------------------------------------------------------------
// File-based CRUD
// ---------------------------------------------------------------------------

fn pipelines_dir() -> PathBuf {
    Paths::in_data_dir("pipelines")
}

fn pipeline_path(id: &str) -> PathBuf {
    pipelines_dir().join(format!("{id}.yaml"))
}

pub fn generate_pipeline_id(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    // Collapse consecutive dashes
    let mut result = String::new();
    let mut prev_dash = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_dash {
                result.push(c);
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    if result.is_empty() {
        format!("pipeline-{}", Utc::now().timestamp_millis())
    } else {
        result
    }
}

pub fn list_pipelines() -> Result<Vec<PipelineManifest>> {
    let dir = pipelines_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut manifests = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        match Pipeline::from_file(&path) {
            Ok(pipeline) => {
                manifests.push(PipelineManifest {
                    id,
                    name: pipeline.name,
                    description: pipeline.description,
                    version: pipeline.version,
                    tags: pipeline.tags,
                    node_count: pipeline.nodes.len(),
                    edge_count: pipeline.edges.len(),
                    created_at: pipeline.created_at,
                    updated_at: pipeline.updated_at,
                });
            }
            Err(e) => {
                tracing::warn!("skipping invalid pipeline file {}: {e}", path.display());
            }
        }
    }

    manifests.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(manifests)
}

pub fn get_pipeline(id: &str) -> Result<Pipeline> {
    let path = pipeline_path(id);
    if !path.exists() {
        bail!("pipeline '{id}' not found");
    }
    Pipeline::from_file(&path)
}

pub fn save_pipeline(id: &str, pipeline: &mut Pipeline) -> Result<String> {
    let dir = pipelines_dir();
    fs::create_dir_all(&dir)?;

    let now = Utc::now();
    if pipeline.created_at.is_none() {
        // Preserve created_at if updating an existing pipeline
        if let Ok(existing) = get_pipeline(id) {
            pipeline.created_at = existing.created_at;
        } else {
            pipeline.created_at = Some(now);
        }
    }
    pipeline.updated_at = Some(now);

    let yaml = pipeline.to_yaml()?;
    let path = pipeline_path(id);
    fs::write(&path, yaml)?;
    Ok(id.to_string())
}

pub fn delete_pipeline(id: &str) -> Result<()> {
    let path = pipeline_path(id);
    if !path.exists() {
        bail!("pipeline '{id}' not found");
    }
    fs::remove_file(path)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    fn sample_yaml() -> &'static str {
        r#"
apiVersion: goose/v1
kind: Pipeline
name: Test Pipeline
description: A test pipeline
nodes:
  - id: trigger
    kind: trigger
    label: Start
  - id: agent1
    kind: agent
    label: Developer
    config:
      prompt: "Write code"
  - id: agent2
    kind: agent
    label: Reviewer
edges:
  - source: trigger
    target: agent1
  - source: agent1
    target: agent2
"#
    }

    #[test]
    fn yaml_roundtrip() {
        let pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        assert_eq!(pipeline.name, "Test Pipeline");
        assert_eq!(pipeline.nodes.len(), 3);
        assert_eq!(pipeline.edges.len(), 2);

        let yaml = pipeline.to_yaml().unwrap();
        let pipeline2 = Pipeline::from_yaml(&yaml).unwrap();
        assert_eq!(pipeline, pipeline2);
    }

    #[test]
    fn json_roundtrip() {
        let pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        let json = pipeline.to_json().unwrap();
        let pipeline2 = Pipeline::from_json(&json).unwrap();
        assert_eq!(pipeline, pipeline2);
    }

    #[test]
    fn valid_pipeline_passes() {
        let pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        assert!(pipeline.validate().is_ok());
    }

    #[test]
    fn empty_name_fails() {
        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        pipeline.name = "  ".to_string();
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("name is required"));
    }

    #[test]
    fn no_nodes_fails() {
        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        pipeline.nodes.clear();
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("at least one node"));
    }

    #[test]
    fn duplicate_node_ids_fails() {
        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        pipeline.nodes[1].id = "trigger".to_string(); // duplicate
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("duplicate node ids"));
    }

    #[test]
    fn dangling_edge_source_fails() {
        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        pipeline.edges.push(PipelineEdge {
            source: "nonexistent".to_string(),
            target: "agent1".to_string(),
            label: None,
            condition: None,
        });
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("not found in nodes"));
    }

    #[test]
    fn dangling_edge_target_fails() {
        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        pipeline.edges.push(PipelineEdge {
            source: "agent1".to_string(),
            target: "nonexistent".to_string(),
            label: None,
            condition: None,
        });
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("not found in nodes"));
    }

    #[test]
    fn cycle_detected() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: Cycle
nodes:
  - id: a
    kind: agent
    label: A
  - id: b
    kind: agent
    label: B
  - id: c
    kind: agent
    label: C
edges:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("cycle"));
    }

    #[test]
    fn diamond_dag_is_valid() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: Diamond
nodes:
  - id: start
    kind: trigger
    label: Start
  - id: left
    kind: agent
    label: Left
  - id: right
    kind: agent
    label: Right
  - id: join
    kind: agent
    label: Join
edges:
  - source: start
    target: left
  - source: start
    target: right
  - source: left
    target: join
  - source: right
    target: join
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        assert!(pipeline.validate().is_ok());
    }

    #[test]
    fn generate_id_from_name() {
        assert_eq!(generate_pipeline_id("My Cool Pipeline"), "my-cool-pipeline");
        assert_eq!(generate_pipeline_id("hello world!"), "hello-world");
        let id = generate_pipeline_id("  Test--Pipeline  ");
        assert!(!id.starts_with('-'));
        assert!(!id.contains("--"));
    }

    #[test]
    fn generate_id_empty_name() {
        let id = generate_pipeline_id("");
        assert!(id.starts_with("pipeline-"));
    }

    #[test]
    #[serial]
    fn file_crud_operations() {
        // Use temp dir via GOOSE_PATH_ROOT
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", tmp.path());

        // List empty
        let list = list_pipelines().unwrap();
        assert!(list.is_empty());

        // Save
        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        let id = save_pipeline("test-pipeline", &mut pipeline).unwrap();
        assert_eq!(id, "test-pipeline");

        // List
        let list = list_pipelines().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test Pipeline");
        assert_eq!(list[0].node_count, 3);

        // Get
        let loaded = get_pipeline("test-pipeline").unwrap();
        assert_eq!(loaded.name, "Test Pipeline");
        assert!(loaded.created_at.is_some());
        assert!(loaded.updated_at.is_some());

        // Delete
        delete_pipeline("test-pipeline").unwrap();
        let list = list_pipelines().unwrap();
        assert!(list.is_empty());

        // Get nonexistent
        assert!(get_pipeline("test-pipeline").is_err());

        // Delete nonexistent
        assert!(delete_pipeline("test-pipeline").is_err());

        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    #[test]
    #[serial]
    fn save_preserves_created_at_on_update() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("GOOSE_PATH_ROOT", tmp.path());

        let mut pipeline = Pipeline::from_yaml(sample_yaml()).unwrap();
        save_pipeline("preserve-test", &mut pipeline).unwrap();

        let first = get_pipeline("preserve-test").unwrap();
        let created = first.created_at.unwrap();

        // Update
        let mut pipeline2 = Pipeline::from_yaml(sample_yaml()).unwrap();
        pipeline2.name = "Updated Name".to_string();
        save_pipeline("preserve-test", &mut pipeline2).unwrap();

        let second = get_pipeline("preserve-test").unwrap();
        assert_eq!(second.created_at.unwrap(), created);
        assert_eq!(second.name, "Updated Name");

        std::env::remove_var("GOOSE_PATH_ROOT");
    }

    #[test]
    fn node_kinds_roundtrip() {
        let kinds = vec![
            NodeKind::Trigger,
            NodeKind::Agent,
            NodeKind::Tool,
            NodeKind::Condition,
            NodeKind::Transform,
            NodeKind::Human,
            NodeKind::A2a,
        ];
        for kind in kinds {
            let json = serde_json::to_string(&kind).unwrap();
            let back: NodeKind = serde_json::from_str(&json).unwrap();
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn serde_defaults_applied() {
        // Minimal YAML without apiVersion/kind — defaults should fill in
        let yaml = r#"
name: Minimal
nodes:
  - id: a
    kind: agent
    label: A
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        assert_eq!(pipeline.api_version, "goose/v1");
        assert_eq!(pipeline.kind, "Pipeline");
        assert!(pipeline.description.is_empty());
        assert!(pipeline.version.is_empty());
        assert!(pipeline.tags.is_empty());
        assert!(pipeline.edges.is_empty());
    }

    #[test]
    fn version_and_tags_roundtrip() {
        let yaml = r#"
apiVersion: goose/v1
kind: Pipeline
name: Tagged Pipeline
version: "1.2.0"
tags:
  - ci
  - production
nodes:
  - id: a
    kind: agent
    label: A
"#;
        let pipeline = Pipeline::from_yaml(yaml).unwrap();
        assert_eq!(pipeline.version, "1.2.0");
        assert_eq!(pipeline.tags, vec!["ci", "production"]);

        let yaml2 = pipeline.to_yaml().unwrap();
        let pipeline2 = Pipeline::from_yaml(&yaml2).unwrap();
        assert_eq!(pipeline, pipeline2);
    }
}
