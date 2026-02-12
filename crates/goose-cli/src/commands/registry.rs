use anyhow::Result;
use console::style;
use goose::registry::manifest::{RegistryEntry, RegistryEntryKind};
use goose::registry::sources::local::LocalRegistrySource;
use goose::registry::RegistryManager;

fn kind_from_str(s: &str) -> Option<RegistryEntryKind> {
    match s.to_lowercase().as_str() {
        "tool" | "tools" => Some(RegistryEntryKind::Tool),
        "skill" | "skills" => Some(RegistryEntryKind::Skill),
        "agent" | "agents" => Some(RegistryEntryKind::Agent),
        "recipe" | "recipes" => Some(RegistryEntryKind::Recipe),
        _ => None,
    }
}

fn default_manager() -> Result<RegistryManager> {
    let mut manager = RegistryManager::new();
    let local = LocalRegistrySource::from_default_paths()?;
    manager.add_source(Box::new(local));
    Ok(manager)
}

fn print_entry(entry: &RegistryEntry, verbose: bool) {
    let kind_icon = match entry.kind {
        RegistryEntryKind::Tool => "\u{1f527}",
        RegistryEntryKind::Skill => "\u{1f4dd}",
        RegistryEntryKind::Agent => "\u{1f916}",
        RegistryEntryKind::Recipe => "\u{1f4e6}",
    };

    println!(
        "  {} {} {}",
        kind_icon,
        style(&entry.name).bold(),
        style(format!("{:?}", entry.kind)).dim()
    );

    if !entry.description.is_empty() {
        println!("    {}", entry.description);
    }

    if verbose {
        if let Some(version) = &entry.version {
            println!("    Version: {}", version);
        }
        if let Some(author) = &entry.author {
            if let Some(name) = &author.name {
                println!("    Author: {}", name);
            }
        }
        if let Some(uri) = &entry.source_uri {
            println!("    Source: {}", uri);
        }
        if !entry.tags.is_empty() {
            println!("    Tags: {}", entry.tags.join(", "));
        }
    }
}

fn print_entries_json(entries: &[RegistryEntry]) -> Result<()> {
    let json = serde_json::to_string_pretty(entries)?;
    println!("{}", json);
    Ok(())
}

pub async fn handle_search(
    query: &str,
    kind: Option<&str>,
    format: &str,
    verbose: bool,
) -> Result<()> {
    let manager = default_manager()?;
    let kind_filter = kind.and_then(kind_from_str);
    let results = manager.search(Some(query), kind_filter).await?;

    if format == "json" {
        return print_entries_json(&results);
    }

    if results.is_empty() {
        println!("{}", style("No entries found.").yellow());
        return Ok(());
    }

    println!(
        "{}",
        style(format!("Found {} entries:", results.len())).green()
    );
    println!();
    for entry in &results {
        print_entry(entry, verbose);
    }

    Ok(())
}

pub async fn handle_list(kind: Option<&str>, format: &str, verbose: bool) -> Result<()> {
    let manager = default_manager()?;
    let kind_filter = kind.and_then(kind_from_str);
    let results = manager.list(kind_filter).await?;

    if format == "json" {
        return print_entries_json(&results);
    }

    if results.is_empty() {
        println!("{}", style("Registry is empty.").yellow());
        return Ok(());
    }

    println!(
        "{}",
        style(format!("{} entries in registry:", results.len())).green()
    );
    println!();
    for entry in &results {
        print_entry(entry, verbose);
    }

    Ok(())
}

pub async fn handle_info(name: &str, kind: Option<&str>) -> Result<()> {
    let manager = default_manager()?;
    let kind_filter = kind.and_then(kind_from_str);
    let entry = manager.get(name, kind_filter).await?;

    match entry {
        Some(e) => {
            println!("{}", style(format!("Registry Entry: {}", e.name)).bold());
            println!("  Kind: {:?}", e.kind);
            if !e.description.is_empty() {
                println!("  Description: {}", e.description);
            }
            if let Some(version) = &e.version {
                println!("  Version: {}", version);
            }
            if let Some(author) = &e.author {
                if let Some(name) = &author.name {
                    println!("  Author: {}", name);
                }
                if let Some(contact) = &author.contact {
                    println!("  Contact: {}", contact);
                }
            }
            if let Some(uri) = &e.source_uri {
                println!("  Source: {}", uri);
            }
            if let Some(path) = &e.local_path {
                println!("  Local path: {}", path.display());
            }
            if !e.tags.is_empty() {
                println!("  Tags: {}", e.tags.join(", "));
            }
            println!();
            println!("  Detail: {:?}", e.detail);
            Ok(())
        }
        None => {
            println!(
                "{}",
                style(format!("Entry '{}' not found in registry.", name)).red()
            );
            Ok(())
        }
    }
}

pub async fn handle_sources() -> Result<()> {
    let manager = default_manager()?;
    let sources = manager.source_names();

    println!("{}", style("Configured registry sources:").bold());
    println!();
    for (i, name) in sources.iter().enumerate() {
        println!("  {}. {}", i + 1, style(name).cyan());
    }

    Ok(())
}

pub async fn handle_add(name: &str, kind_str: Option<&str>) -> Result<()> {
    use goose::registry::install::{install_entry, is_installed};

    let kind = kind_str.and_then(kind_from_str);
    let manager = default_manager()?;

    // Search for the entry
    let entries = manager.search(Some(name), kind).await?;
    let entry = entries.into_iter().find(|e| e.name == name);

    match entry {
        Some(entry) => {
            if is_installed(&entry.name, entry.kind) {
                println!(
                    "{} {} is already installed",
                    style("✓").green(),
                    style(&entry.name).cyan()
                );
                return Ok(());
            }

            let path = install_entry(&entry)?;
            println!(
                "{} Installed {} ({}) to {}",
                style("✓").green(),
                style(&entry.name).cyan(),
                style(format!("{}", entry.kind)).dim(),
                style(path.display()).dim()
            );
            Ok(())
        }
        None => {
            println!("{} No entry found matching '{}'", style("✗").red(), name);
            if kind_str.is_some() {
                println!("  Try without --kind to search across all types");
            }
            Ok(())
        }
    }
}

pub async fn handle_remove(name: &str, kind_str: &str) -> Result<()> {
    use goose::registry::install::{is_installed, remove_entry};

    let kind = kind_from_str(kind_str).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown kind '{}'. Use: tool, skill, agent, or recipe",
            kind_str
        )
    })?;

    if !is_installed(name, kind) {
        println!(
            "{} {} ({}) is not installed",
            style("✗").yellow(),
            style(name).cyan(),
            kind_str,
        );
        return Ok(());
    }

    remove_entry(name, kind)?;
    println!(
        "{} Removed {} ({})",
        style("✓").green(),
        style(name).cyan(),
        kind_str,
    );
    Ok(())
}

pub async fn handle_validate(path: &str) -> Result<()> {
    use goose::registry::publish::validate_for_publish;
    use std::path::Path;

    let manifest_path = Path::new(path);
    if !manifest_path.exists() {
        anyhow::bail!("File not found: {}", path);
    }

    match validate_for_publish(manifest_path) {
        Ok(issues) => {
            if issues.is_empty() {
                println!("{} Manifest is valid for publishing!", style("✓").green());
            } else {
                println!("{} Manifest has issues:", style("⚠").yellow());
                for issue in &issues {
                    println!("  {} {}", style("•").yellow(), issue);
                }
            }
            Ok(())
        }
        Err(e) => {
            println!("{} Failed to validate manifest: {}", style("✗").red(), e);
            Ok(())
        }
    }
}

pub async fn handle_init(name: Option<String>, description: Option<String>) -> Result<()> {
    use goose::registry::publish::init_manifest;

    let agent_name = name.unwrap_or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "my-agent".to_string())
    });

    let desc = description.unwrap_or_else(|| format!("A goose agent: {}", agent_name));

    let dir = std::env::current_dir()?;
    let path = init_manifest(&dir, &agent_name, &desc)?;

    println!(
        "{} Created manifest: {}",
        style("✓").green(),
        style(path.display()).cyan()
    );
    println!();
    println!("  Edit the manifest to configure your agent, then validate with:");
    println!("  {}", style("goose registry validate agent.yaml").dim());

    Ok(())
}
