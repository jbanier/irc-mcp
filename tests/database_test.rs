use chrono::Utc;
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::{DccStatus, DccTransfer, ExtractedFile, IrcMessage, MessageType};
use tempfile::{NamedTempFile, TempDir};

#[test]
fn test_insert_and_retrieve_message() {
    let file = NamedTempFile::new().unwrap();
    let db = Database::new(file.path()).unwrap();

    let msg = IrcMessage {
        id: None,
        timestamp: Utc::now(),
        source_nick: "alice".to_string(),
        target: "#test".to_string(),
        message_type: MessageType::Channel,
        content: "Hello world".to_string(),
        channel: Some("#test".to_string()),
    };

    db.insert_message(&msg).unwrap();
    let messages = db.get_messages("#test", 10, None, None, None).unwrap();

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].source_nick, "alice");
    assert_eq!(messages[0].content, "Hello world");
}

#[test]
fn test_insert_dcc_transfer() {
    let file = NamedTempFile::new().unwrap();
    let db = Database::new(file.path()).unwrap();

    let transfer = DccTransfer {
        id: None,
        timestamp: Utc::now(),
        sender_nick: "bob".to_string(),
        filename: "test.txt".to_string(),
        filepath: Some("/tmp/test.txt".to_string()),
        filesize: 1024,
        received_size: 0,
        status: DccStatus::Pending,
        error: None,
        ip_address: Some("192.168.1.1".to_string()),
        port: Some(12345),
        extracted_files: None,
        extraction_status: None,
        extraction_error: None,
    };

    let id = db.insert_dcc_transfer(&transfer).unwrap();
    assert!(id > 0);

    let transfers = db.list_dcc_transfers(None, 10).unwrap();
    assert_eq!(transfers.len(), 1);
    assert_eq!(transfers[0].filename, "test.txt");
}

#[test]
fn test_dcc_transfer_with_extraction_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    // Create transfer
    let transfer = DccTransfer {
        id: None,
        timestamp: Utc::now(),
        sender_nick: "sender".to_string(),
        filename: "test.zip".to_string(),
        filepath: None,
        filesize: 1000,
        received_size: 0,
        status: DccStatus::Pending,
        error: None,
        ip_address: Some("192.168.1.1".to_string()),
        port: Some(12345),
        extracted_files: None,
        extraction_status: None,
        extraction_error: None,
    };

    let transfer_id = db.insert_dcc_transfer(&transfer).unwrap();

    // Update with extraction data
    let extracted = vec![ExtractedFile {
        relative_path: "file1.txt".to_string(),
        full_path: "/path/to/file1.txt".to_string(),
        size: 100,
    }];

    db.update_extraction_metadata(transfer_id, "extracted", Some(&extracted), None)
        .unwrap();

    // Retrieve and verify
    let transfer = db.get_dcc_transfer(transfer_id).unwrap().unwrap();
    assert_eq!(transfer.extraction_status, Some("extracted".to_string()));
    assert!(transfer.extracted_files.is_some());
    assert_eq!(transfer.extracted_files.as_ref().unwrap().len(), 1);
    assert_eq!(
        transfer.extracted_files.unwrap()[0].relative_path,
        "file1.txt"
    );
}
