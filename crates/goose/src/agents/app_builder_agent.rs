//! App Builder Agent — standalone HTML/CSS/JS app creation.
//!
//! A dedicated agent for creating and iterating sandboxed single-file web applications
//! using the `apps` extension. Intentionally separate from the Developer Agent because
//! the tool stacks are incompatible (apps = sandboxed HTML windows vs developer = filesystem).

use std::collections::HashMap;

use crate::prompt_template;
use crate::registry::manifest::{AgentMode, ToolGroupAccess};
use serde::Serialize;

struct AppMode {
    slug: &'static str,
    name: &'static str,
    description: &'static str,
    when_to_use: &'static str,
    template: &'static str,
    tool_groups: Vec<ToolGroupAccess>,
    recommended_extensions: Vec<&'static str>,
}

pub struct AppBuilderAgent {
    modes: Vec<AppMode>,
    default_mode: String,
}

impl Default for AppBuilderAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AppBuilderAgent {
    pub fn new() -> Self {
        Self {
            modes: vec![
                AppMode {
                    slug: "app_maker",
                    name: "App Creator",
                    description: "Build standalone HTML/CSS/JS applications from a description or PRD",
                    when_to_use: "User wants to create a new standalone web app, prototype, or HTML tool — not a project file",
                    template: "developer/apps_create.md",
                    tool_groups: vec![ToolGroupAccess::Full("apps".into())],
                    recommended_extensions: vec!["apps"],
                },
                AppMode {
                    slug: "app_iterator",
                    name: "App Iterator",
                    description: "Modify and improve existing standalone HTML/CSS/JS applications",
                    when_to_use: "User wants to iterate, update, or fix an existing sandboxed app",
                    template: "developer/apps_iterate.md",
                    tool_groups: vec![ToolGroupAccess::Full("apps".into())],
                    recommended_extensions: vec!["apps"],
                },
            ],
            default_mode: "app_maker".to_string(),
        }
    }

    pub fn default_mode(&self) -> &str {
        &self.default_mode
    }

    pub fn modes(&self) -> Vec<&str> {
        self.modes.iter().map(|m| m.slug).collect()
    }

    pub fn render_mode(&self, slug: &str, context: &HashMap<String, String>) -> Option<String> {
        let m = self.modes.iter().find(|m| m.slug == slug)?;

        #[derive(Serialize)]
        struct Ctx<'a> {
            mode: &'a str,
            extra: &'a HashMap<String, String>,
        }
        let ctx = Ctx {
            mode: slug,
            extra: context,
        };

        prompt_template::render_template(m.template, &ctx).ok()
    }

    pub fn to_agent_modes(&self) -> Vec<AgentMode> {
        self.modes
            .iter()
            .map(|m| AgentMode {
                slug: m.slug.to_string(),
                name: m.name.to_string(),
                description: m.description.to_string(),
                tool_groups: m.tool_groups.clone(),
                when_to_use: Some(m.when_to_use.to_string()),
                instructions: None,
                instructions_file: None,
                is_internal: false,
                deprecated: None,
            })
            .collect()
    }

    pub fn recommended_extensions(&self, slug: &str) -> Vec<String> {
        self.modes
            .iter()
            .find(|m| m.slug == slug)
            .map(|m| {
                m.recommended_extensions
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn tool_groups_for(&self, slug: &str) -> Vec<ToolGroupAccess> {
        self.modes
            .iter()
            .find(|m| m.slug == slug)
            .map(|m| m.tool_groups.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_2_modes() {
        let agent = AppBuilderAgent::new();
        assert_eq!(agent.modes().len(), 2);
    }

    #[test]
    fn test_modes_are_app_maker_and_iterator() {
        let agent = AppBuilderAgent::new();
        let slugs = agent.modes();
        assert!(slugs.contains(&"app_maker"));
        assert!(slugs.contains(&"app_iterator"));
    }

    #[test]
    fn test_default_mode_is_app_maker() {
        let agent = AppBuilderAgent::new();
        assert_eq!(agent.default_mode(), "app_maker");
    }

    #[test]
    fn test_tool_groups_use_apps_extension() {
        let agent = AppBuilderAgent::new();
        let tg = agent.tool_groups_for("app_maker");
        let has_apps = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "apps"));
        assert!(has_apps, "app_maker should have apps tool group");
    }

    #[test]
    fn test_recommended_extensions() {
        let agent = AppBuilderAgent::new();
        let exts = agent.recommended_extensions("app_maker");
        assert!(exts.contains(&"apps".to_string()));
    }

    #[test]
    fn test_render_app_maker() {
        let agent = AppBuilderAgent::new();
        let ctx = HashMap::new();
        let rendered = agent.render_mode("app_maker", &ctx);
        assert!(rendered.is_some());
        assert!(rendered.unwrap().contains("HTML"));
    }

    #[test]
    fn test_render_app_iterator() {
        let agent = AppBuilderAgent::new();
        let ctx = HashMap::new();
        let rendered = agent.render_mode("app_iterator", &ctx);
        assert!(rendered.is_some());
        assert!(rendered.unwrap().contains("HTML"));
    }

    #[test]
    fn test_to_agent_modes() {
        let agent = AppBuilderAgent::new();
        let modes = agent.to_agent_modes();
        assert_eq!(modes.len(), 2);
        assert!(modes.iter().any(|m| m.slug == "app_maker"));
        assert!(modes.iter().any(|m| m.slug == "app_iterator"));
        for mode in &modes {
            assert!(mode.when_to_use.is_some());
        }
    }

    #[test]
    fn test_no_developer_tools() {
        let agent = AppBuilderAgent::new();
        let tg = agent.tool_groups_for("app_maker");
        let has_edit = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "edit"));
        let has_command = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "command"));
        let has_developer = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "developer"));
        assert!(!has_edit, "app modes should not have edit tools");
        assert!(!has_command, "app modes should not have command tools");
        assert!(!has_developer, "app modes should not have developer tools");
    }
}
