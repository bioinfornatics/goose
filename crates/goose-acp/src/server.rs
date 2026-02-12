use agent_client_protocol_schema::{
    AgentCapabilities, AuthMethod, BlobResourceContents, CancelNotification, Content, ContentBlock,
    ContentChunk, EmbeddedResource, EmbeddedResourceResource, ImageContent, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, McpCapabilities, McpServer,
    ModelId, ModelInfo, NewSessionRequest, NewSessionResponse, PermissionOption,
    PermissionOptionKind, PromptCapabilities, PromptRequest, PromptResponse,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse, ResourceLink,
    SessionId, SessionMode, SessionModeId, SessionModeState, SessionModelState,
    SessionNotification, SessionUpdate, SetSessionModeResponse, SetSessionModelResponse,
    StopReason, TextContent, TextResourceContents, ToolCall, ToolCallContent, ToolCallId,
    ToolCallLocation, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use anyhow::Result;
use fs_err as fs;
use goose::agents::extension::{Envs, PLATFORM_EXTENSIONS};
use goose::agents::{Agent, AgentConfig, ExtensionConfig, SessionConfig};
use goose::builtin_extension::register_builtin_extensions;
use goose::config::base::CONFIG_YAML_NAME;
use goose::config::extensions::get_enabled_extensions_with_config;
use goose::config::paths::Paths;
use goose::config::permission::PermissionManager;
use goose::config::Config;
use goose::conversation::message::{ActionRequiredData, Message, MessageContent};
use goose::conversation::Conversation;
use goose::mcp_utils::ToolResult;
use goose::permission::permission_confirmation::PrincipalType;
use goose::permission::{Permission, PermissionConfirmation};
use goose::providers::base::Provider;
use goose::providers::provider_registry::ProviderConstructor;
use goose::registry::manifest::AgentMode;
use goose::session::session_manager::SessionType;
use goose::session::{Session, SessionManager};
use rmcp::model::{CallToolResult, RawContent, ResourceContents, Role};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use url::Url;

struct GooseAcpSession {
    agent: Arc<Agent>,
    messages: Conversation,
    tool_requests: HashMap<String, goose::conversation::message::ToolRequest>,
    cancel_token: Option<CancellationToken>,
    current_mode_id: Option<String>,
}

pub struct GooseAcpAgent {
    sessions: Arc<Mutex<HashMap<String, GooseAcpSession>>>,
    provider_factory: ProviderConstructor,
    config_dir: std::path::PathBuf,
    session_manager: Arc<SessionManager>,
    permission_manager: Arc<PermissionManager>,
    goose_mode: goose::config::GooseMode,
    disable_session_naming: bool,
    builtins: Vec<String>,
    modes: Vec<AgentMode>,
    default_mode: Option<String>,
    notification_sender:
        Arc<tokio::sync::RwLock<Option<Arc<dyn crate::notification::NotificationSender>>>>,
}

fn mcp_server_to_extension_config(mcp_server: McpServer) -> Result<ExtensionConfig, String> {
    match mcp_server {
        McpServer::Stdio(stdio) => Ok(ExtensionConfig::Stdio {
            name: stdio.name,
            description: String::new(),
            cmd: stdio.command.to_string_lossy().to_string(),
            args: stdio.args,
            envs: Envs::new(stdio.env.into_iter().map(|e| (e.name, e.value)).collect()),
            env_keys: vec![],
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        }),
        McpServer::Http(http) => Ok(ExtensionConfig::StreamableHttp {
            name: http.name,
            description: String::new(),
            uri: http.url,
            envs: Envs::default(),
            env_keys: vec![],
            headers: http
                .headers
                .into_iter()
                .map(|h| (h.name, h.value))
                .collect(),
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        }),
        McpServer::Sse(_) => Err("SSE is unsupported, migrate to streamable_http".to_string()),
        _ => Err("Unknown MCP server type".to_string()),
    }
}

fn create_tool_location(path: &str, line: Option<u32>) -> ToolCallLocation {
    let mut loc = ToolCallLocation::new(path);
    if let Some(l) = line {
        loc = loc.line(l);
    }
    loc
}

fn extract_tool_locations(
    tool_request: &goose::conversation::message::ToolRequest,
    tool_response: &goose::conversation::message::ToolResponse,
) -> Vec<ToolCallLocation> {
    let mut locations = Vec::new();

    if let Ok(tool_call) = &tool_request.tool_call {
        if tool_call.name != "developer__text_editor" {
            return locations;
        }

        let path_str = tool_call
            .arguments
            .as_ref()
            .and_then(|args| args.get("path"))
            .and_then(|p| p.as_str());

        if let Some(path_str) = path_str {
            let command = tool_call
                .arguments
                .as_ref()
                .and_then(|args| args.get("command"))
                .and_then(|c| c.as_str());

            if let Ok(result) = &tool_response.tool_result {
                for content in &result.content {
                    if let RawContent::Text(text_content) = &content.raw {
                        let text = &text_content.text;

                        match command {
                            Some("view") => {
                                let line = extract_view_line_range(text)
                                    .map(|range| range.0 as u32)
                                    .or(Some(1));
                                locations.push(create_tool_location(path_str, line));
                            }
                            Some("str_replace") | Some("insert") => {
                                let line = extract_first_line_number(text)
                                    .map(|l| l as u32)
                                    .or(Some(1));
                                locations.push(create_tool_location(path_str, line));
                            }
                            Some("write") => {
                                locations.push(create_tool_location(path_str, Some(1)));
                            }
                            _ => {
                                locations.push(create_tool_location(path_str, Some(1)));
                            }
                        }
                        break;
                    }
                }
            }

            if locations.is_empty() {
                locations.push(create_tool_location(path_str, Some(1)));
            }
        }
    }

    locations
}

fn extract_view_line_range(text: &str) -> Option<(usize, usize)> {
    let re = regex::Regex::new(r"\(lines (\d+)-(\d+|end)\)").ok()?;
    if let Some(caps) = re.captures(text) {
        let start = caps.get(1)?.as_str().parse::<usize>().ok()?;
        let end = if caps.get(2)?.as_str() == "end" {
            start
        } else {
            caps.get(2)?.as_str().parse::<usize>().ok()?
        };
        return Some((start, end));
    }
    None
}

fn extract_first_line_number(text: &str) -> Option<usize> {
    let re = regex::Regex::new(r"```[^\n]*\n(\d+):").ok()?;
    if let Some(caps) = re.captures(text) {
        return caps.get(1)?.as_str().parse::<usize>().ok();
    }
    None
}

fn read_resource_link(link: ResourceLink) -> Option<String> {
    let url = Url::parse(&link.uri).ok()?;
    if url.scheme() == "file" {
        let path = url.to_file_path().ok()?;
        let contents = fs::read_to_string(&path).ok()?;

        Some(format!(
            "\n\n# {}\n```\n{}\n```",
            path.to_string_lossy(),
            contents
        ))
    } else {
        None
    }
}

fn format_tool_name(tool_name: &str) -> String {
    let capitalize = |s: &str| {
        s.split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };

    if let Some((extension, tool)) = tool_name.split_once("__") {
        let formatted_extension = extension.replace('_', " ");
        let formatted_tool = tool.replace('_', " ");
        format!(
            "{}: {}",
            capitalize(&formatted_extension),
            capitalize(&formatted_tool)
        )
    } else {
        let formatted = tool_name.replace('_', " ");
        capitalize(&formatted)
    }
}

async fn add_builtins(agent: &Agent, builtins: Vec<String>) {
    for builtin in builtins {
        let config = if PLATFORM_EXTENSIONS.contains_key(builtin.as_str()) {
            ExtensionConfig::Platform {
                name: builtin.clone(),
                description: builtin.clone(),
                display_name: None,
                bundled: None,
                available_tools: Vec::new(),
            }
        } else {
            ExtensionConfig::Builtin {
                name: builtin.clone(),
                display_name: None,
                timeout: None,
                bundled: None,
                description: builtin.clone(),
                available_tools: Vec::new(),
            }
        };

        match agent
            .extension_manager
            .add_extension(config, None, None, None)
            .await
        {
            Ok(_) => info!(extension = %builtin, "extension loaded"),
            Err(e) => warn!(extension = %builtin, error = %e, "extension load failed"),
        }
    }
}
async fn add_extensions(agent: &Agent, extensions: Vec<ExtensionConfig>) {
    for extension in extensions {
        let name = extension.name().to_string();
        match agent
            .extension_manager
            .add_extension(extension, None, None, None)
            .await
        {
            Ok(_) => info!(extension = %name, "extension loaded"),
            Err(e) => warn!(extension = %name, error = %e, "extension load failed"),
        }
    }
}

async fn build_model_state(
    provider: &dyn Provider,
    current_model: &str,
) -> Result<SessionModelState, agent_client_protocol_schema::Error> {
    let models = provider.fetch_recommended_models().await.map_err(|e| {
        agent_client_protocol_schema::Error::internal_error()
            .data(format!("Failed to fetch models: {}", e))
    })?;
    Ok(SessionModelState::new(
        ModelId::new(current_model),
        models
            .iter()
            .map(|name| ModelInfo::new(ModelId::new(&**name), &**name))
            .collect(),
    ))
}

impl GooseAcpAgent {
    pub fn permission_manager(&self) -> Arc<PermissionManager> {
        Arc::clone(&self.permission_manager)
    }

    pub async fn new(
        provider_factory: ProviderConstructor,
        builtins: Vec<String>,
        data_dir: std::path::PathBuf,
        config_dir: std::path::PathBuf,
        goose_mode: goose::config::GooseMode,
        disable_session_naming: bool,
    ) -> Result<Self> {
        let session_manager = Arc::new(SessionManager::new(data_dir));
        let permission_manager = Arc::new(PermissionManager::new(config_dir.clone()));

        Ok(Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            provider_factory,
            config_dir,
            session_manager,
            permission_manager,
            goose_mode,
            disable_session_naming,
            builtins,
            modes: Vec::new(),
            default_mode: None,
            notification_sender: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    pub fn set_modes(&mut self, modes: Vec<AgentMode>, default_mode: Option<String>) {
        self.default_mode = default_mode;
        self.modes = modes;
    }

    fn build_mode_state(&self) -> Option<SessionModeState> {
        build_mode_state(&self.modes, self.default_mode.as_deref())
    }

    async fn create_agent_for_session(&self) -> Arc<Agent> {
        let agent = Agent::with_config(AgentConfig::new(
            Arc::clone(&self.session_manager),
            Arc::clone(&self.permission_manager),
            None,
            self.goose_mode,
            self.disable_session_naming,
        ));
        let agent = Arc::new(agent);

        let config_path = self.config_dir.join(CONFIG_YAML_NAME);
        if let Ok(config_file) = Config::new(&config_path, "goose") {
            let extensions = get_enabled_extensions_with_config(&config_file);
            add_extensions(&agent, extensions).await;
        }
        add_builtins(&agent, self.builtins.clone()).await;

        agent
    }

    pub async fn has_session(&self, session_id: &str) -> bool {
        self.sessions.lock().await.contains_key(session_id)
    }

    pub async fn set_notification_sender(
        &self,
        sender: Arc<dyn crate::notification::NotificationSender>,
    ) {
        *self.notification_sender.write().await = Some(sender);
    }

    async fn notify(
        &self,
        notification: SessionNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        let sender = self.notification_sender.read().await;
        match sender.as_ref() {
            Some(s) => s
                .send_notification(notification)
                .await
                .map_err(|_| agent_client_protocol_schema::Error::internal_error()),
            None => Err(agent_client_protocol_schema::Error::internal_error()),
        }
    }

    async fn request_permission(
        &self,
        request: RequestPermissionRequest,
    ) -> Result<RequestPermissionResponse, agent_client_protocol_schema::Error> {
        let sender = self.notification_sender.read().await;
        match sender.as_ref() {
            Some(s) => s
                .request_permission(request)
                .await
                .map_err(|_| agent_client_protocol_schema::Error::internal_error()),
            None => Err(agent_client_protocol_schema::Error::internal_error()),
        }
    }

    fn convert_acp_prompt_to_message(&self, prompt: Vec<ContentBlock>) -> Message {
        let mut user_message = Message::user();

        for block in prompt {
            match block {
                ContentBlock::Text(text) => {
                    user_message = user_message.with_text(&text.text);
                }
                ContentBlock::Image(image) => {
                    user_message = user_message.with_image(&image.data, &image.mime_type);
                }
                ContentBlock::Resource(resource) => {
                    if let EmbeddedResourceResource::TextResourceContents(text_resource) =
                        &resource.resource
                    {
                        let header = format!("--- Resource: {} ---\n", text_resource.uri);
                        let content = format!("{}{}\n---\n", header, text_resource.text);
                        user_message = user_message.with_text(&content);
                    }
                }
                ContentBlock::ResourceLink(link) => {
                    if let Some(text) = read_resource_link(link) {
                        user_message = user_message.with_text(text)
                    }
                }
                ContentBlock::Audio(..) | _ => (),
            }
        }

        user_message
    }

    async fn handle_message_content(
        &self,
        content_item: &MessageContent,
        session_id: &SessionId,
        session: &mut GooseAcpSession,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        match content_item {
            MessageContent::Text(text) => {
                self.notify(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(text.text.clone()),
                    ))),
                ))
                .await?;
            }
            MessageContent::ToolRequest(tool_request) => {
                self.handle_tool_request(tool_request, session_id, session)
                    .await?;
            }
            MessageContent::ToolResponse(tool_response) => {
                self.handle_tool_response(tool_response, session_id, session)
                    .await?;
            }
            MessageContent::Thinking(thinking) => {
                self.notify(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new(thinking.thinking.clone()),
                    ))),
                ))
                .await?;
            }
            MessageContent::ActionRequired(action_required) => {
                if let ActionRequiredData::ToolConfirmation {
                    id,
                    tool_name,
                    arguments,
                    prompt,
                } = &action_required.data
                {
                    self.handle_tool_permission_request(
                        &session.agent,
                        session_id,
                        id.clone(),
                        tool_name.clone(),
                        arguments.clone(),
                        prompt.clone(),
                    )
                    .await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_tool_request(
        &self,
        tool_request: &goose::conversation::message::ToolRequest,
        session_id: &SessionId,
        session: &mut GooseAcpSession,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        session
            .tool_requests
            .insert(tool_request.id.clone(), tool_request.clone());

        let tool_name = match &tool_request.tool_call {
            Ok(tool_call) => tool_call.name.to_string(),
            Err(_) => "error".to_string(),
        };

        self.notify(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCall(
                ToolCall::new(
                    ToolCallId::new(tool_request.id.clone()),
                    format_tool_name(&tool_name),
                )
                .status(ToolCallStatus::Pending),
            ),
        ))
        .await?;

        Ok(())
    }

    async fn handle_tool_response(
        &self,
        tool_response: &goose::conversation::message::ToolResponse,
        session_id: &SessionId,
        session: &mut GooseAcpSession,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        let status = match &tool_response.tool_result {
            Ok(result) if result.is_error == Some(true) => ToolCallStatus::Failed,
            Ok(_) => ToolCallStatus::Completed,
            Err(_) => ToolCallStatus::Failed,
        };

        let content = build_tool_call_content(&tool_response.tool_result);

        let locations = if let Some(tool_request) = session.tool_requests.get(&tool_response.id) {
            extract_tool_locations(tool_request, tool_response)
        } else {
            Vec::new()
        };

        let mut fields = ToolCallUpdateFields::new().status(status).content(content);
        if !locations.is_empty() {
            fields = fields.locations(locations);
        }
        self.notify(SessionNotification::new(
            session_id.clone(),
            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                ToolCallId::new(tool_response.id.clone()),
                fields,
            )),
        ))
        .await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_tool_permission_request(
        &self,
        agent: &Arc<Agent>,
        session_id: &SessionId,
        request_id: String,
        tool_name: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        prompt: Option<String>,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        let agent = agent.clone();
        let session_id = session_id.clone();

        let formatted_name = format_tool_name(&tool_name);

        let mut fields = ToolCallUpdateFields::new()
            .title(formatted_name)
            .kind(ToolKind::default())
            .status(ToolCallStatus::Pending)
            .raw_input(serde_json::Value::Object(arguments));
        if let Some(p) = prompt {
            fields = fields.content(vec![ToolCallContent::Content(Content::new(
                ContentBlock::Text(TextContent::new(p)),
            ))]);
        }
        let tool_call_update = ToolCallUpdate::new(ToolCallId::new(request_id.clone()), fields);

        fn option(kind: PermissionOptionKind) -> PermissionOption {
            let id = serde_json::to_value(kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            PermissionOption::new(id.clone(), id, kind)
        }
        let options = vec![
            option(PermissionOptionKind::AllowAlways),
            option(PermissionOptionKind::AllowOnce),
            option(PermissionOptionKind::RejectOnce),
            option(PermissionOptionKind::RejectAlways),
        ];

        let permission_request =
            RequestPermissionRequest::new(session_id, tool_call_update, options);

        match self.request_permission(permission_request).await {
            Ok(response) => {
                agent
                    .handle_confirmation(request_id, outcome_to_confirmation(&response.outcome))
                    .await;
            }
            Err(e) => {
                error!(error = ?e, "permission request failed");
                agent
                    .handle_confirmation(
                        request_id,
                        PermissionConfirmation {
                            principal_type: PrincipalType::Tool,
                            permission: Permission::Cancel,
                        },
                    )
                    .await;
            }
        }
        Ok(())
    }
}

fn outcome_to_confirmation(outcome: &RequestPermissionOutcome) -> PermissionConfirmation {
    let permission = match outcome {
        RequestPermissionOutcome::Cancelled => Permission::Cancel,
        RequestPermissionOutcome::Selected(selected) => {
            match serde_json::from_value::<PermissionOptionKind>(serde_json::Value::String(
                selected.option_id.0.to_string(),
            )) {
                Ok(PermissionOptionKind::AllowAlways) => Permission::AlwaysAllow,
                Ok(PermissionOptionKind::AllowOnce) => Permission::AllowOnce,
                Ok(PermissionOptionKind::RejectOnce) => Permission::DenyOnce,
                Ok(PermissionOptionKind::RejectAlways) => Permission::AlwaysDeny,
                _ => Permission::Cancel,
            }
        }
        _ => Permission::Cancel,
    };
    PermissionConfirmation {
        principal_type: PrincipalType::Tool,
        permission,
    }
}

fn build_tool_call_content(tool_result: &ToolResult<CallToolResult>) -> Vec<ToolCallContent> {
    match tool_result {
        Ok(result) => result
            .content
            .iter()
            .filter_map(|content| match &content.raw {
                RawContent::Text(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Text(TextContent::new(val.text.clone())),
                ))),
                RawContent::Image(val) => Some(ToolCallContent::Content(Content::new(
                    ContentBlock::Image(ImageContent::new(val.data.clone(), val.mime_type.clone())),
                ))),
                RawContent::Resource(val) => {
                    let resource = match &val.resource {
                        ResourceContents::TextResourceContents {
                            mime_type,
                            text,
                            uri,
                            ..
                        } => EmbeddedResourceResource::TextResourceContents(
                            TextResourceContents::new(text.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                        ResourceContents::BlobResourceContents {
                            mime_type,
                            blob,
                            uri,
                            ..
                        } => EmbeddedResourceResource::BlobResourceContents(
                            BlobResourceContents::new(blob.clone(), uri.clone())
                                .mime_type(mime_type.clone()),
                        ),
                    };
                    Some(ToolCallContent::Content(Content::new(
                        ContentBlock::Resource(EmbeddedResource::new(resource)),
                    )))
                }
                RawContent::Audio(_) | RawContent::ResourceLink(_) => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

impl GooseAcpAgent {
    pub async fn on_initialize(
        &self,
        args: InitializeRequest,
    ) -> Result<InitializeResponse, agent_client_protocol_schema::Error> {
        debug!(?args, "initialize request");

        let capabilities = AgentCapabilities::new()
            .load_session(true)
            .prompt_capabilities(
                PromptCapabilities::new()
                    .image(true)
                    .audio(false)
                    .embedded_context(true),
            )
            .mcp_capabilities(McpCapabilities::new().http(true));
        Ok(InitializeResponse::new(args.protocol_version)
            .agent_capabilities(capabilities)
            .auth_methods(vec![AuthMethod::new(
                "goose-provider",
                "Configure Provider",
            )
            .description(
                "Run `goose configure` to set up your AI provider and API key",
            )]))
    }

    pub async fn on_new_session(
        &self,
        args: NewSessionRequest,
    ) -> Result<NewSessionResponse, agent_client_protocol_schema::Error> {
        debug!(?args, "new session request");

        let goose_session = self
            .session_manager
            .create_session(
                args.cwd.clone(),
                "ACP Session".to_string(),
                SessionType::User,
            )
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to create session: {}", e))
            })?;

        let agent = self.create_agent_for_session().await;
        let provider = self
            .init_provider(&agent, &goose_session)
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to set provider: {}", e))
            })?;

        for mcp_server in args.mcp_servers {
            let config = match mcp_server_to_extension_config(mcp_server) {
                Ok(c) => c,
                Err(msg) => {
                    return Err(agent_client_protocol_schema::Error::invalid_params().data(msg));
                }
            };
            let name = config.name().to_string();
            if let Err(e) = agent.add_extension(config, &goose_session.id).await {
                return Err(agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to add MCP server '{}': {}", name, e)));
            }
        }

        let default_mode_id = self
            .default_mode
            .clone()
            .or_else(|| self.modes.first().map(|m| m.slug.clone()));

        if let Some(ref mode_id) = default_mode_id {
            if let Some(mode) = self.modes.iter().find(|m| m.slug == *mode_id) {
                if let Some(instructions) = resolve_mode_instructions(mode) {
                    agent
                        .extend_system_prompt("agent_mode".to_string(), instructions)
                        .await;
                }
            }
        }

        let session = GooseAcpSession {
            agent,
            messages: Conversation::new_unvalidated(Vec::new()),
            tool_requests: HashMap::new(),
            cancel_token: None,
            current_mode_id: default_mode_id,
        };

        let mut sessions = self.sessions.lock().await;
        sessions.insert(goose_session.id.clone(), session);

        info!(
            session_id = %goose_session.id,
            session_type = "acp",
            "Session started"
        );

        let model_state =
            build_model_state(&*provider, &provider.get_model_config().model_name).await?;

        let mut response =
            NewSessionResponse::new(SessionId::new(goose_session.id)).models(model_state);
        if let Some(mode_state) = self.build_mode_state() {
            response = response.modes(mode_state);
        }
        Ok(response)
    }

    async fn init_provider(&self, agent: &Agent, session: &Session) -> Result<Arc<dyn Provider>> {
        let model_config = match &session.model_config {
            Some(config) => config.clone(),
            None => {
                let config_path = self.config_dir.join(CONFIG_YAML_NAME);
                let config = Config::new(&config_path, "goose")?;
                let model_id = config.get_goose_model()?;
                goose::model::ModelConfig::new(&model_id)?
            }
        };
        let provider = (self.provider_factory)(model_config, Vec::new()).await?;
        agent.update_provider(provider.clone(), &session.id).await?;
        Ok(provider)
    }

    pub async fn on_load_session(
        &self,
        args: LoadSessionRequest,
    ) -> Result<LoadSessionResponse, agent_client_protocol_schema::Error> {
        debug!(?args, "load session request");

        let session_id = args.session_id.0.to_string();

        let goose_session = self
            .session_manager
            .get_session(&session_id, true)
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::invalid_params()
                    .data(format!("Failed to load session {}: {}", session_id, e))
            })?;

        let agent = self.create_agent_for_session().await;
        let provider = self
            .init_provider(&agent, &goose_session)
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to set provider: {}", e))
            })?;

        let conversation = goose_session.conversation.ok_or_else(|| {
            agent_client_protocol_schema::Error::internal_error()
                .data(format!("Session {} has no conversation data", session_id))
        })?;

        self.session_manager
            .update(&session_id)
            .working_dir(args.cwd.clone())
            .apply()
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to update session working directory: {}", e))
            })?;

        let default_mode_id = self
            .default_mode
            .clone()
            .or_else(|| self.modes.first().map(|m| m.slug.clone()));

        if let Some(ref mode_id) = default_mode_id {
            if let Some(mode) = self.modes.iter().find(|m| m.slug == *mode_id) {
                if let Some(instructions) = resolve_mode_instructions(mode) {
                    agent
                        .extend_system_prompt("agent_mode".to_string(), instructions)
                        .await;
                }
            }
        }

        let mut session = GooseAcpSession {
            agent,
            messages: conversation.clone(),
            tool_requests: HashMap::new(),
            cancel_token: None,
            current_mode_id: default_mode_id,
        };

        for message in conversation.messages() {
            if !message.metadata.user_visible {
                continue;
            }

            for content_item in &message.content {
                match content_item {
                    MessageContent::Text(text) => {
                        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new(
                            text.text.clone(),
                        )));
                        let update = match message.role {
                            Role::User => SessionUpdate::UserMessageChunk(chunk),
                            Role::Assistant => SessionUpdate::AgentMessageChunk(chunk),
                        };
                        self.notify(SessionNotification::new(args.session_id.clone(), update))
                            .await?;
                    }
                    MessageContent::ToolRequest(tool_request) => {
                        self.handle_tool_request(tool_request, &args.session_id, &mut session)
                            .await?;
                    }
                    MessageContent::ToolResponse(tool_response) => {
                        self.handle_tool_response(tool_response, &args.session_id, &mut session)
                            .await?;
                    }
                    MessageContent::Thinking(thinking) => {
                        self.notify(SessionNotification::new(
                            args.session_id.clone(),
                            SessionUpdate::AgentThoughtChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(thinking.thinking.clone())),
                            )),
                        ))
                        .await?;
                    }
                    _ => {}
                }
            }
        }

        let mut sessions = self.sessions.lock().await;
        sessions.insert(session_id.clone(), session);

        info!(
            session_id = %session_id,
            session_type = "acp",
            "Session loaded"
        );

        let model_state =
            build_model_state(&*provider, &provider.get_model_config().model_name).await?;

        let mut response = LoadSessionResponse::new().models(model_state);
        if let Some(mode_state) = self.build_mode_state() {
            response = response.modes(mode_state);
        }
        Ok(response)
    }

    pub async fn on_set_mode(
        &self,
        session_id: &str,
        mode_id: &str,
    ) -> Result<SetSessionModeResponse, agent_client_protocol_schema::Error> {
        let mode = self
            .modes
            .iter()
            .find(|m| m.slug == mode_id)
            .ok_or_else(|| {
                agent_client_protocol_schema::Error::invalid_params()
                    .data(format!("Unknown mode: {}", mode_id))
            })?;

        let instructions = resolve_mode_instructions(mode);

        let mut sessions = self.sessions.lock().await;
        let session = sessions.get_mut(session_id).ok_or_else(|| {
            agent_client_protocol_schema::Error::invalid_params()
                .data(format!("Session not found: {}", session_id))
        })?;

        session.current_mode_id = Some(mode_id.to_string());

        if let Some(instructions) = instructions {
            session
                .agent
                .extend_system_prompt("agent_mode".to_string(), instructions)
                .await;
        } else {
            session
                .agent
                .extend_system_prompt("agent_mode".to_string(), String::new())
                .await;
        }

        info!(session_id = %session_id, mode_id = %mode_id, "Session mode changed");
        Ok(SetSessionModeResponse::new())
    }

    pub async fn on_prompt(
        &self,
        args: PromptRequest,
    ) -> Result<PromptResponse, agent_client_protocol_schema::Error> {
        let session_id = args.session_id.0.to_string();
        let cancel_token = CancellationToken::new();

        let agent = {
            let mut sessions = self.sessions.lock().await;
            let session = sessions.get_mut(&session_id).ok_or_else(|| {
                agent_client_protocol_schema::Error::invalid_params()
                    .data(format!("Session not found: {}", session_id))
            })?;
            session.cancel_token = Some(cancel_token.clone());
            session.agent.clone()
        };

        let user_message = self.convert_acp_prompt_to_message(args.prompt);

        let session_config = SessionConfig {
            id: session_id.clone(),
            schedule_id: None,
            max_turns: None,
            retry_config: None,
        };

        let mut stream = agent
            .reply(user_message, session_config, Some(cancel_token.clone()))
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Error getting agent reply: {}", e))
            })?;

        use futures::StreamExt;

        let mut was_cancelled = false;

        while let Some(event) = stream.next().await {
            if cancel_token.is_cancelled() {
                was_cancelled = true;
                break;
            }

            match event {
                Ok(goose::agents::AgentEvent::Message(message)) => {
                    let mut sessions = self.sessions.lock().await;
                    let session = sessions.get_mut(&session_id).ok_or_else(|| {
                        agent_client_protocol_schema::Error::invalid_params()
                            .data(format!("Session not found: {}", session_id))
                    })?;

                    session.messages.push(message.clone());

                    for content_item in &message.content {
                        self.handle_message_content(content_item, &args.session_id, session)
                            .await?;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    return Err(agent_client_protocol_schema::Error::internal_error()
                        .data(format!("Error in agent response stream: {}", e)));
                }
            }
        }

        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.cancel_token = None;
        }

        Ok(PromptResponse::new(if was_cancelled {
            StopReason::Cancelled
        } else {
            StopReason::EndTurn
        }))
    }

    pub async fn on_cancel(
        &self,
        args: CancelNotification,
    ) -> Result<(), agent_client_protocol_schema::Error> {
        debug!(?args, "cancel request");

        let session_id = args.session_id.0.to_string();
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.get_mut(&session_id) {
            if let Some(ref token) = session.cancel_token {
                info!(session_id = %session_id, "prompt cancelled");
                token.cancel();
            }
        } else {
            warn!(session_id = %session_id, "cancel request for unknown session");
        }

        Ok(())
    }

    pub async fn on_set_model(
        &self,
        session_id: &str,
        model_id: &str,
    ) -> Result<SetSessionModelResponse, agent_client_protocol_schema::Error> {
        let model_config = goose::model::ModelConfig::new(model_id).map_err(|e| {
            agent_client_protocol_schema::Error::invalid_params()
                .data(format!("Invalid model config: {}", e))
        })?;
        let provider = (self.provider_factory)(model_config, Vec::new())
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to create provider: {}", e))
            })?;

        let agent = {
            let sessions = self.sessions.lock().await;
            let session = sessions.get(session_id).ok_or_else(|| {
                agent_client_protocol_schema::Error::invalid_params()
                    .data(format!("Session not found: {}", session_id))
            })?;
            session.agent.clone()
        };
        agent
            .update_provider(provider, session_id)
            .await
            .map_err(|e| {
                agent_client_protocol_schema::Error::internal_error()
                    .data(format!("Failed to update provider: {}", e))
            })?;

        info!(session_id = %session_id, model_id = %model_id, "Model switched");
        Ok(SetSessionModelResponse::new())
    }
}

/// Serve an ACP agent over the given byte streams using agent-client-protocol.
///
/// Spawns a dedicated OS thread with a tokio LocalSet because
/// AgentSideConnection uses `!Send` futures internally. The returned future
/// is Send-safe and simply waits for the thread to finish.
///
/// A channel bridges the Send world (GooseAcpAgent notifications) to the
/// !Send world (AgentSideConnection's Client trait methods).
pub async fn serve<R, W>(agent: Arc<GooseAcpAgent>, read: R, write: W) -> Result<()>
where
    R: futures::AsyncRead + Unpin + Send + 'static,
    W: futures::AsyncWrite + Unpin + Send + 'static,
{
    use crate::notification::{AcpNotificationSender, ClientCommand};
    use agent_client_protocol::Client;

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<ClientCommand>();

    // Install the channel-based notification sender on the agent so that
    // on_prompt / on_load_session can send notifications back to the client.
    agent
        .set_notification_sender(Arc::new(AcpNotificationSender::new(cmd_tx)))
        .await;

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for ACP serve thread");

        let local = tokio::task::LocalSet::new();

        local.block_on(&rt, async move {
            let bridge = std::rc::Rc::new(crate::bridge::AcpBridge::new(agent));

            let (conn, io_task) =
                agent_client_protocol::AgentSideConnection::new(bridge, write, read, |fut| {
                    tokio::task::spawn_local(fut);
                });

            // Pump commands from the Send world to the !Send AgentSideConnection.
            let conn = std::rc::Rc::new(conn);
            let pump_conn = conn.clone();
            tokio::task::spawn_local(async move {
                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        ClientCommand::Notify {
                            notification,
                            reply,
                        } => {
                            let result = pump_conn.session_notification(notification).await;
                            let _ = reply.send(result);
                        }
                        ClientCommand::RequestPermission { request, reply } => {
                            let result = pump_conn.request_permission(request).await;
                            let _ = reply.send(result);
                        }
                    }
                }
            });

            if let Err(e) = io_task.await {
                tracing::error!("ACP io_task error: {e}");
            }
        });
    });

    tokio::task::spawn_blocking(move || {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("ACP serve thread panicked"))
    })
    .await??;

    Ok(())
}

pub async fn run(builtins: Vec<String>) -> Result<()> {
    register_builtin_extensions(goose_mcp::BUILTIN_EXTENSIONS.clone());
    info!("listening on stdio");

    let outgoing = tokio::io::stdout().compat_write();
    let incoming = tokio::io::stdin().compat();

    let server =
        crate::server_factory::AcpServer::new(crate::server_factory::AcpServerFactoryConfig {
            builtins,
            data_dir: Paths::data_dir(),
            config_dir: Paths::config_dir(),
        });
    let agent = server.create_agent().await?;
    serve(agent, incoming, outgoing).await
}

/// Resolve instructions for a mode — tries inline first, then renders template file
fn resolve_mode_instructions(mode: &AgentMode) -> Option<String> {
    if let Some(ref instructions) = mode.instructions {
        return Some(instructions.clone());
    }
    if let Some(ref file) = mode.instructions_file {
        match goose::prompt_template::render_template(
            file,
            &std::collections::HashMap::<String, String>::new(),
        ) {
            Ok(rendered) => return Some(rendered),
            Err(e) => {
                tracing::warn!(mode = %mode.slug, file = %file, error = %e, "Failed to render mode instructions_file");
            }
        }
    }
    None
}

fn build_mode_state(modes: &[AgentMode], default_mode: Option<&str>) -> Option<SessionModeState> {
    if modes.is_empty() {
        return None;
    }

    let available: Vec<SessionMode> = modes
        .iter()
        .map(|m| {
            SessionMode::new(SessionModeId::new(&*m.slug), &m.name)
                .description(m.description.clone())
        })
        .collect();

    let current = default_mode.unwrap_or(&modes[0].slug);

    Some(SessionModeState::new(
        SessionModeId::new(current),
        available,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol_schema::{
        EnvVariable, HttpHeader, McpServer, McpServerHttp, McpServerSse, McpServerStdio,
        PermissionOptionId, ResourceLink, SelectedPermissionOutcome,
    };
    use std::io::Write;
    use tempfile::NamedTempFile;
    use test_case::test_case;

    #[test_case(
        McpServer::Stdio(
            McpServerStdio::new("github", "/path/to/github-mcp-server")
                .args(vec!["stdio".into()])
                .env(vec![EnvVariable::new("GITHUB_PERSONAL_ACCESS_TOKEN", "ghp_xxxxxxxxxxxx")])
        ),
        Ok(ExtensionConfig::Stdio {
            name: "github".into(),
            description: String::new(),
            cmd: "/path/to/github-mcp-server".into(),
            args: vec!["stdio".into()],
            envs: Envs::new(
                [(
                    "GITHUB_PERSONAL_ACCESS_TOKEN".into(),
                    "ghp_xxxxxxxxxxxx".into()
                )]
                .into()
            ),
            env_keys: vec![],
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        })
    )]
    #[test_case(
        McpServer::Http(
            McpServerHttp::new("github", "https://api.githubcopilot.com/mcp/")
                .headers(vec![HttpHeader::new("Authorization", "Bearer ghp_xxxxxxxxxxxx")])
        ),
        Ok(ExtensionConfig::StreamableHttp {
            name: "github".into(),
            description: String::new(),
            uri: "https://api.githubcopilot.com/mcp/".into(),
            envs: Envs::default(),
            env_keys: vec![],
            headers: HashMap::from([(
                "Authorization".into(),
                "Bearer ghp_xxxxxxxxxxxx".into()
            )]),
            timeout: None,
            bundled: Some(false),
            available_tools: vec![],
        })
    )]
    #[test_case(
        McpServer::Sse(McpServerSse::new("test-sse", "https://agent-fin.biodnd.com/sse")),
        Err("SSE is unsupported, migrate to streamable_http".to_string())
    )]
    fn test_mcp_server_to_extension_config(
        input: McpServer,
        expected: Result<ExtensionConfig, String>,
    ) {
        assert_eq!(mcp_server_to_extension_config(input), expected);
    }

    fn new_resource_link(content: &str) -> anyhow::Result<(ResourceLink, NamedTempFile)> {
        let mut file = NamedTempFile::new()?;
        file.write_all(content.as_bytes())?;

        let name = file
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let uri = format!("file://{}", file.path().to_str().unwrap());
        let link = ResourceLink::new(name, uri);
        Ok((link, file))
    }

    #[test]
    fn test_read_resource_link_non_file_scheme() {
        let (link, file) = new_resource_link("print(\"hello, world\")").unwrap();

        let result = read_resource_link(link).unwrap();
        let expected = format!(
            "

# {}
```
print(\"hello, world\")
```",
            file.path().to_str().unwrap(),
        );

        assert_eq!(result, expected,)
    }

    #[test]
    fn test_format_tool_name_with_extension() {
        assert_eq!(
            format_tool_name("developer__text_editor"),
            "Developer: Text Editor"
        );
        assert_eq!(
            format_tool_name("platform__manage_extensions"),
            "Platform: Manage Extensions"
        );
        assert_eq!(format_tool_name("todo__write"), "Todo: Write");
    }

    #[test]
    fn test_format_tool_name_without_extension() {
        assert_eq!(format_tool_name("simple_tool"), "Simple Tool");
        assert_eq!(format_tool_name("another_name"), "Another Name");
        assert_eq!(format_tool_name("single"), "Single");
    }

    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("allow_once".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AllowOnce };
        "allow_once_maps_to_allow_once"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("allow_always".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AlwaysAllow };
        "allow_always_maps_to_always_allow"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("reject_once".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::DenyOnce };
        "reject_once_maps_to_deny_once"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("reject_always".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::AlwaysDeny };
        "reject_always_maps_to_always_deny"
    )]
    #[test_case(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(PermissionOptionId::from("unknown".to_string()))),
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::Cancel };
        "unknown_option_maps_to_cancel"
    )]
    #[test_case(
        RequestPermissionOutcome::Cancelled,
        PermissionConfirmation { principal_type: PrincipalType::Tool, permission: Permission::Cancel };
        "cancelled_maps_to_cancel"
    )]
    fn test_outcome_to_confirmation(
        input: RequestPermissionOutcome,
        expected: PermissionConfirmation,
    ) {
        assert_eq!(outcome_to_confirmation(&input), expected);
    }

    use goose::providers::errors::ProviderError;

    struct MockModelProvider {
        models: Result<Vec<String>, ProviderError>,
    }

    #[async_trait::async_trait]
    impl goose::providers::base::Provider for MockModelProvider {
        fn get_name(&self) -> &str {
            "mock"
        }

        async fn complete_with_model(
            &self,
            _session_id: Option<&str>,
            _model_config: &goose::model::ModelConfig,
            _system: &str,
            _messages: &[goose::conversation::message::Message],
            _tools: &[rmcp::model::Tool],
        ) -> Result<
            (
                goose::conversation::message::Message,
                goose::providers::base::ProviderUsage,
            ),
            ProviderError,
        > {
            unimplemented!()
        }

        fn get_model_config(&self) -> goose::model::ModelConfig {
            goose::model::ModelConfig::new_or_fail("unused")
        }

        async fn fetch_recommended_models(&self) -> Result<Vec<String>, ProviderError> {
            self.models.clone()
        }
    }

    #[test_case(
        "model-a", Ok(vec!["model-a".into(), "model-b".into()])
        => Ok(SessionModelState::new(
            ModelId::new("model-a"),
            vec![ModelInfo::new(ModelId::new("model-a"), "model-a"),
                 ModelInfo::new(ModelId::new("model-b"), "model-b")],
        ))
        ; "returns current and available models"
    )]
    #[test_case(
        "model-a", Ok(vec![])
        => Ok(SessionModelState::new(ModelId::new("model-a"), vec![]))
        ; "empty model list"
    )]
    #[test_case(
        "model-a", Err(ProviderError::ExecutionError("fail".into()))
        => matches Err(_)
        ; "fetch error propagates"
    )]
    #[test_case(
        "switched-model", Ok(vec!["model-a".into(), "switched-model".into()])
        => Ok(SessionModelState::new(
            ModelId::new("switched-model"),
            vec![ModelInfo::new(ModelId::new("model-a"), "model-a"),
                 ModelInfo::new(ModelId::new("switched-model"), "switched-model")],
        ))
        ; "current model reflects switched model"
    )]
    #[tokio::test]
    async fn test_build_model_state(
        current_model: &str,
        models: Result<Vec<String>, ProviderError>,
    ) -> Result<SessionModelState, agent_client_protocol_schema::Error> {
        let provider = MockModelProvider { models };
        build_model_state(&provider, current_model).await
    }

    mod test_build_mode_state {
        use super::*;
        use goose::registry::manifest::AgentMode;

        fn mode(slug: &str, name: &str, description: &str) -> AgentMode {
            AgentMode {
                slug: slug.into(),
                name: name.into(),
                description: description.into(),
                instructions: None,
                instructions_file: None,
                tool_groups: vec![],
                when_to_use: None,
            }
        }

        #[test]
        fn no_modes_returns_none() {
            assert!(build_mode_state(&[], None).is_none());
        }

        #[test]
        fn single_mode_defaults_to_first() {
            let modes = vec![mode("code", "Code", "Write code")];
            let state = build_mode_state(&modes, None).unwrap();
            assert_eq!(state.current_mode_id.0.as_ref(), "code");
            assert_eq!(state.available_modes.len(), 1);
            assert_eq!(state.available_modes[0].id.0.as_ref(), "code");
        }

        #[test]
        fn explicit_default_mode() {
            let modes = vec![
                mode("ask", "Ask", "Ask questions"),
                mode("code", "Code", "Write code"),
            ];
            let state = build_mode_state(&modes, Some("code")).unwrap();
            assert_eq!(state.current_mode_id.0.as_ref(), "code");
            assert_eq!(state.available_modes.len(), 2);
        }

        #[test]
        fn description_is_set() {
            let modes = vec![mode("code", "Code", "Write and debug code")];
            let state = build_mode_state(&modes, None).unwrap();
            assert_eq!(
                state.available_modes[0].description.as_deref(),
                Some("Write and debug code")
            );
        }

        #[test]
        fn fallback_default_is_first_mode() {
            let modes = vec![mode("ask", "Ask", ""), mode("code", "Code", "")];
            let state = build_mode_state(&modes, None).unwrap();
            assert_eq!(state.current_mode_id.0.as_ref(), "ask");
        }
    }
}
