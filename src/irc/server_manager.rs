use crate::config::IrcMcpConfig;
use crate::irc::client::IrcClientManager;
use crate::storage::Database;
use crate::types::{ConnectionStatus, SharedState};
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

pub struct ServerManager {
    state: SharedState,
    database: Arc<tokio::sync::Mutex<Database>>,
}

impl ServerManager {
    pub fn new(config: IrcMcpConfig, database: Arc<tokio::sync::Mutex<Database>>) -> Result<Self> {
        let state = Arc::new(RwLock::new(crate::types::AppState::new(config)));

        Ok(Self { state, database })
    }

    /// Get shared state
    pub fn state(&self) -> SharedState {
        Arc::clone(&self.state)
    }

    /// Connect to all configured servers
    pub async fn connect_all(&self) -> Result<()> {
        let state = self.state.read().await;
        let servers: Vec<_> = state.config.servers.iter().cloned().collect();
        drop(state);

        for server_config in servers {
            let state = Arc::clone(&self.state);
            let db = Arc::clone(&self.database);

            tokio::spawn(async move {
                if let Err(e) = Self::connect_and_run_server(state, db, server_config).await {
                    error!("Server connection error: {}", e);
                }
            });
        }

        Ok(())
    }

    /// Connect to a specific server and run its message processor
    async fn connect_and_run_server(
        state: SharedState,
        database: Arc<tokio::sync::Mutex<Database>>,
        server_config: crate::config::ServerConfig,
    ) -> Result<()> {
        let server_name = server_config.name.clone();

        info!("Connecting to server: {}", server_name);

        // Update status to connecting
        {
            let mut state_lock = state.write().await;
            if let Some(ctx) = state_lock.servers.get_mut(&server_name) {
                ctx.connection_status = ConnectionStatus::Connecting;
            }
        }

        // Connect
        match IrcClientManager::connect_server(&server_config).await {
            Ok(client) => {
                let (tx, rx) = mpsc::unbounded_channel();

                // Update state with connection
                {
                    let mut state_lock = state.write().await;
                    if let Some(ctx) = state_lock.servers.get_mut(&server_name) {
                        ctx.connection_status = ConnectionStatus::Connected;
                        ctx.irc_sender = Some(tx);
                        ctx.reconnect_attempts = 0;
                    }
                }

                info!("Server {} connected successfully", server_name);

                // Start message processor
                if let Err(e) = crate::irc::client::start_message_processor(
                    client,
                    rx,
                    Arc::clone(&state),
                    database,
                    server_name.clone(),
                )
                .await
                {
                    error!("Message processor error for {}: {}", server_name, e);
                }

                // Connection lost - update status
                let mut state_lock = state.write().await;
                if let Some(ctx) = state_lock.servers.get_mut(&server_name) {
                    ctx.connection_status = ConnectionStatus::Disconnected;
                    ctx.irc_sender = None;
                }
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", server_name, e);
                let mut state_lock = state.write().await;
                if let Some(ctx) = state_lock.servers.get_mut(&server_name) {
                    ctx.connection_status = ConnectionStatus::Error;
                }
            }
        }

        Ok(())
    }

    /// Start reconnection monitoring task
    pub async fn start_reconnection_task(&self) {
        let state = Arc::clone(&self.state);
        let db = Arc::clone(&self.database);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));

            loop {
                interval.tick().await;

                let state_read = state.read().await;
                let disconnected_servers: Vec<_> = state_read
                    .servers
                    .iter()
                    .filter(|(_, ctx)| {
                        ctx.connection_status == ConnectionStatus::Disconnected
                            || ctx.connection_status == ConnectionStatus::Error
                    })
                    .map(|(name, ctx)| (name.clone(), ctx.config.clone(), ctx.reconnect_attempts))
                    .collect();
                drop(state_read);

                for (server_name, server_config, attempts) in disconnected_servers {
                    // Exponential backoff: 5s, 10s, 30s, 60s max
                    let delay = match attempts {
                        0 => 5,
                        1 => 10,
                        2 => 30,
                        _ => 60,
                    };

                    info!(
                        "Reconnecting to {} (attempt {}, delay {}s)",
                        server_name,
                        attempts + 1,
                        delay
                    );

                    // Increment reconnect attempts
                    {
                        let mut state_lock = state.write().await;
                        if let Some(ctx) = state_lock.servers.get_mut(&server_name) {
                            ctx.reconnect_attempts += 1;
                        }
                    }

                    tokio::time::sleep(tokio::time::Duration::from_secs(delay)).await;

                    let state_clone = Arc::clone(&state);
                    let db_clone = Arc::clone(&db);
                    tokio::spawn(async move {
                        if let Err(e) =
                            Self::connect_and_run_server(state_clone, db_clone, server_config).await
                        {
                            error!("Reconnection failed: {}", e);
                        }
                    });
                }
            }
        });
    }

    /// Disconnect from all servers
    pub async fn disconnect_all(&self) -> Result<()> {
        let mut state = self.state.write().await;

        for (name, ctx) in state.servers.iter_mut() {
            if let Some(sender) = &ctx.irc_sender {
                let _ = sender.send(crate::types::IrcCommand::Quit("Shutting down".to_string()));
                info!("Disconnecting from server: {}", name);
            }
            ctx.connection_status = ConnectionStatus::Disconnected;
            ctx.irc_sender = None;
        }

        Ok(())
    }

    /// Get database reference
    pub fn database(&self) -> Arc<tokio::sync::Mutex<Database>> {
        Arc::clone(&self.database)
    }
}
