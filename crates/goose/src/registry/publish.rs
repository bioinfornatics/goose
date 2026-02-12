use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::recipe::Recipe;
use crate::registry::manifest::{
    AgentDetail, AuthorInfo, RecipeDetail, RegistryEntry, RegistryEntryDetail, RegistryEntryKind,
};

/// Generate a RegistryEntry from a Recipe
pub fn recipe_to_registry_entry(recipe: &Recipe) -> RegistryEntry {
    let extension_names: Vec<String> = recipe
        .extensions
        .as_ref()
        .map(|exts| exts.iter().map(|ext| ext.name()).collect())
        .unwrap_or_default();

    let parameters: Vec<String> = recipe
        .parameters
        .as_ref()
        .map(|params| params.iter().map(|p| p.key.clone()).collect())
        .unwrap_or_default();

    let author = recipe.author.as_ref().map(|a| AuthorInfo {
        name: a.contact.clone(),
        contact: a.metadata.clone(),
        url: None,
    });

    RegistryEntry {
        name: recipe.title.clone(),
        kind: RegistryEntryKind::Recipe,
        description: recipe.description.clone(),
        version: Some(recipe.version.clone()),
        author,
        tags: Vec::new(),
        detail: RegistryEntryDetail::Recipe(RecipeDetail {
            instructions: recipe.instructions.clone(),
            prompt: recipe.prompt.clone(),
            extension_names,
            parameters,
        }),
        ..Default::default()
    }
}

/// Generate an agent manifest RegistryEntry from project metadata
pub fn generate_agent_manifest(name: &str, description: &str) -> RegistryEntry {
    RegistryEntry {
        name: name.to_string(),
        kind: RegistryEntryKind::Agent,
        description: description.to_string(),
        version: Some("0.1.0".to_string()),
        detail: RegistryEntryDetail::Agent(AgentDetail {
            instructions: String::new(),
            model: None,
            capabilities: Vec::new(),
            domains: Vec::new(),
            input_content_types: Vec::new(),
            output_content_types: Vec::new(),
        }),
        ..Default::default()
    }
}

/// Validate a manifest file at the given path
pub fn validate_manifest(path: &Path) -> Result<RegistryEntry> {
    let content = std::fs::read_to_string(path)?;

    let entry: RegistryEntry = if path.extension().is_some_and(|e| e == "json") {
        serde_json::from_str(&content)?
    } else {
        serde_yaml::from_str(&content)?
    };

    if entry.name.is_empty() {
        bail!("Manifest name is required");
    }

    Ok(entry)
}

/// Write a manifest to disk
pub fn write_manifest(entry: &RegistryEntry, path: &Path) -> Result<PathBuf> {
    let content = if path.extension().is_some_and(|e| e == "json") {
        serde_json::to_string_pretty(entry)?
    } else {
        serde_yaml::to_string(entry)?
    };

    std::fs::write(path, &content)?;
    Ok(path.to_path_buf())
}

/// Initialize a manifest in the current directory
pub fn init_manifest(dir: &Path, name: &str, description: &str) -> Result<PathBuf> {
    let manifest_path = dir.join("agent.yaml");
    if manifest_path.exists() {
        bail!("agent.yaml already exists in {}", dir.display());
    }

    let entry = generate_agent_manifest(name, description);
    write_manifest(&entry, &manifest_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::extension::ExtensionConfig;
    use crate::recipe::Recipe;

    #[test]
    fn test_recipe_to_registry_entry() {
        let recipe = Recipe {
            title: "Test Recipe".to_string(),
            description: "A test recipe".to_string(),
            version: "1.0.0".to_string(),
            extensions: Some(vec![ExtensionConfig::Builtin {
                name: "developer".to_string(),
                display_name: None,
                description: String::new(),
                timeout: None,
                bundled: None,
                available_tools: Vec::new(),
            }]),
            instructions: None,
            prompt: None,
            settings: None,
            activities: None,
            author: None,
            parameters: None,
            response: None,
            sub_recipes: None,
            retry: None,
        };

        let entry = recipe_to_registry_entry(&recipe);
        assert_eq!(entry.name, "Test Recipe");
        assert_eq!(entry.kind, RegistryEntryKind::Recipe);
        assert_eq!(entry.description, "A test recipe");
        assert_eq!(entry.version, Some("1.0.0".to_string()));

        if let RegistryEntryDetail::Recipe(detail) = &entry.detail {
            assert_eq!(detail.extension_names, vec!["developer"]);
        } else {
            panic!("Expected RecipeDetail");
        }
    }

    #[test]
    fn test_generate_agent_manifest() {
        let entry = generate_agent_manifest("my-agent", "Does things");
        assert_eq!(entry.name, "my-agent");
        assert_eq!(entry.kind, RegistryEntryKind::Agent);
        assert_eq!(entry.description, "Does things");
    }

    #[test]
    fn test_validate_manifest_roundtrip() {
        let entry = generate_agent_manifest("test", "A test agent");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.yaml");

        write_manifest(&entry, &path).unwrap();
        let loaded = validate_manifest(&path).unwrap();

        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.kind, RegistryEntryKind::Agent);
    }

    #[test]
    fn test_init_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = init_manifest(dir.path(), "my-project", "My project agent").unwrap();

        assert!(path.exists());
        let entry = validate_manifest(&path).unwrap();
        assert_eq!(entry.name, "my-project");
    }

    #[test]
    fn test_init_manifest_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        init_manifest(dir.path(), "first", "First").unwrap();
        let result = init_manifest(dir.path(), "second", "Second");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_empty_name_fails() {
        let entry = RegistryEntry {
            name: String::new(),
            kind: RegistryEntryKind::Agent,
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.yaml");
        write_manifest(&entry, &path).unwrap();
        let result = validate_manifest(&path);
        assert!(result.is_err());
    }
}
