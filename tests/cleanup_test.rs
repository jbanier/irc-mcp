// tests/cleanup_test.rs
use chrono::{Duration, Utc};
use irc_mcp_server::storage::cleanup::cleanup_old_data;
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::{IrcMessage, MessageType};
use std::fs;
use std::path::PathBuf;
use tempfile::{NamedTempFile, TempDir};

#[tokio::test]
async fn test_cleanup_old_messages() {
    let db_file = NamedTempFile::new().unwrap();
    let db = Database::new(db_file.path()).unwrap();

    // Insert old and new messages
    let old_msg = IrcMessage {
        id: None,
        timestamp: Utc::now() - Duration::days(100),
        source_nick: "old".to_string(),
        target: "#test".to_string(),
        message_type: MessageType::Channel,
        content: "old".to_string(),
        channel: Some("#test".to_string()),
        server_name: "server1".to_string(),
    };

    let new_msg = IrcMessage {
        id: None,
        timestamp: Utc::now() - Duration::days(10),
        source_nick: "new".to_string(),
        target: "#test".to_string(),
        message_type: MessageType::Channel,
        content: "new".to_string(),
        channel: Some("#test".to_string()),
        server_name: "server1".to_string(),
    };

    db.insert_message(&old_msg).unwrap();
    db.insert_message(&new_msg).unwrap();

    // Run cleanup
    let temp_dir = TempDir::new().unwrap();
    let server_dirs = vec![temp_dir.path().to_path_buf()];
    let cutoff = Utc::now() - Duration::days(90);

    let (deleted_msgs, deleted_files) = cleanup_old_data(&db, &server_dirs, cutoff).unwrap();

    assert_eq!(deleted_msgs, 1);
    assert_eq!(deleted_files, 0);
}

#[tokio::test]
async fn test_cleanup_old_files() {
    use std::io::Write;

    let db_file = NamedTempFile::new().unwrap();
    let db = Database::new(db_file.path()).unwrap();

    let temp_dir = TempDir::new().unwrap();
    let old_file = temp_dir.path().join("old_file.txt");
    let new_file = temp_dir.path().join("new_file.txt");

    // Create old file
    let mut f = fs::File::create(&old_file).unwrap();
    f.write_all(b"old content").unwrap();

    // Set old modified time
    let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(100 * 24 * 3600);
    filetime::set_file_mtime(&old_file, filetime::FileTime::from_system_time(old_time)).unwrap();

    // Create new file
    let mut f = fs::File::create(&new_file).unwrap();
    f.write_all(b"new content").unwrap();

    let server_dirs = vec![temp_dir.path().to_path_buf()];
    let cutoff = Utc::now() - Duration::days(90);

    let (deleted_msgs, deleted_files) = cleanup_old_data(&db, &server_dirs, cutoff).unwrap();

    assert_eq!(deleted_msgs, 0);
    assert_eq!(deleted_files, 1);
    assert!(!old_file.exists());
    assert!(new_file.exists());
}
