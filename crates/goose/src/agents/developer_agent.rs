/// Developer Agent — software engineering persona.
///
/// The Developer Agent is responsible for writing, debugging, reviewing, and
/// planning code. It uses the universal mode set (ask/plan/write/review/debug)
/// plus specialized app modes (app_maker/app_iterator) for building standalone
/// HTML/CSS/JS applications.
///
/// This agent replaces the former `CodingAgent` which conflated personas
/// (PM, QA, Security) with behavioral modes.
use std::collections::HashMap;

use crate::agents::universal_mode::UniversalMode;
use crate::prompt_template;
use crate::registry::manifest::{AgentMode, ToolGroupAccess};

/// Additional tool groups the Developer persona adds on top of UniversalMode defaults.
fn developer_extra_tools(mode: &UniversalMode) -> Vec<ToolGroupAccess> {
    match mode {
        UniversalMode::Ask => vec![ToolGroupAccess::Full("code_execution".into())],
        UniversalMode::Plan => vec![ToolGroupAccess::Full("code_execution".into())],
        UniversalMode::Write => vec![
            ToolGroupAccess::Full("mcp".into()),
            ToolGroupAccess::Full("code_execution".into()),
        ],
        UniversalMode::Review => vec![
            ToolGroupAccess::Full("command".into()),
            ToolGroupAccess::Full("code_execution".into()),
        ],
        UniversalMode::Debug => vec![
            ToolGroupAccess::Full("mcp".into()),
            ToolGroupAccess::Full("code_execution".into()),
        ],
    }
}

/// Recommended MCP extensions per mode.
fn universal_recommended_extensions(mode: &UniversalMode) -> Vec<&'static str> {
    match mode {
        UniversalMode::Ask => vec!["developer", "context7", "memory", "genui"],
        UniversalMode::Plan => vec!["developer", "context7", "memory", "fetch"],
        UniversalMode::Write => vec![
            "developer",
            "github",
            "context7",
            "memory",
            "code_execution",
            "genui",
        ],
        UniversalMode::Review => vec!["developer", "github", "memory"],
        UniversalMode::Debug => vec![
            "developer",
            "github",
            "context7",
            "memory",
            "code_execution",
            "genui",
        ],
    }
}

/// A mode owned by the Developer agent.
enum DeveloperModeKind {
    /// Universal modes shared across agents (ask/plan/write/review/debug).
    Universal(UniversalMode),
    /// Custom modes specific to this agent (app_maker, app_iterator).
    Custom {
        slug: String,
        name: String,
        description: String,
        template_file: String,
        tool_groups: Vec<ToolGroupAccess>,
        when_to_use: String,
        recommended_extensions: Vec<&'static str>,
    },
}

pub struct DeveloperAgent {
    modes: Vec<DeveloperModeKind>,
    default_mode: String,
}

impl Default for DeveloperAgent {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable ordering: universal modes first (ask/plan/write/review/debug),
/// then custom modes (app_maker, app_iterator).
const MODE_ORDER: &[&str] = &[
    "ask",
    "plan",
    "write",
    "review",
    "debug",
    "app_maker",
    "app_iterator",
];

impl DeveloperAgent {
    pub fn new() -> Self {
        let mut modes = Vec::new();

        for um in UniversalMode::all() {
            modes.push(DeveloperModeKind::Universal(*um));
        }

        modes.push(DeveloperModeKind::Custom {
            slug: "app_maker".into(),
            name: "App Creator".into(),
            description: "Build standalone HTML/CSS/JS applications from a description or PRD."
                .into(),
            template_file: "developer/apps_create.md".into(),
            tool_groups: vec![ToolGroupAccess::Full("apps".into())],
            when_to_use: "User wants to create a new standalone web app, prototype, or HTML tool."
                .into(),
            recommended_extensions: vec!["apps"],
        });

        modes.push(DeveloperModeKind::Custom {
            slug: "app_iterator".into(),
            name: "App Iterator".into(),
            description: "Improve an existing Goose app based on feedback.".into(),
            template_file: "developer/apps_iterate.md".into(),
            tool_groups: vec![ToolGroupAccess::Full("apps".into())],
            when_to_use:
                "User wants to modify, improve, or iterate on an existing Goose app with feedback."
                    .into(),
            recommended_extensions: vec!["apps"],
        });

        Self {
            modes,
            default_mode: "write".to_string(),
        }
    }

    pub fn mode(&self, slug: &str) -> Option<&UniversalMode> {
        self.modes.iter().find_map(|mk| match mk {
            DeveloperModeKind::Universal(um) if um.slug() == slug => Some(um),
            _ => None,
        })
    }

    pub fn default_mode(&self) -> &str {
        &self.default_mode
    }

    pub fn modes(&self) -> Vec<&str> {
        let mut slugs: Vec<&str> = self
            .modes
            .iter()
            .map(|mk| match mk {
                DeveloperModeKind::Universal(um) => um.slug(),
                DeveloperModeKind::Custom { slug, .. } => slug.as_str(),
            })
            .collect();
        slugs.sort_by_key(|s| MODE_ORDER.iter().position(|o| o == s).unwrap_or(99));
        slugs
    }

    pub fn render_mode(
        &self,
        slug: &str,
        context: &HashMap<String, String>,
    ) -> anyhow::Result<String> {
        let mk = self
            .modes
            .iter()
            .find(|mk| match mk {
                DeveloperModeKind::Universal(um) => um.slug() == slug,
                DeveloperModeKind::Custom { slug: s, .. } => s == slug,
            })
            .ok_or_else(|| anyhow::anyhow!("Unknown Developer Agent mode: {slug}"))?;
        let template_name = match mk {
            DeveloperModeKind::Universal(um) => format!("developer/{}.md", um.slug()),
            DeveloperModeKind::Custom { template_file, .. } => template_file.clone(),
        };
        Ok(prompt_template::render_template(&template_name, context)?)
    }

    pub fn to_agent_modes(&self) -> Vec<AgentMode> {
        let mut result: Vec<AgentMode> = self
            .modes
            .iter()
            .map(|mk| match mk {
                DeveloperModeKind::Universal(um) => {
                    let mut tool_groups = um.base_tool_groups();
                    tool_groups.extend(developer_extra_tools(um));
                    AgentMode {
                        slug: um.slug().to_string(),
                        name: um.display_name().to_string(),
                        description: um.description().to_string(),
                        instructions: None,
                        instructions_file: Some(format!("developer/{}.md", um.slug())),
                        tool_groups,
                        when_to_use: Some(um.when_to_use().to_string()),
                        is_internal: false,
                        deprecated: None,
                    }
                }
                DeveloperModeKind::Custom {
                    slug,
                    name,
                    description,
                    template_file,
                    tool_groups,
                    when_to_use,
                    ..
                } => AgentMode {
                    slug: slug.clone(),
                    name: name.clone(),
                    description: description.clone(),
                    instructions: None,
                    instructions_file: Some(template_file.clone()),
                    tool_groups: tool_groups.clone(),
                    when_to_use: Some(when_to_use.clone()),
                    is_internal: false,
                    deprecated: None,
                },
            })
            .collect();
        result.sort_by_key(|m| MODE_ORDER.iter().position(|o| *o == m.slug).unwrap_or(99));
        result
    }

    pub fn recommended_extensions(&self, slug: &str) -> Vec<String> {
        self.modes
            .iter()
            .find_map(|mk| match mk {
                DeveloperModeKind::Universal(um) if um.slug() == slug => Some(
                    universal_recommended_extensions(um)
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
                DeveloperModeKind::Custom {
                    slug: s,
                    recommended_extensions,
                    ..
                } if s == slug => Some(
                    recommended_extensions
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn tool_groups_for(&self, slug: &str) -> Vec<ToolGroupAccess> {
        self.modes
            .iter()
            .find_map(|mk| match mk {
                DeveloperModeKind::Universal(um) if um.slug() == slug => {
                    let mut tg = um.base_tool_groups();
                    tg.extend(developer_extra_tools(um));
                    Some(tg)
                }
                DeveloperModeKind::Custom {
                    slug: s,
                    tool_groups,
                    ..
                } if s == slug => Some(tool_groups.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_write() {
        let agent = DeveloperAgent::new();
        assert_eq!(agent.default_mode(), "write");
    }

    #[test]
    fn test_has_universal_and_custom_modes() {
        let agent = DeveloperAgent::new();
        let modes = agent.modes();
        assert_eq!(modes.len(), 7);
        assert_eq!(
            modes,
            vec![
                "ask",
                "plan",
                "write",
                "review",
                "debug",
                "app_maker",
                "app_iterator"
            ]
        );
    }

    #[test]
    fn test_mode_lookup() {
        let agent = DeveloperAgent::new();
        assert!(agent.mode("write").is_some());
        assert!(agent.mode("ask").is_some());
        assert!(agent.mode("plan").is_some());
        assert!(agent.mode("review").is_some());
        assert!(agent.mode("debug").is_some());
        // Custom modes don't return UniversalMode
        assert!(agent.mode("app_maker").is_none());
        assert!(agent.mode("app_iterator").is_none());
        assert!(agent.mode("backend").is_none());
        assert!(agent.mode("pm").is_none());
    }

    #[test]
    fn test_to_agent_modes_ordered() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        assert_eq!(modes.len(), 7);
        let slugs: Vec<&str> = modes.iter().map(|m| m.slug.as_str()).collect();
        assert_eq!(
            slugs,
            vec![
                "ask",
                "plan",
                "write",
                "review",
                "debug",
                "app_maker",
                "app_iterator"
            ]
        );
    }

    #[test]
    fn test_all_modes_have_when_to_use() {
        let agent = DeveloperAgent::new();
        for mode in agent.to_agent_modes() {
            assert!(
                mode.when_to_use.is_some(),
                "Mode {} missing when_to_use",
                mode.slug
            );
        }
    }

    #[test]
    fn test_write_has_edit_and_command() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        let write_mode = modes.iter().find(|m| m.slug == "write").unwrap();
        let tg_str = format!("{:?}", write_mode.tool_groups);
        assert!(tg_str.contains("edit"), "Write mode needs edit: {tg_str}");
        assert!(
            tg_str.contains("command"),
            "Write mode needs command: {tg_str}"
        );
    }

    #[test]
    fn test_ask_is_readonly() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        let ask_mode = modes.iter().find(|m| m.slug == "ask").unwrap();
        let tg_str = format!("{:?}", ask_mode.tool_groups);
        assert!(
            !tg_str.contains("edit"),
            "Ask mode should not have edit: {tg_str}"
        );
    }

    #[test]
    fn test_review_is_readonly_but_can_run_checks() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        let review_mode = modes.iter().find(|m| m.slug == "review").unwrap();
        let tg_str = format!("{:?}", review_mode.tool_groups);
        assert!(
            !tg_str.contains("edit"),
            "Review mode should not have edit: {tg_str}"
        );
        assert!(
            tg_str.contains("command"),
            "Review mode needs command for running checks: {tg_str}"
        );
    }

    #[test]
    fn test_recommended_extensions() {
        let agent = DeveloperAgent::new();
        let exts = agent.recommended_extensions("write");
        assert!(exts.contains(&"developer".to_string()));
        assert!(exts.contains(&"github".to_string()));
    }

    #[test]
    fn test_app_maker_mode() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        let app_maker = modes.iter().find(|m| m.slug == "app_maker").unwrap();
        assert_eq!(app_maker.name, "App Creator");
        let tg_str = format!("{:?}", app_maker.tool_groups);
        assert!(
            tg_str.contains("apps"),
            "App Creator needs apps tools: {tg_str}"
        );
        assert!(app_maker.when_to_use.is_some());
    }

    #[test]
    fn test_app_iterator_mode() {
        let agent = DeveloperAgent::new();
        let modes = agent.to_agent_modes();
        let app_iter = modes.iter().find(|m| m.slug == "app_iterator").unwrap();
        assert_eq!(app_iter.name, "App Iterator");
        let tg_str = format!("{:?}", app_iter.tool_groups);
        assert!(
            tg_str.contains("apps"),
            "App Iterator needs apps tools: {tg_str}"
        );
    }

    #[test]
    fn test_app_mode_recommended_extensions() {
        let agent = DeveloperAgent::new();
        let exts = agent.recommended_extensions("app_maker");
        assert!(exts.contains(&"apps".to_string()));
        let exts = agent.recommended_extensions("app_iterator");
        assert!(exts.contains(&"apps".to_string()));
    }

    #[test]
    fn test_render_app_maker_mode() {
        let agent = DeveloperAgent::new();
        let ctx = HashMap::new();
        let result = agent.render_mode("app_maker", &ctx);
        assert!(
            result.is_ok(),
            "render_mode app_maker failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_render_app_iterator_mode() {
        let agent = DeveloperAgent::new();
        let ctx = HashMap::new();
        let result = agent.render_mode("app_iterator", &ctx);
        assert!(
            result.is_ok(),
            "render_mode app_iterator failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_unknown_mode_returns_none() {
        let agent = DeveloperAgent::new();
        assert!(agent.mode("nonexistent").is_none());
    }

    #[test]
    fn test_render_mode() {
        let agent = DeveloperAgent::new();
        let ctx = HashMap::new();
        let result = agent.render_mode("write", &ctx);
        assert!(result.is_ok(), "render_mode failed: {:?}", result.err());
    }
}
