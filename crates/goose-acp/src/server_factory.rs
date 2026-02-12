use anyhow::Result;
use goose::agents::coding_agent::CodingAgent;
use goose::agents::goose_agent::GooseAgent;
use goose::providers::provider_registry::ProviderConstructor;
use std::sync::Arc;
use tracing::info;

use crate::server::GooseAcpAgent;

pub struct AcpServerFactoryConfig {
    pub builtins: Vec<String>,
    pub data_dir: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
}

pub struct AcpServer {
    config: AcpServerFactoryConfig,
}

impl AcpServer {
    pub fn new(config: AcpServerFactoryConfig) -> Self {
        Self { config }
    }

    pub async fn create_agent(&self) -> Result<Arc<GooseAcpAgent>> {
        let config_path = self
            .config
            .config_dir
            .join(goose::config::base::CONFIG_YAML_NAME);
        let config = goose::config::Config::new(&config_path, "goose")?;

        let goose_mode = config
            .get_goose_mode()
            .unwrap_or(goose::config::GooseMode::Auto);
        let disable_session_naming = config.get_goose_disable_session_naming().unwrap_or(false);

        let config_dir = self.config.config_dir.clone();
        let provider_factory: ProviderConstructor = Arc::new(move |model_config, extensions| {
            let config_dir = config_dir.clone();
            Box::pin(async move {
                let config_path = config_dir.join(goose::config::base::CONFIG_YAML_NAME);
                let config = goose::config::Config::new(&config_path, "goose")?;
                let provider_name = config
                    .get_goose_provider()
                    .map_err(|_| anyhow::anyhow!("No provider configured"))?;
                goose::providers::create(&provider_name, model_config, extensions).await
            })
        });

        let mut agent = GooseAcpAgent::new(
            provider_factory,
            self.config.builtins.clone(),
            self.config.data_dir.clone(),
            self.config.config_dir.clone(),
            goose_mode,
            disable_session_naming,
        )
        .await?;

        // Combine modes from GooseAgent (core Goose behaviors)
        // and CodingAgent (SDLC specialized roles)
        let builtin = GooseAgent::new();
        let coding = CodingAgent::new();

        let mut all_modes = builtin.to_agent_modes();
        all_modes.extend(coding.to_agent_modes());

        let default_mode = builtin.default_mode_slug().to_string();

        agent.set_modes(all_modes.clone(), Some(default_mode.clone()));
        info!(
            "Created ACP agent with {} modes ({} builtin + {} coding, default: {})",
            all_modes.len(),
            builtin.list_modes().len(),
            coding.modes().len(),
            default_mode,
        );

        Ok(Arc::new(agent))
    }
}
