use crate::storage::Database;
use crate::types::{ConnectionStatus, DccStatus, IrcCommand, SharedState};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use std::fs;

/// Get the target server name (from param or active server)
async fn resolve_server_name(
    state: &SharedState,
    server_param: Option<&str>,
) -> Result<String, String> {
    let state_read = state.read().await;

    let server_name = if let Some(server) = server_param {
        server.to_string()
    } else {
        state_read.active_server.clone()
    };

    if !state_read.servers.contains_key(&server_name) {
        return Err(format!("Server '{}' not configured", server_name));
    }

    Ok(server_name)
}

/// Get IRC sender for a server
async fn get_server_sender(
    state: &SharedState,
    server_name: &str,
) -> Result<tokio::sync::mpsc::UnboundedSender<IrcCommand>, String> {
    let state_read = state.read().await;

    let ctx = state_read
        .servers
        .get(server_name)
        .ok_or_else(|| format!("Server '{}' not found", server_name))?;

    let sender = ctx
        .irc_sender
        .clone()
        .ok_or_else(|| format!("Server '{}' is not connected", server_name))?;

    Ok(sender)
}

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
        "irc_set_active_server" => tool_irc_set_active_server(arguments, state).await,
        "irc_get_active_server" => tool_irc_get_active_server(state).await,
        "irc_list_servers" => tool_irc_list_servers(state).await,
        "irc_connect_server" => tool_irc_connect_server(arguments, state).await,
        "irc_disconnect_server" => tool_irc_disconnect_server(arguments, state).await,
        _ => bail!("Unknown tool: {}", tool_name),
    }
}

async fn tool_irc_connect(state: SharedState) -> Result<Value> {
    // In multi-server mode, this tool just returns the active server status
    // Actual connection is managed by ServerManager at startup
    let state_read = state.read().await;
    let server_name = &state_read.active_server;

    let ctx = state_read
        .servers
        .get(server_name)
        .ok_or_else(|| anyhow::anyhow!("Active server '{}' not found", server_name))?;

    if ctx.connection_status == ConnectionStatus::Connected {
        Ok(json!({
            "success": true,
            "message": "Already connected",
            "server": format!("{}:{}", ctx.config.address, ctx.config.port),
            "server_name": server_name,
            "nick": ctx.current_nick,
            "joined_channels": ctx.joined_channels,
        }))
    } else {
        Ok(json!({
            "success": false,
            "message": "Not connected. Use irc_connect_server to connect manually.",
            "server": format!("{}:{}", ctx.config.address, ctx.config.port),
            "server_name": server_name,
            "status": format!("{:?}", ctx.connection_status),
        }))
    }
}

async fn tool_irc_disconnect(arguments: Value, state: SharedState) -> Result<Value> {
    let quit_message = arguments["quit_message"]
        .as_str()
        .unwrap_or("Disconnecting");

    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let mut state_write = state.write().await;
    let ctx = state_write
        .servers
        .get_mut(&server_name)
        .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

    if let Some(sender) = &ctx.irc_sender {
        sender.send(IrcCommand::Quit(quit_message.to_string()))?;
        ctx.connection_status = ConnectionStatus::Disconnected;
        ctx.current_nick = None;
        ctx.joined_channels.clear();

        Ok(json!({
            "success": true,
            "server": server_name,
            "message": "Disconnected from IRC server"
        }))
    } else {
        Ok(json!({
            "success": false,
            "server": server_name,
            "message": "Not connected"
        }))
    }
}

async fn tool_irc_status(state: SharedState) -> Result<Value> {
    let state_read = state.read().await;
    let server_name = &state_read.active_server;

    let ctx = state_read
        .servers
        .get(server_name)
        .ok_or_else(|| anyhow::anyhow!("Active server '{}' not found", server_name))?;

    let uptime_seconds = ctx
        .connection_start
        .map(|start| (Utc::now() - start).num_seconds())
        .unwrap_or(0);

    Ok(json!({
        "connected": ctx.connection_status == ConnectionStatus::Connected,
        "server": format!("{}:{}", ctx.config.address, ctx.config.port),
        "server_name": server_name,
        "nick": ctx.current_nick,
        "channels": ctx.joined_channels,
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

    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let sender = get_server_sender(&state, &server_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    sender.send(IrcCommand::Join(channel.to_string()))?;
    Ok(json!({
        "success": true,
        "server": server_name,
        "channel": channel,
        "message": format!("Joining channel {}", channel)
    }))
}

async fn tool_irc_part_channel(arguments: Value, state: SharedState) -> Result<Value> {
    let channel = arguments["channel"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing channel parameter"))?;

    let message = arguments["message"].as_str().map(|s| s.to_string());
    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let sender = get_server_sender(&state, &server_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    sender.send(IrcCommand::Part(channel.to_string(), message))?;
    Ok(json!({
        "success": true,
        "server": server_name,
        "channel": channel,
    }))
}

async fn tool_irc_send_message(arguments: Value, state: SharedState) -> Result<Value> {
    let target = arguments["target"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing target parameter"))?;

    let message = arguments["message"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing message parameter"))?;

    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let sender = get_server_sender(&state, &server_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    sender.send(IrcCommand::SendMessage(
        target.to_string(),
        message.to_string(),
    ))?;

    Ok(json!({
        "success": true,
        "server": server_name,
        "target": target,
        "message": "Message sent"
    }))
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

    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    let db_path = {
        let state_read = state.read().await;
        state_read.config.storage.database_path.clone()
    };

    let db = Database::new(&db_path)?;
    let messages = db.get_messages(
        target,
        limit,
        since,
        sender_filter,
        search_query,
        Some(&server_name),
    )?;

    Ok(json!({
        "server": server_name,
        "messages": messages,
        "count": messages.len(),
        "has_more": messages.len() >= limit,
    }))
}

async fn tool_irc_get_channel_users(arguments: Value, state: SharedState) -> Result<Value> {
    let channel = arguments["channel"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing channel parameter"))?;

    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let sender = get_server_sender(&state, &server_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // Send NAMES command via raw command
    sender.send(IrcCommand::SendRaw(format!("NAMES {}", channel)))?;

    // Note: Getting the actual user list requires parsing NAMES responses
    // For now, return a pending status
    Ok(json!({
        "server": server_name,
        "channel": channel,
        "message": "NAMES request sent - user list will be in message stream",
    }))
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

    let db_path = {
        let state_read = state.read().await;
        state_read.config.storage.database_path.clone()
    };

    let db = Database::new(&db_path)?;
    let transfers = db.list_dcc_transfers(status_filter, limit)?;

    Ok(json!({ "transfers": transfers }))
}

async fn tool_irc_get_dcc_file_info(arguments: Value, state: SharedState) -> Result<Value> {
    let transfer_id = arguments["transfer_id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("Missing or invalid transfer_id parameter"))?;

    let db_path = {
        let state_read = state.read().await;
        state_read.config.storage.database_path.clone()
    };

    let db = Database::new(&db_path)?;
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

    let db_path = {
        let state_read = state.read().await;
        state_read.config.storage.database_path.clone()
    };

    let db = Database::new(&db_path)?;
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

    let server_param = arguments["server"].as_str();
    let server_name = resolve_server_name(&state, server_param)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let sender = get_server_sender(&state, &server_name)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    sender.send(IrcCommand::SendRaw(command.to_string()))?;

    Ok(json!({
        "success": true,
        "server": server_name,
        "command": command,
    }))
}

async fn tool_irc_search_history(arguments: Value, state: SharedState) -> Result<Value> {
    let query = arguments["query"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing query parameter"))?;

    let channel_filter = arguments["channel_filter"].as_str();
    let limit = arguments["limit"].as_u64().unwrap_or(100) as usize;

    let db_path = {
        let state_read = state.read().await;
        state_read.config.storage.database_path.clone()
    };

    let db = Database::new(&db_path)?;
    let messages = db.search_messages(query, channel_filter, limit)?;

    Ok(json!({
        "messages": messages,
        "count": messages.len(),
        "query": query,
    }))
}

async fn tool_irc_set_active_server(arguments: Value, state: SharedState) -> Result<Value> {
    let server = arguments["server"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing server parameter"))?;

    let mut state_write = state.write().await;

    if !state_write.servers.contains_key(server) {
        bail!("Server '{}' not configured", server);
    }

    state_write.active_server = server.to_string();

    Ok(json!({
        "success": true,
        "active_server": server
    }))
}

async fn tool_irc_get_active_server(state: SharedState) -> Result<Value> {
    let state_read = state.read().await;
    Ok(json!({
        "active_server": state_read.active_server
    }))
}

async fn tool_irc_list_servers(state: SharedState) -> Result<Value> {
    let state_read = state.read().await;

    let servers: Vec<_> = state_read
        .servers
        .iter()
        .map(|(name, ctx)| {
            json!({
                "name": name,
                "address": ctx.config.address,
                "port": ctx.config.port,
                "status": format!("{:?}", ctx.connection_status),
                "channels_joined": ctx.joined_channels.len(),
                "use_tls": ctx.config.use_tls,
                "sasl_enabled": ctx.config.sasl.enabled,
            })
        })
        .collect();

    Ok(json!({ "servers": servers }))
}

async fn tool_irc_connect_server(_arguments: Value, _state: SharedState) -> Result<Value> {
    // Manual connection will be implemented in Task 9 with ServerManager integration
    // For now, connections are managed automatically at startup
    bail!("Manual server connection not yet implemented. Servers are connected automatically at startup. Use irc_status or irc_list_servers to check connection status.")
}

async fn tool_irc_disconnect_server(arguments: Value, state: SharedState) -> Result<Value> {
    let server_name = arguments["server"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing server parameter"))?;

    let mut state_write = state.write().await;

    let ctx = state_write
        .servers
        .get_mut(server_name)
        .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", server_name))?;

    if let Some(sender) = &ctx.irc_sender {
        let _ = sender.send(IrcCommand::Quit("Manual disconnect".to_string()));
    }

    ctx.connection_status = ConnectionStatus::Disconnected;
    ctx.irc_sender = None;

    Ok(json!({
        "success": true,
        "server": server_name
    }))
}
