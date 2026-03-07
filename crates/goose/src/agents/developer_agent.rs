//! Developer Agent — software engineering persona.
//!
//! Responsible for writing, debugging, reviewing, and planning code.
//! Uses universal modes (ask/plan/write/review/debug) with the `developer` extension.

use std::collections::HashMap;

use crate::agents::universal_mode::UniversalMode;
use crate::prompt_template;
use crate::registry::manifest::{AgentMode, ToolGroupAccess};
use serde::Serialize;

fn developer_extra_tools(um: &UniversalMode) -> Vec<ToolGroupAccess> {
    match um {
        UniversalMode::Write | UniversalMode::Debug => vec![
            ToolGroupAccess::Full("edit".into()),
            ToolGroupAccess::Full("command".into()),
            ToolGroupAccess::Full("mcp".into()),
            ToolGroupAccess::Full("code_execution".into()),
            ToolGroupAccess::Full("diagnostics".into()),
        ],
        UniversalMode::Plan => vec![
            ToolGroupAccess::Full("mcp".into()),
            ToolGroupAccess::Full("command".into()),
        ],
        UniversalMode::Review => vec![
            ToolGroupAccess::Full("command".into()),
            ToolGroupAccess::Full("mcp".into()),
        ],
        UniversalMode::Ask => vec![ToolGroupAccess::Full("mcp".into())],
    }
}

fn recommended_extensions(um: &UniversalMode) -> Vec<&'static str> {
    match um {
        UniversalMode::Write | UniversalMode::Debug => {
            vec![
                "developer",
                "github",
                "context7",
                "memory",
                "genui",
                "fetch",
            ]
        }
        UniversalMode::Plan => vec!["developer", "github", "memory"],
        UniversalMode::Review => vec!["developer", "github"],
        UniversalMode::Ask => vec!["developer"],
    }
}

struct DeveloperMode {
    mode: UniversalMode,
    extra_tools: Vec<ToolGroupAccess>,
    recommended_extensions: Vec<&'static str>,
}

pub struct DeveloperAgent {
    modes: HashMap<String, DeveloperMode>,
    default_mode: String,
}

impl Default for DeveloperAgent {
    fn default() -> Self {
        Self::new()
    }
}

const MODE_ORDER: &[&str] = &["ask", "plan", "write", "review", "debug"];

impl DeveloperAgent {
    pub fn new() -> Self {
        let mut modes = HashMap::new();
        for um in UniversalMode::all() {
            modes.insert(
                um.slug().to_string(),
                DeveloperMode {
                    mode: *um,
                    extra_tools: developer_extra_tools(um),
                    recommended_extensions: recommended_extensions(um),
                },
            );
        }
        Self {
            modes,
            default_mode: "write".to_string(),
        }
    }

    pub fn mode(&self, slug: &str) -> Option<&UniversalMode> {
        self.modes.get(slug).map(|dm| &dm.mode)
    }

    pub fn default_mode(&self) -> &str {
        &self.default_mode
    }

    pub fn modes(&self) -> Vec<&str> {
        let mut slugs: Vec<&str> = self.modes.keys().map(|s| s.as_str()).collect();
        slugs.sort_by_key(|s| MODE_ORDER.iter().position(|o| o == s).unwrap_or(99));
        slugs
    }

    pub fn render_mode(&self, slug: &str, context: &HashMap<String, String>) -> Option<String> {
        let dm = self.modes.get(slug)?;
        let template_name = format!("developer/{}.md", dm.mode.slug());

        #[derive(Serialize)]
        struct Ctx<'a> {
            mode: &'a str,
            extra: &'a HashMap<String, String>,
        }
        let ctx = Ctx {
            mode: dm.mode.slug(),
            extra: context,
        };

        prompt_template::render_template(&template_name, &ctx).ok()
    }

    pub fn to_agent_modes(&self) -> Vec<AgentMode> {
        let mut result = Vec::new();
        for slug in self.modes() {
            if let Some(dm) = self.modes.get(slug) {
                let mut mode = dm.mode.to_agent_mode("developer");
                mode.tool_groups.extend(dm.extra_tools.clone());
                result.push(mode);
            }
        }
        result
    }

    pub fn recommended_extensions(&self, slug: &str) -> Vec<String> {
        self.modes
            .get(slug)
            .map(|dm| {
                dm.recommended_extensions
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn tool_groups_for(&self, slug: &str) -> Vec<ToolGroupAccess> {
        self.modes
            .get(slug)
            .map(|dm| {
                let mut tg = dm.mode.base_tool_groups();
                tg.extend(dm.extra_tools.clone());
                tg
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_developer_has_5_modes() {
        let agent = DeveloperAgent::new();
        assert_eq!(agent.modes().len(), 5);
    }

    #[test]
    fn test_developer_modes_order() {
        let agent = DeveloperAgent::new();
        assert_eq!(
            agent.modes(),
            vec!["ask", "plan", "write", "review", "debug"]
        );
    }

    #[test]
    fn test_default_mode_is_write() {
        let agent = DeveloperAgent::new();
        assert_eq!(agent.default_mode(), "write");
    }

    #[test]
    fn test_mode_lookup() {
        let agent = DeveloperAgent::new();
        assert!(agent.mode("write").is_some());
        assert!(agent.mode("nonexistent").is_none());
    }

    #[test]
    fn test_write_mode_has_edit_and_command() {
        let agent = DeveloperAgent::new();
        let tg = agent.tool_groups_for("write");
        let has_edit = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "edit"));
        let has_command = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "command"));
        assert!(has_edit, "write mode should have edit tool group");
        assert!(has_command, "write mode should have command tool group");
    }

    #[test]
    fn test_ask_mode_has_no_edit() {
        let agent = DeveloperAgent::new();
        let tg = agent.tool_groups_for("ask");
        let has_edit = tg
            .iter()
            .any(|t| matches!(t, ToolGroupAccess::Full(n) if n == "edit"));
        assert!(!has_edit, "ask mode should not have edit tool group");
    }

    #[test]
    fn test_recommended_extensions() {
        let agent = DeveloperAgent::new();
        let exts = agent.recommended_extensions("write");
        assert!(exts.contains(&"developer".to_string()));
        assert!(exts.contains(&"github".to_string()));
    }

    #[test]
    fn test_render_mode() {
        let agent = DeveloperAgent::new();
        let ctx = HashMap::new();
        let rendered = agent.render_mode("write", &ctx);
        assert!(rendered.is_some());
        let text = rendered.unwrap();
        assert!(!text.is_empty());
    }

    #[test]
    fn test_to_agent_modes() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        assert_eq!(modes.len(), 5);
        let slugs: Vec<&str> = modes.iter().map(|m| m.slug.as_str()).collect();
        assert!(slugs.contains(&"write"));
        assert!(slugs.contains(&"ask"));
    }
}
