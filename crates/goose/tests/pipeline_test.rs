use goose::pipeline::Pipeline;
use tempfile::TempDir;

/// Helper to create a minimal valid pipeline
fn make_valid_pipeline() -> Pipeline {
    let yaml = r#"
name: Integration Test Pipeline
description: A test pipeline with trigger and agent
nodes:
  - id: trigger-1
    kind: trigger
    label: Start
  - id: agent-1
    kind: agent
    label: Process
edges:
  - source: trigger-1
    target: agent-1
"#;
    Pipeline::from_yaml(yaml).expect("valid pipeline YAML")
}

/// Helper to create a pipeline with a cycle
fn make_cyclic_pipeline() -> Pipeline {
    let yaml = r#"
name: Cyclic Pipeline
nodes:
  - id: a
    kind: agent
    label: Node A
  - id: b
    kind: agent
    label: Node B
edges:
  - source: a
    target: b
  - source: b
    target: a
"#;
    Pipeline::from_yaml(yaml).expect("valid YAML, invalid DAG")
}

#[test]
fn test_pipeline_yaml_roundtrip() {
    let pipeline = make_valid_pipeline();
    assert_eq!(pipeline.name, "Integration Test Pipeline");
    assert_eq!(pipeline.nodes.len(), 2);
    assert_eq!(pipeline.edges.len(), 1);

    // Roundtrip through YAML
    let yaml = pipeline.to_yaml().expect("serialize to YAML");
    let restored = Pipeline::from_yaml(&yaml).expect("deserialize from YAML");
    assert_eq!(restored.name, pipeline.name);
    assert_eq!(restored.nodes.len(), pipeline.nodes.len());
    assert_eq!(restored.edges.len(), pipeline.edges.len());
}

#[test]
fn test_pipeline_json_roundtrip() {
    let pipeline = make_valid_pipeline();

    let json = pipeline.to_json().expect("serialize to JSON");
    let restored = Pipeline::from_json(&json).expect("deserialize from JSON");
    assert_eq!(restored.name, pipeline.name);
    assert_eq!(restored.nodes.len(), pipeline.nodes.len());
}

#[test]
fn test_pipeline_validation_valid() {
    let pipeline = make_valid_pipeline();
    assert!(
        pipeline.validate().is_ok(),
        "valid pipeline should pass validation"
    );
}

#[test]
fn test_pipeline_validation_cycle_detected() {
    let pipeline = make_cyclic_pipeline();
    let result = pipeline.validate();
    assert!(result.is_err(), "cyclic pipeline should fail validation");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("cycle"),
        "error should mention cycle: {err_msg}"
    );
}

#[test]
fn test_pipeline_without_trigger_is_valid() {
    // Pipelines don't require a trigger node — they can start from any node
    let yaml = r#"
name: No Trigger Pipeline
nodes:
  - id: agent-1
    kind: agent
    label: Agent Only
edges: []
"#;
    let pipeline = Pipeline::from_yaml(yaml).expect("parse YAML");
    assert!(
        pipeline.validate().is_ok(),
        "pipeline without trigger should still be valid"
    );
}

#[test]
fn test_pipeline_validation_empty_name() {
    let yaml = r#"
name: ""
nodes:
  - id: trigger-1
    kind: trigger
    label: Start
edges: []
"#;
    let pipeline = Pipeline::from_yaml(yaml).expect("parse YAML");
    let result = pipeline.validate();
    assert!(result.is_err(), "empty name should fail validation");
}

#[test]
fn test_pipeline_validation_duplicate_node_ids() {
    let yaml = r#"
name: Duplicate IDs
nodes:
  - id: trigger-1
    kind: trigger
    label: Start
  - id: trigger-1
    kind: agent
    label: Duplicate
edges: []
"#;
    let pipeline = Pipeline::from_yaml(yaml).expect("parse YAML");
    let result = pipeline.validate();
    assert!(result.is_err(), "duplicate node IDs should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("duplicate"),
        "error should mention duplicate: {err_msg}"
    );
}

#[test]
fn test_pipeline_validation_edge_references_missing_node() {
    let yaml = r#"
name: Bad Edge
nodes:
  - id: trigger-1
    kind: trigger
    label: Start
edges:
  - source: trigger-1
    target: nonexistent
"#;
    let pipeline = Pipeline::from_yaml(yaml).expect("parse YAML");
    let result = pipeline.validate();
    assert!(result.is_err(), "edge to nonexistent node should fail");
}

#[test]
fn test_pipeline_file_crud() {
    let temp_dir = TempDir::new().expect("create temp dir");
    let dir = temp_dir.path().to_path_buf();

    let pipeline = make_valid_pipeline();
    let id = "test-crud-pipeline";

    // Save
    let file_path = dir.join(format!("{id}.yaml"));
    let yaml = pipeline.to_yaml().expect("to_yaml");
    std::fs::write(&file_path, &yaml).expect("write file");

    // Read back
    let content = std::fs::read_to_string(&file_path).expect("read file");
    let loaded = Pipeline::from_yaml(&content).expect("from_yaml");
    assert_eq!(loaded.name, pipeline.name);

    // Delete
    std::fs::remove_file(&file_path).expect("delete");
    assert!(!file_path.exists());
}

#[test]
fn test_pipeline_generate_id() {
    use goose::pipeline::generate_pipeline_id;

    // Normal name → slug (no UUID suffix)
    let id = generate_pipeline_id("My Test Pipeline");
    assert_eq!(id, "my-test-pipeline", "should slugify: {id}");

    // Empty name → "pipeline-<timestamp>"
    let id = generate_pipeline_id("");
    assert!(
        id.starts_with("pipeline-"),
        "empty name should use 'pipeline' prefix: {id}"
    );

    // Special characters stripped and collapsed
    let id = generate_pipeline_id("Hello! @World# 123");
    assert_eq!(
        id, "hello-world-123",
        "special chars should be stripped: {id}"
    );

    // Leading/trailing special chars trimmed
    let id = generate_pipeline_id("  --test--  ");
    assert_eq!(id, "test", "leading/trailing dashes trimmed: {id}");
}

#[test]
fn test_pipeline_complex_dag() {
    let yaml = r#"
name: Complex DAG
description: Diamond-shaped pipeline
nodes:
  - id: trigger
    kind: trigger
    label: Start
  - id: branch-a
    kind: agent
    label: Branch A
  - id: branch-b
    kind: agent
    label: Branch B
  - id: merge
    kind: agent
    label: Merge
edges:
  - source: trigger
    target: branch-a
  - source: trigger
    target: branch-b
  - source: branch-a
    target: merge
  - source: branch-b
    target: merge
"#;
    let pipeline = Pipeline::from_yaml(yaml).expect("parse YAML");
    assert!(pipeline.validate().is_ok(), "diamond DAG should be valid");
    assert_eq!(pipeline.nodes.len(), 4);
    assert_eq!(pipeline.edges.len(), 4);
}

#[test]
fn test_pipeline_all_node_kinds() {
    let yaml = r#"
name: All Node Kinds
nodes:
  - id: t1
    kind: trigger
    label: Trigger
  - id: a1
    kind: agent
    label: Agent
  - id: tool1
    kind: tool
    label: Tool
  - id: c1
    kind: condition
    label: Condition
  - id: tr1
    kind: transform
    label: Transform
  - id: h1
    kind: human
    label: Human Review
  - id: a2a1
    kind: a2a
    label: A2A Call
edges:
  - source: t1
    target: a1
  - source: a1
    target: tool1
  - source: tool1
    target: c1
  - source: c1
    target: tr1
  - source: tr1
    target: h1
  - source: h1
    target: a2a1
"#;
    let pipeline = Pipeline::from_yaml(yaml).expect("parse YAML");
    assert!(pipeline.validate().is_ok());
    assert_eq!(pipeline.nodes.len(), 7);
}
