use crate::config::tls::provider_tls_config_from_config;
use crate::config::Config;
#[cfg(feature = "local-inference")]
use crate::dictation::whisper::LOCAL_WHISPER_MODEL_CONFIG_KEY;
use crate::providers::api_client::{ApiClient, AuthMethod};
// Local helpers — avoids a dependency on the azure_foundry provider module so
// this file can be reviewed and merged independently of the inference provider.
/// Returns true when the endpoint is an Azure AI Foundry project endpoint
/// (`/api/projects/` in the URL path).
fn is_project_endpoint(url: &str) -> bool {
    url.contains("/api/projects/")
}
/// Entra ID resource scope for Azure AI Foundry project endpoints.
const AZURE_FOUNDRY_PROJECT_ENTRA_RESOURCE: &str = "https://ai.azure.com";
use crate::providers::azureauth::AzureAuth;
use crate::providers::openai::parse_openai_base_url;
use anyhow::Result;
use serde::{Deserialize, Serialize};
#[cfg(feature = "local-inference")]
use std::sync::Mutex;
use std::time::Duration;
use utoipa::ToSchema;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const OPENAI_VERSIONLESS_TRANSCRIPTIONS_PATH: &str = "audio/transcriptions";
type OpenAiDictationTarget = (String, Vec<(String, String)>, String);

#[cfg(feature = "local-inference")]
static LOCAL_TRANSCRIBER: once_cell::sync::Lazy<
    Mutex<Option<(String, super::whisper::WhisperTranscriber)>>,
> = once_cell::sync::Lazy::new(|| Mutex::new(None));

#[cfg(feature = "local-inference")]
const WHISPER_TOKENIZER_JSON: &str = include_str!("whisper_data/tokens.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DictationProvider {
    OpenAI,
    ElevenLabs,
    Groq,
    #[serde(rename = "azure_foundry")]
    AzureFoundry,
    #[cfg(feature = "local-inference")]
    Local,
}

pub struct DictationProviderDef {
    pub provider: DictationProvider,
    pub config_key: &'static str,
    pub default_base_url: &'static str,
    pub endpoint_path: &'static str,
    pub host_key: Option<&'static str>,
    pub description: &'static str,
    pub uses_provider_config: bool,
    pub settings_path: Option<&'static str>,
}

pub const PROVIDERS: &[DictationProviderDef] = &[
    DictationProviderDef {
        provider: DictationProvider::OpenAI,
        config_key: "OPENAI_API_KEY",
        default_base_url: "https://api.openai.com",
        endpoint_path: "v1/audio/transcriptions",
        host_key: Some("OPENAI_HOST"),
        description: "Uses OpenAI Whisper API for high-quality transcription.",
        uses_provider_config: true,
        settings_path: Some("Settings > Models"),
    },
    DictationProviderDef {
        provider: DictationProvider::Groq,
        config_key: "GROQ_API_KEY",
        default_base_url: "https://api.groq.com/openai/v1",
        endpoint_path: "audio/transcriptions",
        host_key: None,
        description: "Uses Groq's ultra-fast Whisper implementation with LPU acceleration.",
        uses_provider_config: false,
        settings_path: None,
    },
    DictationProviderDef {
        provider: DictationProvider::ElevenLabs,
        config_key: "ELEVENLABS_API_KEY",
        default_base_url: "https://api.elevenlabs.io",
        endpoint_path: "v1/speech-to-text",
        host_key: None,
        description: "Uses ElevenLabs speech-to-text API for advanced voice processing.",
        uses_provider_config: false,
        settings_path: None,
    },
    DictationProviderDef {
        provider: DictationProvider::AzureFoundry,
        // The Azure AI Foundry hub and the backing Azure AI Services resource share the
        // same API key (unified resource). AZURE_FOUNDRY_API_KEY == the AI Services key.
        // AZURE_SPEECH_KEY is an optional explicit override; code falls back to
        // AZURE_FOUNDRY_API_KEY automatically when it is not set.
        config_key: "AZURE_SPEECH_KEY",
        default_base_url: "",
        endpoint_path: "speechtotext/transcriptions:transcribe",
        host_key: Some("AZURE_SPEECH_ENDPOINT"),
        description: "Uses Azure AI Foundry speech-to-text via the Fast Transcription API. \
                      Set AZURE_SPEECH_ENDPOINT to the cognitiveservices URL (Azure portal → \
                      AI Foundry Hub → AI Services resource → Keys and Endpoint). \
                      The API key is the same as AZURE_FOUNDRY_API_KEY — no extra key needed.",
        uses_provider_config: false,
        settings_path: None,
    },
];

#[cfg(feature = "local-inference")]
pub const LOCAL_PROVIDER_DEF: DictationProviderDef = DictationProviderDef {
    provider: DictationProvider::Local,
    config_key: LOCAL_WHISPER_MODEL_CONFIG_KEY,
    default_base_url: "",
    endpoint_path: "",
    host_key: None,
    description: "Uses local Whisper model for transcription. No API key needed.",
    uses_provider_config: false,
    settings_path: None,
};

/// Returns all provider definitions, including Local when the `local-inference` feature is enabled.
pub fn all_providers() -> Vec<&'static DictationProviderDef> {
    #[cfg(not(feature = "local-inference"))]
    {
        PROVIDERS.iter().collect()
    }
    #[cfg(feature = "local-inference")]
    {
        let mut all: Vec<&DictationProviderDef> = PROVIDERS.iter().collect();
        all.push(&LOCAL_PROVIDER_DEF);
        all
    }
}

pub fn get_provider_def(provider: DictationProvider) -> &'static DictationProviderDef {
    #[cfg(feature = "local-inference")]
    if provider == DictationProvider::Local {
        return &LOCAL_PROVIDER_DEF;
    }
    PROVIDERS
        .iter()
        .find(|def| def.provider == provider)
        .unwrap()
}

pub fn is_configured(provider: DictationProvider) -> bool {
    let config = Config::global();

    match provider {
        #[cfg(feature = "local-inference")]
        DictationProvider::Local => config
            .get(LOCAL_WHISPER_MODEL_CONFIG_KEY, false)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .and_then(|id| super::whisper::get_model(&id))
            .is_some_and(|m| m.is_downloaded()),
        DictationProvider::AzureFoundry => {
            // Configured when either:
            // • AZURE_SPEECH_ENDPOINT (cognitiveservices URL) is set — direct path.
            // • AZURE_FOUNDRY_ENDPOINT is a *.services.ai.azure.com hub — the speech
            //   endpoint can be auto-derived (derive_cognitive_services_from_foundry
            //   returns Some only for hub domains, not for MaaS *.models.ai.azure.com).
            //   A raw MaaS endpoint alone is NOT sufficient: Fast Transcription is
            //   served from a Speech resource (cognitiveservices), not from a model
            //   endpoint, so we must not report it as speech-configured.
            config.get_param::<String>("AZURE_SPEECH_ENDPOINT").is_ok()
                || config
                    .get_param::<String>("AZURE_FOUNDRY_ENDPOINT")
                    .ok()
                    .and_then(|ep| derive_cognitive_services_from_foundry(&ep))
                    .is_some()
        }
        _ => {
            let def = get_provider_def(provider);
            config.get_secret::<String>(def.config_key).is_ok()
        }
    }
}

#[cfg(feature = "local-inference")]
pub async fn transcribe_local(audio_bytes: Vec<u8>) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let config = Config::global();
        let model_id = config
            .get(LOCAL_WHISPER_MODEL_CONFIG_KEY, false)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| anyhow::anyhow!("Local Whisper model not configured"))?;

        let model = super::whisper::get_model(&model_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown model: {}", model_id))?;
        let model_path = model.local_path();

        let mut transcriber_lock = LOCAL_TRANSCRIBER
            .lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock transcriber: {}", e))?;

        let model_path_str = model_path.to_string_lossy().to_string();
        let needs_reload = match transcriber_lock.as_ref() {
            None => true,
            Some((cached_path, _)) => cached_path != &model_path_str,
        };

        if needs_reload {
            tracing::info!("Loading Whisper model from: {}", model_path.display());

            let transcriber = super::whisper::WhisperTranscriber::new_with_tokenizer(
                &model_id,
                &model_path,
                WHISPER_TOKENIZER_JSON,
            )?;

            *transcriber_lock = Some((model_path_str, transcriber));
        }

        let (_, transcriber) = transcriber_lock.as_mut().unwrap();
        let text = transcriber.transcribe(&audio_bytes).map_err(|e| {
            tracing::error!("Transcription failed: {}", e);
            e
        })?;

        Ok(text)
    })
    .await
    .map_err(|e| {
        tracing::error!("Transcription task failed: {}", e);
        anyhow::anyhow!(e)
    })?
}

fn openai_dictation_target(raw_url: &str) -> Result<OpenAiDictationTarget> {
    let (host, query_params, has_v1) = parse_openai_base_url(raw_url)?;
    let endpoint_path = if has_v1 {
        "v1/audio/transcriptions".to_string()
    } else {
        OPENAI_VERSIONLESS_TRANSCRIPTIONS_PATH.to_string()
    };
    Ok((host, query_params, endpoint_path))
}

fn resolve_openai_base_url_target(raw_url: Option<&str>) -> Result<Option<OpenAiDictationTarget>> {
    raw_url
        .map(str::trim)
        .filter(|raw_url| !raw_url.is_empty())
        .map(openai_dictation_target)
        .transpose()
}

/// Tries to derive an Azure AI Services (cognitiveservices) endpoint from an AI Foundry
/// hub URL. The hub and the backing AI Services resource share the same name prefix:
///   `{name}.services.ai.azure.com` → `https://{name}.cognitiveservices.azure.com/`
///
/// Returns `None` when the endpoint does not follow the expected `services.ai.azure.com`
/// pattern (e.g. custom domains, MaaS model-specific URLs).
fn derive_cognitive_services_from_foundry(foundry_endpoint: &str) -> Option<String> {
    let url = foundry_endpoint.trim_end_matches('/');
    // Strip scheme
    let after_scheme = url.strip_prefix("https://")?;
    // Take only the host part (before first '/')
    let host = after_scheme.split('/').next()?;
    // Only handle *.services.ai.azure.com hosts
    let name = host.strip_suffix(".services.ai.azure.com")?;
    Some(format!("https://{}.cognitiveservices.azure.com/", name))
}

/// Azure AI Foundry Fast Transcription API version.
/// API version query-param for the Azure Fast Transcription REST API.
/// Used by both `*.services.ai.azure.com` and `*.cognitiveservices.azure.com` endpoints.
const AZURE_FOUNDRY_SPEECH_API_VERSION: &str = "2024-11-15";
// *.cognitiveservices.azure.com uses the SAME Fast Transcription URL format as
// *.services.ai.azure.com (?api-version=2024-11-15), but requires the header
// "Ocp-Apim-Subscription-Key" instead of "api-key" for subscription key auth.

/// Derives the speech transcription URL from an Azure AI Foundry endpoint.
///
/// Two endpoint formats are supported:
///
/// - **Project** (`https://<hub>.services.ai.azure.com/api/projects/<project>`):
///   The `Azure-Speech-to-text` model is deployed within the project scope, so the
///   speech endpoint is at the project level — the project API key is valid here.
///   URL: `{project_endpoint}/speechtotext/transcriptions:transcribe?api-version=…`
///
/// - **MaaS** (`https://<hub>.services.ai.azure.com/models`): strip the `/models`
///   suffix to obtain the hub base URL; the speech endpoint is at hub level and
///   requires an Azure AI Services key (`AZURE_SPEECH_KEY`) or Entra ID with the
///   `https://cognitiveservices.azure.com` scope.
fn derive_azure_foundry_speech_url(foundry_endpoint: &str) -> String {
    let trimmed = foundry_endpoint.trim_end_matches('/');

    let base = if trimmed.contains("/api/projects/") {
        // Project endpoint — speech is available at the project scope because
        // Azure-Speech-to-text is deployed within the project.
        // Keep the full project path so that the project API key is valid.
        trimmed
    } else {
        // MaaS endpoint — strip the trailing /models segment if present.
        trimmed
            .strip_suffix("models")
            .unwrap_or(trimmed)
            .trim_end_matches('/')
    };

    format!(
        "{}/speechtotext/transcriptions:transcribe?api-version={}",
        base, AZURE_FOUNDRY_SPEECH_API_VERSION
    )
}

/// Transcribes audio using the Azure Fast Transcription REST API.
///
/// **Priority 1 — explicit `AZURE_SPEECH_ENDPOINT`** (recommended):
/// Set `AZURE_SPEECH_ENDPOINT` to the Azure AI Services (Cognitive Services) URL
/// shown in Azure portal under your AI Foundry Hub → "Azure AI services resource"
/// → "Keys and Endpoint", e.g. `https://<name>.cognitiveservices.azure.com/`.
/// The "Azure-Speech-to-text" model from registry `azureml-cogsvc` shows "Adjust"
/// in the Foundry portal because it is built into the AI Services resource — no
/// separate deployment is required.
/// Auth: the Azure AI Foundry hub and the Azure AI Services resource are **unified**
/// — `AZURE_FOUNDRY_API_KEY` and the AI Services key are the same key.
/// Key resolution: `AZURE_SPEECH_KEY` (explicit override) → `AZURE_FOUNDRY_API_KEY`
/// (automatic fallback, no extra configuration needed) → Entra ID (`az login`,
/// `https://cognitiveservices.azure.com` scope).
///
/// **Priority 2 — derive from `AZURE_FOUNDRY_ENDPOINT`** (legacy fallback):
/// For project endpoints (`/api/projects/…`), uses the project API key.
/// For MaaS endpoints, uses `AZURE_SPEECH_KEY` or Entra ID.
async fn transcribe_with_azure_foundry(
    audio_bytes: Vec<u8>,
    extension: &str,
    mime_type: &str,
) -> Result<String> {
    let config = Config::global();

    // Optional locale, e.g. "fr-FR", "en-US".  Empty string = Azure auto-detect.
    let locale: Option<String> = config
        .get_param::<String>("AZURE_SPEECH_LOCALE")
        .ok()
        .filter(|l: &String| !l.is_empty());

    // Priority 1: explicit Azure AI Services (Cognitive Services) endpoint.
    // This is the endpoint shown in Azure portal → AI Foundry Hub → AI Services resource
    // → "Keys and Endpoint" (e.g. https://<name>.cognitiveservices.azure.com/).
    // The Azure-Speech-to-text model (azureml-cogsvc registry) is built into this
    // resource — no separate deployment is needed ("Adjust" action in the portal).
    if let Ok(speech_endpoint) = config.get_param::<String>("AZURE_SPEECH_ENDPOINT") {
        // Key resolution: AZURE_SPEECH_KEY (explicit override) →
        // AZURE_FOUNDRY_API_KEY (same unified key, automatic fallback) → Entra ID.
        let speech_key = config
            .get_secret("AZURE_SPEECH_KEY")
            .ok()
            .filter(|k: &String| !k.is_empty())
            .or_else(|| {
                config
                    .get_secret("AZURE_FOUNDRY_API_KEY")
                    .ok()
                    .filter(|k: &String| !k.is_empty())
            });
        // cognitiveservices endpoint — use cognitiveservices.azure.com Entra ID scope.
        let auth = AzureAuth::new(speech_key, None).map_err(anyhow::Error::from)?;
        let (mut header_name, header_value) =
            auth.auth_header().await.map_err(anyhow::Error::from)?;
        let base = speech_endpoint.trim_end_matches('/');
        // Both *.cognitiveservices.azure.com and *.services.ai.azure.com use the same
        // ?api-version= URL format for the Fast Transcription API.
        // The difference is only in the auth header name for key-based auth.
        let speech_url = format!(
            "{}/speechtotext/transcriptions:transcribe?api-version={}",
            base, AZURE_FOUNDRY_SPEECH_API_VERSION
        );
        // cognitiveservices.azure.com requires "Ocp-Apim-Subscription-Key".
        // services.ai.azure.com uses "api-key". Bearer tokens work on both.
        if base.contains("cognitiveservices.azure.com") && header_name == "api-key" {
            header_name = "Ocp-Apim-Subscription-Key".to_string();
        }
        tracing::info!(
            speech_url,
            "azure_foundry dictation: using AZURE_SPEECH_ENDPOINT"
        );
        return transcribe_speech_request(
            &speech_url,
            &header_name,
            &header_value,
            audio_bytes,
            extension,
            mime_type,
            locale.as_deref(),
        )
        .await;
    }

    // Priority 2: derive speech URL from AZURE_FOUNDRY_ENDPOINT.
    let foundry_endpoint: String = config
        .get_param("AZURE_FOUNDRY_ENDPOINT")
        .map_err(|_| anyhow::anyhow!(
            "Configure AZURE_SPEECH_ENDPOINT (cognitiveservices URL from Azure portal              → AI Foundry Hub → AI Services resource → Keys and Endpoint)              or AZURE_FOUNDRY_ENDPOINT"
        ))?;

    // Priority 2a: auto-derive cognitiveservices URL from the hub domain.
    // *.services.ai.azure.com → *.cognitiveservices.azure.com (same resource, unified key).
    // This is tried before the project-scope path because the cognitiveservices endpoint
    // exposes the Fast Transcription API reliably, while the project-scope path may return
    // "400 API version not supported" for the /speechtotext path.
    if let Some(derived_cs) = derive_cognitive_services_from_foundry(&foundry_endpoint) {
        let speech_key = config
            .get_secret("AZURE_SPEECH_KEY")
            .ok()
            .filter(|k: &String| !k.is_empty())
            .or_else(|| {
                config
                    .get_secret("AZURE_FOUNDRY_API_KEY")
                    .ok()
                    .filter(|k: &String| !k.is_empty())
            });
        let auth = AzureAuth::new(speech_key, None).map_err(anyhow::Error::from)?;
        let (mut header_name, header_value) =
            auth.auth_header().await.map_err(anyhow::Error::from)?;
        let base = derived_cs.trim_end_matches('/');
        let speech_url = format!(
            "{}/speechtotext/transcriptions:transcribe?api-version={}",
            base, AZURE_FOUNDRY_SPEECH_API_VERSION
        );
        if header_name == "api-key" {
            header_name = "Ocp-Apim-Subscription-Key".to_string();
        }
        tracing::info!(
            speech_url,
            "azure_foundry dictation: auto-derived cognitiveservices endpoint"
        );
        return transcribe_speech_request(
            &speech_url,
            &header_name,
            &header_value,
            audio_bytes,
            extension,
            mime_type,
            locale.as_deref(),
        )
        .await;
    }

    // Priority 2b: project or MaaS endpoint fallback (no auto-derivation possible).
    let auth = if is_project_endpoint(&foundry_endpoint) {
        let api_key = config
            .get_secret("AZURE_FOUNDRY_API_KEY")
            .ok()
            .filter(|k: &String| !k.is_empty());
        AzureAuth::new_with_resource(api_key, AZURE_FOUNDRY_PROJECT_ENTRA_RESOURCE.to_string())
            .map_err(anyhow::Error::from)?
    } else {
        let speech_key = config
            .get_secret("AZURE_SPEECH_KEY")
            .ok()
            .filter(|k: &String| !k.is_empty())
            .or_else(|| {
                config
                    .get_secret("AZURE_FOUNDRY_API_KEY")
                    .ok()
                    .filter(|k: &String| !k.is_empty())
            });
        AzureAuth::new(speech_key, None).map_err(anyhow::Error::from)?
    };

    let (auth_header_name, auth_header_value) =
        auth.auth_header().await.map_err(anyhow::Error::from)?;

    let speech_url = derive_azure_foundry_speech_url(&foundry_endpoint);
    tracing::info!(
        speech_url,
        "azure_foundry dictation: using foundry endpoint path"
    );

    transcribe_speech_request(
        &speech_url,
        &auth_header_name,
        &auth_header_value,
        audio_bytes,
        extension,
        mime_type,
        locale.as_deref(),
    )
    .await
}

/// Shared HTTP call for the Azure Fast Transcription API.
/// Used by both the `AZURE_SPEECH_ENDPOINT` path and the legacy `AZURE_FOUNDRY_ENDPOINT` path.
async fn transcribe_speech_request(
    speech_url: &str,
    auth_header_name: &str,
    auth_header_value: &str,
    audio_bytes: Vec<u8>,
    extension: &str,
    mime_type: &str,
    locale: Option<&str>,
) -> Result<String> {
    let audio_part = reqwest::multipart::Part::bytes(audio_bytes)
        .file_name(format!("audio.{}", extension))
        .mime_str(mime_type)
        .map_err(|e| anyhow::anyhow!("Failed to set audio MIME type: {}", e))?;

    // Azure Fast Transcription definition JSON.
    // Specifying locales prevents wrong-language detection (e.g. French → Chinese).
    let definition = match locale {
        Some(loc) => format!(r#"{{"locales":["{}"]}}"#, loc),
        None => "{}".to_string(),
    };
    tracing::debug!(speech_url, locale = ?locale, "azure_foundry dictation: definition");

    let form = reqwest::multipart::Form::new()
        .part("audio", audio_part)
        .text("definition", definition);

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()?;

    tracing::debug!(speech_url, "azure_foundry dictation: POST speech request");
    let response = client
        .post(speech_url)
        .header(auth_header_name, auth_header_value)
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(speech_url, error = %e, "Azure Foundry speech request failed");
            e
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        if status == 401 || status == 403 {
            anyhow::bail!(
                "Invalid API key: Azure speech returned {} — {}",
                status,
                error_text
            );
        } else if status == 429 {
            anyhow::bail!("Rate limit exceeded");
        } else if error_text.contains("too short") {
            return Ok(String::new());
        } else {
            anyhow::bail!(
                "Azure Foundry speech API error ({}): {}",
                status,
                error_text
            );
        }
    }

    let data: serde_json::Value = response.json().await.map_err(|e| {
        tracing::error!("Failed to parse Azure Foundry speech response: {}", e);
        anyhow::anyhow!(e)
    })?;

    let text = data["combinedPhrases"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|p| p["text"].as_str())
        .unwrap_or("")
        .to_string();

    Ok(text)
}

fn build_api_client(provider: DictationProvider) -> Result<(ApiClient, String)> {
    let config = Config::global();
    let def = get_provider_def(provider);

    let api_key = config.get_secret(def.config_key).map_err(|e| {
        tracing::error!("{} not configured: {}", def.config_key, e);
        anyhow::anyhow!("{} not configured", def.config_key)
    })?;

    let (base_url, query_params, endpoint_path) = if provider == DictationProvider::OpenAI {
        let openai_base_url = config.get_param::<String>("OPENAI_BASE_URL").ok();

        if let Ok(host) = std::env::var("OPENAI_HOST") {
            (host, vec![], def.endpoint_path.to_string())
        } else if let Some(target) = resolve_openai_base_url_target(openai_base_url.as_deref())? {
            target
        } else if let Ok(host) = config.get_param::<String>("OPENAI_HOST") {
            (host, vec![], def.endpoint_path.to_string())
        } else {
            (
                def.default_base_url.to_string(),
                vec![],
                def.endpoint_path.to_string(),
            )
        }
    } else if let Some(host_key) = def.host_key {
        let base_url = config
            .get(host_key, false)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| def.default_base_url.to_string());
        (base_url, vec![], def.endpoint_path.to_string())
    } else {
        (
            def.default_base_url.to_string(),
            vec![],
            def.endpoint_path.to_string(),
        )
    };

    let auth = match provider {
        DictationProvider::OpenAI => AuthMethod::BearerToken(api_key),
        DictationProvider::Groq => AuthMethod::BearerToken(api_key),
        DictationProvider::ElevenLabs => AuthMethod::ApiKey {
            header_name: "xi-api-key".to_string(),
            key: api_key,
        },
        DictationProvider::AzureFoundry => {
            anyhow::bail!("Azure Foundry uses a dedicated transcription path")
        }
        #[cfg(feature = "local-inference")]
        DictationProvider::Local => anyhow::bail!("Local provider should not use API client"),
    };

    let tls = provider_tls_config_from_config(config)?;
    let mut client = ApiClient::with_timeout_and_tls(base_url, auth, REQUEST_TIMEOUT, tls)
        .map_err(|e| {
            tracing::error!("Failed to create API client: {}", e);
            e
        })?;
    if !query_params.is_empty() {
        client = client.with_query(query_params);
    }
    Ok((client, endpoint_path))
}

pub async fn transcribe_with_provider(
    provider: DictationProvider,
    model_param: String,
    model_value: String,
    audio_bytes: Vec<u8>,
    extension: &str,
    mime_type: &str,
) -> Result<String> {
    if provider == DictationProvider::AzureFoundry {
        return transcribe_with_azure_foundry(audio_bytes, extension, mime_type).await;
    }

    let (client, endpoint_path) = build_api_client(provider)?;

    let part = reqwest::multipart::Part::bytes(audio_bytes)
        .file_name(format!("audio.{}", extension))
        .mime_str(mime_type)
        .map_err(|e| {
            tracing::error!("Failed to create multipart: {}", e);
            anyhow::anyhow!(e)
        })?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text(model_param, model_value);

    let response = client
        .request(None, &endpoint_path)
        .multipart_post(form)
        .await
        .map_err(|e| {
            tracing::error!("Request failed: {}", e);
            e
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();

        if status == 401 || error_text.contains("Invalid API key") {
            anyhow::bail!("Invalid API key");
        } else if status == 429 || error_text.contains("quota") {
            anyhow::bail!("Rate limit exceeded");
        } else if error_text.contains("too short") {
            return Ok(String::new());
        } else {
            anyhow::bail!("API error: {}", error_text);
        }
    }

    let data: serde_json::Value = response.json().await.map_err(|e| {
        tracing::error!("Failed to parse response: {}", e);
        anyhow::anyhow!(e)
    })?;

    let text = data["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'text' field in response"))?
        .to_string();

    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::{
        derive_azure_foundry_speech_url, is_project_endpoint, openai_dictation_target,
        resolve_openai_base_url_target, AZURE_FOUNDRY_SPEECH_API_VERSION,
        OPENAI_VERSIONLESS_TRANSCRIPTIONS_PATH,
    };

    #[test]
    fn openai_dictation_target_preserves_prefix_and_query_params() {
        let (host, query_params, endpoint_path) = openai_dictation_target(
            "https://user:pass@gateway.example.com/openai/v1?api-version=2024-02-01",
        )
        .unwrap();
        assert_eq!(host, "https://user:pass@gateway.example.com/openai");
        assert_eq!(
            query_params,
            vec![("api-version".to_string(), "2024-02-01".to_string())]
        );
        assert_eq!(endpoint_path, "v1/audio/transcriptions");
    }

    #[test]
    fn openai_dictation_target_uses_versionless_endpoint_without_v1() {
        let (host, query_params, endpoint_path) =
            openai_dictation_target("https://gateway.example.com/custom/api").unwrap();
        assert_eq!(host, "https://gateway.example.com/custom/api");
        assert!(query_params.is_empty());
        assert_eq!(endpoint_path, OPENAI_VERSIONLESS_TRANSCRIPTIONS_PATH);
    }

    #[test]
    fn openai_dictation_target_keeps_v1_endpoint_for_bare_host() {
        let (host, query_params, endpoint_path) =
            openai_dictation_target("https://api.openai.com").unwrap();
        assert_eq!(host, "https://api.openai.com");
        assert!(query_params.is_empty());
        assert_eq!(endpoint_path, "v1/audio/transcriptions");
    }

    #[test]
    fn resolve_openai_base_url_target_ignores_blank_values() {
        assert!(resolve_openai_base_url_target(Some("   "))
            .unwrap()
            .is_none());
    }

    #[test]
    fn azure_foundry_speech_url_strips_models_suffix() {
        let url = derive_azure_foundry_speech_url("https://myproject.services.ai.azure.com/models");
        assert_eq!(
            url,
            format!(
                "https://myproject.services.ai.azure.com/speechtotext/transcriptions:transcribe?api-version={}",
                AZURE_FOUNDRY_SPEECH_API_VERSION
            )
        );
    }

    #[test]
    fn azure_foundry_speech_url_handles_trailing_slash() {
        let url =
            derive_azure_foundry_speech_url("https://myproject.services.ai.azure.com/models/");
        assert_eq!(
            url,
            format!(
                "https://myproject.services.ai.azure.com/speechtotext/transcriptions:transcribe?api-version={}",
                AZURE_FOUNDRY_SPEECH_API_VERSION
            )
        );
    }

    #[test]
    fn azure_foundry_speech_url_handles_bare_host() {
        let url = derive_azure_foundry_speech_url("https://myproject.services.ai.azure.com");
        assert_eq!(
            url,
            format!(
                "https://myproject.services.ai.azure.com/speechtotext/transcriptions:transcribe?api-version={}",
                AZURE_FOUNDRY_SPEECH_API_VERSION
            )
        );
    }

    #[test]
    fn azure_foundry_speech_url_keeps_project_scope() {
        // Project endpoint: the Azure-Speech-to-text model is deployed within the
        // project, so the speech API is at the project level (not hub level).
        // The project API key is valid at this path.
        let url = derive_azure_foundry_speech_url(
            "https://myhub.services.ai.azure.com/api/projects/my-project",
        );
        assert_eq!(
            url,
            format!(
                "https://myhub.services.ai.azure.com/api/projects/my-project/speechtotext/transcriptions:transcribe?api-version={}",
                AZURE_FOUNDRY_SPEECH_API_VERSION
            )
        );
    }

    #[test]
    fn azure_foundry_speech_url_project_with_trailing_slash() {
        let url = derive_azure_foundry_speech_url(
            "https://myhub.services.ai.azure.com/api/projects/my-project/",
        );
        assert_eq!(
            url,
            format!(
                "https://myhub.services.ai.azure.com/api/projects/my-project/speechtotext/transcriptions:transcribe?api-version={}",
                AZURE_FOUNDRY_SPEECH_API_VERSION
            )
        );
    }

    // ── AZURE_SPEECH_ENDPOINT (cognitiveservices) URL construction ────────────

    #[test]
    fn derive_cognitive_services_from_project_endpoint() {
        use super::derive_cognitive_services_from_foundry;
        let cs = derive_cognitive_services_from_foundry(
            "https://servier-difa-foundry-nprd.services.ai.azure.com/api/projects/proj",
        );
        assert_eq!(
            cs,
            Some("https://servier-difa-foundry-nprd.cognitiveservices.azure.com/".to_string())
        );
    }

    #[test]
    fn derive_cognitive_services_from_maas_endpoint() {
        use super::derive_cognitive_services_from_foundry;
        // MaaS model-specific endpoint doesn't match *.services.ai.azure.com → None
        let cs =
            derive_cognitive_services_from_foundry("https://myphi4.models.ai.azure.com/models");
        assert_eq!(cs, None);
    }

    #[test]
    fn cognitive_services_endpoint_uses_query_param_url() {
        // cognitiveservices.azure.com uses the SAME ?api-version= format — NOT /v3.2/.
        // The only difference vs services.ai.azure.com is the auth header name.
        let base = "https://myresource.cognitiveservices.azure.com";
        let url = format!(
            "{}/speechtotext/transcriptions:transcribe?api-version={}",
            base, AZURE_FOUNDRY_SPEECH_API_VERSION
        );
        assert_eq!(
            url,
            format!("https://myresource.cognitiveservices.azure.com/speechtotext/transcriptions:transcribe?api-version={}", AZURE_FOUNDRY_SPEECH_API_VERSION)
        );
    }

    #[test]
    fn cognitive_services_overrides_api_key_header() {
        // api-key → 401 on cognitiveservices.azure.com.
        // Ocp-Apim-Subscription-Key → 200.
        let base = "https://myresource.cognitiveservices.azure.com";
        let mut header = "api-key".to_string();
        if base.contains("cognitiveservices.azure.com") && header == "api-key" {
            header = "Ocp-Apim-Subscription-Key".to_string();
        }
        assert_eq!(header, "Ocp-Apim-Subscription-Key");
    }

    #[test]
    fn services_ai_endpoint_uses_query_param_versioned_url() {
        let base = "https://myhub.services.ai.azure.com";
        let url = format!(
            "{}/speechtotext/transcriptions:transcribe?api-version={}",
            base, AZURE_FOUNDRY_SPEECH_API_VERSION
        );
        assert_eq!(
            url,
            format!("https://myhub.services.ai.azure.com/speechtotext/transcriptions:transcribe?api-version={}", AZURE_FOUNDRY_SPEECH_API_VERSION)
        );
    }

    #[test]
    fn azure_speech_endpoint_url_no_trailing_slash() {
        let base = "https://myresource.cognitiveservices.azure.com/";
        let url = format!(
            "{}/speechtotext/transcriptions:transcribe?api-version={}",
            base.trim_end_matches('/'),
            AZURE_FOUNDRY_SPEECH_API_VERSION
        );
        assert_eq!(
            url,
            format!("https://myresource.cognitiveservices.azure.com/speechtotext/transcriptions:transcribe?api-version={}", AZURE_FOUNDRY_SPEECH_API_VERSION)
        );
    }

    // ── Entra ID resource selection ───────────────────────────────────────────

    #[test]
    fn azure_foundry_project_endpoint_uses_ai_azure_com_resource() {
        // is_project_endpoint is a local helper — no import needed.
        assert!(is_project_endpoint(
            "https://myhub.services.ai.azure.com/api/projects/proj"
        ));
    }

    #[test]
    fn azure_foundry_maas_endpoint_does_not_match_project() {
        assert!(!is_project_endpoint(
            "https://myhub.services.ai.azure.com/models"
        ));
    }
}
