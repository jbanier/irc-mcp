use crate::config::IrcMcpConfig;
use crate::storage::Database;
use crate::types::{
    ConnectionStatus, DccStatus, DccTransfer, IrcCommand, IrcMessage, MessageType, SharedState,
};
use anyhow::{Context, Result};
use chrono::Utc;
use futures_util::stream::StreamExt;
use irc::client::prelude::*;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

pub struct IrcClientManager {
    config: IrcMcpConfig,
}

impl IrcClientManager {
    pub fn new(config: IrcMcpConfig) -> Self {
        Self { config }
    }

    /// Create and connect IRC client
    pub async fn connect(&self) -> Result<Client> {
        let irc_config = Config {
            nickname: Some(self.config.identity.nickname.clone()),
            username: Some(self.config.identity.username.clone()),
            realname: Some(self.config.identity.realname.clone()),
            server: Some(self.config.server.address.clone()),
            port: Some(self.config.server.port),
            use_tls: Some(self.config.server.use_tls),
            channels: self.config.channels.clone(),
            ..Default::default()
        };

        let client = Client::from_config(irc_config)
            .await
            .context("Failed to create IRC client")?;

        client
            .identify()
            .context("Failed to identify to IRC server")?;

        info!(
            "Connected to IRC server: {}:{}",
            self.config.server.address, self.config.server.port
        );

        Ok(client)
    }
}

/// Start background task to process IRC messages and commands
pub async fn start_message_processor(
    mut client: Client,
    mut cmd_receiver: mpsc::UnboundedReceiver<IrcCommand>,
    state: SharedState,
) -> Result<()> {
    let mut stream = client.stream()?;

    loop {
        tokio::select! {
            // Process incoming IRC messages
            message = stream.next() => {
                match message {
                    Some(Ok(msg)) => {
                        if let Err(e) = process_message(&msg, &state).await {
                            error!("Error processing IRC message: {}", e);
                        }
                    }
                    Some(Err(e)) => {
                        error!("IRC stream error: {}", e);
                        break;
                    }
                    None => {
                        warn!("IRC stream ended");
                        break;
                    }
                }
            }

            // Process outgoing commands
            Some(cmd) = cmd_receiver.recv() => {
                if let Err(e) = execute_command(&client, cmd, &state).await {
                    error!("Error executing IRC command: {}", e);
                }
            }
        }
    }

    // Connection lost
    warn!("IRC connection lost");
    let mut state_lock = state.lock().await;
    state_lock.connection_status = ConnectionStatus::Disconnected;
    state_lock.irc_sender = None;

    Ok(())
}

/// Execute an IRC command
async fn execute_command(client: &Client, cmd: IrcCommand, state: &SharedState) -> Result<()> {
    match cmd {
        IrcCommand::Join(channel) => {
            client.send_join(&channel)?;
        }
        IrcCommand::Part(channel, message) => {
            if let Some(msg) = message {
                client.send(format!("PART {} :{}", channel, msg).as_str())?;
            } else {
                client.send_part(&channel)?;
            }
        }
        IrcCommand::SendMessage(target, message) => {
            client.send_privmsg(&target, &message)?;
        }
        IrcCommand::SendRaw(raw) => {
            client.send(raw.as_str())?;
        }
        IrcCommand::Quit(message) => {
            client.send_quit(&message)?;
            // Mark as disconnecting
            let mut state_lock = state.lock().await;
            state_lock.connection_status = ConnectionStatus::Disconnected;
        }
    }
    Ok(())
}

/// Process a single IRC message
async fn process_message(message: &Message, state: &SharedState) -> Result<()> {
    debug!("IRC message: {:?}", message);

    match &message.command {
        Command::PRIVMSG(target, content) => {
            handle_privmsg(message, target, content, state).await?;
        }
        Command::NOTICE(target, content) => {
            handle_notice(message, target, content, state).await?;
        }
        Command::JOIN(channel, _, _) => {
            handle_join(message, channel, state).await?;
        }
        Command::PART(channel, _) => {
            handle_part(message, channel, state).await?;
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
) -> Result<()> {
    let source_nick = message.source_nickname().unwrap_or("unknown").to_string();

    // Check if this is a CTCP message
    if content.starts_with('\x01') && content.ends_with('\x01') {
        return handle_ctcp(message, &source_nick, target, content, state).await;
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
    };

    // Store in database
    let state_lock = state.lock().await;
    if let Err(e) = Database::new(&state_lock.config.storage.database_path)
        .and_then(|db| db.insert_message(&irc_msg))
    {
        error!("Failed to store message: {}", e);
    }

    Ok(())
}

/// Handle NOTICE command
async fn handle_notice(
    message: &Message,
    target: &str,
    content: &str,
    state: &SharedState,
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
    };

    let state_lock = state.lock().await;
    if let Err(e) = Database::new(&state_lock.config.storage.database_path)
        .and_then(|db| db.insert_message(&irc_msg))
    {
        error!("Failed to store notice: {}", e);
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
) -> Result<()> {
    // Check if this is a DCC SEND offer
    if content.contains("DCC SEND") {
        if let Ok(offer) = crate::irc::parse_dcc_send(content) {
            info!(
                "Received DCC SEND offer from {}: {} ({} bytes)",
                source_nick, offer.filename, offer.filesize
            );

            let state_lock = state.lock().await;

            // Check if DCC is enabled
            if !state_lock.config.dcc.enabled || !state_lock.config.dcc.auto_accept {
                info!("DCC auto-accept disabled, ignoring offer");
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
            };

            let db = Database::new(&state_lock.config.storage.database_path)?;
            let transfer_id = db.insert_dcc_transfer(&transfer)?;

            info!("Created DCC transfer record with ID: {}", transfer_id);

            // Spawn download task
            let download_dir = state_lock.config.dcc.download_directory.clone();
            let max_size = state_lock.config.dcc.max_file_size_bytes;
            let db_path = state_lock.config.storage.database_path.clone();

            drop(state_lock); // Release lock before spawning

            tokio::spawn(async move {
                let result = crate::irc::download_dcc_file(
                    &offer,
                    std::path::Path::new(&download_dir),
                    max_size,
                )
                .await;

                let db = match Database::new(&db_path) {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Failed to open database: {}", e);
                        return;
                    }
                };

                match result {
                    Ok((filepath, size, extracted_files)) => {
                        info!("DCC download completed: {:?}", filepath);

                        // Canonicalize path to ensure it's absolute and usable from any working directory
                        let absolute_path = match filepath.canonicalize() {
                            Ok(path) => path.to_string_lossy().to_string(),
                            Err(e) => {
                                error!("Failed to canonicalize path {:?}: {}", filepath, e);
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
                            error!("Failed to update transfer status: {}", e);
                        }

                        // Update extraction metadata if zip was extracted
                        if let Some(files) = extracted_files {
                            if let Err(e) = db.update_extraction_metadata(
                                transfer_id,
                                "extracted",
                                Some(&files),
                                None,
                            ) {
                                error!("Failed to update extraction metadata: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("DCC download failed: {}", e);
                        if let Err(e) = db.update_dcc_transfer_status(
                            transfer_id,
                            DccStatus::Failed,
                            0,
                            None,
                            Some(&e.to_string()),
                        ) {
                            error!("Failed to update transfer status: {}", e);
                        }
                    }
                }
            });
        }
    }

    Ok(())
}

/// Handle JOIN command
async fn handle_join(message: &Message, channel: &str, state: &SharedState) -> Result<()> {
    if let Some(nick) = message.source_nickname() {
        let mut state_lock = state.lock().await;

        // Check if it's our own join
        if Some(nick) == state_lock.current_nick.as_deref()
            && !state_lock.joined_channels.contains(&channel.to_string())
        {
            state_lock.joined_channels.push(channel.to_string());
            info!("Joined channel: {}", channel);
        }
    }

    Ok(())
}

/// Handle PART command
async fn handle_part(message: &Message, channel: &str, state: &SharedState) -> Result<()> {
    if let Some(nick) = message.source_nickname() {
        let mut state_lock = state.lock().await;

        // Check if it's our own part
        if Some(nick) == state_lock.current_nick.as_deref() {
            state_lock.joined_channels.retain(|c| c != channel);
            info!("Left channel: {}", channel);
        }
    }

    Ok(())
}
