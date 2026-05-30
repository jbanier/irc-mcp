use anyhow::Result;
use chrono::Utc;
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::{IrcMessage, MessageType};
use tempfile::tempdir;

fn main() -> Result<()> {
    println!("Testing database multi-server support...\n");

    // Test 1: Insert message with server_name
    println!("Test 1: Insert message with server_name");
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
    println!("  ✓ Inserted message with ID: {}", id);

    // Test 2: Filter messages by server
    println!("\nTest 2: Filter messages by server");

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

    let freenode_msgs = db.get_messages("#rust", 10, None, None, None, Some("freenode"))?;
    println!("  ✓ Freenode messages: {}", freenode_msgs.len());
    assert_eq!(freenode_msgs.len(), 2); // includes earlier message

    let libera_msgs = db.get_messages("#rust", 10, None, None, None, Some("libera"))?;
    println!("  ✓ Libera messages: {}", libera_msgs.len());
    assert_eq!(libera_msgs.len(), 1);

    let all_msgs = db.get_messages("#rust", 10, None, None, None, None)?;
    println!("  ✓ All messages: {}", all_msgs.len());
    assert_eq!(all_msgs.len(), 3);

    // Test 3: Delete messages before timestamp
    println!("\nTest 3: Delete messages before timestamp");

    let old_time = Utc::now() - chrono::Duration::days(10);
    let recent_time = Utc::now();

    let old_msg = IrcMessage {
        id: None,
        timestamp: old_time,
        source_nick: "charlie".to_string(),
        target: "#test".to_string(),
        message_type: MessageType::Channel,
        content: "Old message".to_string(),
        channel: Some("#test".to_string()),
        server_name: "freenode".to_string(),
    };

    let recent_msg = IrcMessage {
        id: None,
        timestamp: recent_time,
        source_nick: "dave".to_string(),
        target: "#test".to_string(),
        message_type: MessageType::Channel,
        content: "Recent message".to_string(),
        channel: Some("#test".to_string()),
        server_name: "freenode".to_string(),
    };

    db.insert_message(&old_msg)?;
    db.insert_message(&recent_msg)?;

    let cutoff = Utc::now() - chrono::Duration::days(5);
    let deleted_count = db.delete_messages_before(cutoff)?;
    println!("  ✓ Deleted {} old messages", deleted_count);
    assert_eq!(deleted_count, 1);

    let remaining_msgs = db.get_messages("#test", 10, None, None, None, Some("freenode"))?;
    println!("  ✓ Remaining messages: {}", remaining_msgs.len());
    assert_eq!(remaining_msgs.len(), 1);
    assert_eq!(remaining_msgs[0].content, "Recent message");

    println!("\n✅ All database multi-server tests passed!");

    Ok(())
}
