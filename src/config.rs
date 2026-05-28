use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IrcMcpConfig {
    pub server: ServerConfig,
    pub identity: IdentityConfig,
    pub channels: Vec<String>,
    pub dcc: DccConfig,
    pub storage: StorageConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
    pub use_tls: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IdentityConfig {
    pub nickname: String,
    pub username: String,
    pub realname: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DccConfig {
    pub enabled: bool,
    pub download_directory: String,
    pub max_file_size_bytes: u64,
    pub auto_accept: bool,
    pub allowed_extensions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub database_path: String,
    pub message_retention_days: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    pub listen_address: String,
    pub port: u16,
}

impl IrcMcpConfig {
    /// Load configuration from YAML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: IrcMcpConfig =
            serde_yml::from_str(&content).context("Failed to parse YAML configuration")?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    fn validate(&self) -> Result<()> {
        if self.server.address.is_empty() {
            anyhow::bail!("Server address cannot be empty");
        }

        if self.identity.nickname.is_empty() {
            anyhow::bail!("Nickname cannot be empty");
        }

        if self.storage.database_path.is_empty() {
            anyhow::bail!("Database path cannot be empty");
        }

        Ok(())
    }

    /// Expand shell variables in paths
    pub fn expand_paths(&mut self) {
        self.dcc.download_directory = shellexpand::tilde(&self.dcc.download_directory).to_string();
        self.storage.database_path = shellexpand::tilde(&self.storage.database_path).to_string();
    }
}
