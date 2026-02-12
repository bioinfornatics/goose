use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// The four kinds of artifacts in the registry.
///
/// Aligns with the existing `SourceKind` enum in summon_extension.rs
/// but adds Tool (which is managed separately by ExtensionManager today).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum RegistryEntryKind {
    #[default]
    Tool,
    Skill,
    Agent,
    Recipe,
}

impl std::fmt::Display for RegistryEntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tool => write!(f, "tool"),
            Self::Skill => write!(f, "skill"),
            Self::Agent => write!(f, "agent"),
            Self::Recipe => write!(f, "recipe"),
        }
    }
}

/// A unified registry entry that can represent any of the 4 artifact types.
///
/// Designed to be the common currency across all registry sources (local, GitHub, HTTP).
/// Inspired by ACP Agent Manifest and A2A Agent Cards but adapted for Goose's needs.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct RegistryEntry {
    pub name: String,
    pub kind: RegistryEntryKind,
    pub description: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<AuthorInfo>,

    /// Where this entry was resolved from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_uri: Option<String>,

    /// Local path if available (e.g. from filesystem scan).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>)]
    pub local_path: Option<PathBuf>,

    /// Tags for search and categorization.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Kind-specific payload.
    #[serde(flatten)]
    pub detail: RegistryEntryDetail,

    /// Additional metadata from external registries.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl RegistryEntry {
    /// Merge metadata from another entry with the same name+kind.
    /// Used when the same artifact appears in multiple sources.
    pub fn merge_metadata(&mut self, other: &RegistryEntry) {
        for (k, v) in &other.metadata {
            self.metadata.entry(k.clone()).or_insert_with(|| v.clone());
        }
        if self.version.is_none() {
            self.version.clone_from(&other.version);
        }
        if self.author.is_none() {
            self.author.clone_from(&other.author);
        }
    }
}

/// Kind-specific details for each registry entry type.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "detail_type")]
pub enum RegistryEntryDetail {
    #[serde(rename = "tool")]
    Tool(ToolDetail),
    #[serde(rename = "skill")]
    Skill(SkillDetail),
    #[serde(rename = "agent")]
    Agent(AgentDetail),
    #[serde(rename = "recipe")]
    Recipe(RecipeDetail),
}

impl Default for RegistryEntryDetail {
    fn default() -> Self {
        Self::Tool(ToolDetail::default())
    }
}

/// Detail for a Tool (MCP extension).
///
/// Mirrors the fields from ExtensionConfig that matter for registry display.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ToolDetail {
    pub transport: ToolTransport,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_keys: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ToolTransport {
    Stdio {
        cmd: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    StreamableHttp {
        uri: String,
    },
    #[default]
    Builtin,
}

/// Detail for a Skill (short prompt instruction).
///
/// Skills are markdown files with YAML frontmatter: name + description + body.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SkillDetail {
    pub content: String,
    pub builtin: bool,
}

/// Detail for an Agent definition.
///
/// Agents are markdown files with YAML frontmatter: name, description, model.
/// Inspired by ACP Agent Manifest capabilities/domains fields.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgentDetail {
    pub instructions: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_content_types: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_content_types: Vec<String>,
}

/// Detail for a Recipe (complete agent config).
///
/// References the existing Recipe struct fields without duplicating them.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RecipeDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extension_names: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<String>,
}

/// Author information, compatible with the existing Author struct in recipe/mod.rs.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuthorInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_tool_entry() {
        let entry = RegistryEntry {
            name: "developer".into(),
            kind: RegistryEntryKind::Tool,
            description: "Developer tools for code editing and shell".into(),
            version: Some("1.0.0".into()),
            author: None,
            source_uri: None,
            local_path: None,
            tags: vec!["coding".into(), "shell".into()],
            detail: RegistryEntryDetail::Tool(ToolDetail {
                transport: ToolTransport::Builtin,
                capabilities: vec!["text_editor".into(), "shell".into()],
                env_keys: vec![],
            }),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("developer"));
        assert!(json.contains("tool"));

        let roundtrip: RegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "developer");
        assert_eq!(roundtrip.kind, RegistryEntryKind::Tool);
    }

    #[test]
    fn serialize_skill_entry() {
        let entry = RegistryEntry {
            name: "goose-doc-guide".into(),
            kind: RegistryEntryKind::Skill,
            description: "Guide for fetching goose documentation".into(),
            version: None,
            author: None,
            source_uri: None,
            local_path: Some(PathBuf::from(
                "/home/user/.config/goose/skills/doc-guide/SKILL.md",
            )),
            tags: vec!["documentation".into()],
            detail: RegistryEntryDetail::Skill(SkillDetail {
                content: "When the user asks about goose...".into(),
                builtin: true,
            }),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("goose-doc-guide"));
        assert!(json.contains("skill"));
    }

    #[test]
    fn serialize_agent_entry() {
        let entry = RegistryEntry {
            name: "code-reviewer".into(),
            kind: RegistryEntryKind::Agent,
            description: "Reviews code for quality and security".into(),
            version: Some("0.1.0".into()),
            author: Some(AuthorInfo {
                name: Some("Block".into()),
                contact: None,
                url: Some("https://block.xyz".into()),
            }),
            source_uri: None,
            local_path: None,
            tags: vec!["coding".into(), "review".into()],
            detail: RegistryEntryDetail::Agent(AgentDetail {
                instructions: "You are a code reviewer...".into(),
                model: Some("claude-sonnet-4".into()),
                capabilities: vec!["code-review".into()],
                domains: vec!["software-development".into()],
                input_content_types: vec!["text/plain".into()],
                output_content_types: vec!["text/markdown".into()],
            }),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("code-reviewer"));
        assert!(json.contains("agent"));
        assert!(json.contains("Block"));

        let roundtrip: RegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "code-reviewer");
    }

    #[test]
    fn serialize_recipe_entry() {
        let entry = RegistryEntry {
            name: "analyze-pr".into(),
            kind: RegistryEntryKind::Recipe,
            description: "Analyze a pull request".into(),
            version: Some("1.0.0".into()),
            author: Some(AuthorInfo {
                name: Some("Goose Team".into()),
                contact: None,
                url: None,
            }),
            source_uri: Some("https://github.com/block/goose/recipes/analyze-pr.yaml".into()),
            local_path: None,
            tags: vec!["github".into(), "code-review".into()],
            detail: RegistryEntryDetail::Recipe(RecipeDetail {
                instructions: Some("Analyze the given PR...".into()),
                prompt: Some("Please analyze PR #{{pr_number}}".into()),
                extension_names: vec!["developer".into(), "memory".into()],
                parameters: vec!["pr_number".into(), "repo".into()],
            }),
            metadata: HashMap::new(),
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("analyze-pr"));
        assert!(json.contains("recipe"));
    }

    #[test]
    fn merge_metadata_combines_entries() {
        let mut entry1 = RegistryEntry {
            name: "test".into(),
            kind: RegistryEntryKind::Tool,
            description: "test tool".into(),
            version: None,
            author: None,
            source_uri: None,
            local_path: None,
            tags: vec![],
            detail: RegistryEntryDetail::Tool(ToolDetail {
                transport: ToolTransport::Builtin,
                capabilities: vec![],
                env_keys: vec![],
            }),
            metadata: HashMap::new(),
        };

        let entry2 = RegistryEntry {
            name: "test".into(),
            kind: RegistryEntryKind::Tool,
            description: "test tool from remote".into(),
            version: Some("2.0.0".into()),
            author: Some(AuthorInfo {
                name: Some("Remote".into()),
                contact: None,
                url: None,
            }),
            source_uri: Some("https://example.com".into()),
            local_path: None,
            tags: vec![],
            detail: RegistryEntryDetail::Tool(ToolDetail {
                transport: ToolTransport::Builtin,
                capabilities: vec![],
                env_keys: vec![],
            }),
            metadata: {
                let mut m = HashMap::new();
                m.insert("rating".into(), "A".into());
                m
            },
        };

        entry1.merge_metadata(&entry2);
        assert_eq!(entry1.version, Some("2.0.0".into()));
        assert_eq!(entry1.author.unwrap().name, Some("Remote".into()));
        assert_eq!(entry1.metadata.get("rating"), Some(&"A".into()));
    }

    #[test]
    fn entry_kind_display() {
        assert_eq!(RegistryEntryKind::Tool.to_string(), "tool");
        assert_eq!(RegistryEntryKind::Skill.to_string(), "skill");
        assert_eq!(RegistryEntryKind::Agent.to_string(), "agent");
        assert_eq!(RegistryEntryKind::Recipe.to_string(), "recipe");
    }
}
