/// Thin Azure AI Foundry provider — chat/completions surface.
///
/// Follows the same pattern as `azure.rs`: a small `ProviderDef` that builds
/// an [`OpenAiCompatibleProvider`] with Azure auth, no bespoke routing.
///
/// **Coverage**: Phi-4, Llama, Mistral, Cohere, AI21, GLM, DeepSeek — any
/// deployment whose wire format is OpenAI-compatible chat/completions.
///
/// **Not covered here**:
/// * Claude deployments → Anthropic Messages API format (separate provider)
/// * GPT-5 / o-series → OpenAI Responses API format (separate provider)
///
/// The project endpoint path `/openai/v1/chat/completions` is used when the
/// endpoint contains `/api/projects/` (Azure AI Foundry hub project).
/// MaaS endpoints (`/models`) use `/chat/completions` at the root.
///
/// Auth: `AZURE_FOUNDRY_API_KEY` (static key) or Entra ID via `az login`
/// (DefaultCredential, scope `https://ml.azure.com` for MaaS,
/// `https://ai.azure.com` for project endpoints).
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use goose_providers::model::ModelConfig;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::AzureAuth;
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;

pub const AZURE_FOUNDRY_PROVIDER_NAME: &str = "azure_foundry";
const AZURE_FOUNDRY_DEFAULT_MODEL: &str = "Phi-4";
const AZURE_FOUNDRY_DOC_URL: &str = "https://ai.azure.com/explore/models";

/// Entra ID scope for Azure AI Foundry project endpoints.
const AZURE_PROJECT_ENTRA_RESOURCE: &str = "https://ai.azure.com";
/// Entra ID scope for MaaS (serverless) endpoints.
const AZURE_MAAS_ENTRA_RESOURCE: &str = "https://ml.azure.com";

/// Known models — static fallback list shown before any live discovery.
const AZURE_FOUNDRY_KNOWN_MODELS: &[&str] = &[
    "Phi-4",
    "Phi-4-mini",
    "Mistral-large-2411",
    "Mistral-small-2501",
    "Meta-Llama-3.1-70B-Instruct",
    "Meta-Llama-3.1-405B-Instruct",
    "Meta-Llama-3.3-70B-Instruct",
    "Cohere-command-r-plus-08-2024",
    "AI21-Jamba-1.5-Large",
    "glm-4.7",
    "glm-4.5",
    "glm-5",
    "DeepSeek-R1",
    "DeepSeek-V3",
];

/// Returns `true` when the endpoint is an Azure AI Foundry project endpoint
/// (`/api/projects/` in the URL path) rather than a MaaS model endpoint.
fn is_project_endpoint(url: &str) -> bool {
    url.contains("/api/projects/")
}

/// Custom auth provider that injects Azure credentials into every request.
struct AzureFoundryAuthProvider {
    auth: AzureAuth,
}

#[async_trait]
impl AuthProvider for AzureFoundryAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        self.auth
            .auth_header()
            .await
            .map_err(|e| anyhow::anyhow!("Azure Foundry auth failed: {}", e))
    }
}

/// Marker struct — the actual provider returned by `from_env` is
/// [`OpenAiCompatibleProvider`], same as `azure.rs`.
pub struct AzureFoundryProvider;

impl ProviderDef for AzureFoundryProvider {
    type Provider = OpenAiCompatibleProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AZURE_FOUNDRY_PROVIDER_NAME,
            "Azure AI Foundry",
            "Models through Azure AI Foundry serverless endpoints — Phi, Llama, Mistral, \
             Cohere, GLM, DeepSeek and other chat/completions-compatible publishers. \
             (For Claude deployments use the Azure Foundry Anthropic provider; \
             for GPT-5/o-series use the Azure Foundry Responses provider.)",
            AZURE_FOUNDRY_DEFAULT_MODEL,
            AZURE_FOUNDRY_KNOWN_MODELS.to_vec(),
            AZURE_FOUNDRY_DOC_URL,
            vec![
                ConfigKey::new("AZURE_FOUNDRY_ENDPOINT", true, false, None, true),
                ConfigKey::new("AZURE_FOUNDRY_API_KEY", false, true, Some(""), true),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
        tls_config: Option<crate::providers::api_client::TlsConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let endpoint: String = config.get_param("AZURE_FOUNDRY_ENDPOINT")?;

            let api_key = config
                .get_secret("AZURE_FOUNDRY_API_KEY")
                .ok()
                .filter(|k: &String| !k.is_empty());

            // Select the Entra ID resource scope that matches the endpoint type.
            // Project endpoints (…/api/projects/…) → ai.azure.com
            // MaaS endpoints (…/models) → ml.azure.com
            let entra_resource = if is_project_endpoint(&endpoint) {
                AZURE_PROJECT_ENTRA_RESOURCE
            } else {
                AZURE_MAAS_ENTRA_RESOURCE
            };

            let auth = AzureAuth::new_with_resource(api_key, entra_resource.to_string())
                .map_err(anyhow::Error::from)?;

            let auth_provider = AzureFoundryAuthProvider { auth };

            // Project endpoints expose chat/completions at /openai/v1/chat/completions.
            // MaaS endpoints expose it at /chat/completions (no path prefix needed).
            let (host, prefix) = if is_project_endpoint(&endpoint) {
                (
                    endpoint.trim_end_matches('/').to_string(),
                    "openai/v1/".to_string(),
                )
            } else {
                (endpoint.trim_end_matches('/').to_string(), String::new())
            };

            let api_client = ApiClient::new_with_tls(
                host,
                AuthMethod::Custom(Box::new(auth_provider)),
                tls_config,
            )?;

            Ok(OpenAiCompatibleProvider::new(
                AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
                api_client,
                model,
                prefix,
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_endpoint_detected() {
        assert!(is_project_endpoint(
            "https://hub.services.ai.azure.com/api/projects/my-proj"
        ));
        assert!(!is_project_endpoint(
            "https://hub.services.ai.azure.com/models"
        ));
    }

    #[test]
    fn project_endpoint_chat_path_has_slash() {
        // Regression: "openai/v1" (missing /) → "openai/v1chat/completions"
        let prefix = "openai/v1/";
        assert_eq!(
            format!("{}chat/completions", prefix),
            "openai/v1/chat/completions"
        );
    }

    #[test]
    fn metadata_name() {
        assert_eq!(AzureFoundryProvider::metadata().name, "azure_foundry");
    }

    #[test]
    fn metadata_config_keys() {
        let keys = AzureFoundryProvider::metadata().config_keys;
        assert!(keys
            .iter()
            .any(|k| k.name == "AZURE_FOUNDRY_ENDPOINT" && k.required));
        assert!(keys
            .iter()
            .any(|k| k.name == "AZURE_FOUNDRY_API_KEY" && !k.required));
    }
}
