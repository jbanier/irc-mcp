use chrono::Utc;
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::{DccStatus, DccTransfer, ExtractedFile};
use tempfile::TempDir;

#[test]
fn test_mcp_get_dcc_info_with_extracted_files() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    // Create transfer
    let transfer = DccTransfer {
        id: None,
        timestamp: Utc::now(),
        sender_nick: "sender".to_string(),
        filename: "archive.zip".to_string(),
        filepath: None,
        filesize: 5000,
        received_size: 0,
        status: DccStatus::Pending,
        error: None,
        ip_address: Some("192.168.1.1".to_string()),
        port: Some(8080),
        extracted_files: None,
        extraction_status: None,
        extraction_error: None,
    };

    let transfer_id = db.insert_dcc_transfer(&transfer).unwrap();

    // Update with extraction data
    let extracted = vec![
        ExtractedFile {
            relative_path: "file1.txt".to_string(),
            full_path: "/downloads/archive_extracted/file1.txt".to_string(),
            size: 100,
        },
        ExtractedFile {
            relative_path: "file2.txt".to_string(),
            full_path: "/downloads/archive_extracted/file2.txt".to_string(),
            size: 200,
        },
    ];

    db.update_extraction_metadata(transfer_id, "extracted", Some(&extracted), None)
        .unwrap();

    // Retrieve and verify
    let transfer = db.get_dcc_transfer(transfer_id).unwrap().unwrap();

    assert_eq!(transfer.extraction_status, Some("extracted".to_string()));
    assert!(transfer.extracted_files.is_some());

    let files = transfer.extracted_files.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].relative_path, "file1.txt");
    assert_eq!(files[0].size, 100);
    assert_eq!(files[1].relative_path, "file2.txt");
    assert_eq!(files[1].size, 200);
}

#[test]
fn test_mcp_list_transfers_includes_extraction_status() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();

    // Create multiple transfers with different extraction statuses
    let transfer1 = DccTransfer {
        id: None,
        timestamp: Utc::now(),
        sender_nick: "user1".to_string(),
        filename: "file1.zip".to_string(),
        filepath: None,
        filesize: 1000,
        received_size: 0,
        status: DccStatus::Pending,
        error: None,
        ip_address: None,
        port: None,
        extracted_files: None,
        extraction_status: None,
        extraction_error: None,
    };

    let transfer2 = DccTransfer {
        id: None,
        timestamp: Utc::now(),
        sender_nick: "user2".to_string(),
        filename: "file2.txt".to_string(),
        filepath: None,
        filesize: 2000,
        received_size: 0,
        status: DccStatus::Pending,
        error: None,
        ip_address: None,
        port: None,
        extracted_files: None,
        extraction_status: None,
        extraction_error: None,
    };

    let transfer3 = DccTransfer {
        id: None,
        timestamp: Utc::now(),
        sender_nick: "user3".to_string(),
        filename: "file3.zip".to_string(),
        filepath: None,
        filesize: 3000,
        received_size: 0,
        status: DccStatus::Pending,
        error: None,
        ip_address: None,
        port: None,
        extracted_files: None,
        extraction_status: None,
        extraction_error: None,
    };

    let id1 = db.insert_dcc_transfer(&transfer1).unwrap();
    let id2 = db.insert_dcc_transfer(&transfer2).unwrap();
    let id3 = db.insert_dcc_transfer(&transfer3).unwrap();

    // Mark first as extracted
    db.update_extraction_metadata(id1, "extracted", Some(&vec![]), None)
        .unwrap();

    // Mark third as failed
    db.update_extraction_metadata(id3, "failed", None, Some("Too many files"))
        .unwrap();

    // List all transfers
    let transfers = db.list_dcc_transfers(None, 10).unwrap();
    assert_eq!(transfers.len(), 3);

    // Find by filename and verify extraction status
    let t1 = transfers
        .iter()
        .find(|t| t.filename == "file1.zip")
        .unwrap();
    assert_eq!(t1.extraction_status, Some("extracted".to_string()));
    assert!(t1.extracted_files.is_some());

    let t2 = transfers
        .iter()
        .find(|t| t.filename == "file2.txt")
        .unwrap();
    assert_eq!(t2.extraction_status, None);

    let t3 = transfers
        .iter()
        .find(|t| t.filename == "file3.zip")
        .unwrap();
    assert_eq!(t3.extraction_status, Some("failed".to_string()));
    assert_eq!(t3.extraction_error, Some("Too many files".to_string()));
}
