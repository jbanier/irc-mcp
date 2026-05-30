use axum::body::Body;
use axum::http::{Request, StatusCode};
use irc_mcp_server::config::IrcMcpConfig;
use irc_mcp_server::mcp::create_mcp_server;
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::{AppState, ServerContext};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn create_test_state() -> (Arc<RwLock<AppState>>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let download_dir = temp_dir.path().join("downloads");

    std::fs::create_dir_all(&download_dir).unwrap();

    let config = IrcMcpConfig {
        servers: vec![irc_mcp_server::config::ServerConfig {
            name: "testserver".to_string(),
            address: "irc.test.org".to_string(),
            port: 6667,
            use_tls: false,
            password: None,
            sasl: irc_mcp_server::config::SaslConfig::default(),
            identity: irc_mcp_server::config::IdentityConfig {
                nickname: "testbot".to_string(),
                username: "test".to_string(),
                realname: "Test Bot".to_string(),
            },
            channels: vec!["#test".to_string()],
            dcc: irc_mcp_server::config::DccConfig {
                enabled: true,
                download_directory: download_dir.to_string_lossy().to_string(),
                max_file_size_bytes: 10485760,
                auto_accept: true,
                allowed_extensions: vec![],
            },
        }],
        storage: irc_mcp_server::config::StorageConfig {
            database_path: db_path.to_string_lossy().to_string(),
            message_retention_days: 30,
            cleanup_interval_hours: 24,
        },
        mcp: irc_mcp_server::config::McpConfig {
            listen_address: "127.0.0.1".to_string(),
            port: 5001,
            default_server: "testserver".to_string(),
        },
    };

    let _db = Database::new(&config.storage.database_path).unwrap();

    let state = Arc::new(RwLock::new(AppState::new(config)));

    (state, temp_dir)
}

#[tokio::test]
async fn test_mcp_initialize() {
    let (state, _temp_dir) = create_test_state().await;
    let app = create_mcp_server(state);

    let request = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["result"]["serverInfo"]["name"], "irc-mcp-server");
}

#[tokio::test]
async fn test_mcp_tools_list() {
    let (state, _temp_dir) = create_test_state().await;
    let app = create_mcp_server(state);

    let request = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/list"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    let tools = json["result"]["tools"].as_array().unwrap();
    assert!(tools.len() > 0);

    // Check for expected tools
    let tool_names: Vec<String> = tools
        .iter()
        .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
        .collect();

    assert!(tool_names.contains(&"irc_connect".to_string()));
    assert!(tool_names.contains(&"irc_send_message".to_string()));
    assert!(tool_names.contains(&"irc_get_messages".to_string()));
}

#[tokio::test]
async fn test_mcp_status_tool() {
    let (state, _temp_dir) = create_test_state().await;
    let app = create_mcp_server(state);

    let request = Request::builder()
        .uri("/mcp")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "irc_status",
                    "arguments": {}
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["result"]["connected"], false);
}
