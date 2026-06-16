use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::{AuthError, AzureAuth, AzureCredentials};
use super::base::{ConfigKey, ProviderDef, ProviderMetadata};
use super::openai_compatible::OpenAiCompatibleProvider;
use crate::model::ModelConfig;

const AZURE_FOUNDRY_PROVIDER_NAME: &str = "azure_foundry";
const AZURE_FOUNDRY_DEFAULT_MODEL: &str = "Phi-4";
const AZURE_FOUNDRY_DOC_URL: &str = "https://learn.microsoft.com/en-us/azure/ai-foundry/foundry-models/how-to/deploy-models-serverless";

/// Entra ID resource scope for Azure AI Foundry serverless (MaaS) endpoints.
/// `.models.ai.azure.com` endpoints use the ML workspace scope, not Cognitive Services.
const AZURE_FOUNDRY_ENTRA_RESOURCE: &str = "https://ml.azure.com";

pub const AZURE_FOUNDRY_KNOWN_MODELS: &[&str] = &[
    "Phi-4",
    "Phi-4-mini",
    "Mistral-large-2411",
    "Mistral-small-2501",
    "Meta-Llama-3.1-70B-Instruct",
    "Meta-Llama-3.1-405B-Instruct",
    "Meta-Llama-3.3-70B-Instruct",
    "Cohere-command-r-plus-08-2024",
    "AI21-Jamba-1.5-Large",
];

pub struct AzureFoundryProvider;

struct AzureFoundryAuthProvider {
    auth: AzureAuth,
}

#[async_trait]
impl AuthProvider for AzureFoundryAuthProvider {
    async fn get_auth_header(&self) -> Result<(String, String)> {
        let auth_token = self
            .auth
            .get_token()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get authentication token: {}", e))?;

        match self.auth.credential_type() {
            AzureCredentials::ApiKey(_) => Ok(("api-key".to_string(), auth_token.token_value)),
            AzureCredentials::DefaultCredential => Ok((
                "Authorization".to_string(),
                format!("Bearer {}", auth_token.token_value),
            )),
        }
    }
}

impl ProviderDef for AzureFoundryProvider {
    type Provider = OpenAiCompatibleProvider;

    fn metadata() -> ProviderMetadata {
        ProviderMetadata::new(
            AZURE_FOUNDRY_PROVIDER_NAME,
            "Azure AI Foundry",
            "Models through Azure AI Foundry serverless endpoints (MaaS)",
            AZURE_FOUNDRY_DEFAULT_MODEL,
            AZURE_FOUNDRY_KNOWN_MODELS.to_vec(),
            AZURE_FOUNDRY_DOC_URL,
            vec![
                ConfigKey::new("AZURE_FOUNDRY_ENDPOINT", true, false, None, true),
                ConfigKey::new("AZURE_FOUNDRY_DEPLOYMENT", false, false, None, false),
                ConfigKey::new("AZURE_FOUNDRY_API_KEY", false, true, Some(""), true),
                ConfigKey::new("AZURE_FOUNDRY_API_VERSION", false, false, None, false),
            ],
        )
    }

    fn from_env(
        model: ModelConfig,
        _extensions: Vec<crate::config::ExtensionConfig>,
    ) -> BoxFuture<'static, Result<Self::Provider>> {
        Box::pin(async move {
            let config = crate::config::Config::global();
            let endpoint: String = config.get_param("AZURE_FOUNDRY_ENDPOINT")?;
            let deployment: Option<String> = config.get_param("AZURE_FOUNDRY_DEPLOYMENT").ok();
            let api_version: Option<String> = config.get_param("AZURE_FOUNDRY_API_VERSION").ok();

            let api_key = config
                .get_secret("AZURE_FOUNDRY_API_KEY")
                .ok()
                .filter(|key: &String| !key.is_empty());

            let auth =
                AzureAuth::new_with_resource(api_key, AZURE_FOUNDRY_ENTRA_RESOURCE.to_string())
                    .map_err(|e| match e {
                        AuthError::Credentials(msg) => {
                            anyhow::anyhow!("Credentials error: {}", msg)
                        }
                        AuthError::TokenExchange(msg) => {
                            anyhow::anyhow!("Token exchange error: {}", msg)
                        }
                    })?;

            let auth_provider = AzureFoundryAuthProvider { auth };
            let host = endpoint.trim_end_matches('/').to_string();
            let mut api_client = ApiClient::new(host, AuthMethod::Custom(Box::new(auth_provider)))?;
            if let Some(version) = api_version {
                api_client = api_client.with_query(vec![("api-version".to_string(), version)]);
            }

            // When AZURE_FOUNDRY_DEPLOYMENT is set, use it as the model name sent in the
            // request body. This lets users alias the endpoint without renaming their model.
            let effective_model = match deployment {
                Some(dep) if !dep.is_empty() => ModelConfig {
                    model_name: dep,
                    ..model
                },
                _ => model,
            };

            Ok(OpenAiCompatibleProvider::new(
                AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
                api_client,
                effective_model,
                // MaaS endpoints expose /chat/completions at the root — no deployment prefix.
                String::new(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_name() {
        let meta = AzureFoundryProvider::metadata();
        assert_eq!(meta.name, "azure_foundry");
        assert_eq!(meta.display_name, "Azure AI Foundry");
        assert_eq!(meta.default_model, "Phi-4");
    }

    #[test]
    fn test_config_keys() {
        let meta = AzureFoundryProvider::metadata();
        let key_names: Vec<&str> = meta.config_keys.iter().map(|k| k.name.as_str()).collect();
        assert!(key_names.contains(&"AZURE_FOUNDRY_ENDPOINT"));
        assert!(key_names.contains(&"AZURE_FOUNDRY_API_KEY"));

        let endpoint_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "AZURE_FOUNDRY_ENDPOINT")
            .unwrap();
        assert!(endpoint_key.required);
        assert!(!endpoint_key.secret);

        let api_key = meta
            .config_keys
            .iter()
            .find(|k| k.name == "AZURE_FOUNDRY_API_KEY")
            .unwrap();
        assert!(!api_key.required);
        assert!(api_key.secret);
    }
}
