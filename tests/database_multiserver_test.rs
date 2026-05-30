use anyhow::Result;
use chrono::Utc;
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::{IrcMessage, MessageType};
use tempfile::tempdir;

#[test]
fn test_insert_message_with_server_name() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let db = Database::new(&db_path)?;

    let msg = IrcMessage {
        id: None,
        timestamp: Utc::now(),
        source_nick: "alice".to_string(),
        target: "#rust".to_string(),
        message_type: MessageType::Channel,
        content: "Hello from freenode".to_string(),
        channel: Some("#rust".to_string()),
        server_name: "freenode".to_string(),
    };

    let id = db.insert_message(&msg)?;
    assert!(id > 0);

    // Verify server_name round-trips correctly
    let retrieved_msgs = db.get_messages("#rust", 10, None, None, None, Some("freenode"))?;
    assert_eq!(retrieved_msgs.len(), 1);
    assert_eq!(retrieved_msgs[0].server_name, "freenode");
    assert_eq!(retrieved_msgs[0].content, "Hello from freenode");

    Ok(())
}

#[test]
fn test_get_messages_filters_by_server() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let db = Database::new(&db_path)?;

    // Insert messages from two servers
    let msg1 = IrcMessage {
        id: None,
        timestamp: Utc::now(),
        source_nick: "alice".to_string(),
        target: "#rust".to_string(),
        message_type: MessageType::Channel,
        content: "Message from freenode".to_string(),
        channel: Some("#rust".to_string()),
        server_name: "freenode".to_string(),
    };

    let msg2 = IrcMessage {
        id: None,
        timestamp: Utc::now(),
        source_nick: "bob".to_string(),
        target: "#rust".to_string(),
        message_type: MessageType::Channel,
        content: "Message from libera".to_string(),
        channel: Some("#rust".to_string()),
        server_name: "libera".to_string(),
    };

    db.insert_message(&msg1)?;
    db.insert_message(&msg2)?;

    // Filter by freenode only
    let freenode_msgs = db.get_messages("#rust", 10, None, None, None, Some("freenode"))?;
    assert_eq!(freenode_msgs.len(), 1);
    assert_eq!(freenode_msgs[0].server_name, "freenode");
    assert_eq!(freenode_msgs[0].content, "Message from freenode");

    // Filter by libera only
    let libera_msgs = db.get_messages("#rust", 10, None, None, None, Some("libera"))?;
    assert_eq!(libera_msgs.len(), 1);
    assert_eq!(libera_msgs[0].server_name, "libera");
    assert_eq!(libera_msgs[0].content, "Message from libera");

    // No filter - should get both
    let all_msgs = db.get_messages("#rust", 10, None, None, None, None)?;
    assert_eq!(all_msgs.len(), 2);

    Ok(())
}

#[test]
fn test_delete_messages_before() -> Result<()> {
    let dir = tempdir()?;
    let db_path = dir.path().join("test.db");
    let db = Database::new(&db_path)?;

    let old_time = Utc::now() - chrono::Duration::days(10);
    let recent_time = Utc::now();

    let old_msg = IrcMessage {
        id: None,
        timestamp: old_time,
        source_nick: "alice".to_string(),
        target: "#rust".to_string(),
        message_type: MessageType::Channel,
        content: "Old message".to_string(),
        channel: Some("#rust".to_string()),
        server_name: "freenode".to_string(),
    };

    let recent_msg = IrcMessage {
        id: None,
        timestamp: recent_time,
        source_nick: "bob".to_string(),
        target: "#rust".to_string(),
        message_type: MessageType::Channel,
        content: "Recent message".to_string(),
        channel: Some("#rust".to_string()),
        server_name: "freenode".to_string(),
    };

    db.insert_message(&old_msg)?;
    db.insert_message(&recent_msg)?;

    // Delete messages older than 5 days
    let cutoff = Utc::now() - chrono::Duration::days(5);
    let deleted_count = db.delete_messages_before(cutoff)?;
    assert_eq!(deleted_count, 1);

    // Only recent message should remain
    let remaining_msgs = db.get_messages("#rust", 10, None, None, None, Some("freenode"))?;
    assert_eq!(remaining_msgs.len(), 1);
    assert_eq!(remaining_msgs[0].content, "Recent message");

    Ok(())
}
