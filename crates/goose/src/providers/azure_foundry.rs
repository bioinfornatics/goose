use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use reqwest::StatusCode;

use super::api_client::{ApiClient, AuthMethod, AuthProvider};
use super::azureauth::{AuthError, AzureAuth, AzureCredentials};
use super::base::{ConfigKey, MessageStream, Provider, ProviderDef, ProviderMetadata};
use super::openai_compatible::{
    handle_status, stream_anthropic_compat, stream_responses_compat, OpenAiCompatibleProvider,
};
use super::retry::{ProviderRetry, RetryConfig};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::formats::anthropic::{
    create_request_with_options_for_provider as create_anthropic_request, AnthropicFormatOptions,
};
use crate::providers::formats::openai_responses::create_responses_request;
use crate::providers::utils::RequestLog;
use goose_providers::errors::ProviderError;

use rmcp::model::Tool;

const AZURE_FOUNDRY_PROVIDER_NAME: &str = "azure_foundry";
const AZURE_FOUNDRY_DEFAULT_MODEL: &str = "Phi-4";
const AZURE_FOUNDRY_DOC_URL: &str = "https://learn.microsoft.com/en-us/azure/ai-foundry/foundry-models/how-to/deploy-models-serverless";

/// Entra ID resource scope for Azure AI Foundry serverless (MaaS) endpoints.
/// MaaS endpoints (`*.models.ai.azure.com` / `.../models`) use the ML workspace scope.
const AZURE_FOUNDRY_ENTRA_RESOURCE: &str = "https://ml.azure.com";
/// Required by the Anthropic Messages API on Azure Foundry (hub-level endpoint).
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Project endpoints (`…/api/projects/…`) use the AI Foundry scope.
/// This matches the scope used by the Azure AI Projects TypeScript SDK:
/// `scopes: ["https://ai.azure.com/.default"]`
const AZURE_FOUNDRY_PROJECT_ENTRA_RESOURCE: &str = "https://ai.azure.com";

pub const AZURE_FOUNDRY_KNOWN_MODELS: &[&str] = &[
    // Microsoft
    "Phi-4",
    "Phi-4-mini",
    // Mistral
    "Mistral-large-2411",
    "Mistral-small-2501",
    // Meta
    "Meta-Llama-3.1-70B-Instruct",
    "Meta-Llama-3.1-405B-Instruct",
    "Meta-Llama-3.3-70B-Instruct",
    // Cohere
    "Cohere-command-r-plus-08-2024",
    // AI21
    "AI21-Jamba-1.5-Large",
    // Anthropic
    "claude-sonnet-4-6",
    "claude-opus-4",
    "claude-haiku-3-5",
    // Zhipu (GLM) — OpenAI-compatible chat/completions on Azure Foundry
    // Note: GLM-5.x via Z.AI uses Anthropic-compat, but Azure Foundry
    // exposes GLM via the OpenAI-compat path. Verify with newer versions.
    "glm-4.7",
    "glm-4.5",
    "glm-5",
];

/// Returns true when `endpoint` is an Azure AI Projects endpoint
/// (`https://<hub>.services.ai.azure.com/api/projects/<project>`).
///
/// Projects endpoints differ from MaaS endpoints in two ways relevant to inference:
/// - They expose `/openai/v1/chat/completions` (version embedded in path).
/// - They do NOT require an `api-version` query parameter for inference.
fn is_project_endpoint(endpoint: &str) -> bool {
    endpoint.contains("/api/projects/")
}

/// Marker struct implementing `ProviderDef`; the actual runtime provider is
/// [`AzureFoundryActualProvider`].
pub struct AzureFoundryProvider;

/// The live provider returned by `from_env`.  Wraps [`OpenAiCompatibleProvider`]
/// for inference and calls `GET /deployments` to discover deployed models
/// dynamically (inspired by the Azure SDK for JS `project.deployments.list()`).
pub struct AzureFoundryActualProvider {
    /// OpenAI-compatible chat/completions — Llama, Phi, Mistral, GLM, Cohere, AI21,
    /// and all MaaS per-model endpoints.
    inner: OpenAiCompatibleProvider,
    /// Client for the OpenAI **Responses API** (`openai/v1/responses`).
    /// Present only on project endpoints. Routes gpt-5, o-series, and unknown future
    /// models (open-world default). `None` on MaaS endpoints.
    responses_client: Option<ApiClient>,
    /// Path for the Responses API, e.g. `"openai/v1/responses"`.
    responses_path: String,
    /// Client for the **Anthropic Messages API** (`anthropic/v1/messages`).
    /// Present only on project endpoints. Routes `claude-*` models natively,
    /// preserving prompt caching and extended thinking. `None` on MaaS endpoints.
    anthropic_client: Option<ApiClient>,
    /// Path for the Anthropic Messages API, e.g. `"anthropic/v1/messages"`.
    anthropic_path: String,
    /// Base endpoint, e.g. `https://<hub>.services.ai.azure.com/api/projects/<proj>`
    endpoint: String,
    /// Optional `api-version` query parameter forwarded to the deployments endpoint.
    api_version: Option<String>,
    /// Shared auth handle used for both inference and deployment listing.
    auth: Arc<AzureAuth>,
    /// Maps **deployment name** (user-defined free text) → [`ModelPublisher`].
    ///
    /// Populated lazily on the first call to `fetch_supported_models()` by
    /// querying `GET {endpoint}/deployments`, which returns the authoritative
    /// `modelPublisher` field from Azure regardless of the user-defined name.
    ///
    /// Falls back to [`ModelPublisher::from_model_name`] when the cache is empty
    /// (before first model fetch, MaaS endpoint, or offline mode).
    deployment_cache: Arc<Mutex<HashMap<String, ModelPublisher>>>,
}

// ── Auth adapter ──────────────────────────────────────────────────────────────

struct AzureFoundryAuthProvider {
    auth: Arc<AzureAuth>,
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
            AzureCredentials::BearerToken(_) | AzureCredentials::DefaultCredential => Ok((
                "Authorization".to_string(),
                format!("Bearer {}", auth_token.token_value),
            )),
        }
    }
}

// ── AzureFoundryActualProvider ────────────────────────────────────────────────

impl AzureFoundryActualProvider {
    /// Queries `GET {endpoint}/deployments` and returns both the list of deployment
    /// names (for model selection) and a publisher cache (for routing decisions).
    /// The single network call populates both.
    async fn fetch_deployments_and_publishers(
        &self,
    ) -> Result<(Vec<String>, HashMap<String, ModelPublisher>), ProviderError> {
        let auth_token = self
            .auth
            .get_token()
            .await
            .map_err(|e| ProviderError::Authentication(e.to_string()))?;

        let (header_name, header_value) = match self.auth.credential_type() {
            AzureCredentials::ApiKey(_) => ("api-key".to_string(), auth_token.token_value),
            AzureCredentials::BearerToken(_) | AzureCredentials::DefaultCredential => (
                "Authorization".to_string(),
                format!("Bearer {}", auth_token.token_value),
            ),
        };

        let base = self.endpoint.trim_end_matches('/');
        let effective_api_version = self
            .api_version
            .as_deref()
            .or_else(|| is_project_endpoint(&self.endpoint).then_some("v1"));
        let first_url = match effective_api_version {
            Some(v) => format!("{}/deployments?api-version={}", base, v),
            None => format!("{}/deployments", base),
        };

        let client = reqwest::Client::new();
        let mut names: Vec<String> = Vec::new();
        let mut publishers: HashMap<String, ModelPublisher> = HashMap::new();
        let mut next_url: Option<String> = Some(first_url);

        while let Some(url) = next_url {
            let response = client
                .get(&url)
                .header(header_name.as_str(), header_value.as_str())
                .send()
                .await
                .map_err(|e| ProviderError::NetworkError(e.to_string()))?;

            let status = response.status();
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                return Err(ProviderError::Authentication(format!(
                    "Azure Foundry deployments endpoint returned {status}"
                )));
            }
            if !status.is_success() {
                return Err(ProviderError::RequestFailed(format!(
                    "Azure Foundry deployments endpoint returned {status}"
                )));
            }

            let json: serde_json::Value = response.json().await.map_err(|e| {
                ProviderError::RequestFailed(format!("Failed to parse deployments response: {e}"))
            })?;

            if let Some(items) = json.get("value").and_then(|v| v.as_array()) {
                for item in items {
                    let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    names.push(name.to_string());
                    let publisher = item
                        .get("modelPublisher")
                        .and_then(|v| v.as_str())
                        .map(ModelPublisher::from_publisher_str)
                        .unwrap_or(ModelPublisher::OpenAI);
                    publishers.insert(name.to_string(), publisher);
                }
            }

            next_url = json
                .get("nextLink")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }

        names.sort();
        names.dedup();
        Ok((names, publishers))
    }

    /// Fetch all deployment names from `GET {endpoint}/deployments`.
    async fn fetch_deployments(&self) -> Result<Vec<String>, ProviderError> {
        let (names, publishers) = self.fetch_deployments_and_publishers().await?;
        // Populate the publisher cache as a side-effect of listing deployments.
        if let Ok(mut cache) = self.deployment_cache.lock() {
            *cache = publishers;
        }
        Ok(names)
    }
}

#[async_trait]
impl Provider for AzureFoundryActualProvider {
    fn get_name(&self) -> &str {
        self.inner.get_name()
    }

    fn get_model_config(&self) -> ModelConfig {
        self.inner.get_model_config()
    }

    fn retry_config(&self) -> RetryConfig {
        Provider::retry_config(&self.inner)
    }

    /// Azure Foundry deployment names are free text (e.g. `"claude-sonnet-4-6"`,
    /// `"my-prod-llm"`).  The canonical registry has no Azure Foundry entries and
    /// the heuristic string-matching would silently drop most deployments.
    /// Bypassing the filter ensures every deployment returned by the API is shown.
    fn skip_canonical_filtering(&self) -> bool {
        true
    }

    /// List deployed models by querying `GET {endpoint}/deployments`.
    /// Falls back to the static known-models list when the endpoint is
    /// unreachable or returns an error.
    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        match self.fetch_deployments().await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => {
                // Empty response — fall back to the built-in list so the UI is
                // never left with zero choices.
                Ok(AZURE_FOUNDRY_KNOWN_MODELS
                    .iter()
                    .map(|s| s.to_string())
                    .collect())
            }
            Err(e) => {
                tracing::debug!(
                    "Failed to fetch Azure Foundry deployments ({}); using static model list",
                    e
                );
                Ok(AZURE_FOUNDRY_KNOWN_MODELS
                    .iter()
                    .map(|s| s.to_string())
                    .collect())
            }
        }
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        // Resolve the publisher for this deployment.
        //
        // Priority:
        //   1. deployment_cache (populated at startup from GET /deployments)
        //      → authoritative: uses Azure's `modelPublisher` field, independent
        //        of the user-defined deployment name ("my-prod-llm", etc.)
        //   2. ModelPublisher::from_model_name (heuristic fallback)
        //      → only reliable when the deployment name happens to match the
        //        model name prefix; unreliable for arbitrary deployment names.
        let publisher = self
            .deployment_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&model_config.model_name).copied())
            .unwrap_or_else(|| ModelPublisher::from_model_name(&model_config.model_name));

        // Three-branch routing based on publisher:
        //
        // 1. Anthropic → native /anthropic/v1/messages
        //    Preserves prompt caching (cache_control) and extended thinking.
        //    Only on project endpoints (anthropic_client is Some).
        //
        // 2. OpenAI open-world → /openai/v1/responses (Responses API)
        //    Default for anything NOT in is_known_completion_model.
        //    Covers gpt-5, o-series, and unknown future models.
        //    Only on project endpoints (responses_client is Some).
        //
        // 3. chat/completions fallback (inner)
        //    Llama, Phi, Mistral, GLM, Cohere, AI21, all MaaS endpoints,
        //    and Anthropic when no anthropic_client is available.
        if matches!(publisher, ModelPublisher::Anthropic) && self.anthropic_client.is_some() {
            return self
                .stream_via_anthropic(model_config, session_id, system, messages, tools)
                .await;
        }
        if self.responses_client.is_some() && uses_responses_api_on_foundry_publisher(publisher) {
            return self
                .stream_via_responses(model_config, session_id, system, messages, tools)
                .await;
        }
        self.inner
            .stream(model_config, session_id, system, messages, tools)
            .await
    }
}

// ── Anthropic Messages API support ───────────────────────────────────────────

impl AzureFoundryActualProvider {
    /// Posts to the Anthropic Messages API (`anthropic/v1/messages`) and
    /// returns a streaming [`MessageStream`].
    ///
    /// Uses the native Anthropic wire format — including `cache_control` for
    /// prompt caching and `input_schema` for tools — rather than the lossy
    /// OpenAI-compatibility shim at `/openai/v1/chat/completions`.
    ///
    /// Only called when `ModelPublisher::Anthropic` is detected and
    /// `anthropic_client` is `Some` (i.e. project endpoints only).
    async fn stream_via_anthropic(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let Some(ref anthropic_client) = self.anthropic_client else {
            return self
                .inner
                .stream(model_config, session_id, system, messages, tools)
                .await;
        };

        let mut payload = create_anthropic_request(
            AZURE_FOUNDRY_PROVIDER_NAME,
            model_config,
            system,
            messages,
            tools,
            AnthropicFormatOptions::default(),
        )
        .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        payload["stream"] = serde_json::Value::Bool(true);

        let mut log = RequestLog::start(model_config, &payload)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                let resp = anthropic_client
                    .response_post(Some(session_id), &self.anthropic_path, &payload_clone)
                    .await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_anthropic_compat(response, log)
    }
}

// ── Responses API support ─────────────────────────────────────────────────────

impl AzureFoundryActualProvider {
    /// Posts to the OpenAI Responses API endpoint (`openai/v1/responses`) and
    /// returns a streaming [`MessageStream`].
    ///
    /// Only called when [`is_openai_responses_model`] returns `true` and a
    /// `responses_client` has been configured (i.e. project endpoints only).
    async fn stream_via_responses(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let Some(ref responses_client) = self.responses_client else {
            return self
                .inner
                .stream(model_config, session_id, system, messages, tools)
                .await;
        };

        let mut payload = create_responses_request(model_config, system, messages, tools)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        payload["stream"] = serde_json::Value::Bool(true);

        let mut log = RequestLog::start(model_config, &payload)
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;

        let response = self
            .with_retry(|| async {
                let payload_clone = payload.clone();
                let resp = responses_client
                    .response_post(Some(session_id), &self.responses_path, &payload_clone)
                    .await?;
                handle_status(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        stream_responses_compat(response, log)
    }
}

// ── Routing helpers — TDD stubs, implement to make tests green ───────────────

/// Identifies the publisher of a model deployed on Azure AI Foundry.
///
/// The publisher is inferred from well-known model name prefixes.
/// Anything not recognised defaults to [`ModelPublisher::OpenAI`] to honour the
/// open-world principle: unknown future models are assumed OpenAI-family and
/// routed to the Responses API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelPublisher {
    OpenAI,
    Anthropic,
    Meta,
    Microsoft,
    Mistral,
    Cohere,
    AI21,
    /// Zhipu AI GLM models.
    ///
    /// On Azure Foundry, GLM is exposed via the **OpenAI-compatible** chat/completions
    /// path (not Responses API, not Anthropic format).
    ///
    /// Note: GLM-5.x via Z.AI uses Anthropic-compat format, but that is a different
    /// endpoint/provider. Verify API format compatibility for future GLM versions
    /// when Azure Foundry updates its model catalog.
    Zhipu,
}

impl ModelPublisher {
    /// Map an Azure Foundry `modelPublisher` string (from `GET /deployments`) to a
    /// [`ModelPublisher`] variant.
    ///
    /// This is the **authoritative** source: it uses the metadata returned by Azure,
    /// independent of the user-defined deployment name.
    pub fn from_publisher_str(publisher: &str) -> Self {
        match publisher.to_lowercase().as_str() {
            "anthropic" => Self::Anthropic,
            "meta" | "meta-llama" => Self::Meta,
            "microsoft" => Self::Microsoft,
            "mistralai" | "mistral" => Self::Mistral,
            "cohere" => Self::Cohere,
            "ai21" | "ai21 labs" | "ai21labs" => Self::AI21,
            "zhipu" | "zhipuai" => Self::Zhipu,
            // Default to OpenAI for unknown publishers → Responses API (open-world)
            _ => Self::OpenAI,
        }
    }

    /// Infer the publisher from the model **name prefix**.
    ///
    /// This is a **heuristic fallback** used when the deployment cache is unavailable
    /// (network error at startup, MaaS endpoint without `/deployments` API).
    ///
    /// ⚠ Deployment names on Azure Foundry are **free text** chosen by the user.
    /// A deployment named `"my-prod-llm"` will resolve to `OpenAI` here even if
    /// the underlying model is Claude. Always prefer [`from_publisher_str`] when
    /// the authoritative `modelPublisher` metadata is available.
    pub fn from_model_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.starts_with("claude") {
            Self::Anthropic
        } else if lower.starts_with("phi") {
            Self::Microsoft
        } else if lower.starts_with("meta-llama") || lower.starts_with("llama") {
            Self::Meta
        } else if lower.starts_with("mistral") {
            Self::Mistral
        } else if lower.starts_with("cohere") {
            Self::Cohere
        } else if lower.starts_with("ai21") {
            Self::AI21
        } else if lower.starts_with("glm") {
            Self::Zhipu
        } else {
            // Open-world default: route to Responses API.
            Self::OpenAI
        }
    }
}

/// Returns `true` when the model should be sent to the OpenAI Responses API
/// on Azure AI Foundry project endpoints.
///
/// **Inverted logic**: the Responses API is the *default* (open world).
/// Only a finite, explicitly listed set of non-OpenAI-publisher models that
/// are known to be incompatible with the Responses API are excluded.
/// Everything else — including unknown future models — defaults to Responses.
///
/// This is the inverse of `is_openai_responses_model()` used elsewhere in goose,
/// which maintains a *positive* list and would wrongly route future models to the
/// deprecated chat/completions endpoint.
/// Whether a model should use the Responses API, given its resolved publisher.
/// Called from `stream()` after the authoritative cache lookup.
pub fn uses_responses_api_on_foundry_publisher(publisher: ModelPublisher) -> bool {
    !is_known_completion_publisher(publisher)
}

/// Name-based convenience wrapper — **only use when no deployment cache is available**.
pub fn uses_responses_api_on_foundry(model_name: &str) -> bool {
    !is_known_completion_model(model_name)
}

/// Returns `true` for models that are known to use the OpenAI-compatible
/// `chat/completions` path on Azure Foundry — i.e. non-OpenAI publishers whose
/// native API is adapted by Azure behind `/openai/v1/chat/completions`.
///
/// This is a **finite exception list**.  Anything not listed here defaults to
/// the Responses API (open-world principle).
/// Publisher-based check — used with the authoritative cache.
pub fn is_known_completion_publisher(publisher: ModelPublisher) -> bool {
    matches!(
        publisher,
        ModelPublisher::Anthropic
            | ModelPublisher::Meta
            | ModelPublisher::Microsoft
            | ModelPublisher::Mistral
            | ModelPublisher::Cohere
            | ModelPublisher::AI21
            | ModelPublisher::Zhipu
    )
}

/// Name-based heuristic fallback — only used when the deployment cache is empty.
///
/// ⚠ Unreliable for user-defined deployment names that don't match model name
/// prefixes (e.g. `"my-prod-llm"`). The deployment cache built from
/// `GET /deployments` is the authoritative source.
pub fn is_known_completion_model(model_name: &str) -> bool {
    is_known_completion_publisher(ModelPublisher::from_model_name(model_name))
}

// ── ProviderDef ───────────────────────────────────────────────────────────────

impl ProviderDef for AzureFoundryProvider {
    type Provider = AzureFoundryActualProvider;

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

            let api_version: Option<String> = config.get_param("AZURE_FOUNDRY_API_VERSION").ok();

            let api_key = config
                .get_secret("AZURE_FOUNDRY_API_KEY")
                .ok()
                .filter(|key: &String| !key.is_empty());

            // Select the correct Entra ID token scope for the endpoint type.
            // Project endpoints (…/api/projects/…) use the AI Foundry scope —
            // matching the Azure AI Projects TypeScript SDK: `https://ai.azure.com/.default`.
            // MaaS endpoints use the ML workspace scope.
            let entra_resource = if is_project_endpoint(&endpoint) {
                AZURE_FOUNDRY_PROJECT_ENTRA_RESOURCE.to_string()
            } else {
                AZURE_FOUNDRY_ENTRA_RESOURCE.to_string()
            };

            let auth = Arc::new(
                AzureAuth::new_with_resource(api_key, entra_resource).map_err(|e| match e {
                    AuthError::Credentials(msg) => {
                        anyhow::anyhow!("Credentials error: {}", msg)
                    }
                    AuthError::TokenExchange(msg) => {
                        anyhow::anyhow!("Token exchange error: {}", msg)
                    }
                })?,
            );

            let auth_provider = AzureFoundryAuthProvider {
                auth: Arc::clone(&auth),
            };

            // Project-style endpoints (…/api/projects/<proj>) follow the Azure AI Projects
            // SDK convention: inference lives at /openai/v1/chat/completions — the version
            // is embedded in the path, so no api-version query param is needed.
            //
            // MaaS-style endpoints (…/models) use /chat/completions at the root and may
            // carry an explicit api-version query param when the user configures one.
            let (path_prefix, resolved_api_version) =
                if is_project_endpoint(&endpoint) && api_version.is_none() {
                    ("openai/v1".to_string(), None)
                } else {
                    (String::new(), api_version.clone())
                };

            let host = endpoint.trim_end_matches('/').to_string();
            let mut api_client = ApiClient::new(host, AuthMethod::Custom(Box::new(auth_provider)))?;
            if let Some(ref version) = resolved_api_version {
                api_client =
                    api_client.with_query(vec![("api-version".to_string(), version.clone())]);
            }

            let effective_model = model;

            let inner = OpenAiCompatibleProvider::new(
                AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
                api_client,
                effective_model,
                path_prefix,
            );

            // For project endpoints, create a second client targeting the Responses API
            // path (`openai/v1/responses`).  This is used for OpenAI reasoning / GPT-5
            // models which require the Responses API; all other models continue to use
            // the inner chat/completions client.
            // MaaS endpoints do not expose the Responses API path, so responses_client
            // is left as None there.
            // Resolve base URLs for the two inference plane levels:
            //
            //   project_host  = https://{hub}/api/projects/{proj}   (OpenAI inference, scoped)
            //   hub_host      = https://{hub}                        (Anthropic, Speech — hub level)
            //
            // The management plane (deployments) lives at project_host.
            // OpenAI inference  → project_host/openai/v1/...   (matches TypeScript SDK)
            // Anthropic inference → hub_host/anthropic/v1/...  (hub level, confirmed by Azure)
            // Speech            → hub_host/speechtotext/...    (hub level, same pattern)
            let project_host = endpoint.trim_end_matches('/').to_string();
            let hub_host = if is_project_endpoint(&endpoint) {
                endpoint
                    .split("/api/projects/")
                    .next()
                    .unwrap_or(&endpoint)
                    .trim_end_matches('/')
                    .to_string()
            } else {
                project_host.clone()
            };

            let (responses_client, responses_path, anthropic_client, anthropic_path) =
                if is_project_endpoint(&endpoint) {
                    let auth_resp = AzureFoundryAuthProvider {
                        auth: Arc::clone(&auth),
                    };
                    let auth_anth = AzureFoundryAuthProvider {
                        auth: Arc::clone(&auth),
                    };
                    // Responses API: project-scoped (same base as OpenAI inference)
                    let rc = ApiClient::new(
                        project_host.clone(),
                        AuthMethod::Custom(Box::new(auth_resp)),
                    )
                    .ok();
                    // Anthropic API: hub-level, uses Anthropic wire format.
                    //
                    // Auth differs from other Azure Foundry endpoints:
                    // - API key → `x-api-key` header  (Anthropic convention, not Azure `api-key`)
                    // - Entra ID → `Authorization: Bearer` (same as other endpoints)
                    //
                    // The `anthropic-version` header is always required.
                    let anthropic_auth = match auth.credential_type() {
                        AzureCredentials::ApiKey(key) => AuthMethod::ApiKey {
                            header_name: "x-api-key".to_string(),
                            key: key.clone(),
                        },
                        _ => AuthMethod::Custom(Box::new(auth_anth)),
                    };
                    let ac = ApiClient::new(hub_host.clone(), anthropic_auth)
                        .and_then(|c| c.with_header("anthropic-version", ANTHROPIC_VERSION))
                        .ok();
                    (
                        rc,
                        "openai/v1/responses".to_string(),
                        ac,
                        "anthropic/v1/messages".to_string(),
                    )
                } else {
                    (None, String::new(), None, String::new())
                };

            // Publisher cache is populated lazily on first fetch_supported_models() call.
            // Keeping startup free of network calls ensures fast provider initialization.
            let deployment_cache = Arc::new(Mutex::new(HashMap::new()));

            Ok(AzureFoundryActualProvider {
                inner,
                responses_client,
                responses_path,
                anthropic_client,
                anthropic_path,
                endpoint,
                api_version,
                auth,
                deployment_cache,
            })
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_inference_provider(server_uri: &str) -> OpenAiCompatibleProvider {
        OpenAiCompatibleProvider::new(
            AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
            ApiClient::new(server_uri.to_string(), AuthMethod::NoAuth).unwrap(),
            ModelConfig::new_or_fail("Phi-4"),
            String::new(),
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

    /// Build a fake `/deployments` API page.
    ///
    /// `name` = deployment name (user-defined, used by goose as the model ID).
    /// `modelName` = underlying Azure model name (different from deployment name in
    /// production; kept here for completeness but NOT used for model listing).
    fn deployments_page(models: &[&str], next_link: Option<&str>) -> serde_json::Value {
        let value: Vec<serde_json::Value> = models
            .iter()
            .map(|m| {
                json!({
                    "type": "ModelDeployment",
                    // name = deployment name; this is what the user sets and what
                    // goose must show and use when calling the API.
                    "name": m,
                    // modelName = underlying model (different in real deployments,
                    // e.g. name="my-phi-prod", modelName="Phi-4").
                    "modelName": format!("{}-underlying", m),
                    "modelPublisher": "Microsoft",
                    "modelVersion": "1"
                })
            })
            .collect();
        let mut page = json!({ "value": value });
        if let Some(link) = next_link {
            page["nextLink"] = json!(link);
        }
        page
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

        let provider = make_inference_provider(&server.uri()).with_supports_streaming(false);
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

        let provider = make_inference_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let (msg, usage) = provider
            .complete(&model, "test-session", "You are helpful.", &[], &[])
            .await
            .expect("streaming request should succeed");

        let text = msg.as_concat_text();
        assert!(text.contains("Hello world"), "got: {text}");
        assert_eq!(usage.usage.output_tokens, Some(2));
    }

    // ── deployments listing ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_fetch_deployments_single_page() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/deployments"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(deployments_page(&["Phi-4", "Mistral-large-2411"], None)),
            )
            .mount(&server)
            .await;

        let auth = Arc::new(
            AzureAuth::new_with_resource(
                Some("test-key".to_string()),
                AZURE_FOUNDRY_ENTRA_RESOURCE.to_string(),
            )
            .unwrap(),
        );
        let inner = make_inference_provider(&server.uri());
        let provider = AzureFoundryActualProvider {
            inner,
            responses_client: None,
            responses_path: String::new(),
            anthropic_client: None,
            anthropic_path: String::new(),
            endpoint: server.uri(),
            api_version: None,
            auth,
            deployment_cache: Arc::new(Mutex::new(HashMap::new())),
        };

        let models = provider
            .fetch_supported_models()
            .await
            .expect("should succeed");
        assert!(models.contains(&"Phi-4".to_string()));
        assert!(models.contains(&"Mistral-large-2411".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_deployments_pagination() {
        let server = MockServer::start().await;

        let page2_url = format!("{}/deployments?page=2", server.uri());
        let page1 = deployments_page(
            &["Phi-4"],
            Some(&format!("{}/deployments?page=2", server.uri())),
        );
        let page2 = deployments_page(&["Mistral-large-2411", "Meta-Llama-3.3-70B-Instruct"], None);

        Mock::given(method("GET"))
            .and(path("/deployments"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page2))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/deployments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page1))
            .mount(&server)
            .await;

        let auth = Arc::new(
            AzureAuth::new_with_resource(
                Some("test-key".to_string()),
                AZURE_FOUNDRY_ENTRA_RESOURCE.to_string(),
            )
            .unwrap(),
        );
        let inner = make_inference_provider(&server.uri());
        let provider = AzureFoundryActualProvider {
            inner,
            responses_client: None,
            responses_path: String::new(),
            anthropic_client: None,
            anthropic_path: String::new(),
            endpoint: server.uri(),
            api_version: None,
            auth,
            deployment_cache: Arc::new(Mutex::new(HashMap::new())),
        };

        let _ = page2_url; // used in mock setup above
        let models = provider
            .fetch_supported_models()
            .await
            .expect("should succeed");
        assert_eq!(models.len(), 3);
        assert!(models.contains(&"Phi-4".to_string()));
        assert!(models.contains(&"Meta-Llama-3.3-70B-Instruct".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_deployments_falls_back_on_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/deployments"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let auth = Arc::new(
            AzureAuth::new_with_resource(
                Some("test-key".to_string()),
                AZURE_FOUNDRY_ENTRA_RESOURCE.to_string(),
            )
            .unwrap(),
        );
        let inner = make_inference_provider(&server.uri());
        let provider = AzureFoundryActualProvider {
            inner,
            responses_client: None,
            responses_path: String::new(),
            anthropic_client: None,
            anthropic_path: String::new(),
            endpoint: server.uri(),
            api_version: None,
            auth,
            deployment_cache: Arc::new(Mutex::new(HashMap::new())),
        };

        let models = provider
            .fetch_supported_models()
            .await
            .expect("should fall back gracefully");
        // Should return the static list, not an error
        assert!(!models.is_empty());
        assert!(models.contains(&"Phi-4".to_string()));
    }

    #[tokio::test]
    async fn test_fetch_deployments_with_api_version() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/deployments"))
            .and(query_param("api-version", "2025-01-01"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(deployments_page(&["Phi-4-mini"], None)),
            )
            .mount(&server)
            .await;

        let auth = Arc::new(
            AzureAuth::new_with_resource(
                Some("test-key".to_string()),
                AZURE_FOUNDRY_ENTRA_RESOURCE.to_string(),
            )
            .unwrap(),
        );
        let inner = make_inference_provider(&server.uri());
        let provider = AzureFoundryActualProvider {
            inner,
            responses_client: None,
            responses_path: String::new(),
            anthropic_client: None,
            anthropic_path: String::new(),
            endpoint: server.uri(),
            api_version: Some("2025-01-01".to_string()),
            auth,
            deployment_cache: Arc::new(Mutex::new(HashMap::new())),
        };

        let models = provider
            .fetch_supported_models()
            .await
            .expect("should succeed");
        assert!(models.contains(&"Phi-4-mini".to_string()));
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

        let provider = make_inference_provider(&server.uri());
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

        let provider = make_inference_provider(&server.uri()).with_no_retries();
        let model = ModelConfig::new_or_fail("Phi-4");
        let err = provider
            .complete(&model, "s", "", &[], &[])
            .await
            .expect_err("429 should be an error");

        assert!(
            matches!(
                err,
                ProviderError::RateLimitExceeded {
                    retry_delay: Some(d),
                    ..
                } if d == std::time::Duration::from_secs(30)
            ),
            "expected RateLimitExceeded with retry_delay=30s, got {err:?}"
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

        let provider = make_inference_provider(&server.uri()).with_no_retries();
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

        let provider = make_inference_provider(&server.uri()).with_no_retries();
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

    // ── SSE error handling ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_malformed_sse_chunk_returns_network_error() {
        let server = MockServer::start().await;
        let bad_body = "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"Phi-4\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"index\":0}]}\n\ndata: NOT_JSON\n\ndata: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(bad_body)
                    .append_header("content-type", "text/event-stream"),
            )
            .mount(&server)
            .await;

        let provider = make_inference_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        let err = provider
            .complete(&model, "s", "", &[], &[])
            .await
            .expect_err("malformed SSE chunk should be an error");

        assert!(
            matches!(err, ProviderError::NetworkError(_)),
            "expected NetworkError for malformed SSE, got {err:?}"
        );
    }

    // ── config validation ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_missing_endpoint_from_env_returns_err() {
        // Skip if already configured via env var or goose global config.
        if std::env::var("AZURE_FOUNDRY_ENDPOINT").is_ok() {
            return;
        }
        if crate::config::Config::global()
            .get_param::<String>("AZURE_FOUNDRY_ENDPOINT")
            .is_ok()
        {
            return;
        }
        let model = ModelConfig::new_or_fail("Phi-4");
        let result = AzureFoundryProvider::from_env(model, vec![]).await;
        assert!(
            result.is_err(),
            "from_env should fail when AZURE_FOUNDRY_ENDPOINT is not configured"
        );
    }

    // ── endpoint type detection ───────────────────────────────────────────────

    #[test]
    fn test_is_project_endpoint_detects_api_projects_path() {
        assert!(is_project_endpoint(
            "https://hub.services.ai.azure.com/api/projects/my-project"
        ));
        assert!(is_project_endpoint(
            "https://hub.services.ai.azure.com/api/projects/my-project/"
        ));
    }

    #[test]
    fn test_is_project_endpoint_rejects_maas() {
        assert!(!is_project_endpoint(
            "https://hub.services.ai.azure.com/models"
        ));
        assert!(!is_project_endpoint("https://hub.models.ai.azure.com"));
    }

    // ── deployment override ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_deployment_override_sets_model_in_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(non_streaming_success_body()))
            .mount(&server)
            .await;

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

    // ═══════════════════════════════════════════════════════════════════════════
    // TDD — Routing logic & publisher detection
    //
    // These tests encode the DESIRED behaviour before the implementation exists.
    // Run `cargo test` to see them go red; implement the functions to turn them green.
    //
    // Design axioms:
    //   i)  Responses API = open world (default for unknown / future models).
    //       chat/completions = FINITE known-exception list.
    //   ii) Publisher is detected from the model name prefix so that future
    //       routing decisions (e.g. /anthropic/v1/messages) can branch per publisher.
    // ═══════════════════════════════════════════════════════════════════════════

    // ── i) Inverted routing: Responses API is the default ────────────────────

    #[test]
    fn unknown_future_model_defaults_to_responses_api() {
        // Core principle: anything NOT in the finite exclusion list → Responses API.
        // This ensures future OpenAI models work without code changes.
        assert!(uses_responses_api_on_foundry("future-model-2027"));
        assert!(uses_responses_api_on_foundry("gpt-6"));
        assert!(uses_responses_api_on_foundry("openai-reasoning-ultra"));
    }

    #[test]
    fn known_openai_models_use_responses_api() {
        assert!(uses_responses_api_on_foundry("o3"));
        assert!(uses_responses_api_on_foundry("o4-mini"));
        assert!(uses_responses_api_on_foundry("gpt-5"));
        assert!(uses_responses_api_on_foundry("gpt-5-mini"));
        // gpt-4o and gpt-4.1 also support the Responses API
        assert!(uses_responses_api_on_foundry("gpt-4o"));
        assert!(uses_responses_api_on_foundry("gpt-4.1"));
    }

    #[test]
    fn known_non_openai_models_use_chat_completions() {
        // These are the EXCEPTIONS — a finite list of non-OpenAI publisher models
        // that Azure Foundry exposes via the OpenAI-compatible chat/completions path
        // but which do NOT implement the Responses API.
        assert!(!uses_responses_api_on_foundry("Phi-4"));
        assert!(!uses_responses_api_on_foundry("Phi-4-mini"));
        assert!(!uses_responses_api_on_foundry(
            "Meta-Llama-3.3-70B-Instruct"
        ));
        assert!(!uses_responses_api_on_foundry(
            "Meta-Llama-3.1-70B-Instruct"
        ));
        assert!(!uses_responses_api_on_foundry("Mistral-large-2411"));
        assert!(!uses_responses_api_on_foundry("Mistral-small-2501"));
        assert!(!uses_responses_api_on_foundry(
            "Cohere-command-r-plus-08-2024"
        ));
        assert!(!uses_responses_api_on_foundry("AI21-Jamba-1.5-Large"));
    }

    #[test]
    fn claude_does_not_use_openai_responses_api() {
        // Anthropic Claude has its own message format; routing it to
        // /openai/v1/responses would fail. It uses chat/completions (openai-compat)
        // for now; a future /anthropic/v1/messages route can be added via publisher
        // detection (see tests below).
        assert!(!uses_responses_api_on_foundry("claude-sonnet-4-6"));
        assert!(!uses_responses_api_on_foundry("claude-opus-4"));
        assert!(!uses_responses_api_on_foundry("claude-haiku-3-5"));
        assert!(!uses_responses_api_on_foundry("Claude-3.5-Sonnet"));
    }

    // ── ii) Publisher detection ───────────────────────────────────────────────

    #[test]
    fn detect_publisher_microsoft_for_phi() {
        assert_eq!(
            ModelPublisher::from_model_name("Phi-4"),
            ModelPublisher::Microsoft
        );
        assert_eq!(
            ModelPublisher::from_model_name("Phi-4-mini"),
            ModelPublisher::Microsoft
        );
        assert_eq!(
            ModelPublisher::from_model_name("phi-3-medium"),
            ModelPublisher::Microsoft
        );
    }

    #[test]
    fn detect_publisher_meta_for_llama() {
        assert_eq!(
            ModelPublisher::from_model_name("Meta-Llama-3.3-70B-Instruct"),
            ModelPublisher::Meta
        );
        assert_eq!(
            ModelPublisher::from_model_name("Llama-3.1-8B-Instruct"),
            ModelPublisher::Meta
        );
    }

    #[test]
    fn detect_publisher_mistral_for_mistral() {
        assert_eq!(
            ModelPublisher::from_model_name("Mistral-large-2411"),
            ModelPublisher::Mistral
        );
        assert_eq!(
            ModelPublisher::from_model_name("mistral-small-2501"),
            ModelPublisher::Mistral
        );
    }

    #[test]
    fn detect_publisher_anthropic_for_claude() {
        assert_eq!(
            ModelPublisher::from_model_name("claude-sonnet-4-6"),
            ModelPublisher::Anthropic
        );
        assert_eq!(
            ModelPublisher::from_model_name("claude-opus-4"),
            ModelPublisher::Anthropic
        );
        assert_eq!(
            ModelPublisher::from_model_name("Claude-3.5-Sonnet"),
            ModelPublisher::Anthropic
        );
    }

    #[test]
    fn detect_publisher_cohere() {
        assert_eq!(
            ModelPublisher::from_model_name("Cohere-command-r-plus-08-2024"),
            ModelPublisher::Cohere
        );
    }

    #[test]
    fn detect_publisher_openai_for_gpt_and_o_series() {
        assert_eq!(
            ModelPublisher::from_model_name("gpt-5"),
            ModelPublisher::OpenAI
        );
        assert_eq!(
            ModelPublisher::from_model_name("gpt-4o"),
            ModelPublisher::OpenAI
        );
        assert_eq!(
            ModelPublisher::from_model_name("o3"),
            ModelPublisher::OpenAI
        );
        assert_eq!(
            ModelPublisher::from_model_name("o4-mini"),
            ModelPublisher::OpenAI
        );
    }

    #[test]
    fn unknown_model_defaults_to_openai_publisher() {
        // Open-world default: if the publisher cannot be identified, treat the
        // model as OpenAI-family and route to the Responses API.
        assert_eq!(
            ModelPublisher::from_model_name("future-unknown-model-xyz"),
            ModelPublisher::OpenAI
        );
    }

    // ── iii) Integration: mock-server routing per model on project endpoint ───

    // ── ii-b) Publisher detection: GLM + Anthropic additions ─────────────────

    #[test]
    fn detect_publisher_zhipu_for_glm() {
        assert_eq!(
            ModelPublisher::from_model_name("glm-4.7"),
            ModelPublisher::Zhipu
        );
        assert_eq!(
            ModelPublisher::from_model_name("glm-5"),
            ModelPublisher::Zhipu
        );
        assert_eq!(
            ModelPublisher::from_model_name("glm-4.5-flash"),
            ModelPublisher::Zhipu
        );
    }

    #[test]
    fn glm_does_not_use_responses_api() {
        // GLM uses OpenAI-compat chat/completions on Azure Foundry, not Responses API.
        assert!(!uses_responses_api_on_foundry("glm-4.7"));
        assert!(!uses_responses_api_on_foundry("glm-5"));
    }

    // ── iii-b) Integration: Anthropic and GLM routing ─────────────────────────

    #[tokio::test]
    async fn project_endpoint_routes_claude_to_anthropic_messages() {
        // Claude must land on /anthropic/v1/messages — NOT /openai/v1/chat/completions.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/anthropic/v1/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_anthropic_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_project_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("claude-sonnet-4-6");
        provider
            .complete(&model, "s", "system prompt", &[], &[])
            .await
            .expect("Claude should route to /anthropic/v1/messages");
    }

    #[tokio::test]
    async fn project_endpoint_routes_glm_to_chat_completions() {
        // GLM uses chat/completions (OpenAI-compat), NOT the Responses API.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_success_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_project_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("glm-4.7");
        provider
            .complete(&model, "s", "system prompt", &[], &[])
            .await
            .expect("GLM should route to /openai/v1/chat/completions");
    }

    #[tokio::test]
    async fn project_endpoint_claude_request_uses_anthropic_body_format() {
        // Verify the request body sent to /anthropic/v1/messages uses Anthropic format:
        // it must contain a top-level "max_tokens" field and a "messages" array,
        // confirming the Anthropic payload (not the OpenAI chat/completions format).
        use std::sync::{Arc as StdArc, Mutex};
        use wiremock::{Request, Respond, ResponseTemplate};

        struct CapturingResponder {
            captured: StdArc<Mutex<Option<serde_json::Value>>>,
        }
        impl Respond for CapturingResponder {
            fn respond(&self, req: &Request) -> ResponseTemplate {
                if let Ok(body) = serde_json::from_slice::<serde_json::Value>(&req.body) {
                    *self.captured.lock().unwrap() = Some(body);
                }
                ResponseTemplate::new(200)
                    .set_body_string(sse_anthropic_body())
                    .append_header("content-type", "text/event-stream")
            }
        }

        let captured = StdArc::new(Mutex::new(None));
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/anthropic/v1/messages"))
            .respond_with(CapturingResponder {
                captured: StdArc::clone(&captured),
            })
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_project_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("claude-sonnet-4-6");
        provider
            .complete(&model, "s", "You are helpful", &[], &[])
            .await
            .expect("Claude request body must reach /anthropic/v1/messages");

        let body = captured
            .lock()
            .unwrap()
            .clone()
            .expect("body must be captured");
        // Anthropic format: top-level "max_tokens" (not inside messages)
        assert!(
            body.get("max_tokens").is_some(),
            "must have top-level max_tokens"
        );
        // Anthropic format: "messages" array
        assert!(
            body.get("messages").and_then(|v| v.as_array()).is_some(),
            "must have messages array"
        );
        // Anthropic format: "model" field
        assert_eq!(body["model"], "claude-sonnet-4-6");
        // OpenAI format would have system inside messages[0]; Anthropic uses top-level "system"
        let first_msg_role = body["messages"][0]["role"].as_str().unwrap_or("");
        assert_ne!(
            first_msg_role, "system",
            "system must NOT be the first message role in Anthropic format"
        );
    }

    /// Minimal valid Anthropic SSE stream with one text delta.
    /// Format: message_start → content_block_start → content_block_delta → message_stop
    fn sse_anthropic_body() -> String {
        let message_start = r#"data: {"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-6","stop_reason":null,"usage":{"input_tokens":5,"output_tokens":0}}}"#;
        let block_start = r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
        let delta = r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
        let block_stop = r#"data: {"type":"content_block_stop","index":0}"#;
        let msg_delta = r#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}}"#;
        let msg_stop = r#"data: {"type":"message_stop"}"#;
        format!("{message_start}\n\n{block_start}\n\n{delta}\n\n{block_stop}\n\n{msg_delta}\n\n{msg_stop}\n\n")
    }

    #[tokio::test]
    async fn project_endpoint_routes_unknown_model_to_responses() {
        // An unrecognised model must land on the Responses API path, not chat/completions.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_responses_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_project_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("future-unknown-model-xyz");
        provider
            .complete(&model, "s", "system", &[], &[])
            .await
            .expect("unknown model should route to responses API");
    }

    #[tokio::test]
    async fn project_endpoint_routes_phi_to_chat_completions() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_success_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_project_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("Phi-4");
        provider
            .complete(&model, "s", "system", &[], &[])
            .await
            .expect("Phi-4 should route to chat/completions");
    }

    #[tokio::test]
    async fn project_endpoint_routes_o3_to_responses() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/responses"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_responses_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_project_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("o3");
        provider
            .complete(&model, "s", "system", &[], &[])
            .await
            .expect("o3 should route to responses API");
    }

    #[tokio::test]
    async fn maas_endpoint_routes_o3_to_chat_completions() {
        // MaaS endpoints do not expose /responses — even OpenAI models fall back
        // to chat/completions because responses_client is None.
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(sse_success_body())
                    .append_header("content-type", "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let provider = make_maas_azure_foundry_provider(&server.uri());
        let model = ModelConfig::new_or_fail("o3");
        provider
            .complete(&model, "s", "system", &[], &[])
            .await
            .expect("o3 on MaaS should fall back to chat/completions");
    }

    // ── TDD helpers (referenced above, not yet implemented) ───────────────────

    // ── TDD integration helpers ───────────────────────────────────────────────

    /// Minimal valid Responses API SSE stream with one text delta.
    /// Format matches what `responses_api_to_streaming_message` expects.
    fn sse_responses_body() -> String {
        // Each line is a `data:` event; the parser ignores `event:` lines.
        let created = r#"data: {"type":"response.created","sequence_number":1,"response":{"id":"resp_t","object":"response","created_at":1737368310,"status":"in_progress","model":"test-model","output":[]}}"#;
        let delta = r#"data: {"type":"response.output_text.delta","sequence_number":2,"item_id":"m1","output_index":0,"content_index":0,"delta":"Hello"}"#;
        let completed = r#"data: {"type":"response.completed","sequence_number":3,"response":{"id":"resp_t","object":"response","created_at":1737368310,"status":"completed","model":"test-model","output":[],"usage":{"input_tokens":5,"output_tokens":1,"total_tokens":6}}}"#;
        let done = "data: [DONE]";
        format!("{created}\n\n{delta}\n\n{completed}\n\n{done}\n\n")
    }

    /// Builds an `AzureFoundryActualProvider` configured for a PROJECT endpoint.
    ///
    /// Both the `inner` (chat/completions) and `responses_client` point at
    /// `server_uri` so the mock server can intercept either path.
    fn make_project_azure_foundry_provider(server_uri: &str) -> AzureFoundryActualProvider {
        let auth = Arc::new(
            AzureAuth::new_with_resource(
                Some("test-key".to_string()),
                AZURE_FOUNDRY_PROJECT_ENTRA_RESOURCE.to_string(),
            )
            .unwrap(),
        );
        let inner = OpenAiCompatibleProvider::new(
            AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
            ApiClient::new(server_uri.to_string(), AuthMethod::NoAuth).unwrap(),
            ModelConfig::new_or_fail("placeholder"),
            "openai/v1/".to_string(), // project endpoint prefix → /openai/v1/chat/completions
        );
        let responses_client = ApiClient::new(server_uri.to_string(), AuthMethod::NoAuth).ok();
        let anthropic_client = ApiClient::new(server_uri.to_string(), AuthMethod::NoAuth).ok();
        AzureFoundryActualProvider {
            inner,
            responses_client,
            responses_path: "openai/v1/responses".to_string(),
            anthropic_client,
            anthropic_path: "anthropic/v1/messages".to_string(),
            endpoint: server_uri.to_string(),
            api_version: None,
            auth,
            deployment_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Builds an `AzureFoundryActualProvider` configured for a MAAS endpoint.
    ///
    /// Both `responses_client` and `anthropic_client` are `None` — MaaS
    /// endpoints expose a single per-model chat/completions URL.
    fn make_maas_azure_foundry_provider(server_uri: &str) -> AzureFoundryActualProvider {
        let auth = Arc::new(
            AzureAuth::new_with_resource(
                Some("test-key".to_string()),
                AZURE_FOUNDRY_ENTRA_RESOURCE.to_string(),
            )
            .unwrap(),
        );
        let inner = OpenAiCompatibleProvider::new(
            AZURE_FOUNDRY_PROVIDER_NAME.to_string(),
            ApiClient::new(server_uri.to_string(), AuthMethod::NoAuth).unwrap(),
            ModelConfig::new_or_fail("placeholder"),
            String::new(),
        );
        AzureFoundryActualProvider {
            inner,
            responses_client: None,
            responses_path: String::new(),
            anthropic_client: None,
            anthropic_path: String::new(),
            endpoint: server_uri.to_string(),
            api_version: None,
            auth,
            deployment_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
