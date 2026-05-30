use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Connection status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    #[allow(dead_code)]
    Error,
}

/// IRC message stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrcMessage {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub source_nick: String,
    pub target: String,
    pub message_type: MessageType,
    pub content: String,
    pub channel: Option<String>,
    #[serde(default)]
    pub server_name: String,
}

/// Message type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Channel,
    Private,
    Notice,
    Ctcp,
    System,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::Channel => "channel",
            MessageType::Private => "private",
            MessageType::Notice => "notice",
            MessageType::Ctcp => "ctcp",
            MessageType::System => "system",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "channel" => Some(MessageType::Channel),
            "private" => Some(MessageType::Private),
            "notice" => Some(MessageType::Notice),
            "ctcp" => Some(MessageType::Ctcp),
            "system" => Some(MessageType::System),
            _ => None,
        }
    }
}

/// DCC transfer status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DccStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
}

impl DccStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DccStatus::Pending => "pending",
            DccStatus::Downloading => "downloading",
            DccStatus::Completed => "completed",
            DccStatus::Failed => "failed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(DccStatus::Pending),
            "downloading" => Some(DccStatus::Downloading),
            "completed" => Some(DccStatus::Completed),
            "failed" => Some(DccStatus::Failed),
            _ => None,
        }
    }
}

/// DCC transfer record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DccTransfer {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub sender_nick: String,
    pub filename: String,
    pub filepath: Option<String>,
    pub filesize: u64,
    pub received_size: u64,
    pub status: DccStatus,
    pub error: Option<String>,
    pub ip_address: Option<String>,
    pub port: Option<u16>,
    pub extracted_files: Option<Vec<ExtractedFile>>,
    pub extraction_status: Option<String>,
    pub extraction_error: Option<String>,
    #[serde(default)]
    pub server_name: String,
}

/// Extracted file metadata from zip archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFile {
    pub relative_path: String,
    pub full_path: String,
    pub size: u64,
}

/// Per-server runtime context
#[derive(Debug)]
pub struct ServerContext {
    pub config: crate::config::ServerConfig,
    pub client: Option<irc::client::Client>,
    pub connection_status: ConnectionStatus,
    pub connection_start: Option<DateTime<Utc>>,
    pub current_nick: Option<String>,
    pub irc_sender: Option<tokio::sync::mpsc::UnboundedSender<IrcCommand>>,
    pub joined_channels: HashSet<String>,
    pub dcc_transfers: HashMap<String, DccTransfer>,
    pub reconnect_attempts: u32,
}

impl ServerContext {
    pub fn new(config: crate::config::ServerConfig) -> Self {
        Self {
            config,
            client: None,
            connection_status: ConnectionStatus::Disconnected,
            connection_start: None,
            current_nick: None,
            irc_sender: None,
            joined_channels: HashSet::new(),
            dcc_transfers: HashMap::new(),
            reconnect_attempts: 0,
        }
    }
}

/// Application state shared across handlers
pub struct AppState {
    pub servers: HashMap<String, ServerContext>,
    pub active_server: String,
    pub config: crate::config::IrcMcpConfig,
}

impl AppState {
    pub fn new(config: crate::config::IrcMcpConfig) -> Self {
        let active_server = config.mcp.default_server.clone();
        let servers = config
            .servers
            .iter()
            .map(|sc| (sc.name.clone(), ServerContext::new(sc.clone())))
            .collect();

        Self {
            servers,
            active_server,
            config,
        }
    }
}

/// IRC commands that can be sent to a specific server's client task
#[derive(Debug)]
pub enum IrcCommand {
    Join(String),
    Part(String, Option<String>),
    SendMessage(String, String),
    SendRaw(String),
    Quit(String),
}

pub type SharedState = Arc<RwLock<AppState>>;
