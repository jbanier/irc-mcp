use crate::irc::{start_message_processor, IrcClientManager};
use crate::storage::Database;
use crate::types::{ConnectionStatus, DccStatus, IrcCommand, SharedState};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::fs;
use tokio::sync::mpsc;
use tracing::{error, info};

/// Handle MCP tool call
pub async fn handle_tool_call(params: Value, state: SharedState) -> Result<Value> {
    let tool_name = params["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing tool name"))?;

    let arguments = params["arguments"].clone();

    match tool_name {
        "irc_connect" => tool_irc_connect(state).await,
        "irc_disconnect" => tool_irc_disconnect(arguments, state).await,
        "irc_status" => tool_irc_status(state).await,
        "irc_join_channel" => tool_irc_join_channel(arguments, state).await,
        "irc_part_channel" => tool_irc_part_channel(arguments, state).await,
        "irc_send_message" => tool_irc_send_message(arguments, state).await,
        "irc_get_messages" => tool_irc_get_messages(arguments, state).await,
        "irc_get_channel_users" => tool_irc_get_channel_users(arguments, state).await,
        "irc_list_dcc_transfers" => tool_irc_list_dcc_transfers(arguments, state).await,
        "irc_get_dcc_file_info" => tool_irc_get_dcc_file_info(arguments, state).await,
        "irc_read_dcc_file" => tool_irc_read_dcc_file(arguments, state).await,
        "irc_send_raw" => tool_irc_send_raw(arguments, state).await,
        "irc_search_history" => tool_irc_search_history(arguments, state).await,
        _ => bail!("Unknown tool: {}", tool_name),
    }
}

async fn tool_irc_connect(state: SharedState) -> Result<Value> {
    let mut state_lock = state.lock().await;

    if state_lock.connection_status == ConnectionStatus::Connected {
        return Ok(json!({
            "success": true,
            "message": "Already connected",
            "server": format!("{}:{}", state_lock.config.server.address, state_lock.config.server.port),
            "nick": state_lock.current_nick,
            "joined_channels": state_lock.joined_channels,
        }));
    }

    info!("Connecting to IRC server...");
    state_lock.connection_status = ConnectionStatus::Connecting;

    let manager = IrcClientManager::new(state_lock.config.clone());
    let client = manager.connect().await?;

    let nick = state_lock.config.identity.nickname.clone();
    state_lock.current_nick = Some(nick.clone());
    state_lock.connection_status = ConnectionStatus::Connected;
    state_lock.connection_start = Some(Utc::now());

    // Create command channel
    let (cmd_sender, cmd_receiver) = mpsc::unbounded_channel();
    state_lock.irc_sender = Some(cmd_sender);

    // Spawn message processor
    let state_clone = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_message_processor(client, cmd_receiver, state_clone).await {
            error!("Message processor error: {}", e);
        }
    });

    Ok(json!({
        "success": true,
        "server": format!("{}:{}", state_lock.config.server.address, state_lock.config.server.port),
        "nick": nick,
        "joined_channels": state_lock.config.channels,
    }))
}

async fn tool_irc_disconnect(arguments: Value, state: SharedState) -> Result<Value> {
    let quit_message = arguments["quit_message"]
        .as_str()
        .unwrap_or("Disconnecting");

    let mut state_lock = state.lock().await;

    if let Some(sender) = &state_lock.irc_sender {
        sender.send(IrcCommand::Quit(quit_message.to_string()))?;
        state_lock.connection_status = ConnectionStatus::Disconnected;
        state_lock.current_nick = None;
        state_lock.joined_channels.clear();

        Ok(json!({
            "success": true,
            "message": "Disconnected from IRC server"
        }))
    } else {
        Ok(json!({
            "success": false,
            "message": "Not connected"
        }))
    }
}

async fn tool_irc_status(state: SharedState) -> Result<Value> {
    let state_lock = state.lock().await;

    let uptime_seconds = state_lock
        .connection_start
        .map(|start| (Utc::now() - start).num_seconds())
        .unwrap_or(0);

    Ok(json!({
        "connected": state_lock.connection_status == ConnectionStatus::Connected,
        "server": format!("{}:{}", state_lock.config.server.address, state_lock.config.server.port),
        "nick": state_lock.current_nick,
        "channels": state_lock.joined_channels,
        "uptime_seconds": uptime_seconds,
    }))
}

async fn tool_irc_join_channel(arguments: Value, state: SharedState) -> Result<Value> {
    let channel = arguments["channel"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing channel parameter"))?;

    if !channel.starts_with('#') {
        bail!("Channel name must start with #");
    }

    let state_lock = state.lock().await;

    if let Some(sender) = &state_lock.irc_sender {
        sender.send(IrcCommand::Join(channel.to_string()))?;
        Ok(json!({
            "success": true,
            "channel": channel,
            "message": format!("Joining channel {}", channel)
        }))
    } else {
        bail!("Not connected to IRC server");
    }
}

async fn tool_irc_part_channel(arguments: Value, state: SharedState) -> Result<Value> {
    let channel = arguments["channel"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing channel parameter"))?;

    let message = arguments["message"].as_str().map(|s| s.to_string());

    let state_lock = state.lock().await;

    if let Some(sender) = &state_lock.irc_sender {
        sender.send(IrcCommand::Part(channel.to_string(), message))?;
        Ok(json!({
            "success": true,
            "channel": channel,
        }))
    } else {
        bail!("Not connected to IRC server");
    }
}

async fn tool_irc_send_message(arguments: Value, state: SharedState) -> Result<Value> {
    let target = arguments["target"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing target parameter"))?;

    let message = arguments["message"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing message parameter"))?;

    let state_lock = state.lock().await;

    if let Some(sender) = &state_lock.irc_sender {
        sender.send(IrcCommand::SendMessage(
            target.to_string(),
            message.to_string(),
        ))?;

        Ok(json!({
            "success": true,
            "target": target,
            "message": "Message sent"
        }))
    } else {
        bail!("Not connected to IRC server");
    }
}

async fn tool_irc_get_messages(arguments: Value, state: SharedState) -> Result<Value> {
    let target = arguments["target"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing target parameter"))?;

    let limit = arguments["limit"].as_u64().unwrap_or(100) as usize;

    let since = arguments["since_timestamp"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let sender_filter = arguments["sender_filter"].as_str();
    let search_query = arguments["search_query"].as_str();

    let state_lock = state.lock().await;
    let db = Database::new(&state_lock.config.storage.database_path)?;

    let messages = db.get_messages(target, limit, since, sender_filter, search_query)?;

    Ok(json!({
        "messages": messages,
        "count": messages.len(),
        "has_more": messages.len() >= limit,
    }))
}

async fn tool_irc_get_channel_users(arguments: Value, state: SharedState) -> Result<Value> {
    let channel = arguments["channel"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing channel parameter"))?;

    let state_lock = state.lock().await;

    if let Some(sender) = &state_lock.irc_sender {
        // Send NAMES command via raw command
        sender.send(IrcCommand::SendRaw(format!("NAMES {}", channel)))?;

        // Note: Getting the actual user list requires parsing NAMES responses
        // For now, return a pending status
        Ok(json!({
            "channel": channel,
            "message": "NAMES request sent - user list will be in message stream",
        }))
    } else {
        bail!("Not connected to IRC server");
    }
}

async fn tool_irc_list_dcc_transfers(arguments: Value, state: SharedState) -> Result<Value> {
    let status_filter_str = arguments["status_filter"].as_str();
    let status_filter = status_filter_str.and_then(|s| match s {
        "pending" => Some(DccStatus::Pending),
        "downloading" => Some(DccStatus::Downloading),
        "completed" => Some(DccStatus::Completed),
        "failed" => Some(DccStatus::Failed),
        _ => None,
    });

    let limit = arguments["limit"].as_u64().unwrap_or(50) as usize;

    let state_lock = state.lock().await;
    let db = Database::new(&state_lock.config.storage.database_path)?;

    let transfers = db.list_dcc_transfers(status_filter, limit)?;

    Ok(json!({ "transfers": transfers }))
}

async fn tool_irc_get_dcc_file_info(arguments: Value, state: SharedState) -> Result<Value> {
    let transfer_id = arguments["transfer_id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid transfer_id parameter"))?;

    let state_lock = state.lock().await;
    let db = Database::new(&state_lock.config.storage.database_path)?;

    let transfer = db
        .get_dcc_transfer(transfer_id)?
        .ok_or_else(|| anyhow::anyhow!("Transfer not found"))?;

    Ok(json!(transfer))
}

async fn tool_irc_read_dcc_file(arguments: Value, state: SharedState) -> Result<Value> {
    let transfer_id = arguments["transfer_id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid transfer_id parameter"))?;

    let offset = arguments["offset"].as_u64().unwrap_or(0);
    let length = arguments["length"].as_u64().unwrap_or(0);
    let encoding = arguments["encoding"].as_str().unwrap_or("utf8");

    let state_lock = state.lock().await;
    let db = Database::new(&state_lock.config.storage.database_path)?;

    let transfer = db
        .get_dcc_transfer(transfer_id)?
        .ok_or_else(|| anyhow::anyhow!("Transfer not found"))?;

    if transfer.status != DccStatus::Completed {
        bail!("Transfer not completed");
    }

    let filepath = transfer
        .filepath
        .ok_or_else(|| anyhow::anyhow!("File path not available"))?;

    let mut file_content =
        fs::read(&filepath).with_context(|| format!("Failed to read file: {}", filepath))?;

    // Apply offset and length
    let start = offset as usize;
    let end = if length == 0 {
        file_content.len()
    } else {
        std::cmp::min(start + length as usize, file_content.len())
    };

    if start >= file_content.len() {
        file_content = Vec::new();
    } else {
        file_content = file_content[start..end].to_vec();
    }

    let content = match encoding {
        "base64" => {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&file_content)
        }
        "utf8" => String::from_utf8_lossy(&file_content).to_string(),
        _ => bail!("Invalid encoding: must be 'utf8' or 'base64'"),
    };

    Ok(json!({
        "transfer_id": transfer_id,
        "filename": transfer.filename,
        "content": content,
        "encoding": encoding,
        "bytes_returned": file_content.len(),
    }))
}

async fn tool_irc_send_raw(arguments: Value, state: SharedState) -> Result<Value> {
    let command = arguments["command"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing command parameter"))?;

    let state_lock = state.lock().await;

    if let Some(sender) = &state_lock.irc_sender {
        sender.send(IrcCommand::SendRaw(command.to_string()))?;

        Ok(json!({
            "success": true,
            "command": command,
        }))
    } else {
        bail!("Not connected to IRC server");
    }
}

async fn tool_irc_search_history(arguments: Value, state: SharedState) -> Result<Value> {
    let query = arguments["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing query parameter"))?;

    let channel_filter = arguments["channel_filter"].as_str();
    let limit = arguments["limit"].as_u64().unwrap_or(100) as usize;

    let state_lock = state.lock().await;
    let db = Database::new(&state_lock.config.storage.database_path)?;

    let messages = db.search_messages(query, channel_filter, limit)?;

    Ok(json!({
        "messages": messages,
        "count": messages.len(),
        "query": query,
    }))
}
