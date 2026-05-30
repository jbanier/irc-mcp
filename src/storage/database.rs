use crate::types::{DccStatus, DccTransfer, ExtractedFile, IrcMessage, MessageType};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Create or open database at given path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        // Create parent directories if needed
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent).context("Failed to create database directory")?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("Failed to open database: {}", path.as_ref().display()))?;

        let db = Database { conn };
        db.init_schema()?;
        db.migrate_schema()?;
        Ok(db)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                source_nick TEXT NOT NULL,
                target TEXT NOT NULL,
                message_type TEXT NOT NULL,
                content TEXT NOT NULL,
                channel TEXT,
                server_name TEXT NOT NULL DEFAULT '',
                UNIQUE(timestamp, source_nick, target, content, server_name)
            );

            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_target ON messages(target, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_server ON messages(server_name, timestamp DESC);

            CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(content, content=messages, content_rowid=id);

            CREATE TABLE IF NOT EXISTS dcc_transfers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                sender_nick TEXT NOT NULL,
                filename TEXT NOT NULL,
                filepath TEXT,
                filesize INTEGER NOT NULL,
                received_size INTEGER DEFAULT 0,
                status TEXT NOT NULL,
                error TEXT,
                ip_address TEXT,
                port INTEGER,
                extraction_status TEXT,
                extraction_error TEXT,
                extracted_files_json TEXT,
                server_name TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_dcc_status ON dcc_transfers(status, timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_dcc_server ON dcc_transfers(server_name, timestamp DESC);

            CREATE TABLE IF NOT EXISTS channels (
                channel_name TEXT PRIMARY KEY,
                joined_at TEXT NOT NULL,
                last_activity TEXT
            );
            "#,
        )
        .context("Failed to initialize database schema")?;

        Ok(())
    }

    /// Migrate database schema to add extraction columns if missing
    fn migrate_schema(&self) -> Result<()> {
        // Check if extraction columns exist in dcc_transfers
        let dcc_columns: Vec<String> = self
            .conn
            .prepare("PRAGMA table_info(dcc_transfers)")?
            .query_map([], |row| row.get(1))?
            .collect::<Result<Vec<_>, _>>()?;

        if !dcc_columns.contains(&"extraction_status".to_string()) {
            self.conn.execute_batch(
                r#"
                ALTER TABLE dcc_transfers ADD COLUMN extraction_status TEXT;
                ALTER TABLE dcc_transfers ADD COLUMN extraction_error TEXT;
                ALTER TABLE dcc_transfers ADD COLUMN extracted_files_json TEXT;
                "#,
            )?;
        }

        // Add server_name column to dcc_transfers if missing
        if !dcc_columns.contains(&"server_name".to_string()) {
            self.conn.execute(
                "ALTER TABLE dcc_transfers ADD COLUMN server_name TEXT NOT NULL DEFAULT ''",
                [],
            )?;
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_dcc_server ON dcc_transfers(server_name, timestamp DESC)",
                [],
            )?;
        }

        // Check if server_name exists in messages table
        let msg_columns: Vec<String> = self
            .conn
            .prepare("PRAGMA table_info(messages)")?
            .query_map([], |row| row.get(1))?
            .collect::<Result<Vec<_>, _>>()?;

        if !msg_columns.contains(&"server_name".to_string()) {
            self.conn.execute(
                "ALTER TABLE messages ADD COLUMN server_name TEXT NOT NULL DEFAULT ''",
                [],
            )?;
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_messages_server ON messages(server_name, timestamp DESC)",
                [],
            )?;
        }

        Ok(())
    }

    /// Insert a message into the database
    pub fn insert_message(&self, msg: &IrcMessage) -> Result<i64> {
        let timestamp = msg.timestamp.to_rfc3339();
        let message_type = msg.message_type.as_str();

        self.conn
            .execute(
                "INSERT OR IGNORE INTO messages (timestamp, source_nick, target, message_type, content, channel, server_name)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    timestamp,
                    &msg.source_nick,
                    &msg.target,
                    message_type,
                    &msg.content,
                    &msg.channel,
                    &msg.server_name,
                ],
            )
            .context("Failed to insert message")?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get messages from a channel or user
    pub fn get_messages(
        &self,
        target: &str,
        limit: usize,
        since: Option<DateTime<Utc>>,
        sender_filter: Option<&str>,
        search_query: Option<&str>,
        server_filter: Option<&str>,
    ) -> Result<Vec<IrcMessage>> {
        let mut query = String::from(
            "SELECT id, timestamp, source_nick, target, message_type, content, channel, server_name FROM messages WHERE target = ?"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(target.to_string())];

        if let Some(since_dt) = since {
            query.push_str(" AND timestamp > ?");
            params.push(Box::new(since_dt.to_rfc3339()));
        }

        if let Some(sender) = sender_filter {
            query.push_str(" AND source_nick = ?");
            params.push(Box::new(sender.to_string()));
        }

        if let Some(search) = search_query {
            query.push_str(" AND content LIKE ?");
            params.push(Box::new(format!("%{}%", search)));
        }

        if let Some(server) = server_filter {
            query.push_str(" AND server_name = ?");
            params.push(Box::new(server.to_string()));
        }

        query.push_str(" ORDER BY timestamp DESC LIMIT ?");
        params.push(Box::new(limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> = params
            .iter()
            .map(|b| &**b as &dyn rusqlite::ToSql)
            .collect();

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map(&params_refs[..], |row| {
            let timestamp_str: String = row.get(1)?;
            let message_type_str: String = row.get(4)?;

            Ok(IrcMessage {
                id: Some(row.get(0)?),
                timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                    .unwrap()
                    .with_timezone(&Utc),
                source_nick: row.get(2)?,
                target: row.get(3)?,
                message_type: MessageType::from_str(&message_type_str)
                    .unwrap_or(MessageType::System),
                content: row.get(5)?,
                channel: row.get(6)?,
                server_name: row.get(7)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to retrieve messages")
    }

    /// Insert a DCC transfer record
    pub fn insert_dcc_transfer(&self, transfer: &DccTransfer) -> Result<i64> {
        let timestamp = transfer.timestamp.to_rfc3339();
        let status = transfer.status.as_str();

        self.conn
            .execute(
                "INSERT INTO dcc_transfers (timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    timestamp,
                    &transfer.sender_nick,
                    &transfer.filename,
                    &transfer.filepath,
                    transfer.filesize as i64,
                    transfer.received_size as i64,
                    status,
                    &transfer.error,
                    &transfer.ip_address,
                    transfer.port.map(|p| p as i64),
                ],
            )
            .context("Failed to insert DCC transfer")?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Update DCC transfer status
    pub fn update_dcc_transfer_status(
        &self,
        id: i64,
        status: DccStatus,
        received_size: u64,
        filepath: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn
            .execute(
                "UPDATE dcc_transfers SET status = ?, received_size = ?, filepath = ?, error = ? WHERE id = ?",
                params![
                    status.as_str(),
                    received_size as i64,
                    filepath,
                    error,
                    id,
                ],
            )
            .context("Failed to update DCC transfer")?;

        Ok(())
    }

    /// Update extraction metadata for a DCC transfer
    pub fn update_extraction_metadata(
        &self,
        transfer_id: i64,
        status: &str,
        extracted_files: Option<&Vec<ExtractedFile>>,
        error: Option<&str>,
    ) -> Result<()> {
        let extracted_json =
            extracted_files.map(|files| serde_json::to_string(files).unwrap_or_default());

        self.conn.execute(
            "UPDATE dcc_transfers SET extraction_status = ?, extracted_files_json = ?, extraction_error = ? WHERE id = ?",
            params![status, extracted_json, error, transfer_id],
        )?;

        Ok(())
    }

    /// List DCC transfers with optional status filter
    pub fn list_dcc_transfers(
        &self,
        status_filter: Option<DccStatus>,
        limit: usize,
    ) -> Result<Vec<DccTransfer>> {
        let query = if let Some(status) = status_filter {
            format!(
                "SELECT id, timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port, extraction_status, extraction_error, extracted_files_json, server_name
                 FROM dcc_transfers WHERE status = '{}' ORDER BY timestamp DESC LIMIT {}",
                status.as_str(),
                limit
            )
        } else {
            format!(
                "SELECT id, timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port, extraction_status, extraction_error, extracted_files_json, server_name
                 FROM dcc_transfers ORDER BY timestamp DESC LIMIT {}",
                limit
            )
        };

        let mut stmt = self.conn.prepare(&query)?;
        let rows = stmt.query_map([], |row| {
            let timestamp_str: String = row.get(1)?;
            let status_str: String = row.get(7)?;
            let extracted_json: Option<String> = row.get(13)?;

            let extracted_files = extracted_json.and_then(|json| serde_json::from_str(&json).ok());

            Ok(DccTransfer {
                id: Some(row.get(0)?),
                timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                    .unwrap()
                    .with_timezone(&Utc),
                sender_nick: row.get(2)?,
                filename: row.get(3)?,
                filepath: row.get(4)?,
                filesize: row.get::<_, i64>(5)? as u64,
                received_size: row.get::<_, i64>(6)? as u64,
                status: DccStatus::from_str(&status_str).unwrap_or(DccStatus::Failed),
                error: row.get(8)?,
                ip_address: row.get(9)?,
                port: row.get::<_, Option<i64>>(10)?.map(|p| p as u16),
                extracted_files,
                extraction_status: row.get(11)?,
                extraction_error: row.get(12)?,
                server_name: row.get(14)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to list DCC transfers")
    }

    /// Get DCC transfer by ID
    pub fn get_dcc_transfer(&self, id: i64) -> Result<Option<DccTransfer>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port, extraction_status, extraction_error, extracted_files_json, server_name
             FROM dcc_transfers WHERE id = ?"
        )?;

        let transfer = stmt
            .query_row(params![id], |row| {
                let timestamp_str: String = row.get(1)?;
                let status_str: String = row.get(7)?;
                let extracted_json: Option<String> = row.get(13)?;

                let extracted_files =
                    extracted_json.and_then(|json| serde_json::from_str(&json).ok());

                Ok(DccTransfer {
                    id: Some(row.get(0)?),
                    timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                        .unwrap()
                        .with_timezone(&Utc),
                    sender_nick: row.get(2)?,
                    filename: row.get(3)?,
                    filepath: row.get(4)?,
                    filesize: row.get::<_, i64>(5)? as u64,
                    received_size: row.get::<_, i64>(6)? as u64,
                    status: DccStatus::from_str(&status_str).unwrap_or(DccStatus::Failed),
                    error: row.get(8)?,
                    ip_address: row.get(9)?,
                    port: row.get::<_, Option<i64>>(10)?.map(|p| p as u16),
                    extracted_files,
                    extraction_status: row.get(11)?,
                    extraction_error: row.get(12)?,
                    server_name: row.get(14)?,
                })
            })
            .optional()
            .context("Failed to get DCC transfer")?;

        Ok(transfer)
    }

    /// Search messages using full-text search
    pub fn search_messages(
        &self,
        query: &str,
        channel_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<IrcMessage>> {
        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::ToSql>>) = if let Some(channel) =
            channel_filter
        {
            (
                format!(
                    "SELECT m.id, m.timestamp, m.source_nick, m.target, m.message_type, m.content, m.channel, m.server_name
                     FROM messages m JOIN messages_fts ON m.id = messages_fts.rowid
                     WHERE messages_fts MATCH ? AND m.channel = ?
                     ORDER BY m.timestamp DESC LIMIT {}",
                    limit
                ),
                vec![Box::new(query.to_string()), Box::new(channel.to_string())]
            )
        } else {
            (
                format!(
                    "SELECT m.id, m.timestamp, m.source_nick, m.target, m.message_type, m.content, m.channel, m.server_name
                     FROM messages m JOIN messages_fts ON m.id = messages_fts.rowid
                     WHERE messages_fts MATCH ?
                     ORDER BY m.timestamp DESC LIMIT {}",
                    limit
                ),
                vec![Box::new(query.to_string())]
            )
        };

        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
            .iter()
            .map(|b| &**b as &dyn rusqlite::ToSql)
            .collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&params_refs[..], |row| {
            let timestamp_str: String = row.get(1)?;
            let message_type_str: String = row.get(4)?;

            Ok(IrcMessage {
                id: Some(row.get(0)?),
                timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                    .unwrap()
                    .with_timezone(&Utc),
                source_nick: row.get(2)?,
                target: row.get(3)?,
                message_type: MessageType::from_str(&message_type_str)
                    .unwrap_or(MessageType::System),
                content: row.get(5)?,
                channel: row.get(6)?,
                server_name: row.get(7)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to search messages")
    }

    /// Delete messages older than the given timestamp
    pub fn delete_messages_before(&self, before: DateTime<Utc>) -> Result<usize> {
        let timestamp = before.to_rfc3339();
        let deleted = self
            .conn
            .execute(
                "DELETE FROM messages WHERE timestamp < ?",
                params![timestamp],
            )
            .context("Failed to delete old messages")?;

        Ok(deleted)
    }
}
