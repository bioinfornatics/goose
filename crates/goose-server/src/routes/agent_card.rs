use axum::{routing::get, Json, Router};
use goose::registry::formats::{generate_a2a_agent_card, A2aAgentCard};
use goose::registry::manifest::{
    AgentDetail, AgentSkill, RegistryEntry, RegistryEntryDetail, RegistryEntryKind,
};

use crate::routes::errors::ErrorResponse;

fn goose_registry_entry() -> RegistryEntry {
    RegistryEntry {
        name: "Goose".to_string(),
        kind: RegistryEntryKind::Agent,
        description: "An open-source AI agent by Block that automates engineering tasks"
            .to_string(),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        detail: RegistryEntryDetail::Agent(Box::new(AgentDetail {
            capabilities: vec![
                "Code Generation".to_string(),
                "Code Review".to_string(),
                "Shell Execution".to_string(),
                "File Editing".to_string(),
            ],
            domains: vec!["software-development".to_string(), "devops".to_string()],
            skills: vec![AgentSkill {
                id: "general-coding".to_string(),
                name: "General Coding".to_string(),
                description: Some(
                    "Write, review, refactor, and debug code across languages".to_string(),
                ),
                tags: Vec::new(),
                examples: Vec::new(),
            }],
            input_content_types: vec!["text/plain".to_string(), "image/png".to_string()],
            output_content_types: vec!["text/plain".to_string(), "application/json".to_string()],
            ..Default::default()
        })),
        ..Default::default()
    }
}

#[utoipa::path(
    get,
    path = "/.well-known/agent-card.json",
    responses(
        (status = 200, description = "A2A Agent Card for this Goose instance"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Discovery"
)]
pub async fn agent_card() -> Result<Json<A2aAgentCard>, ErrorResponse> {
    let entry = goose_registry_entry();
    let json_str = generate_a2a_agent_card(&entry, "")
        .map_err(|e| ErrorResponse::internal(format!("Failed to generate agent card: {e}")))?;
    let card: A2aAgentCard = serde_json::from_str(&json_str)
        .map_err(|e| ErrorResponse::internal(format!("Failed to parse agent card: {e}")))?;
    Ok(Json(card))
}

pub fn routes() -> Router {
    Router::new().route("/.well-known/agent-card.json", get(agent_card))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goose_registry_entry_is_agent() {
        let entry = goose_registry_entry();
        assert_eq!(entry.kind, RegistryEntryKind::Agent);
        assert_eq!(entry.name, "Goose");
        assert!(matches!(entry.detail, RegistryEntryDetail::Agent(_)));
    }

    #[test]
    fn test_agent_card_generation() {
        let entry = goose_registry_entry();
        let json = generate_a2a_agent_card(&entry, "https://localhost:3000").unwrap();
        let card: A2aAgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(card.name, "Goose");
        assert!(!card.skills.is_empty());
        assert!(!card.supported_interfaces.is_empty());
    }
}
