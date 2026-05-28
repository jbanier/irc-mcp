mod config;
mod irc;
mod mcp;
mod storage;
mod types;

use crate::config::IrcMcpConfig;
use crate::storage::Database;
use crate::types::{AppState, ConnectionStatus};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "irc_mcp_server=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let config_path = if args.len() > 2 && args[1] == "--config" {
        args[2].clone()
    } else {
        "irc-mcp-config.yaml".to_string()
    };

    info!("Loading configuration from: {}", config_path);

    // Load and expand configuration
    let mut config =
        IrcMcpConfig::from_file(&config_path).context("Failed to load configuration")?;
    config.expand_paths();

    info!(
        "Configuration loaded - server: {}:{}, nick: {}",
        config.server.address, config.server.port, config.identity.nickname
    );

    // Initialize database
    let _db =
        Database::new(&config.storage.database_path).context("Failed to initialize database")?;
    info!("Database initialized: {}", config.storage.database_path);

    // Create shared application state
    let state = Arc::new(Mutex::new(AppState {
        irc_sender: None,
        connection_status: ConnectionStatus::Disconnected,
        connection_start: None,
        current_nick: None,
        joined_channels: Vec::new(),
        db_path: config.storage.database_path.clone(),
        config: config.clone(),
        active_dcc_transfers: HashMap::new(),
    }));

    // Start MCP server
    info!("Starting MCP server...");
    if let Err(e) = mcp::start_server(&config.mcp.listen_address, config.mcp.port, state).await {
        error!("MCP server error: {}", e);
        return Err(e);
    }

    Ok(())
}
