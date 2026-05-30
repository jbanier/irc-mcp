use crate::types::SharedState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::SocketAddr;
use tracing::{debug, info};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// Create MCP server with Axum
pub fn create_mcp_server(state: SharedState) -> Router {
    Router::new()
        .route("/mcp", post(handle_mcp_request))
        .with_state(state)
}

/// Handle MCP JSON-RPC requests
async fn handle_mcp_request(
    State(state): State<SharedState>,
    Json(request): Json<JsonRpcRequest>,
) -> impl IntoResponse {
    debug!("MCP request: method={} id={:?}", request.method, request.id);

    let response = match request.method.as_str() {
        "initialize" => handle_initialize(request.id).await,
        "tools/list" => handle_tools_list(request.id).await,
        "tools/call" => handle_tools_call(request.id, request.params, state).await,
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
        },
    };

    (StatusCode::OK, Json(response))
}

/// Handle initialize request
async fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(json!({
            "protocolVersion": "2025-03-26",
            "serverInfo": {
                "name": "irc-mcp-server",
                "version": "0.1.0"
            },
            "capabilities": {
                "tools": {}
            }
        })),
        error: None,
    }
}

/// Handle tools/list request
async fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let tools = vec![
        json!({
            "name": "irc_connect",
            "description": "Connect to the configured IRC server",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "irc_disconnect",
            "description": "Disconnect from IRC server",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "quit_message": {
                        "type": "string",
                        "description": "Optional quit message"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                }
            }
        }),
        json!({
            "name": "irc_status",
            "description": "Get current IRC connection status",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }),
        json!({
            "name": "irc_join_channel",
            "description": "Join an IRC channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel name (must start with #)"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                },
                "required": ["channel"]
            }
        }),
        json!({
            "name": "irc_part_channel",
            "description": "Leave an IRC channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel name"
                    },
                    "message": {
                        "type": "string",
                        "description": "Optional part message"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                },
                "required": ["channel"]
            }
        }),
        json!({
            "name": "irc_send_message",
            "description": "Send a message to a channel or user",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Channel (#channel) or nickname"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message content"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                },
                "required": ["target", "message"]
            }
        }),
        json!({
            "name": "irc_get_messages",
            "description": "Retrieve messages from a channel or user",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Channel or nickname"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum messages to return",
                        "default": 100
                    },
                    "since_timestamp": {
                        "type": "string",
                        "description": "ISO 8601 timestamp"
                    },
                    "sender_filter": {
                        "type": "string",
                        "description": "Filter by sender nickname"
                    },
                    "search_query": {
                        "type": "string",
                        "description": "Search in message content"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                },
                "required": ["target"]
            }
        }),
        json!({
            "name": "irc_get_channel_users",
            "description": "List users in a channel",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel name"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                },
                "required": ["channel"]
            }
        }),
        json!({
            "name": "irc_list_dcc_transfers",
            "description": "List DCC file transfers",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status_filter": {
                        "type": "string",
                        "enum": ["all", "pending", "downloading", "completed", "failed"],
                        "default": "all"
                    },
                    "limit": {
                        "type": "integer",
                        "default": 50
                    }
                }
            }
        }),
        json!({
            "name": "irc_get_dcc_file_info",
            "description": "Get details about a DCC file transfer",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "transfer_id": {
                        "type": "integer",
                        "description": "DCC transfer ID"
                    }
                },
                "required": ["transfer_id"]
            }
        }),
        json!({
            "name": "irc_read_dcc_file",
            "description": "Read content from a received DCC file",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "transfer_id": {
                        "type": "integer",
                        "description": "DCC transfer ID"
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Byte offset to start reading from",
                        "default": 0
                    },
                    "length": {
                        "type": "integer",
                        "description": "Number of bytes to read (0 = all)",
                        "default": 0
                    },
                    "encoding": {
                        "type": "string",
                        "enum": ["utf8", "base64"],
                        "description": "How to encode the content",
                        "default": "utf8"
                    }
                },
                "required": ["transfer_id"]
            }
        }),
        json!({
            "name": "irc_send_raw",
            "description": "Send a raw IRC command",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Raw IRC command"
                    },
                    "server": {
                        "type": "string",
                        "description": "Server name (defaults to active server)"
                    }
                },
                "required": ["command"]
            }
        }),
        json!({
            "name": "irc_search_history",
            "description": "Full-text search across message history",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "channel_filter": {
                        "type": "string",
                        "description": "Only search in this channel"
                    },
                    "limit": {
                        "type": "integer",
                        "default": 100
                    }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "irc_set_active_server",
            "description": "Set the active IRC server for subsequent commands",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "server": {
                        "type": "string",
                        "description": "Server name to set as active"
                    }
                },
                "required": ["server"]
            }
        }),
        json!({
            "name": "irc_get_active_server",
            "description": "Get the currently active IRC server name",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "irc_list_servers",
            "description": "List all configured IRC servers with their connection status",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }),
        json!({
            "name": "irc_connect_server",
            "description": "Manually connect to a specific IRC server",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "server": {
                        "type": "string",
                        "description": "Server name to connect"
                    }
                },
                "required": ["server"]
            }
        }),
        json!({
            "name": "irc_disconnect_server",
            "description": "Disconnect from a specific IRC server",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "server": {
                        "type": "string",
                        "description": "Server name to disconnect"
                    }
                },
                "required": ["server"]
            }
        }),
    ];

    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(json!({ "tools": tools })),
        error: None,
    }
}

/// Handle tools/call request
async fn handle_tools_call(
    id: Option<Value>,
    params: Option<Value>,
    state: SharedState,
) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            };
        }
    };

    match crate::mcp::handle_tool_call(params, state).await {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        },
        Err(e) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32603,
                message: e.to_string(),
                data: None,
            }),
        },
    }
}

/// Start MCP server
pub async fn start_mcp_server(
    addr: &str,
    state: crate::types::SharedState,
    _database: std::sync::Arc<tokio::sync::Mutex<crate::storage::Database>>,
) -> anyhow::Result<()> {
    let app = create_mcp_server(state);
    let socket_addr: SocketAddr = addr.parse()?;

    info!("IRC MCP Server listening on http://{}", socket_addr);

    let listener = tokio::net::TcpListener::bind(socket_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
