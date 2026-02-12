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
