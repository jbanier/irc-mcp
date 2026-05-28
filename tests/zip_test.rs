use irc_mcp_server::irc::zip::{
    extract_zip_file, is_zip_file, sanitize_path_component, ExtractionLimits,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;
use zip::write::{FileOptions, ZipWriter};

#[test]
fn test_is_zip_file_by_extension_and_magic() {
    let temp_dir = TempDir::new().unwrap();
    let zip_path = temp_dir.path().join("test.zip");

    // Write zip magic bytes
    let mut file = File::create(&zip_path).unwrap();
    file.write_all(b"PK\x03\x04test content").unwrap();
    drop(file);

    assert!(is_zip_file(&zip_path));
}

#[test]
fn test_is_zip_file_rejects_non_zip() {
    let temp_dir = TempDir::new().unwrap();
    let txt_path = temp_dir.path().join("test.txt");

    let mut file = File::create(&txt_path).unwrap();
    file.write_all(b"plain text content").unwrap();
    drop(file);

    assert!(!is_zip_file(&txt_path));
}

#[test]
fn test_is_zip_file_rejects_wrong_extension() {
    let temp_dir = TempDir::new().unwrap();
    let txt_path = temp_dir.path().join("test.txt");

    // Wrong extension even with zip magic bytes
    let mut file = File::create(&txt_path).unwrap();
    file.write_all(b"PK\x03\x04content").unwrap();
    drop(file);

    assert!(!is_zip_file(&txt_path));
}

#[test]
fn test_sanitize_rejects_parent_directory() {
    assert!(sanitize_path_component("../etc/passwd").is_err());
    assert!(sanitize_path_component("..\\windows\\system32").is_err());
    assert!(sanitize_path_component("folder/../secret").is_err());
}

#[test]
fn test_sanitize_rejects_absolute_paths() {
    assert!(sanitize_path_component("/etc/passwd").is_err());
    assert!(sanitize_path_component("C:\\Windows\\System32").is_err());
}

#[test]
fn test_sanitize_allows_safe_paths() {
    assert!(sanitize_path_component("folder/file.txt").is_ok());
    assert!(sanitize_path_component("file.txt").is_ok());
    assert_eq!(
        sanitize_path_component("folder/file.txt").unwrap(),
        "folder/file.txt"
    );
}

fn create_simple_test_zip(path: &PathBuf) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);

    zip.start_file("file1.txt", FileOptions::default()).unwrap();
    zip.write_all(b"content1").unwrap();

    zip.start_file("file2.txt", FileOptions::default()).unwrap();
    zip.write_all(b"content2").unwrap();

    zip.start_file("subdir/file3.txt", FileOptions::default())
        .unwrap();
    zip.write_all(b"content3").unwrap();

    zip.finish().unwrap();
}

#[test]
fn test_extract_simple_zip() {
    let temp_dir = TempDir::new().unwrap();
    let zip_path = temp_dir.path().join("test.zip");
    let extract_dir = temp_dir.path().join("extracted");

    create_simple_test_zip(&zip_path);

    let limits = ExtractionLimits::default();
    let extracted = extract_zip_file(&zip_path, &extract_dir, &limits).unwrap();

    assert_eq!(extracted.len(), 3);

    // Verify files extracted
    assert!(extract_dir.join("file1.txt").exists());
    assert!(extract_dir.join("file2.txt").exists());
    assert!(extract_dir.join("subdir/file3.txt").exists());

    // Verify content
    let content1 = fs::read_to_string(extract_dir.join("file1.txt")).unwrap();
    assert_eq!(content1, "content1");
}

fn create_malicious_zip(path: &PathBuf) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);

    // Try to write outside extraction dir
    let options = FileOptions::default();
    zip.start_file("../../../etc/passwd", options).unwrap();
    zip.write_all(b"malicious").unwrap();

    zip.start_file("safe.txt", options).unwrap();
    zip.write_all(b"safe content").unwrap();

    zip.finish().unwrap();
}

#[test]
fn test_path_traversal_protection() {
    let temp_dir = TempDir::new().unwrap();
    let zip_path = temp_dir.path().join("malicious.zip");
    let extract_dir = temp_dir.path().join("extracted");

    create_malicious_zip(&zip_path);

    let limits = ExtractionLimits::default();
    let extracted = extract_zip_file(&zip_path, &extract_dir, &limits).unwrap();

    // Should only extract safe file, skip malicious one
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].relative_path, "safe.txt");
    assert!(extract_dir.join("safe.txt").exists());

    // Verify malicious file not written to /etc/passwd
    assert!(
        !PathBuf::from("/etc/passwd").exists()
            || fs::read_to_string("/etc/passwd").unwrap() != "malicious"
    );
}

fn create_large_zip(path: &PathBuf, file_count: usize) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);

    for i in 0..file_count {
        zip.start_file(format!("file_{}.txt", i), FileOptions::default())
            .unwrap();
        zip.write_all(b"content").unwrap();
    }

    zip.finish().unwrap();
}

#[test]
fn test_file_count_limit() {
    let temp_dir = TempDir::new().unwrap();
    let zip_path = temp_dir.path().join("large.zip");
    let extract_dir = temp_dir.path().join("extracted");

    // Create zip with 1500 files
    create_large_zip(&zip_path, 1500);

    let limits = ExtractionLimits::default();
    let result = extract_zip_file(&zip_path, &extract_dir, &limits);

    // Should fail due to file count limit
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Too many files"));
}

fn create_nested_zip(path: &PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let inner_zip = temp_dir.path().join("inner.zip");

    // Create inner zip
    let inner_file = File::create(&inner_zip).unwrap();
    let mut inner = ZipWriter::new(inner_file);
    inner
        .start_file("nested.txt", FileOptions::default())
        .unwrap();
    inner.write_all(b"nested content").unwrap();
    inner.finish().unwrap();

    // Create outer zip containing inner zip
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);

    zip.start_file("inner.zip", FileOptions::default()).unwrap();
    let inner_content = fs::read(&inner_zip).unwrap();
    zip.write_all(&inner_content).unwrap();

    zip.start_file("outer.txt", FileOptions::default()).unwrap();
    zip.write_all(b"outer content").unwrap();

    zip.finish().unwrap();
}

#[test]
fn test_nested_zip_not_extracted() {
    let temp_dir = TempDir::new().unwrap();
    let zip_path = temp_dir.path().join("nested.zip");
    let extract_dir = temp_dir.path().join("extracted");

    create_nested_zip(&zip_path);

    let limits = ExtractionLimits::default();
    let extracted = extract_zip_file(&zip_path, &extract_dir, &limits).unwrap();

    assert_eq!(extracted.len(), 2);
    assert!(extract_dir.join("inner.zip").exists());
    assert!(extract_dir.join("outer.txt").exists());

    // Nested zip should remain as file, not extracted
    assert!(!extract_dir.join("nested.txt").exists());
}
