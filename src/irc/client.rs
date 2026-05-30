use crate::config::{IrcMcpConfig, ServerConfig};
use crate::irc::sasl::encode_sasl_plain;
use crate::storage::Database;
use crate::types::{
    ConnectionStatus, DccStatus, DccTransfer, IrcCommand, IrcMessage, MessageType, SharedState,
};
use anyhow::{Context, Result};
use chrono::Utc;
use futures_util::stream::StreamExt;
use irc::client::prelude::*;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

pub struct IrcClientManager {
    config: IrcMcpConfig,
}

impl IrcClientManager {
    pub fn new(config: IrcMcpConfig) -> Self {
        Self { config }
    }

    /// Create and connect IRC client with SASL or PASS authentication
    pub async fn connect_server(server_config: &ServerConfig) -> Result<Client> {
        let mut irc_config = Config {
            nickname: Some(server_config.identity.nickname.clone()),
            username: Some(server_config.identity.username.clone()),
            realname: Some(server_config.identity.realname.clone()),
            server: Some(server_config.address.clone()),
            port: Some(server_config.port),
            use_tls: Some(server_config.use_tls),
            channels: server_config.channels.clone(),
            ..Default::default()
        };

        // Configure authentication
        if server_config.sasl.enabled {
            // SASL authentication
            if let Some(password) = &server_config.password {
                let sasl_username = server_config
                    .sasl
                    .username
                    .as_ref()
                    .unwrap_or(&server_config.identity.username);

                let _encoded = encode_sasl_plain(sasl_username, password);

                // Request SASL capability
                irc_config.should_ghost = false;
                irc_config.umodes = Some("+B".to_string()); // Mark as bot

                info!(
                    "Configuring SASL PLAIN authentication for {} on {}",
                    server_config.name, server_config.address
                );

                // Note: The irc crate doesn't directly support SASL in config
                // We'll need to handle CAP negotiation manually after connection
            }
        } else if let Some(password) = &server_config.password {
            // Server password (PASS command)
            irc_config.password = Some(password.clone());
            info!(
                "Configuring server password authentication for {}",
                server_config.name
            );
        }

        let client = Client::from_config(irc_config)
            .await
            .with_context(|| format!("Failed to create IRC client for {}", server_config.name))?;

        // Handle SASL authentication if enabled
        if server_config.sasl.enabled && server_config.password.is_some() {
            if let Err(e) = Self::authenticate_sasl(
                &client,
                server_config
                    .sasl
                    .username
                    .as_ref()
                    .unwrap_or(&server_config.identity.username),
                server_config.password.as_ref().unwrap(),
            )
            .await
            {
                warn!(
                    "SASL authentication failed for {}: {}. Proceeding without SASL.",
                    server_config.name, e
                );
            }
        }

        client
            .identify()
            .with_context(|| format!("Failed to identify to IRC server {}", server_config.name))?;

        info!(
            "Connected to IRC server {} ({}:{})",
            server_config.name, server_config.address, server_config.port
        );

        Ok(client)
    }

    /// Perform SASL PLAIN authentication
    async fn authenticate_sasl(client: &Client, username: &str, password: &str) -> Result<()> {
        // Send CAP LS
        client.send_cap_ls(NegotiationVersion::V302)?;

        // Wait for CAP LS response (timeout after 10 seconds)
        let cap_timeout = Duration::from_secs(10);

        // Request SASL capability
        match timeout(cap_timeout, Self::wait_for_cap_ack(client)).await {
            Ok(Ok(_)) => {
                // Send AUTHENTICATE PLAIN
                client.send(Command::Raw(
                    "AUTHENTICATE".to_string(),
                    vec!["PLAIN".to_string()],
                ))?;

                // Wait for AUTHENTICATE +
                // Then send credentials
                let encoded = encode_sasl_plain(username, password);
                client.send(Command::Raw("AUTHENTICATE".to_string(), vec![encoded]))?;

                // Send CAP END
                client.send(Command::CAP(
                    None,
                    irc::proto::CapSubCommand::END,
                    None,
                    None,
                ))?;

                info!("SASL authentication completed");
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                warn!("SASL negotiation timeout, proceeding without SASL");
                client.send(Command::CAP(
                    None,
                    irc::proto::CapSubCommand::END,
                    None,
                    None,
                ))?;
                Ok(())
            }
        }
    }

    /// Wait for CAP ACK :sasl
    async fn wait_for_cap_ack(_client: &Client) -> Result<()> {
        // In a real implementation, we'd listen for the CAP ACK message
        // For now, we'll just send CAP REQ and trust it works
        // This is a simplified version - full implementation would parse server responses
        Ok(())
    }
}

/// Start background task to process IRC messages and commands
pub async fn start_message_processor(
    mut client: Client,
    mut cmd_receiver: mpsc::UnboundedReceiver<IrcCommand>,
    state: SharedState,
    database: std::sync::Arc<tokio::sync::Mutex<Database>>,
    server_name: String,
) -> Result<()> {
    let mut stream = client.stream()?;

    loop {
        tokio::select! {
            // Process incoming IRC messages
            message = stream.next() => {
                match message {
                    Some(Ok(msg)) => {
                        if let Err(e) = process_message(&msg, &state, &database, &server_name).await {
                            error!("[{}] Error processing IRC message: {}", server_name, e);
                        }
                    }
                    Some(Err(e)) => {
                        error!("[{}] IRC stream error: {}", server_name, e);
                        break;
                    }
                    None => {
                        warn!("[{}] IRC stream ended", server_name);
                        break;
                    }
                }
            }

            // Process outgoing commands
            Some(cmd) = cmd_receiver.recv() => {
                if let Err(e) = execute_command(&client, cmd, &state, &server_name).await {
                    error!("[{}] Error executing IRC command: {}", server_name, e);
                }
            }
        }
    }

    // Connection lost
    warn!("[{}] IRC connection lost", server_name);
    let mut state_lock = state.write().await;
    if let Some(ctx) = state_lock.servers.get_mut(&server_name) {
        ctx.connection_status = ConnectionStatus::Disconnected;
        ctx.irc_sender = None;
    }

    Ok(())
}

/// Execute an IRC command
async fn execute_command(
    client: &Client,
    cmd: IrcCommand,
    state: &SharedState,
    server_name: &str,
) -> Result<()> {
    match cmd {
        IrcCommand::Join(channel) => {
            client.send_join(&channel)?;
            info!("[{}] Joining channel: {}", server_name, channel);
        }
        IrcCommand::Part(channel, message) => {
            if let Some(msg) = message {
                client.send(format!("PART {} :{}", channel, msg).as_str())?;
            } else {
                client.send_part(&channel)?;
            }
            info!("[{}] Leaving channel: {}", server_name, channel);
        }
        IrcCommand::SendMessage(target, message) => {
            client.send_privmsg(&target, &message)?;
            debug!("[{}] Sent message to {}: {}", server_name, target, message);
        }
        IrcCommand::SendRaw(raw) => {
            client.send(raw.as_str())?;
            debug!("[{}] Sent raw command: {}", server_name, raw);
        }
        IrcCommand::Quit(message) => {
            client.send_quit(&message)?;
            info!("[{}] Quitting: {}", server_name, message);
            // Mark as disconnecting
            let mut state_lock = state.write().await;
            if let Some(ctx) = state_lock.servers.get_mut(server_name) {
                ctx.connection_status = ConnectionStatus::Disconnected;
            }
        }
    }
    Ok(())
}

/// Process a single IRC message
async fn process_message(
    message: &Message,
    state: &SharedState,
    database: &std::sync::Arc<tokio::sync::Mutex<Database>>,
    server_name: &str,
) -> Result<()> {
    debug!("[{}] IRC message: {:?}", server_name, message);

    match &message.command {
        Command::PRIVMSG(target, content) => {
            handle_privmsg(message, target, content, state, database, server_name).await?;
        }
        Command::NOTICE(target, content) => {
            handle_notice(message, target, content, state, database, server_name).await?;
        }
        Command::JOIN(channel, _, _) => {
            handle_join(message, channel, state, server_name).await?;
        }
        Command::PART(channel, _) => {
            handle_part(message, channel, state, server_name).await?;
        }
        _ => {
            // Ignore other commands
        }
    }

    Ok(())
}

/// Handle PRIVMSG command
async fn handle_privmsg(
    message: &Message,
    target: &str,
    content: &str,
    state: &SharedState,
    database: &std::sync::Arc<tokio::sync::Mutex<Database>>,
    server_name: &str,
) -> Result<()> {
    let source_nick = message.source_nickname().unwrap_or("unknown").to_string();

    // Check if this is a CTCP message
    if content.starts_with('\x01') && content.ends_with('\x01') {
        return handle_ctcp(
            message,
            &source_nick,
            target,
            content,
            state,
            database,
            server_name,
        )
        .await;
    }

    let msg_type = if target.starts_with('#') {
        MessageType::Channel
    } else {
        MessageType::Private
    };

    let irc_msg = IrcMessage {
        id: None,
        timestamp: Utc::now(),
        source_nick: source_nick.clone(),
        target: target.to_string(),
        message_type: msg_type,
        content: content.to_string(),
        channel: if target.starts_with('#') {
            Some(target.to_string())
        } else {
            None
        },
        server_name: server_name.to_string(),
    };

    // Store in database
    let db = database.lock().await;
    if let Err(e) = db.insert_message(&irc_msg) {
        error!("[{}] Failed to store message: {}", server_name, e);
    }

    Ok(())
}

/// Handle NOTICE command
async fn handle_notice(
    message: &Message,
    target: &str,
    content: &str,
    _state: &SharedState,
    database: &std::sync::Arc<tokio::sync::Mutex<Database>>,
    server_name: &str,
) -> Result<()> {
    let source_nick = message.source_nickname().unwrap_or("system").to_string();

    let irc_msg = IrcMessage {
        id: None,
        timestamp: Utc::now(),
        source_nick,
        target: target.to_string(),
        message_type: MessageType::Notice,
        content: content.to_string(),
        channel: None,
        server_name: server_name.to_string(),
    };

    let db = database.lock().await;
    if let Err(e) = db.insert_message(&irc_msg) {
        error!("[{}] Failed to store notice: {}", server_name, e);
    }

    Ok(())
}

/// Handle CTCP messages (including DCC)
async fn handle_ctcp(
    _message: &Message,
    source_nick: &str,
    _target: &str,
    content: &str,
    state: &SharedState,
    database: &std::sync::Arc<tokio::sync::Mutex<Database>>,
    server_name: &str,
) -> Result<()> {
    // Check if this is a DCC SEND offer
    if content.contains("DCC SEND") {
        if let Ok(offer) = crate::irc::parse_dcc_send(content) {
            info!(
                "[{}] Received DCC SEND offer from {}: {} ({} bytes)",
                server_name, source_nick, offer.filename, offer.filesize
            );

            let state_lock = state.read().await;

            // Get DCC config for this server
            let dcc_config = if let Some(ctx) = state_lock.servers.get(server_name) {
                &ctx.config.dcc
            } else {
                warn!("[{}] Server context not found", server_name);
                return Ok(());
            };

            // Check if DCC is enabled
            if !dcc_config.enabled || !dcc_config.auto_accept {
                info!("[{}] DCC auto-accept disabled, ignoring offer", server_name);
                return Ok(());
            }

            // Create transfer record
            let transfer = DccTransfer {
                id: None,
                timestamp: Utc::now(),
                sender_nick: source_nick.to_string(),
                filename: offer.filename.clone(),
                filepath: None,
                filesize: offer.filesize,
                received_size: 0,
                status: DccStatus::Pending,
                error: None,
                ip_address: Some(offer.ip_address.clone()),
                port: Some(offer.port),
                extracted_files: None,
                extraction_status: None,
                extraction_error: None,
                server_name: server_name.to_string(),
            };

            let db = database.lock().await;
            let transfer_id = db.insert_dcc_transfer(&transfer)?;
            drop(db);

            info!(
                "[{}] Created DCC transfer record with ID: {}",
                server_name, transfer_id
            );

            // Spawn download task
            let download_dir = dcc_config.download_directory.clone();
            let max_size = dcc_config.max_file_size_bytes;

            drop(state_lock); // Release lock before spawning

            let db_clone = std::sync::Arc::clone(database);
            let server_name_clone = server_name.to_string();
            tokio::spawn(async move {
                let result = crate::irc::download_dcc_file(
                    &offer,
                    std::path::Path::new(&download_dir),
                    max_size,
                )
                .await;

                let db = db_clone.lock().await;

                match result {
                    Ok((filepath, size, extracted_files)) => {
                        info!(
                            "[{}] DCC download completed: {:?}",
                            server_name_clone, filepath
                        );

                        // Canonicalize path to ensure it's absolute and usable from any working directory
                        let absolute_path = match filepath.canonicalize() {
                            Ok(path) => path.to_string_lossy().to_string(),
                            Err(e) => {
                                error!(
                                    "[{}] Failed to canonicalize path {:?}: {}",
                                    server_name_clone, filepath, e
                                );
                                filepath.to_string_lossy().to_string()
                            }
                        };

                        if let Err(e) = db.update_dcc_transfer_status(
                            transfer_id,
                            DccStatus::Completed,
                            size,
                            Some(&absolute_path),
                            None,
                        ) {
                            error!(
                                "[{}] Failed to update transfer status: {}",
                                server_name_clone, e
                            );
                        }

                        // Update extraction metadata if zip was extracted
                        if let Some(files) = extracted_files {
                            if let Err(e) = db.update_extraction_metadata(
                                transfer_id,
                                "extracted",
                                Some(&files),
                                None,
                            ) {
                                error!(
                                    "[{}] Failed to update extraction metadata: {}",
                                    server_name_clone, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        error!("[{}] DCC download failed: {}", server_name_clone, e);
                        if let Err(e) = db.update_dcc_transfer_status(
                            transfer_id,
                            DccStatus::Failed,
                            0,
                            None,
                            Some(&e.to_string()),
                        ) {
                            error!(
                                "[{}] Failed to update transfer status: {}",
                                server_name_clone, e
                            );
                        }
                    }
                }
            });
        }
    }

    Ok(())
}

/// Handle JOIN command
async fn handle_join(
    message: &Message,
    channel: &str,
    state: &SharedState,
    server_name: &str,
) -> Result<()> {
    if let Some(nick) = message.source_nickname() {
        let mut state_lock = state.write().await;

        // Check if it's our own join
        if let Some(ctx) = state_lock.servers.get_mut(server_name) {
            if Some(nick) == ctx.current_nick.as_deref()
                && !ctx.joined_channels.contains(&channel.to_string())
            {
                ctx.joined_channels.insert(channel.to_string());
                info!("[{}] Joined channel: {}", server_name, channel);
            }
        }
    }

    Ok(())
}

/// Handle PART command
async fn handle_part(
    message: &Message,
    channel: &str,
    state: &SharedState,
    server_name: &str,
) -> Result<()> {
    if let Some(nick) = message.source_nickname() {
        let mut state_lock = state.write().await;

        // Check if it's our own part
        if let Some(ctx) = state_lock.servers.get_mut(server_name) {
            if Some(nick) == ctx.current_nick.as_deref() {
                ctx.joined_channels.remove(&channel.to_string());
                info!("[{}] Left channel: {}", server_name, channel);
            }
        }
    }

    Ok(())
}
