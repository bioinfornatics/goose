use std::collections::HashMap;
use std::sync::Arc;

use agent_client_protocol::{Agent, ClientSideConnection, ProtocolVersion};
use agent_client_protocol_schema::{
    InitializeRequest, InitializeResponse, NewSessionRequest, NewSessionResponse, PromptRequest,
    PromptResponse, SetSessionModeRequest, SetSessionModeResponse,
};
use anyhow::{bail, Result};
use futures::FutureExt;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::LocalSet;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::agent_manager::spawner::{spawn_agent, SpawnedAgent};
use crate::registry::manifest::{AgentDistribution, RegistryEntry, RegistryEntryDetail};

pub struct AgentHandle {
    tx: mpsc::Sender<AgentCommand>,
    pub info: InitializeResponse,
    pub agent_id: String,
}

impl AgentHandle {
    pub async fn new_session(&self, req: NewSessionRequest) -> Result<NewSessionResponse> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(AgentCommand::NewSession {
                req,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("agent connection closed"))?;
        reply_rx.await?
    }

    pub async fn prompt(&self, req: PromptRequest) -> Result<PromptResponse> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(AgentCommand::Prompt {
                req,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("agent connection closed"))?;
        reply_rx.await?
    }

    pub async fn set_mode(&self, req: SetSessionModeRequest) -> Result<SetSessionModeResponse> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(AgentCommand::SetMode {
                req,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("agent connection closed"))?;
        reply_rx.await?
    }

    pub async fn shutdown(self) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let _ = self
            .tx
            .send(AgentCommand::Shutdown { reply: reply_tx })
            .await;
        reply_rx.await.unwrap_or(Ok(()))
    }
}

enum AgentCommand {
    NewSession {
        req: NewSessionRequest,
        reply: oneshot::Sender<Result<NewSessionResponse>>,
    },
    Prompt {
        req: PromptRequest,
        reply: oneshot::Sender<Result<PromptResponse>>,
    },
    SetMode {
        req: SetSessionModeRequest,
        reply: oneshot::Sender<Result<SetSessionModeResponse>>,
    },
    Shutdown {
        reply: oneshot::Sender<Result<()>>,
    },
}

struct OrchestratorClient;

#[async_trait::async_trait(?Send)]
impl agent_client_protocol::Client for OrchestratorClient {
    async fn request_permission(
        &self,
        args: agent_client_protocol_schema::RequestPermissionRequest,
    ) -> agent_client_protocol_schema::Result<agent_client_protocol_schema::RequestPermissionResponse>
    {
        let option_id = args
            .options
            .first()
            .map(|o| o.option_id.clone())
            .unwrap_or_else(|| {
                agent_client_protocol_schema::PermissionOptionId::from("allow_once".to_string())
            });
        Ok(
            agent_client_protocol_schema::RequestPermissionResponse::new(
                agent_client_protocol_schema::RequestPermissionOutcome::Selected(
                    agent_client_protocol_schema::SelectedPermissionOutcome::new(option_id),
                ),
            ),
        )
    }

    async fn session_notification(
        &self,
        _args: agent_client_protocol_schema::SessionNotification,
    ) -> agent_client_protocol_schema::Result<()> {
        Ok(())
    }
}

pub struct AgentClientManager {
    agents: Arc<Mutex<HashMap<String, AgentHandle>>>,
}

impl AgentClientManager {
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn connect_agent(&self, agent_id: String, entry: &RegistryEntry) -> Result<()> {
        let dist = match &entry.detail {
            RegistryEntryDetail::Agent(detail) => detail
                .distribution
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("agent has no distribution info"))?,
            _ => bail!("registry entry is not an agent"),
        };
        let dist = dist.clone();

        let (cmd_tx, cmd_rx) = mpsc::channel::<AgentCommand>(32);
        let (init_tx, init_rx) = oneshot::channel::<Result<InitializeResponse>>();

        std::thread::Builder::new()
            .name(format!("acp-client-{agent_id}"))
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime");
                let local = LocalSet::new();
                local.block_on(&rt, async move {
                    let result = run_agent_connection(dist, cmd_rx, init_tx).await;
                    if let Err(e) = result {
                        tracing::error!("agent connection error: {e}");
                    }
                });
            })?;

        let info = init_rx.await??;

        let handle = AgentHandle {
            tx: cmd_tx,
            info,
            agent_id: agent_id.clone(),
        };

        self.agents.lock().await.insert(agent_id, handle);
        Ok(())
    }

    pub async fn prompt_agent(&self, agent_id: &str, req: PromptRequest) -> Result<PromptResponse> {
        let agents = self.agents.lock().await;
        let handle = agents
            .get(agent_id)
            .ok_or_else(|| anyhow::anyhow!("agent '{agent_id}' not connected"))?;
        handle.prompt(req).await
    }

    pub async fn list_agents(&self) -> Vec<String> {
        self.agents.lock().await.keys().cloned().collect()
    }

    pub async fn disconnect_agent(&self, agent_id: &str) -> Result<()> {
        let handle = self
            .agents
            .lock()
            .await
            .remove(agent_id)
            .ok_or_else(|| anyhow::anyhow!("agent '{agent_id}' not connected"))?;
        handle.shutdown().await
    }

    pub async fn shutdown_all(&self) {
        let agents: Vec<_> = {
            let mut map = self.agents.lock().await;
            map.drain().map(|(_, h)| h).collect()
        };
        for handle in agents {
            let _ = handle.shutdown().await;
        }
    }
}

impl Default for AgentClientManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn run_agent_connection(
    dist: AgentDistribution,
    mut cmd_rx: mpsc::Receiver<AgentCommand>,
    init_tx: oneshot::Sender<Result<InitializeResponse>>,
) -> Result<()> {
    let spawned = spawn_agent(&dist).await?;
    let SpawnedAgent {
        child: _child,
        stdin,
        stdout,
    } = spawned;

    let client = OrchestratorClient;
    let (conn, io_task) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |fut| {
            tokio::task::spawn_local(fut);
        });

    tokio::task::spawn_local(io_task.map(|r| {
        if let Err(e) = r {
            tracing::error!("agent IO task error: {e}");
        }
    }));

    let init_result = conn
        .initialize(InitializeRequest::new(ProtocolVersion::LATEST))
        .await;

    match init_result {
        Ok(resp) => {
            let _ = init_tx.send(Ok(resp));
        }
        Err(e) => {
            let _ = init_tx.send(Err(anyhow::anyhow!("initialize failed: {e}")));
            return Ok(());
        }
    }

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            AgentCommand::NewSession { req, reply } => {
                let result = conn
                    .new_session(req)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"));
                let _ = reply.send(result);
            }
            AgentCommand::Prompt { req, reply } => {
                let result = conn.prompt(req).await.map_err(|e| anyhow::anyhow!("{e}"));
                let _ = reply.send(result);
            }
            AgentCommand::SetMode { req, reply } => {
                let result = conn
                    .set_session_mode(req)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"));
                let _ = reply.send(result);
            }
            AgentCommand::Shutdown { reply } => {
                let _ = reply.send(Ok(()));
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol_schema::SessionId;

    #[test]
    fn manager_default() {
        let mgr = AgentClientManager::default();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let agents = rt.block_on(mgr.list_agents());
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn prompt_nonexistent_agent_fails() {
        let mgr = AgentClientManager::new();
        let result = mgr
            .prompt_agent(
                "nonexistent",
                PromptRequest::new(SessionId::from("s1"), vec![]),
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not connected"));
    }

    #[tokio::test]
    async fn disconnect_nonexistent_agent_fails() {
        let mgr = AgentClientManager::new();
        let result = mgr.disconnect_agent("nonexistent").await;
        assert!(result.is_err());
    }
}
