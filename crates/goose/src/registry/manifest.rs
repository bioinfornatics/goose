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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

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
        if self.license.is_none() {
            self.license.clone_from(&other.license);
        }
    }

    /// Check if this entry has enough metadata to be published to a registry.
    pub fn validate_for_publish(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.name.is_empty() {
            issues.push("name is required".into());
        }
        if self.description.is_empty() {
            issues.push("description is required".into());
        }
        if self.version.is_none() {
            issues.push("version is required for publishing".into());
        }
        if self.author.is_none() {
            issues.push("author is recommended for publishing".into());
        }
        if self.license.is_none() {
            issues.push("license is recommended for publishing".into());
        }

        if let RegistryEntryDetail::Agent(ref agent) = self.detail {
            if agent.instructions.is_empty() {
                issues.push("agent instructions are required".into());
            }
            if agent.capabilities.is_empty() {
                issues.push("at least one capability is recommended".into());
            }
        }

        issues
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

/// A dependency required by an agent or recipe.
///
/// Inspired by ACP Agent Manifest `dependencies` field.
/// Allows declaring what tools, skills, or other agents are needed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AgentDependency {
    /// Type of dependency: "tool", "skill", "agent", "recipe"
    #[serde(rename = "type")]
    pub dep_type: RegistryEntryKind,

    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Whether this dependency is required or optional
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Detail for an Agent definition.
///
/// Agents are markdown files with YAML frontmatter: name, description, model.
/// Schema aligned with ACP Agent Manifest for publishability:
/// - capabilities/domains for discovery
/// - dependencies for install resolution
/// - required_extensions for MCP server setup
/// - recommended_models for model flexibility
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct AgentDetail {
    pub instructions: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Multiple model recommendations (ACP-inspired).
    /// When non-empty, `model` is the primary and these are alternatives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_models: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub domains: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_content_types: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_content_types: Vec<String>,

    /// MCP extension names this agent requires (e.g., "developer", "memory").
    /// Used by `goose registry add` to auto-install dependencies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_extensions: Vec<String>,

    /// Structured dependencies on other registry artifacts.
    /// Enables dependency resolution during install.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<AgentDependency>,
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
            license: Some("Apache-2.0".into()),
            tags: vec!["coding".into(), "shell".into()],
            detail: RegistryEntryDetail::Tool(ToolDetail {
                transport: ToolTransport::Builtin,
                capabilities: vec!["text_editor".into(), "shell".into()],
                env_keys: vec![],
            }),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("developer"));
        assert!(json.contains("tool"));
        assert!(json.contains("Apache-2.0"));

        let roundtrip: RegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "developer");
        assert_eq!(roundtrip.kind, RegistryEntryKind::Tool);
        assert_eq!(roundtrip.license, Some("Apache-2.0".into()));
    }

    #[test]
    fn serialize_skill_entry() {
        let entry = RegistryEntry {
            name: "goose-doc-guide".into(),
            kind: RegistryEntryKind::Skill,
            description: "Guide for fetching goose documentation".into(),
            local_path: Some(PathBuf::from(
                "/home/user/.config/goose/skills/doc-guide/SKILL.md",
            )),
            tags: vec!["documentation".into()],
            detail: RegistryEntryDetail::Skill(SkillDetail {
                content: "When the user asks about goose...".into(),
                builtin: true,
            }),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("goose-doc-guide"));
        assert!(json.contains("skill"));
    }

    #[test]
    fn serialize_agent_entry_with_deps() {
        let entry = RegistryEntry {
            name: "code-reviewer".into(),
            kind: RegistryEntryKind::Agent,
            description: "Reviews code for quality and security".into(),
            version: Some("0.1.0".into()),
            license: Some("MIT".into()),
            author: Some(AuthorInfo {
                name: Some("Block".into()),
                contact: None,
                url: Some("https://block.xyz".into()),
            }),
            tags: vec!["coding".into(), "review".into()],
            detail: RegistryEntryDetail::Agent(AgentDetail {
                instructions: "You are a code reviewer...".into(),
                model: Some("claude-sonnet-4".into()),
                recommended_models: vec!["claude-sonnet-4".into(), "gpt-4o".into()],
                capabilities: vec!["code-review".into(), "security-audit".into()],
                domains: vec!["software-development".into()],
                input_content_types: vec!["text/plain".into()],
                output_content_types: vec!["text/markdown".into()],
                required_extensions: vec!["developer".into(), "memory".into()],
                dependencies: vec![
                    AgentDependency {
                        dep_type: RegistryEntryKind::Tool,
                        name: "developer".into(),
                        version: None,
                        required: true,
                    },
                    AgentDependency {
                        dep_type: RegistryEntryKind::Skill,
                        name: "goose-doc-guide".into(),
                        version: None,
                        required: false,
                    },
                ],
            }),
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(json.contains("code-reviewer"));
        assert!(json.contains("agent"));
        assert!(json.contains("Block"));
        assert!(json.contains("developer"));
        assert!(json.contains("recommended_models"));
        assert!(json.contains("required_extensions"));
        assert!(json.contains("dependencies"));

        let roundtrip: RegistryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "code-reviewer");
        assert_eq!(roundtrip.license, Some("MIT".into()));

        if let RegistryEntryDetail::Agent(ref detail) = roundtrip.detail {
            assert_eq!(detail.recommended_models.len(), 2);
            assert_eq!(detail.required_extensions, vec!["developer", "memory"]);
            assert_eq!(detail.dependencies.len(), 2);
            assert!(detail.dependencies[0].required);
            assert!(!detail.dependencies[1].required);
        } else {
            panic!("Expected AgentDetail");
        }
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
            tags: vec!["github".into(), "code-review".into()],
            detail: RegistryEntryDetail::Recipe(RecipeDetail {
                instructions: Some("Analyze the given PR...".into()),
                prompt: Some("Please analyze PR #{{pr_number}}".into()),
                extension_names: vec!["developer".into(), "memory".into()],
                parameters: vec!["pr_number".into(), "repo".into()],
            }),
            ..Default::default()
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
            detail: RegistryEntryDetail::Tool(ToolDetail {
                transport: ToolTransport::Builtin,
                capabilities: vec![],
                env_keys: vec![],
            }),
            ..Default::default()
        };

        let entry2 = RegistryEntry {
            name: "test".into(),
            kind: RegistryEntryKind::Tool,
            description: "test tool from remote".into(),
            version: Some("2.0.0".into()),
            license: Some("MIT".into()),
            author: Some(AuthorInfo {
                name: Some("Remote".into()),
                contact: None,
                url: None,
            }),
            source_uri: Some("https://example.com".into()),
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
            ..Default::default()
        };

        entry1.merge_metadata(&entry2);
        assert_eq!(entry1.version, Some("2.0.0".into()));
        assert_eq!(entry1.license, Some("MIT".into()));
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

    #[test]
    fn validate_for_publish_complete_agent() {
        let entry = RegistryEntry {
            name: "my-agent".into(),
            kind: RegistryEntryKind::Agent,
            description: "A useful agent".into(),
            version: Some("1.0.0".into()),
            license: Some("Apache-2.0".into()),
            author: Some(AuthorInfo {
                name: Some("Test".into()),
                contact: None,
                url: None,
            }),
            detail: RegistryEntryDetail::Agent(AgentDetail {
                instructions: "You are a helpful agent.".into(),
                model: Some("claude-sonnet-4".into()),
                recommended_models: vec!["claude-sonnet-4".into()],
                capabilities: vec!["general".into()],
                domains: vec![],
                input_content_types: vec!["text/plain".into()],
                output_content_types: vec!["text/markdown".into()],
                required_extensions: vec!["developer".into()],
                dependencies: vec![AgentDependency {
                    dep_type: RegistryEntryKind::Tool,
                    name: "developer".into(),
                    version: None,
                    required: true,
                }],
            }),
            ..Default::default()
        };

        let issues = entry.validate_for_publish();
        assert!(
            issues.is_empty(),
            "Expected no issues but got: {:?}",
            issues
        );
    }

    #[test]
    fn validate_for_publish_incomplete_agent() {
        let entry = RegistryEntry {
            name: String::new(),
            kind: RegistryEntryKind::Agent,
            description: String::new(),
            detail: RegistryEntryDetail::Agent(AgentDetail {
                instructions: String::new(),
                model: None,
                recommended_models: vec![],
                capabilities: vec![],
                domains: vec![],
                input_content_types: vec![],
                output_content_types: vec![],
                required_extensions: vec![],
                dependencies: vec![],
            }),
            ..Default::default()
        };

        let issues = entry.validate_for_publish();
        assert!(
            issues.len() >= 4,
            "Expected at least 4 issues: {:?}",
            issues
        );
        assert!(issues.iter().any(|i| i.contains("name")));
        assert!(issues.iter().any(|i| i.contains("description")));
        assert!(issues.iter().any(|i| i.contains("version")));
        assert!(issues.iter().any(|i| i.contains("instructions")));
    }

    #[test]
    fn agent_dependency_serialization() {
        let dep = AgentDependency {
            dep_type: RegistryEntryKind::Tool,
            name: "developer".into(),
            version: Some("1.0.0".into()),
            required: true,
        };

        let json = serde_json::to_string_pretty(&dep).unwrap();
        assert!(json.contains("\"type\": \"tool\""));
        assert!(json.contains("developer"));

        let roundtrip: AgentDependency = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.dep_type, RegistryEntryKind::Tool);
        assert_eq!(roundtrip.name, "developer");
        assert!(roundtrip.required);
    }
}
