use super::{
    map_permission_response, spawn_acp_server_in_process, Connection, PermissionDecision,
    PermissionMapping, Session, TestConnectionConfig, TestOutput,
};
use agent_client_protocol::ProtocolVersion;
use agent_client_protocol_schema::{
    ContentBlock, InitializeRequest, LoadSessionRequest, McpServer, NewSessionRequest,
    PromptRequest, RequestPermissionRequest, RequestPermissionResponse, SessionModelState,
    SessionNotification, SessionUpdate, SetSessionModelRequest, StopReason, TextContent,
    ToolCallStatus,
};
use async_trait::async_trait;
use goose::config::PermissionManager;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

/// A test Client impl that collects notifications and answers permission requests.
struct TestClient {
    updates: Arc<Mutex<Vec<SessionNotification>>>,
    permission: Arc<Mutex<PermissionDecision>>,
    notify: Arc<Notify>,
}

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Client for TestClient {
    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol_schema::Result<()> {
        self.updates.lock().unwrap().push(args);
        self.notify.notify_waiters();
        Ok(())
    }

    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol_schema::Result<RequestPermissionResponse> {
        let decision = *self.permission.lock().unwrap();
        let mapping = PermissionMapping;
        Ok(map_permission_response(&mapping, &args, decision))
    }
}

pub struct ClientToAgentConnection {
    /// Sends Agent-trait calls to the !Send ClientSideConnection on the LocalSet thread.
    agent_tx: tokio::sync::mpsc::UnboundedSender<AgentCommand>,
    pending_mcp_servers: Vec<McpServer>,
    updates: Arc<Mutex<Vec<SessionNotification>>>,
    permission: Arc<Mutex<PermissionDecision>>,
    notify: Arc<Notify>,
    permission_manager: Arc<PermissionManager>,
    _openai: super::OpenAiFixture,
    _temp_dir: Option<tempfile::TempDir>,
}

pub struct ClientToAgentSession {
    agent_tx: tokio::sync::mpsc::UnboundedSender<AgentCommand>,
    session_id: agent_client_protocol_schema::SessionId,
    updates: Arc<Mutex<Vec<SessionNotification>>>,
    permission: Arc<Mutex<PermissionDecision>>,
    notify: Arc<Notify>,
}

/// Commands sent from the Send test world to the !Send LocalSet.
enum AgentCommand {
    NewSession {
        request: NewSessionRequest,
        reply: tokio::sync::oneshot::Sender<agent_client_protocol_schema::NewSessionResponse>,
    },
    LoadSession {
        request: LoadSessionRequest,
        reply: tokio::sync::oneshot::Sender<agent_client_protocol_schema::LoadSessionResponse>,
    },
    Prompt {
        request: PromptRequest,
        reply: tokio::sync::oneshot::Sender<agent_client_protocol_schema::PromptResponse>,
    },
    SetModel {
        request: SetSessionModelRequest,
        reply: tokio::sync::oneshot::Sender<agent_client_protocol_schema::SetSessionModelResponse>,
    },
}

#[async_trait]
impl Connection for ClientToAgentConnection {
    type Session = ClientToAgentSession;

    async fn new(config: TestConnectionConfig, openai: super::OpenAiFixture) -> Self {
        let (data_root, temp_dir) = match config.data_root.as_os_str().is_empty() {
            true => {
                let temp_dir = tempfile::tempdir().unwrap();
                (temp_dir.path().to_path_buf(), Some(temp_dir))
            }
            false => (config.data_root.clone(), None),
        };

        let (client_write, client_read, _handle, permission_manager) = spawn_acp_server_in_process(
            openai.uri(),
            &config.builtins,
            data_root.as_path(),
            config.goose_mode,
            config.provider_factory,
        )
        .await;

        let updates = Arc::new(Mutex::new(Vec::new()));
        let notify = Arc::new(Notify::new());
        let permission = Arc::new(Mutex::new(PermissionDecision::Cancel));

        let test_client = TestClient {
            updates: updates.clone(),
            permission: permission.clone(),
            notify: notify.clone(),
        };

        let (agent_tx, mut agent_rx) = tokio::sync::mpsc::unbounded_channel::<AgentCommand>();

        // Spawn a dedicated thread with LocalSet for the !Send ClientSideConnection.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test client runtime");

            let local = tokio::task::LocalSet::new();

            local.block_on(&rt, async move {
                use agent_client_protocol::Agent;

                let (conn, io_task) = agent_client_protocol::ClientSideConnection::new(
                    test_client,
                    client_write,
                    client_read,
                    |fut| {
                        tokio::task::spawn_local(fut);
                    },
                );

                let conn = std::rc::Rc::new(conn);

                // Spawn io_task first so I/O is being driven when we call initialize.
                tokio::task::spawn_local(async move {
                    if let Err(e) = io_task.await {
                        tracing::error!("test client io error: {e}");
                    }
                });

                // Initialize
                conn.initialize(InitializeRequest::new(ProtocolVersion::LATEST))
                    .await
                    .unwrap();

                // Command pump
                let pump_conn = conn;
                while let Some(cmd) = agent_rx.recv().await {
                    match cmd {
                        AgentCommand::NewSession { request, reply } => {
                            let result = pump_conn.new_session(request).await.unwrap();
                            let _ = reply.send(result);
                        }
                        AgentCommand::LoadSession { request, reply } => {
                            let result = pump_conn.load_session(request).await.unwrap();
                            let _ = reply.send(result);
                        }
                        AgentCommand::Prompt { request, reply } => {
                            let result = pump_conn.prompt(request).await.unwrap();
                            let _ = reply.send(result);
                        }
                        AgentCommand::SetModel { request, reply } => {
                            let result = pump_conn.set_session_model(request).await.unwrap();
                            let _ = reply.send(result);
                        }
                    }
                }
            });
        });

        Self {
            agent_tx,
            pending_mcp_servers: config.mcp_servers,
            updates,
            permission,
            notify,
            permission_manager,
            _openai: openai,
            _temp_dir: temp_dir,
        }
    }

    async fn new_session(&mut self) -> (ClientToAgentSession, Option<SessionModelState>) {
        let work_dir = tempfile::tempdir().unwrap();
        let mcp_servers = std::mem::take(&mut self.pending_mcp_servers);
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.agent_tx
            .send(AgentCommand::NewSession {
                request: NewSessionRequest::new(work_dir.path()).mcp_servers(mcp_servers),
                reply: reply_tx,
            })
            .unwrap();
        let response = reply_rx.await.unwrap();
        let session = ClientToAgentSession {
            agent_tx: self.agent_tx.clone(),
            session_id: response.session_id.clone(),
            updates: self.updates.clone(),
            permission: self.permission.clone(),
            notify: self.notify.clone(),
        };
        (session, response.models)
    }

    async fn load_session(
        &mut self,
        session_id: &str,
    ) -> (ClientToAgentSession, Option<SessionModelState>) {
        self.updates.lock().unwrap().clear();
        let work_dir = tempfile::tempdir().unwrap();
        let session_id = agent_client_protocol_schema::SessionId::new(session_id.to_string());
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.agent_tx
            .send(AgentCommand::LoadSession {
                request: LoadSessionRequest::new(session_id.clone(), work_dir.path()),
                reply: reply_tx,
            })
            .unwrap();
        let response = reply_rx.await.unwrap();
        let session = ClientToAgentSession {
            agent_tx: self.agent_tx.clone(),
            session_id,
            updates: self.updates.clone(),
            permission: self.permission.clone(),
            notify: self.notify.clone(),
        };
        (session, response.models)
    }

    fn reset_openai(&self) {
        self._openai.reset();
    }

    fn reset_permissions(&self) {
        self.permission_manager.remove_extension("");
    }
}

#[async_trait]
impl Session for ClientToAgentSession {
    fn session_id(&self) -> &agent_client_protocol_schema::SessionId {
        &self.session_id
    }

    async fn prompt(&mut self, text: &str, decision: PermissionDecision) -> TestOutput {
        *self.permission.lock().unwrap() = decision;
        self.updates.lock().unwrap().clear();

        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.agent_tx
            .send(AgentCommand::Prompt {
                request: PromptRequest::new(
                    self.session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new(text))],
                ),
                reply: reply_tx,
            })
            .unwrap();
        let response = reply_rx.await.unwrap();

        assert_eq!(response.stop_reason, StopReason::EndTurn);

        let mut updates_len = self.updates.lock().unwrap().len();
        while updates_len == 0 {
            self.notify.notified().await;
            updates_len = self.updates.lock().unwrap().len();
        }

        let text = collect_agent_text(&self.updates);
        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        let mut tool_status = extract_tool_status(&self.updates);
        while tool_status.is_none() && tokio::time::Instant::now() < deadline {
            tokio::task::yield_now().await;
            tool_status = extract_tool_status(&self.updates);
        }

        TestOutput { text, tool_status }
    }

    async fn set_model(&self, model_id: &str) {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        self.agent_tx
            .send(AgentCommand::SetModel {
                request: SetSessionModelRequest::new(
                    self.session_id.clone(),
                    agent_client_protocol_schema::ModelId::new(model_id),
                ),
                reply: reply_tx,
            })
            .unwrap();
        reply_rx.await.unwrap();
    }
}

fn collect_agent_text(updates: &Arc<Mutex<Vec<SessionNotification>>>) -> String {
    let guard = updates.lock().unwrap();
    let mut text = String::new();

    for notification in guard.iter() {
        if let SessionUpdate::AgentMessageChunk(chunk) = &notification.update {
            if let ContentBlock::Text(t) = &chunk.content {
                text.push_str(&t.text);
            }
        }
    }

    text
}

fn extract_tool_status(updates: &Arc<Mutex<Vec<SessionNotification>>>) -> Option<ToolCallStatus> {
    let guard = updates.lock().unwrap();
    guard.iter().find_map(|notification| {
        if let SessionUpdate::ToolCallUpdate(update) = &notification.update {
            return update.fields.status;
        }
        None
    })
}
