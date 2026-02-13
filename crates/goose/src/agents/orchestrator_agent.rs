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
//!   ├─ LLM classifies intent → RoutingDecision
//!   ├─ (fallback) IntentRouter keyword matching
//!   └─ Return RoutingDecision { agent_name, mode_slug, confidence, reasoning }
//! ```
//!
//! # Feature Flag
//!
//! Set `GOOSE_ORCHESTRATOR_ENABLED=true` to use LLM routing.
//! When disabled (default), falls back to IntentRouter for backward compatibility.

use crate::agents::coding_agent::CodingAgent;
use crate::agents::goose_agent::GooseAgent;
use crate::agents::intent_router::{IntentRouter, RoutingDecision};
use crate::prompt_template;
use crate::providers::base::Provider;
use crate::registry::manifest::AgentMode;

use anyhow::Result;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Whether LLM-based orchestration is enabled (feature flag).
fn is_orchestrator_enabled() -> bool {
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

    /// Route a user message to the best agent and mode.
    ///
    /// If `GOOSE_ORCHESTRATOR_ENABLED=true` and a provider is available,
    /// uses LLM-based routing. Otherwise falls back to keyword matching.
    pub async fn route(&self, user_message: &str) -> RoutingDecision {
        if is_orchestrator_enabled() {
            match self.route_with_llm(user_message).await {
                Ok(decision) => {
                    info!(
                        agent_name = %decision.agent_name,
                        mode_slug = %decision.mode_slug,
                        confidence = %decision.confidence,
                        "LLM orchestrator routed message"
                    );
                    return decision;
                }
                Err(e) => {
                    warn!(
                        "LLM routing failed, falling back to keyword matching: {}",
                        e
                    );
                }
            }
        }

        // Fallback to keyword-based IntentRouter
        let decision = self.intent_router.route(user_message);
        debug!(
            agent_name = %decision.agent_name,
            mode_slug = %decision.mode_slug,
            confidence = %decision.confidence,
            "Keyword router fallback"
        );
        decision
    }

    /// Use the LLM to classify the user's intent and select the best agent/mode.
    async fn route_with_llm(&self, user_message: &str) -> Result<RoutingDecision> {
        let provider_guard = self.provider.lock().await;
        let provider = provider_guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No provider available for LLM routing"))?;

        let catalog_text = self.build_catalog_text();
        let context = RoutingPromptContext {
            user_message: user_message.to_string(),
            agent_catalog: catalog_text,
        };

        let routing_prompt = prompt_template::render_template("orchestrator/routing.md", &context)?;

        let messages = vec![crate::conversation::message::Message::user().with_text(user_message)];

        let (response, _usage) = provider
            .complete("orchestrator-routing", &routing_prompt, &messages, &[])
            .await?;

        self.parse_routing_response(&response)
    }

    /// Build a human-readable catalog of all available agents and their modes.
    fn build_catalog_text(&self) -> String {
        let mut text = String::new();
        for entry in &self.catalog {
            text.push_str(&format!(
                "### {} — {}
",
                entry.name, entry.description
            ));
            text.push_str(&format!(
                "Default mode: {}
",
                entry.default_mode
            ));
            text.push_str(
                "Modes:
",
            );
            for mode in &entry.modes {
                let when = mode.when_to_use.as_deref().unwrap_or(&mode.description);
                text.push_str(&format!(
                    "  - **{}** ({}): {} | Use when: {}
",
                    mode.slug, mode.name, mode.description, when
                ));
            }
            text.push('\n');
        }
        text
    }

    /// Parse the LLM's JSON response into a RoutingDecision.
    fn parse_routing_response(
        &self,
        response: &crate::conversation::message::Message,
    ) -> Result<RoutingDecision> {
        let text = response
            .content
            .iter()
            .filter_map(|c| match c {
                crate::conversation::message::MessageContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        // Try to extract JSON from the response (may be wrapped in markdown code blocks)
        let json_str = extract_json(&text)?;

        let parsed: serde_json::Value = serde_json::from_str(&json_str)?;

        let agent_name = parsed["agent_name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing agent_name in routing response"))?;
        let mode_slug = parsed["mode_slug"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing mode_slug in routing response"))?;
        let confidence = parsed["confidence"].as_f64().unwrap_or(0.5) as f32;
        let reasoning = parsed["reasoning"]
            .as_str()
            .unwrap_or("LLM routing decision")
            .to_string();

        // Validate agent_name exists in catalog
        if !self.catalog.iter().any(|e| e.name == agent_name) {
            return Err(anyhow::anyhow!(
                "LLM selected unknown agent '{}', available: {:?}",
                agent_name,
                self.catalog.iter().map(|e| &e.name).collect::<Vec<_>>()
            ));
        }

        Ok(RoutingDecision {
            agent_name: agent_name.to_string(),
            mode_slug: mode_slug.to_string(),
            confidence,
            reasoning,
        })
    }
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

    #[test]
    fn test_build_catalog_text() {
        let provider = Arc::new(Mutex::new(None));
        let orch = OrchestratorAgent::new(provider);
        let catalog = orch.build_catalog_text();

        assert!(catalog.contains("Goose Agent"));
        assert!(catalog.contains("Coding Agent"));
        assert!(catalog.contains("assistant"));
        assert!(catalog.contains("backend"));
        assert!(catalog.contains("architect"));
    }

    #[test]
    fn test_parse_routing_response_json() {
        let provider = Arc::new(Mutex::new(None));
        let orch = OrchestratorAgent::new(provider);

        let response = crate::conversation::message::Message::assistant()
            .with_text(r#"{"agent_name": "Coding Agent", "mode_slug": "backend", "confidence": 0.9, "reasoning": "API implementation task"}"#);

        let decision = orch.parse_routing_response(&response).unwrap();
        assert_eq!(decision.agent_name, "Coding Agent");
        assert_eq!(decision.mode_slug, "backend");
        assert!((decision.confidence - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_parse_routing_response_markdown_wrapped() {
        let provider = Arc::new(Mutex::new(None));
        let orch = OrchestratorAgent::new(provider);

        let text = concat!(
            "Here's my routing decision:

",
            "```json
",
            r#"{"agent_name": "Goose Agent", "mode_slug": "planner", "confidence": 0.85, "reasoning": "Planning task"}"#,
            "
```"
        );
        let response = crate::conversation::message::Message::assistant().with_text(text);

        let decision = orch.parse_routing_response(&response).unwrap();
        assert_eq!(decision.agent_name, "Goose Agent");
        assert_eq!(decision.mode_slug, "planner");
    }

    #[test]
    fn test_parse_routing_response_invalid_agent() {
        let provider = Arc::new(Mutex::new(None));
        let orch = OrchestratorAgent::new(provider);

        let response = crate::conversation::message::Message::assistant().with_text(
            r#"{"agent_name": "NonExistent Agent", "mode_slug": "foo", "confidence": 0.5}"#,
        );

        let result = orch.parse_routing_response(&response);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_json_raw() {
        let text = r#"{"agent_name": "Goose Agent", "mode_slug": "assistant"}"#;
        let json = extract_json(text).unwrap();
        assert!(json.contains("Goose Agent"));
    }

    #[test]
    fn test_extract_json_code_block() {
        let text = concat!(
            "Some text
",
            "```json
",
            r#"{"key": "value"}"#,
            "
```
",
            "More text"
        );
        let json = extract_json(text).unwrap();
        assert_eq!(json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_no_json() {
        let text = "Just plain text with no JSON";
        let result = extract_json(text);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_route_fallback_to_keyword() {
        let provider = Arc::new(Mutex::new(None));
        let orch = OrchestratorAgent::new(provider);

        // Without GOOSE_ORCHESTRATOR_ENABLED, should use keyword fallback
        let decision = orch
            .route("implement a REST API endpoint for user authentication")
            .await;

        // Should route to something (keyword router handles it)
        assert!(!decision.agent_name.is_empty());
        assert!(!decision.mode_slug.is_empty());
    }

    #[test]
    fn test_is_orchestrator_disabled_by_default() {
        // Clean env
        std::env::remove_var("GOOSE_ORCHESTRATOR_ENABLED");
        assert!(!is_orchestrator_enabled());
    }
}
