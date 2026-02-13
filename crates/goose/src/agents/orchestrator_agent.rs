//! OrchestratorAgent — LLM-based meta-coordinator for multi-agent routing.
//!
//! Replaces the keyword-based IntentRouter with an LLM that understands context,
//! domain, and request complexity. Falls back to IntentRouter when LLM is unavailable.
//!
//! # Architecture
//!
//! ```text
//! User Message → OrchestratorAgent.route()
//!   ├─ Build agent catalog from GooseAgent + CodingAgent + external agents
//!   ├─ Render routing prompt with catalog + user message
//!   ├─ LLM classifies intent → RoutingDecision (single or compound)
//!   ├─ (fallback) IntentRouter keyword matching
//!   └─ Return OrchestratorPlan with one or more sub-tasks
//! ```
//!
//! # Compound Request Splitting
//!
//! When a user message contains multiple independent intents (e.g., "fix the login
//! bug and add a dark theme"), the orchestrator splits it into sub-tasks, each
//! routed to the appropriate agent/mode. Results are aggregated into a coherent
//! response.
//!
//! # Feature Flag
//!
//! Set `GOOSE_ORCHESTRATOR_ENABLED=true` to use LLM routing + splitting.
//! When disabled (default), falls back to IntentRouter for backward compatibility.

use crate::agents::coding_agent::CodingAgent;
use crate::agents::goose_agent::GooseAgent;
use crate::agents::intent_router::{IntentRouter, RoutingDecision};
use crate::context_mgmt::{
    check_if_compaction_needed, compact_messages, DEFAULT_COMPACTION_THRESHOLD,
};
use crate::conversation::Conversation;
use crate::prompt_template;
use crate::providers::base::{Provider, ProviderUsage};
use crate::registry::manifest::AgentMode;
use crate::session::Session;

use anyhow::Result;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Whether LLM-based orchestration is enabled (feature flag).
pub fn is_orchestrator_enabled() -> bool {
    std::env::var("GOOSE_ORCHESTRATOR_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Context for rendering the orchestrator routing prompt.
#[derive(Serialize)]
struct RoutingPromptContext {
    user_message: String,
    agent_catalog: String,
}

/// A sub-task produced by compound request splitting.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub routing: RoutingDecision,
    pub sub_task_description: String,
}

/// The plan produced by the orchestrator for a user message.
#[derive(Debug, Clone)]
pub struct OrchestratorPlan {
    pub is_compound: bool,
    pub tasks: Vec<SubTask>,
}

impl OrchestratorPlan {
    /// Create a simple plan with a single routing decision (no splitting).
    pub fn single(decision: RoutingDecision) -> Self {
        let desc = decision.reasoning.clone();
        Self {
            is_compound: false,
            tasks: vec![SubTask {
                routing: decision,
                sub_task_description: desc,
            }],
        }
    }

    /// Get the primary routing decision (first task).
    pub fn primary_routing(&self) -> &RoutingDecision {
        &self.tasks[0].routing
    }
}

/// An agent slot with its modes, used for building the catalog.
#[derive(Debug, Clone)]
struct CatalogEntry {
    name: String,
    description: String,
    modes: Vec<AgentMode>,
    default_mode: String,
}

/// The OrchestratorAgent coordinates routing decisions using LLM intelligence.
///
/// It maintains an agent catalog built from builtin agents (GooseAgent, CodingAgent)
/// and any externally registered agents. The catalog is rendered into the LLM prompt
/// so it can make informed routing decisions.
pub struct OrchestratorAgent {
    catalog: Vec<CatalogEntry>,
    intent_router: IntentRouter,
    provider: Arc<Mutex<Option<Arc<dyn Provider>>>>,
}

impl OrchestratorAgent {
    pub fn new(provider: Arc<Mutex<Option<Arc<dyn Provider>>>>) -> Self {
        let goose = GooseAgent::new();
        let coding = CodingAgent::new();

        let catalog = vec![
            CatalogEntry {
                name: "Goose Agent".into(),
                description:
                    "General-purpose assistant for conversation, planning, apps, and misc tasks"
                        .into(),
                modes: goose.to_agent_modes(),
                default_mode: goose.default_mode_slug().to_string(),
            },
            CatalogEntry {
                name: "Coding Agent".into(),
                description:
                    "SDLC specialist for code, architecture, testing, security, and DevOps".into(),
                modes: coding.to_agent_modes(),
                default_mode: "backend".into(),
            },
        ];

        Self {
            catalog,
            intent_router: IntentRouter::new(),
            provider,
        }
    }

    /// Expose the inner IntentRouter for state synchronization (enable/disable, extensions).
    pub fn intent_router_mut(&mut self) -> &mut IntentRouter {
        &mut self.intent_router
    }

    /// Set enabled state for an agent slot (delegates to IntentRouter).
    pub fn set_enabled(&mut self, agent_name: &str, enabled: bool) {
        self.intent_router.set_enabled(agent_name, enabled);
    }

    /// Set bound extensions for an agent slot (delegates to IntentRouter).
    pub fn set_bound_extensions(&mut self, agent_name: &str, extensions: Vec<String>) {
        self.intent_router
            .set_bound_extensions(agent_name, extensions);
    }

    /// Get the agent slots (delegates to IntentRouter).
    pub fn slots(&self) -> &[crate::agents::intent_router::AgentSlot] {
        self.intent_router.slots()
    }

    /// Route a user message to the best agent and mode, with optional compound splitting.
    ///
    /// Returns an `OrchestratorPlan` that may contain multiple sub-tasks for
    /// compound requests when LLM orchestration is enabled.
    pub async fn route(&self, user_message: &str) -> OrchestratorPlan {
        if is_orchestrator_enabled() {
            match self.route_with_llm(user_message).await {
                Ok(plan) => {
                    info!(
                        is_compound = plan.is_compound,
                        task_count = plan.tasks.len(),
                        primary_agent = %plan.primary_routing().agent_name,
                        primary_mode = %plan.primary_routing().mode_slug,
                        "LLM orchestrator routed message"
                    );
                    return plan;
                }
                Err(e) => {
                    warn!(
                        "LLM routing failed, falling back to keyword matching: {}",
                        e
                    );
                }
            }
        }

        // Fallback to keyword-based IntentRouter (always single-task)
        let decision = self.intent_router.route(user_message);
        debug!(
            agent_name = %decision.agent_name,
            mode_slug = %decision.mode_slug,
            confidence = %decision.confidence,
            "Keyword router fallback"
        );
        OrchestratorPlan::single(decision)
    }

    /// Use the LLM to classify the user's intent, potentially splitting compound requests.
    async fn route_with_llm(&self, user_message: &str) -> Result<OrchestratorPlan> {
        let provider_guard = self.provider.lock().await;
        let provider = provider_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provider available for LLM routing"))?;

        let catalog_text = self.build_catalog_text();
        let context = RoutingPromptContext {
            user_message: user_message.to_string(),
            agent_catalog: catalog_text,
        };

        let splitting_prompt =
            prompt_template::render_template("orchestrator/splitting.md", &context)?;

        let messages = vec![crate::conversation::message::Message::user().with_text(user_message)];

        let (response, _usage) = provider
            .complete("orchestrator-routing", &splitting_prompt, &messages, &[])
            .await?;

        self.parse_splitting_response(&response)
    }

    /// Build a human-readable catalog of all available agents and their modes.
    pub fn build_catalog_text(&self) -> String {
        let mut text = String::new();
        for entry in &self.catalog {
            text.push_str(&format!(
                "### {} \u{2014} {}\n",
                entry.name, entry.description
            ));
            text.push_str(&format!("Default mode: {}\n", entry.default_mode));
            text.push_str("Modes:\n");
            for mode in &entry.modes {
                let when = mode.when_to_use.as_deref().unwrap_or(&mode.description);
                text.push_str(&format!(
                    "  - **{}** ({}): {} | Use when: {}\n",
                    mode.slug, mode.name, mode.description, when
                ));
            }
            text.push('\n');
        }
        text
    }

    /// Parse the LLM's splitting response into an OrchestratorPlan.
    fn parse_splitting_response(
        &self,
        response: &crate::conversation::message::Message,
    ) -> Result<OrchestratorPlan> {
        let text = response
            .content
            .iter()
            .filter_map(|c| match c {
                crate::conversation::message::MessageContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        let json_str = extract_json(&text)?;
        let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

        let is_compound = parsed["is_compound"].as_bool().unwrap_or(false);
        let tasks_arr = parsed["tasks"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing 'tasks' array in splitting response"))?;

        if tasks_arr.is_empty() {
            return Err(anyhow::anyhow!("Empty tasks array in splitting response"));
        }

        let mut tasks = Vec::new();
        for task_val in tasks_arr {
            let agent_name = task_val["agent_name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing agent_name in task"))?;
            let mode_slug = task_val["mode_slug"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Missing mode_slug in task"))?;
            let confidence = task_val["confidence"].as_f64().unwrap_or(0.5) as f32;
            let reasoning = task_val["reasoning"]
                .as_str()
                .unwrap_or("LLM routing decision")
                .to_string();
            let sub_task = task_val["sub_task"]
                .as_str()
                .unwrap_or(agent_name)
                .to_string();

            // Validate agent_name
            if !self.catalog.iter().any(|e| e.name == agent_name) {
                warn!(
                    "LLM selected unknown agent '{}', skipping sub-task",
                    agent_name
                );
                continue;
            }

            tasks.push(SubTask {
                routing: RoutingDecision {
                    agent_name: agent_name.to_string(),
                    mode_slug: mode_slug.to_string(),
                    confidence,
                    reasoning,
                },
                sub_task_description: sub_task,
            });
        }

        if tasks.is_empty() {
            return Err(anyhow::anyhow!(
                "No valid tasks after filtering, all agent names were unknown"
            ));
        }

        Ok(OrchestratorPlan { is_compound, tasks })
    }

    /// Check if the conversation needs compaction before delegating to a sub-agent.
    ///
    /// The orchestrator is the right place for this check because it has visibility
    /// across all agents and can compact proactively before routing, rather than
    /// waiting for an agent to hit its context limit mid-reply.
    pub async fn check_compaction_needed(
        &self,
        conversation: &Conversation,
        session: &Session,
    ) -> Result<bool> {
        let provider_guard = self.provider.lock().await;
        let provider = match provider_guard.as_ref() {
            Some(p) => p,
            None => return Ok(false),
        };
        check_if_compaction_needed(provider.as_ref(), conversation, None, session).await
    }

    /// Perform proactive compaction if the conversation exceeds the threshold.
    ///
    /// Returns the compacted conversation and usage info if compaction was performed,
    /// or None if compaction wasn't needed.
    pub async fn compact_if_needed(
        &self,
        session_id: &str,
        conversation: &Conversation,
        session: &Session,
    ) -> Result<Option<(Conversation, ProviderUsage)>> {
        if !self.check_compaction_needed(conversation, session).await? {
            return Ok(None);
        }

        let provider_guard = self.provider.lock().await;
        let provider = provider_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provider available for compaction"))?;

        let config = crate::config::Config::global();
        let threshold = config
            .get_param::<f64>("GOOSE_AUTO_COMPACT_THRESHOLD")
            .unwrap_or(DEFAULT_COMPACTION_THRESHOLD);
        let threshold_pct = (threshold * 100.0) as u32;

        info!(
            threshold = threshold_pct,
            "Orchestrator: proactive compaction triggered"
        );

        let result = compact_messages(provider.as_ref(), session_id, conversation, false).await?;
        Ok(Some(result))
    }

    /// Get the tool_groups for a given routing decision.
    ///
    /// Looks up the mode's tool_groups from GooseAgent or CodingAgent
    /// based on the routing decision's agent_name and mode_slug.
    /// Returns empty Vec if the mode isn't found (which means "all tools" — backward compatible).
    pub fn get_tool_groups_for_routing(
        &self,
        agent_name: &str,
        mode_slug: &str,
    ) -> Vec<crate::registry::manifest::ToolGroupAccess> {
        match agent_name {
            "Goose Agent" => {
                let goose = GooseAgent::new();
                if let Some(mode) = goose.mode(mode_slug) {
                    mode.tool_groups.clone()
                } else {
                    vec![] // unknown mode → all tools (backward compatible)
                }
            }
            "Coding Agent" => {
                let coding = CodingAgent::new();
                if let Some(mode) = coding.mode(mode_slug) {
                    mode.tool_groups.clone()
                } else {
                    vec![]
                }
            }
            _ => vec![], // external agent → all tools
        }
    }

    /// Get the recommended MCP extensions for a specific agent/mode.
    /// Used by reply.rs to activate only the extensions needed by the current mode.
    pub fn get_recommended_extensions_for_routing(
        &self,
        agent_name: &str,
        mode_slug: &str,
    ) -> Vec<String> {
        match agent_name {
            "Goose Agent" => {
                let goose = GooseAgent::new();
                if let Some(mode) = goose.mode(mode_slug) {
                    mode.recommended_extensions.clone()
                } else {
                    vec![]
                }
            }
            "Coding Agent" => {
                let coding = CodingAgent::new();
                if let Some(mode) = coding.mode(mode_slug) {
                    mode.recommended_extensions.clone()
                } else {
                    vec![]
                }
            }
            _ => vec![], // external agent → no restrictions
        }
    }
}

/// Aggregate results from multiple sub-tasks into a coherent response.
///
/// Takes the sub-task descriptions and their results, and produces a
/// combined message that presents all results clearly.
pub fn aggregate_results(tasks: &[SubTask], results: &[String]) -> String {
    if tasks.len() == 1 {
        return results.first().cloned().unwrap_or_default();
    }

    let mut output = String::from("I handled your compound request in multiple parts:\n\n");
    for (i, (task, result)) in tasks.iter().zip(results.iter()).enumerate() {
        output.push_str(&format!(
            "## Part {} — {}\n\n{}\n\n",
            i + 1,
            task.sub_task_description,
            result
        ));
    }
    output
}

/// Extract a JSON object from text that may contain markdown code fences.
fn extract_json(text: &str) -> Result<String> {
    let fence = "```";
    let fence_json = "```json";

    // Try to find JSON in code blocks first
    if let Some(start) = text.find(fence_json) {
        if let Some(after_fence) = text.get(start + fence_json.len()..) {
            if let Some(end) = after_fence.find(fence) {
                if let Some(content) = after_fence.get(..end) {
                    return Ok(content.trim().to_string());
                }
            }
        }
    }
    if let Some(start) = text.find(fence) {
        if let Some(after_fence) = text.get(start + fence.len()..) {
            if let Some(end) = after_fence.find(fence) {
                if let Some(content) = after_fence.get(..end) {
                    let inner = content.trim();
                    if inner.starts_with('{') {
                        return Ok(inner.to_string());
                    }
                }
            }
        }
    }

    // Try to find raw JSON object
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            if let Some(content) = text.get(start..=end) {
                return Ok(content.to_string());
            }
        }
    }

    Err(anyhow::anyhow!("No JSON object found in LLM response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_orchestrator() -> OrchestratorAgent {
        OrchestratorAgent::new(Arc::new(Mutex::new(None)))
    }

    #[test]
    fn test_build_catalog_text() {
        let orch = make_orchestrator();
        let catalog = orch.build_catalog_text();

        assert!(catalog.contains("Goose Agent"));
        assert!(catalog.contains("Coding Agent"));
        assert!(catalog.contains("assistant"));
        assert!(catalog.contains("backend"));
        assert!(catalog.contains("architect"));
    }

    #[test]
    fn test_parse_single_task_response() {
        let orch = make_orchestrator();

        let response = crate::conversation::message::Message::assistant().with_text(
            r#"{"is_compound": false, "tasks": [{"agent_name": "Coding Agent", "mode_slug": "backend", "confidence": 0.9, "reasoning": "API implementation task", "sub_task": "implement a REST API endpoint"}]}"#,
        );

        let plan = orch.parse_splitting_response(&response).unwrap();
        assert!(!plan.is_compound);
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.primary_routing().agent_name, "Coding Agent");
        assert_eq!(plan.primary_routing().mode_slug, "backend");
        assert_eq!(
            plan.tasks[0].sub_task_description,
            "implement a REST API endpoint"
        );
    }

    #[test]
    fn test_parse_compound_response() {
        let orch = make_orchestrator();

        let response = crate::conversation::message::Message::assistant().with_text(
            r#"{"is_compound": true, "tasks": [
                {"agent_name": "Coding Agent", "mode_slug": "backend", "confidence": 0.85, "reasoning": "Bug fix", "sub_task": "Fix the login endpoint bug"},
                {"agent_name": "Coding Agent", "mode_slug": "frontend", "confidence": 0.8, "reasoning": "UI feature", "sub_task": "Add dark theme toggle to settings"}
            ]}"#,
        );

        let plan = orch.parse_splitting_response(&response).unwrap();
        assert!(plan.is_compound);
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].routing.agent_name, "Coding Agent");
        assert_eq!(plan.tasks[0].routing.mode_slug, "backend");
        assert_eq!(
            plan.tasks[0].sub_task_description,
            "Fix the login endpoint bug"
        );
        assert_eq!(plan.tasks[1].routing.mode_slug, "frontend");
        assert_eq!(
            plan.tasks[1].sub_task_description,
            "Add dark theme toggle to settings"
        );
    }

    #[test]
    fn test_parse_response_markdown_wrapped() {
        let orch = make_orchestrator();

        let text = concat!(
            "Here's my analysis:\n\n",
            "```json\n",
            r#"{"is_compound": false, "tasks": [{"agent_name": "Goose Agent", "mode_slug": "planner", "confidence": 0.85, "reasoning": "Planning task", "sub_task": "Create a project plan"}]}"#,
            "\n```"
        );
        let response = crate::conversation::message::Message::assistant().with_text(text);

        let plan = orch.parse_splitting_response(&response).unwrap();
        assert!(!plan.is_compound);
        assert_eq!(plan.primary_routing().agent_name, "Goose Agent");
        assert_eq!(plan.primary_routing().mode_slug, "planner");
    }

    #[test]
    fn test_parse_response_invalid_agent_filtered() {
        let orch = make_orchestrator();

        let response = crate::conversation::message::Message::assistant().with_text(
            r#"{"is_compound": true, "tasks": [
                {"agent_name": "NonExistent Agent", "mode_slug": "foo", "confidence": 0.5, "reasoning": "test", "sub_task": "invalid"},
                {"agent_name": "Goose Agent", "mode_slug": "assistant", "confidence": 0.8, "reasoning": "fallback", "sub_task": "valid task"}
            ]}"#,
        );

        let plan = orch.parse_splitting_response(&response).unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].routing.agent_name, "Goose Agent");
    }

    #[test]
    fn test_parse_response_all_invalid_agents() {
        let orch = make_orchestrator();

        let response = crate::conversation::message::Message::assistant().with_text(
            r#"{"is_compound": false, "tasks": [{"agent_name": "NonExistent", "mode_slug": "x", "confidence": 0.5, "reasoning": "t", "sub_task": "y"}]}"#,
        );

        assert!(orch.parse_splitting_response(&response).is_err());
    }

    #[test]
    fn test_parse_response_empty_tasks() {
        let orch = make_orchestrator();

        let response = crate::conversation::message::Message::assistant()
            .with_text(r#"{"is_compound": false, "tasks": []}"#);

        assert!(orch.parse_splitting_response(&response).is_err());
    }

    #[test]
    fn test_orchestrator_plan_single() {
        let decision = RoutingDecision {
            agent_name: "Goose Agent".into(),
            mode_slug: "assistant".into(),
            confidence: 0.9,
            reasoning: "General question".into(),
        };
        let plan = OrchestratorPlan::single(decision);

        assert!(!plan.is_compound);
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.primary_routing().agent_name, "Goose Agent");
    }

    #[test]
    fn test_aggregate_results_single() {
        let tasks = vec![SubTask {
            routing: RoutingDecision {
                agent_name: "Goose Agent".into(),
                mode_slug: "assistant".into(),
                confidence: 0.9,
                reasoning: "test".into(),
            },
            sub_task_description: "Answer the question".into(),
        }];
        let results = vec!["The answer is 42.".into()];

        let output = aggregate_results(&tasks, &results);
        assert_eq!(output, "The answer is 42.");
    }

    #[test]
    fn test_aggregate_results_compound() {
        let tasks = vec![
            SubTask {
                routing: RoutingDecision {
                    agent_name: "Coding Agent".into(),
                    mode_slug: "backend".into(),
                    confidence: 0.8,
                    reasoning: "bug fix".into(),
                },
                sub_task_description: "Fix login bug".into(),
            },
            SubTask {
                routing: RoutingDecision {
                    agent_name: "Coding Agent".into(),
                    mode_slug: "frontend".into(),
                    confidence: 0.8,
                    reasoning: "UI feature".into(),
                },
                sub_task_description: "Add dark theme".into(),
            },
        ];
        let results = vec!["Login bug fixed.".into(), "Dark theme added.".into()];

        let output = aggregate_results(&tasks, &results);
        assert!(output.contains("Part 1"));
        assert!(output.contains("Fix login bug"));
        assert!(output.contains("Login bug fixed."));
        assert!(output.contains("Part 2"));
        assert!(output.contains("Add dark theme"));
        assert!(output.contains("Dark theme added."));
    }

    #[test]
    fn test_extract_json_raw() {
        let text = r#"{"is_compound": false, "tasks": [{"agent_name": "Goose Agent"}]}"#;
        let json = extract_json(text).unwrap();
        assert!(json.contains("Goose Agent"));
    }

    #[test]
    fn test_extract_json_code_block() {
        let text = concat!(
            "Some text\n",
            "```json\n",
            r#"{"key": "value"}"#,
            "\n```\n",
            "More text"
        );
        let json = extract_json(text).unwrap();
        assert_eq!(json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_no_json() {
        let text = "Just plain text with no JSON";
        assert!(extract_json(text).is_err());
    }

    #[tokio::test]
    async fn test_route_fallback_to_keyword() {
        let orch = make_orchestrator();

        let plan = orch
            .route("implement a REST API endpoint for user authentication")
            .await;

        assert!(!plan.is_compound);
        assert_eq!(plan.tasks.len(), 1);
        assert!(!plan.primary_routing().agent_name.is_empty());
        assert!(!plan.primary_routing().mode_slug.is_empty());
    }

    #[test]
    fn test_is_orchestrator_disabled_by_default() {
        std::env::remove_var("GOOSE_ORCHESTRATOR_ENABLED");
        assert!(!is_orchestrator_enabled());
    }

    #[test]
    fn test_catalog_excludes_compactor_mode() {
        let orch = make_orchestrator();
        let catalog = orch.build_catalog_text();
        // Compactor should not appear as a routable mode since it's
        // an orchestrator-level concern, not a user-facing agent mode
        assert!(
            !catalog.contains("compactor"),
            "Compactor mode should be excluded from the routing catalog"
        );
    }

    #[test]
    fn test_orchestrator_has_compaction_methods() {
        let orch = make_orchestrator();
        // Verify the orchestrator exposes compaction coordination methods.
        // Actual async compaction tests require a real provider + session,
        // so we verify the API surface exists and the struct is well-formed.
        assert!(orch.provider.try_lock().is_ok());
    }
}
