//! Built-in Goose agent with specialized behavioral modes.
//!
//! Instead of separate prompt templates loaded ad-hoc by different subsystems,
//! the built-in agent formalizes all Goose behaviors as `BuiltinMode`s.
//! Each mode maps to what was previously a standalone .md prompt template.
//!
//! # Mode Categories
//!
//! 1. **Session modes** — affect the main agent's system prompt
//!    - `assistant` (system.md) — default personality
//!    - `specialist` (specialist.md) — bounded task execution
//!
//! 2. **LLM-only modes** — direct provider.complete() with specialized prompt
//!    - `judge` (permission_judge.md) — read-only detection
//!    - `compactor` (compaction.md) — conversation summarization
//!    - `app_maker` (apps_create.md) — generate new apps
//!    - `app_iterator` (apps_iterate.md) — update existing apps
//!
//! 3. **Prompt-only modes** — just return a rendered prompt string
//!    - `recipe_maker` (recipe.md) — recipe generation prompt
//!    - `planner` (plan.md) — step-by-step planning prompt
//!
//! # Migration
//!
//! Callers currently use `prompt_template::render_template("foo.md", &ctx)` directly.
//! The migration path:
//! 1. `GooseAgent::mode("judge").render(&ctx)` — same result, but discoverable
//! 2. `GooseAgent::mode("judge").complete(provider, messages)` — encapsulates the LLM call
//! 3. Eventually, modes become ACP SessionModes advertised to clients

use crate::prompt_template;
use crate::registry::manifest::{AgentMode, ToolGroupAccess};
use serde::Serialize;
use std::collections::HashMap;

/// A built-in mode that maps to a prompt template.
#[derive(Debug, Clone)]
pub struct BuiltinMode {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub template_name: String,
    pub category: ModeCategory,
    pub tool_groups: Vec<ToolGroupAccess>,
}

/// How the mode is executed.
#[derive(Debug, Clone, PartialEq)]
pub enum ModeCategory {
    /// Affects the main agent's system prompt (creates Agent or overrides prompt)
    Session,
    /// Direct LLM call with specialized system prompt (provider.complete)
    LlmOnly,
    /// Just returns a rendered prompt string
    PromptOnly,
}

/// The built-in Goose agent definition.
/// All standard Goose behaviors are modes of this agent.
pub struct GooseAgent {
    modes: HashMap<String, BuiltinMode>,
    default_mode: String,
}

impl Default for GooseAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl GooseAgent {
    pub fn new() -> Self {
        let modes = vec![
            BuiltinMode {
                slug: "assistant".into(),
                name: "🦆 Assistant".into(),
                description: "General-purpose assistant — the default Goose personality".into(),
                template_name: "system.md".into(),
                category: ModeCategory::Session,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "specialist".into(),
                name: "🔧 Specialist".into(),
                description: "Focused task execution with bounded turns".into(),
                template_name: "specialist.md".into(),
                category: ModeCategory::Session,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "recipe_maker".into(),
                name: "📋 Recipe Maker".into(),
                description: "Generate recipe files from conversations".into(),
                template_name: "recipe.md".into(),
                category: ModeCategory::PromptOnly,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "app_maker".into(),
                name: "🎨 App Creator".into(),
                description: "Create new Goose apps from user instructions".into(),
                template_name: "apps_create.md".into(),
                category: ModeCategory::LlmOnly,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "app_iterator".into(),
                name: "🔄 App Iterator".into(),
                description: "Update existing Goose apps based on feedback".into(),
                template_name: "apps_iterate.md".into(),
                category: ModeCategory::LlmOnly,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "judge".into(),
                name: "⚖️ Permission Judge".into(),
                description: "Analyze tool operations for read-only detection".into(),
                template_name: "permission_judge.md".into(),
                category: ModeCategory::LlmOnly,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "planner".into(),
                name: "🗺️ Planner".into(),
                description: "Create step-by-step execution plans".into(),
                template_name: "plan.md".into(),
                category: ModeCategory::PromptOnly,
                tool_groups: vec![],
            },
            BuiltinMode {
                slug: "compactor".into(),
                name: "📦 Compactor".into(),
                description: "Summarize conversation history when context limits are reached"
                    .into(),
                template_name: "compaction.md".into(),
                category: ModeCategory::LlmOnly,
                tool_groups: vec![],
            },
        ];

        let mode_map = modes.into_iter().map(|m| (m.slug.clone(), m)).collect();

        Self {
            modes: mode_map,
            default_mode: "assistant".into(),
        }
    }

    /// Get a mode by slug.
    pub fn mode(&self, slug: &str) -> Option<&BuiltinMode> {
        self.modes.get(slug)
    }

    /// Get the default mode.
    pub fn default_mode(&self) -> &BuiltinMode {
        self.modes
            .get(&self.default_mode)
            .expect("default mode must exist")
    }

    /// List all available modes.
    pub fn list_modes(&self) -> Vec<&BuiltinMode> {
        let mut modes: Vec<_> = self.modes.values().collect();
        modes.sort_by_key(|m| &m.slug);
        modes
    }

    /// Convert built-in modes to registry AgentMode format.
    /// This allows built-in modes to be advertised via ACP SessionModeState.
    pub fn to_agent_modes(&self) -> Vec<AgentMode> {
        self.list_modes()
            .into_iter()
            .map(|m| AgentMode {
                slug: m.slug.clone(),
                name: m.name.clone(),
                description: m.description.clone(),
                instructions: None,
                instructions_file: Some(m.template_name.clone()),
                tool_groups: m.tool_groups.clone(),
                when_to_use: Some(m.description.clone()),
            })
            .collect()
    }

    /// Get the default mode slug.
    pub fn default_mode_slug(&self) -> &str {
        &self.default_mode
    }
}

impl BuiltinMode {
    /// Render this mode's template with the given context.
    /// This is the same as calling `prompt_template::render_template` directly,
    /// but makes the mode → template mapping explicit and discoverable.
    pub fn render<T: Serialize>(&self, context: &T) -> anyhow::Result<String> {
        prompt_template::render_template(&self.template_name, context).map_err(|e| {
            anyhow::anyhow!(
                "Failed to render mode '{}' template '{}': {}",
                self.slug,
                self.template_name,
                e
            )
        })
    }

    pub fn is_session_mode(&self) -> bool {
        self.category == ModeCategory::Session
    }

    pub fn is_llm_only(&self) -> bool {
        self.category == ModeCategory::LlmOnly
    }

    pub fn is_prompt_only(&self) -> bool {
        self.category == ModeCategory::PromptOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agent_has_all_modes() {
        let agent = GooseAgent::new();
        let modes = agent.list_modes();
        assert_eq!(modes.len(), 8);
    }

    #[test]
    fn test_default_mode_is_assistant() {
        let agent = GooseAgent::new();
        assert_eq!(agent.default_mode_slug(), "assistant");
        assert_eq!(agent.default_mode().template_name, "system.md");
    }

    #[test]
    fn test_mode_lookup() {
        let agent = GooseAgent::new();
        let judge = agent.mode("judge").unwrap();
        assert_eq!(judge.template_name, "permission_judge.md");
        assert!(judge.is_llm_only());
    }

    #[test]
    fn test_specialist_is_session_mode() {
        let agent = GooseAgent::new();
        let specialist = agent.mode("specialist").unwrap();
        assert!(specialist.is_session_mode());
        assert_eq!(specialist.template_name, "specialist.md");
    }

    #[test]
    fn test_planner_is_prompt_only() {
        let agent = GooseAgent::new();
        let planner = agent.mode("planner").unwrap();
        assert!(planner.is_prompt_only());
        assert_eq!(planner.template_name, "plan.md");
    }

    #[test]
    fn test_to_agent_modes() {
        let agent = GooseAgent::new();
        let agent_modes = agent.to_agent_modes();
        assert_eq!(agent_modes.len(), 8);

        let assistant = agent_modes.iter().find(|m| m.slug == "assistant").unwrap();
        assert_eq!(assistant.instructions_file.as_deref(), Some("system.md"));
    }

    #[test]
    fn test_render_assistant_mode() {
        let agent = GooseAgent::new();
        let assistant = agent.mode("assistant").unwrap();
        // system.md requires a template context — use empty HashMap
        // This should render without error (template exists)
        let ctx: HashMap<String, String> = HashMap::new();
        let result = assistant.render(&ctx);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("goose"));
    }

    #[test]
    fn test_nonexistent_mode() {
        let agent = GooseAgent::new();
        assert!(agent.mode("nonexistent").is_none());
    }
}
