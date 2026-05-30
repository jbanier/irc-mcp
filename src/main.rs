use anyhow::{Context, Result};
use irc_mcp_server::config::IrcMcpConfig;
use irc_mcp_server::irc::server_manager::ServerManager;
use irc_mcp_server::mcp::start_mcp_server;
use irc_mcp_server::storage::cleanup::start_cleanup_loop;
use irc_mcp_server::storage::Database;
use std::path::PathBuf;
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

    // Load configuration
    let mut config = IrcMcpConfig::from_file(&config_path)
        .with_context(|| format!("Failed to load config from {}", config_path))?;

    config.expand_paths();

    info!("Loaded configuration with {} server(s)", config.servers.len());

    // Initialize database
    let database = Arc::new(Mutex::new(
        Database::new(&config.storage.database_path)
            .context("Failed to initialize database")?,
    ));

    info!("Database initialized at {}", config.storage.database_path);

    // Create server manager
    let server_manager = ServerManager::new(config.clone(), Arc::clone(&database))
        .context("Failed to create server manager")?;

    // Start cleanup thread
    let server_download_dirs: Vec<PathBuf> = config
        .servers
        .iter()
        .map(|s| PathBuf::from(&s.dcc.download_directory))
        .collect();

    tokio::spawn(start_cleanup_loop(
        Arc::clone(&database),
        server_download_dirs,
        config.storage.cleanup_interval_hours,
        config.storage.message_retention_days,
    ));

    info!(
        "Cleanup thread started (interval: {}h, retention: {}d)",
        config.storage.cleanup_interval_hours, config.storage.message_retention_days
    );

    // Connect to all servers
    server_manager
        .connect_all()
        .await
        .context("Failed to connect to servers")?;

    // Start reconnection monitoring
    server_manager.start_reconnection_task().await;

    info!("Reconnection monitoring started");

    // Start MCP server
    let mcp_addr = format!("{}:{}", config.mcp.listen_address, config.mcp.port);
    info!("Starting MCP server on {}", mcp_addr);

    if let Err(e) = start_mcp_server(
        &mcp_addr,
        server_manager.state(),
        server_manager.database(),
    )
    .await
    {
        error!("MCP server error: {}", e);
    }

    Ok(())
}
