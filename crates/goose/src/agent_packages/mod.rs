use crate::config::extensions::name_to_key;
use crate::config::paths::Paths;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentPackageManifestV1 {
    pub schema_version: u32,
    pub agent_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_mode: Option<String>,
    #[serde(default)]
    pub modes: Vec<ModeRefV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeRefV1 {
    pub mode_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeManifestV1 {
    pub schema_version: u32,
    pub mode_id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub instructions: ModeInstructionsV1,
    #[serde(default)]
    pub extensions: Option<ModeExtensionsV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ModeInstructionsV1 {
    Inline(String),
    File { file: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeExtensionsV1 {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPackageDraft {
    pub display_name: String,
    pub description: Option<String>,
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeDraft {
    pub display_name: String,
    pub description: Option<String>,
    pub instructions_md: String,
    pub extensions_allow: Vec<String>,
    pub extensions_deny: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedMode {
    pub agent_id: String,
    pub mode_id: String,
    pub mode_dir: PathBuf,
    pub mode_manifest_path: PathBuf,
    pub instructions_path: PathBuf,
}

pub struct AgentPackageStore {
    agents_root: PathBuf,
}

impl Default for AgentPackageStore {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentPackageStore {
    pub fn new() -> Self {
        Self {
            agents_root: Paths::in_config_dir("agents"),
        }
    }

    pub fn agents_root(&self) -> &Path {
        &self.agents_root
    }

    pub fn agent_dir(&self, agent_id: &str) -> PathBuf {
        self.agents_root.join(agent_id)
    }

    pub fn manifest_path(&self, agent_id: &str) -> PathBuf {
        self.agent_dir(agent_id).join("package.yaml")
    }

    pub fn list(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let dir = &self.agents_root;
        if !dir.exists() {
            return Ok(out);
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry
                .file_name()
                .to_str()
                .ok_or_else(|| anyhow!("Non-utf8 agent directory name"))?
                .to_string();

            if self.manifest_path(&name).is_file() {
                out.push(name);
            }
        }

        out.sort();
        Ok(out)
    }

    pub fn load_manifest(&self, agent_id: &str) -> Result<AgentPackageManifestV1> {
        let manifest_path = self.manifest_path(agent_id);
        let raw = fs::read_to_string(&manifest_path)?;
        let manifest: AgentPackageManifestV1 = serde_yaml::from_str(&raw)?;

        if manifest.schema_version != 1 {
            bail!("Unsupported schema_version {}", manifest.schema_version);
        }

        if manifest.agent_id != agent_id {
            bail!(
                "agent_id mismatch: manifest has '{}', directory is '{}'",
                manifest.agent_id,
                agent_id
            );
        }

        Ok(manifest)
    }

    pub fn init_agent_package(&self, agent_id: &str, draft: AgentPackageDraft) -> Result<()> {
        if agent_id.is_empty() {
            bail!("agent_id must not be empty");
        }

        let canonical = name_to_key(agent_id);
        if canonical != agent_id {
            bail!(
                "Invalid agent_id '{}'. Canonical form would be '{}'",
                agent_id,
                canonical
            );
        }

        let dir = self.agent_dir(agent_id);
        fs::create_dir_all(&dir)?;

        let manifest_path = self.manifest_path(agent_id);
        if manifest_path.exists() {
            bail!("Agent package already exists: {}", manifest_path.display());
        }

        let manifest = AgentPackageManifestV1 {
            schema_version: 1,
            agent_id: agent_id.to_string(),
            display_name: draft.display_name,
            description: draft.description,
            default_mode: draft.default_mode,
            modes: Vec::new(),
        };

        write_yaml_atomic(&manifest_path, &manifest)?;
        Ok(())
    }

    pub fn create_mode(&self, agent_id: &str, draft: ModeDraft) -> Result<CreatedMode> {
        let mut manifest = self.load_manifest(agent_id)?;

        let mode_id = name_to_key(&draft.display_name);
        if mode_id.is_empty() {
            bail!("Mode display_name yields empty mode_id");
        }

        if manifest.modes.iter().any(|m| m.mode_id == mode_id) {
            bail!("Mode already exists: {}", mode_id);
        }

        let mode_dir_rel = PathBuf::from("modes").join(&mode_id);
        let mode_dir = self.agent_dir(agent_id).join(&mode_dir_rel);
        fs::create_dir_all(&mode_dir)?;

        let instructions_path = mode_dir.join("instructions.md");
        write_text_atomic(&instructions_path, &draft.instructions_md)?;

        let mode_manifest_path = mode_dir.join("mode.yaml");

        let extensions = if draft.extensions_allow.is_empty() && draft.extensions_deny.is_empty() {
            None
        } else {
            Some(ModeExtensionsV1 {
                allow: draft.extensions_allow,
                deny: draft.extensions_deny,
            })
        };

        let mode_manifest = ModeManifestV1 {
            schema_version: 1,
            mode_id: mode_id.clone(),
            display_name: draft.display_name,
            description: draft.description,
            instructions: ModeInstructionsV1::File {
                file: "instructions.md".to_string(),
            },
            extensions,
        };

        write_yaml_atomic(&mode_manifest_path, &mode_manifest)?;

        let mode_path_rel = mode_dir_rel.join("mode.yaml");
        let mode_ref = ModeRefV1 {
            mode_id: mode_id.clone(),
            path: mode_path_rel
                .to_str()
                .ok_or_else(|| anyhow!("Non-utf8 mode path"))?
                .to_string(),
        };
        manifest.modes.push(mode_ref);

        let manifest_path = self.manifest_path(agent_id);
        write_yaml_atomic(&manifest_path, &manifest)?;

        Ok(CreatedMode {
            agent_id: agent_id.to_string(),
            mode_id,
            mode_dir,
            mode_manifest_path,
            instructions_path,
        })
    }
}

fn write_yaml_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let s = serde_yaml::to_string(value)?;
    write_text_atomic(path, &s)
}

fn write_text_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| anyhow!("Path has no parent: {}", path.display()))?;
    fs::create_dir_all(dir)?;

    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(contents.as_bytes())?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| anyhow!(e))?;
    Ok(())
}
