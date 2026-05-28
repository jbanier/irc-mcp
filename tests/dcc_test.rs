use irc_mcp_server::irc::dcc::{download_dcc_file, parse_dcc_send, DccSendOffer};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

#[test]
fn test_parse_dcc_send() {
    let ctcp = "\x01DCC SEND testfile.txt 3232235777 12345 1024\x01";
    let offer = parse_dcc_send(ctcp).unwrap();

    assert_eq!(offer.filename, "testfile.txt");
    assert_eq!(offer.ip_address, "192.168.1.1");
    assert_eq!(offer.port, 12345);
    assert_eq!(offer.filesize, 1024);
}

#[tokio::test]
async fn test_download_dcc_file_with_relative_path() {
    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();
    let download_dir = temp_dir.path().join("downloads");
    std::fs::create_dir_all(&download_dir).unwrap();

    // Start a mock DCC sender
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    // Test data
    let test_content = b"Hello, this is test file content!";
    let filesize = test_content.len() as u64;

    // Spawn sender task
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            // Send file content
            let _ = stream.write_all(test_content).await;
            // Read acknowledgements (DCC protocol requirement)
            let mut ack = [0u8; 4];
            let _ = tokio::io::AsyncReadExt::read_exact(&mut stream, &mut ack).await;
        }
    });

    // Give the listener a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Create DCC offer
    let offer = DccSendOffer {
        filename: "test_file.txt".to_string(),
        ip_address: "127.0.0.1".to_string(),
        port,
        filesize,
    };

    // Download file
    let result = download_dcc_file(&offer, &download_dir, 1024 * 1024).await;

    assert!(result.is_ok(), "Download should succeed");
    let (filepath, size, _extracted) = result.unwrap();

    // Verify the file was downloaded
    assert_eq!(size, filesize);

    // Verify file exists
    assert!(filepath.exists(), "Downloaded file should exist");

    // Verify content
    let content = std::fs::read(&filepath).unwrap();
    assert_eq!(&content[..], test_content, "File content should match");
}

#[test]
fn test_path_canonicalization_scenario() {
    // This test demonstrates the issue and solution:
    // When a path is stored as relative and the working directory changes,
    // the path becomes invalid. Canonicalization fixes this.

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    std::fs::write(&file_path, b"test content").unwrap();

    // Simulate what happens: download returns a path, we need to store it
    let returned_path = file_path.clone();

    // Canonicalize the path (this is what the fix does)
    let canonical_path = returned_path.canonicalize().unwrap();

    // Verify it's absolute
    assert!(
        canonical_path.is_absolute(),
        "Canonicalized path should be absolute"
    );

    // Change to a different directory
    let other_dir = TempDir::new().unwrap();
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(other_dir.path()).unwrap();

    // The canonical path should still work from the new directory
    let content = std::fs::read(&canonical_path).unwrap();
    assert_eq!(
        &content[..],
        b"test content",
        "Canonical path should work from any working directory"
    );

    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
}

use std::fs::File;
use std::io::Write;
use zip::write::{FileOptions, ZipWriter};

fn create_test_zip_file(path: &std::path::Path) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);

    zip.start_file("readme.txt", FileOptions::default())
        .unwrap();
    zip.write_all(b"This is a test file").unwrap();

    zip.start_file("data.json", FileOptions::default()).unwrap();
    zip.write_all(b"{\"test\": true}").unwrap();

    zip.finish().unwrap();
}

#[tokio::test]
async fn test_dcc_download_and_extract_zip() {
    // Note: This test uses a mock zip file rather than actual DCC connection
    // Real DCC testing requires network setup

    let temp_dir = TempDir::new().unwrap();
    let download_dir = temp_dir.path().join("downloads");
    std::fs::create_dir_all(&download_dir).unwrap();

    let zip_path = download_dir.join("test.zip");
    create_test_zip_file(&zip_path);

    // Verify extraction happens for zip files
    let result = irc_mcp_server::irc::zip::extract_zip_file(
        &zip_path,
        &download_dir.join("test_extracted"),
        &irc_mcp_server::irc::zip::ExtractionLimits::default(),
    );

    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 2);

    // Verify extracted files exist
    assert!(download_dir.join("test_extracted/readme.txt").exists());
    assert!(download_dir.join("test_extracted/data.json").exists());
}

#[test]
fn test_non_zip_file_no_extraction() {
    let temp_dir = TempDir::new().unwrap();
    let download_dir = temp_dir.path().join("downloads");
    std::fs::create_dir_all(&download_dir).unwrap();

    let txt_path = download_dir.join("test.txt");
    std::fs::write(&txt_path, "plain text content").unwrap();

    // Should not be detected as zip
    assert!(!irc_mcp_server::irc::zip::is_zip_file(&txt_path));
}
