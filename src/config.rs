// src/config.rs - replace entire file
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IrcMcpConfig {
    pub servers: Vec<ServerConfig>,
    pub storage: StorageConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub use_tls: bool,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub sasl: SaslConfig,
    pub identity: IdentityConfig,
    pub channels: Vec<String>,
    pub dcc: DccConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SaslConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub username: Option<String>,
}

impl Default for SaslConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            username: None,
        }
    }
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
    #[serde(default = "default_download_dir")]
    pub download_directory: String,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_bytes: u64,
    #[serde(default)]
    pub auto_accept: bool,
    #[serde(default)]
    pub allowed_extensions: Vec<String>,
}

fn default_download_dir() -> String {
    "./data/downloads".to_string()
}

fn default_max_file_size() -> u64 {
    104_857_600 // 100 MB
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageConfig {
    pub database_path: String,
    #[serde(default = "default_retention_days")]
    pub message_retention_days: u32,
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_hours: u64,
}

fn default_retention_days() -> u32 {
    90
}

fn default_cleanup_interval() -> u64 {
    24
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpConfig {
    pub listen_address: String,
    pub port: u16,
    pub default_server: String,
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
        if self.servers.is_empty() {
            anyhow::bail!("At least one server must be configured");
        }

        // Check for duplicate server names
        let mut seen_names = HashSet::new();
        for server in &self.servers {
            if !seen_names.insert(&server.name) {
                anyhow::bail!("Duplicate server name: {}", server.name);
            }
        }

        // Validate each server
        for server in &self.servers {
            if server.name.is_empty() {
                anyhow::bail!("Server name cannot be empty");
            }
            if server.address.is_empty() {
                anyhow::bail!("Server address cannot be empty for server '{}'", server.name);
            }
            if server.identity.nickname.is_empty() {
                anyhow::bail!("Nickname cannot be empty for server '{}'", server.name);
            }
        }

        // Validate default_server exists
        if !self.servers.iter().any(|s| s.name == self.mcp.default_server) {
            anyhow::bail!(
                "default_server '{}' not found in servers list",
                self.mcp.default_server
            );
        }

        if self.storage.database_path.is_empty() {
            anyhow::bail!("Database path cannot be empty");
        }

        Ok(())
    }

    /// Expand shell variables in paths
    pub fn expand_paths(&mut self) {
        for server in &mut self.servers {
            server.dcc.download_directory =
                shellexpand::tilde(&server.dcc.download_directory).to_string();
        }
        self.storage.database_path = shellexpand::tilde(&self.storage.database_path).to_string();
    }
}
