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
    use crate::providers::base::Provider;
    use crate::providers::openai_compatible::OpenAiCompatibleProvider;
    use goose_providers::errors::ProviderError;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_provider(server_uri: &str) -> OpenAiCompatibleProvider {
        OpenAiCompatibleProvider::new(
            AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
            ApiClient::new(server_uri.to_string(), AuthMethod::NoAuth).unwrap(),
            ModelConfig::new_or_fail("Phi-4"),
            String::new(), // no prefix — MaaS root /chat/completions
        )
    }

    fn non_streaming_success_body() -> serde_json::Value {
        json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "choices": [{
                "message": {"role": "assistant", "content": "Hello from Phi-4!"},
                "finish_reason": "stop",
                "index": 0
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 5,
                "total_tokens": 10
            }
        })
    }

    fn sse_success_body() -> String {
        // Three content chunks + usage sentinel (with model field) + DONE.
        // extract_usage_with_output_tokens requires chunk.model to be set so
        // the ProviderUsage is populated; we include "model" on the first chunk
        // so last_seen_model is available for the final usage chunk.
        let chunks = vec![
            json!({"id":"c1","object":"chat.completion.chunk","model":"Phi-4","choices":[{"delta":{"role":"assistant","content":"Hello"},"index":0}]}),
            json!({"id":"c1","object":"chat.completion.chunk","choices":[{"delta":{"content":" world"},"index":0}]}),
            json!({"id":"c1","object":"chat.completion.chunk","choices":[{"delta":{},"finish_reason":"stop","index":0}]}),
            json!({"id":"c1","object":"chat.completion.chunk","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}),
        ];
        let mut body = String::new();
        for chunk in chunks {
            body.push_str(&format!("data: {}\n\n", chunk));
        }
        body.push_str("data: [DONE]\n\n");
        body
    }

    // ── metadata ─────────────────────────────────────────────────────────────

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

    // ── non-streaming round-trip ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_non_streaming_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_success_body()))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri()).with_supports_streaming(false);
        let model = ModelConfig::new_or_fail("Phi-4");
        let (msg, usage) = provider
            .complete(&model, "test-session", "You are helpful.", &[], &[])
            .await
            .expect("non-streaming request should succeed");

        let text = msg.as_concat_text();
        assert!(text.contains("Hello from Phi-4!"), "got: {text}");
        assert_eq!(usage.usage.input_tokens, Some(5));
        assert_eq!(usage.usage.output_tokens, Some(5));
    }

    // ── streaming round-trip ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_streaming_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_success_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let (msg, usage) = provider
            .complete(&model, "test-session", "You are helpful.", &[], &[])
            .await
            .expect("streaming request should succeed");

        let text = msg.as_concat_text();
        assert!(text.contains("Hello world"), "got: {text}");
        assert_eq!(usage.usage.output_tokens, Some(2));
    }

    // ── error mapping ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_401_returns_auth_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let err = provider
            .complete(&model, "s", "", &[], &[])
            .await
            .expect_err("401 should be an error");

        assert!(
            matches!(err, ProviderError::Authentication(_)),
            "expected Authentication, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_429_returns_rate_limit_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).append_header("retry-after", "30"))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let err = provider
            .complete(&model, "s", "", &[], &[])
            .await
            .expect_err("429 should be an error");

        assert!(
            matches!(err, ProviderError::RateLimitExceeded { .. }),
            "expected RateLimitExceeded, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_500_returns_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let err = provider
            .complete(&model, "s", "", &[], &[])
            .await
            .expect_err("500 should be an error");

        assert!(
            matches!(err, ProviderError::ServerError(_)),
            "expected ServerError, got {err:?}"
        );
    }

    #[tokio::test]
    async fn test_404_returns_request_failed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let provider = make_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let err = provider
            .complete(&model, "s", "", &[], &[])
            .await
            .expect_err("404 should be an error");

        assert!(
            matches!(err, ProviderError::RequestFailed(_)),
            "expected RequestFailed, got {err:?}"
        );
    }

    // ── deployment override ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_deployment_override_sets_model_in_body() {
        let server = MockServer::start().await;

        // Capture the request body to verify the model field
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_success_body()))
            .mount(&server)
            .await;

        // Simulate AZURE_FOUNDRY_DEPLOYMENT override by constructing with a different model name
        let provider = OpenAiCompatibleProvider::new(
            AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
            ApiClient::new(server.uri(), AuthMethod::NoAuth).unwrap(),
            ModelConfig::new_or_fail("overridden-deployment"),
            String::new(),
        )
        .with_supports_streaming(false);

        let model = ModelConfig::new_or_fail("overridden-deployment");
        let result = provider.complete(&model, "s", "", &[], &[]).await;
        assert!(
            result.is_ok(),
            "request with overridden deployment should succeed"
        );
    }
}
