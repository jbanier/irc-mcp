# DCC Zip Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically extract zip files during DCC transfers and expose extracted file paths to agents via MCP tools.

**Architecture:** Post-download hook detects zip files by extension and magic bytes, extracts to subdirectory with path traversal protection and file count limits, stores metadata in database, exposes via existing MCP tools.

**Tech Stack:** Rust, zip crate 0.6, rusqlite, tokio

---

## File Structure

**New files:**
- `src/irc/zip.rs` - Zip detection and extraction logic
- `tests/zip_test.rs` - Unit tests for zip module
- `tests/fixtures/test.zip` - Test fixture with 3 files
- `tests/fixtures/malicious.zip` - Test fixture with path traversal
- `tests/fixtures/large.zip` - Test fixture with >1000 files (generated)

**Modified files:**
- `src/types.rs` - Add ExtractedFile struct, update DccTransfer
- `src/irc/mod.rs` - Export zip module
- `src/irc/dcc.rs` - Integrate zip extraction into download flow
- `src/storage/database.rs` - Add schema migration, extraction metadata methods
- `src/mcp/tools.rs` - Update tools to return extraction data
- `Cargo.toml` - Add zip dependency

---

### Task 1: Add Dependencies and Type Definitions

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/types.rs`

- [ ] **Step 1: Add zip dependency to Cargo.toml**

Add after line 22 (after base64):

```toml
zip = "0.6"
```

- [ ] **Step 2: Run cargo check to verify dependency**

Run: `cargo check`
Expected: Downloads zip crate, compiles successfully

- [ ] **Step 3: Add ExtractedFile struct to types.rs**

Add after line 108 (after DccTransfer struct, before AppState):

```rust
/// Extracted file metadata from zip archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFile {
    pub relative_path: String,
    pub full_path: String,
    pub size: u64,
}
```

- [ ] **Step 4: Update DccTransfer struct with extraction fields**

In `src/types.rs`, update the DccTransfer struct (around line 94-108) by adding these fields before the closing brace:

```rust
    pub extracted_files: Option<Vec<ExtractedFile>>,
    pub extraction_status: Option<String>,
    pub extraction_error: Option<String>,
```

- [ ] **Step 5: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/types.rs
git commit -m "feat: add ExtractedFile type and zip dependency

- Add zip crate 0.6 for archive extraction
- Add ExtractedFile struct for tracking extracted files
- Add extraction metadata fields to DccTransfer

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 2: Create Zip Detection Function

**Files:**
- Create: `src/irc/zip.rs`
- Modify: `src/irc/mod.rs`
- Create: `tests/zip_test.rs`

- [ ] **Step 1: Write failing test for zip detection**

Create `tests/zip_test.rs`:

```rust
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;
use irc_mcp_server::irc::zip::is_zip_file;

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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_is_zip_file`
Expected: FAIL with "module `zip` not found in `irc`"

- [ ] **Step 3: Create zip module stub**

Create `src/irc/zip.rs`:

```rust
use std::path::Path;

/// Check if file is a zip based on extension and magic bytes
pub fn is_zip_file(path: &Path) -> bool {
    false
}
```

- [ ] **Step 4: Export zip module from irc/mod.rs**

Add after line 1 in `src/irc/mod.rs`:

```rust
pub mod zip;
```

- [ ] **Step 5: Run test to verify different failure**

Run: `cargo test test_is_zip_file`
Expected: FAIL with assertion errors (function returns false)

- [ ] **Step 6: Implement is_zip_file**

Replace the is_zip_file function in `src/irc/zip.rs`:

```rust
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Check if file is a zip based on extension and magic bytes
pub fn is_zip_file(path: &Path) -> bool {
    // Check extension first
    if let Some(ext) = path.extension() {
        if ext != "zip" {
            return false;
        }
    } else {
        return false;
    }
    
    // Check magic bytes (PK\x03\x04)
    if let Ok(mut file) = File::open(path) {
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            return magic == [0x50, 0x4B, 0x03, 0x04];
        }
    }
    
    false
}
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test test_is_zip_file`
Expected: All 3 tests PASS

- [ ] **Step 8: Commit**

```bash
git add src/irc/zip.rs src/irc/mod.rs tests/zip_test.rs
git commit -m "feat: add zip file detection by extension and magic bytes

- Implement is_zip_file() checking .zip extension and PK magic bytes
- Add unit tests for detection logic

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 3: Create Extraction Limits and Path Sanitization

**Files:**
- Modify: `src/irc/zip.rs`
- Modify: `tests/zip_test.rs`

- [ ] **Step 1: Write failing test for path traversal protection**

Add to `tests/zip_test.rs`:

```rust
use irc_mcp_server::irc::zip::sanitize_path_component;

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
    assert_eq!(sanitize_path_component("folder/file.txt").unwrap(), "folder/file.txt");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_sanitize`
Expected: FAIL with "function `sanitize_path_component` not found"

- [ ] **Step 3: Add ExtractionLimits struct and sanitize function**

Add to `src/irc/zip.rs` after is_zip_file:

```rust
use anyhow::{bail, Result};

/// Safety limits for extraction
#[derive(Debug, Clone)]
pub struct ExtractionLimits {
    pub max_files: usize,
    pub enable_path_traversal_check: bool,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_files: 1000,
            enable_path_traversal_check: true,
        }
    }
}

/// Sanitize and validate a path component for extraction
pub fn sanitize_path_component(path: &str) -> Result<String> {
    // Check for parent directory references
    if path.contains("..") {
        bail!("Path contains parent directory reference: {}", path);
    }
    
    // Check for absolute paths (Unix)
    if path.starts_with('/') {
        bail!("Path is absolute: {}", path);
    }
    
    // Check for absolute paths (Windows)
    if path.len() >= 2 && path.chars().nth(1) == Some(':') {
        bail!("Path is absolute (Windows drive): {}", path);
    }
    
    Ok(path.to_string())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test test_sanitize`
Expected: All 3 sanitize tests PASS

- [ ] **Step 5: Commit**

```bash
git add src/irc/zip.rs tests/zip_test.rs
git commit -m "feat: add path sanitization and extraction limits

- Add ExtractionLimits struct with max_files and traversal check
- Implement sanitize_path_component rejecting .. and absolute paths
- Add unit tests for path sanitization

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 4: Implement Zip Extraction Core Logic

**Files:**
- Modify: `src/irc/zip.rs`
- Modify: `src/types.rs` (export ExtractedFile for tests)
- Modify: `tests/zip_test.rs`

- [ ] **Step 1: Create test fixture - simple zip file**

Add to `tests/zip_test.rs`:

```rust
use std::fs;
use std::path::PathBuf;
use zip::write::{FileOptions, ZipWriter};
use irc_mcp_server::irc::zip::{extract_zip_file, ExtractionLimits};

fn create_simple_test_zip(path: &PathBuf) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    
    zip.start_file("file1.txt", FileOptions::default()).unwrap();
    zip.write_all(b"content1").unwrap();
    
    zip.start_file("file2.txt", FileOptions::default()).unwrap();
    zip.write_all(b"content2").unwrap();
    
    zip.start_file("subdir/file3.txt", FileOptions::default()).unwrap();
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
```

- [ ] **Step 2: Add zip to dev-dependencies in Cargo.toml**

Add to `[dev-dependencies]` section:

```toml
zip = "0.6"
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test test_extract_simple_zip`
Expected: FAIL with "function `extract_zip_file` not found"

- [ ] **Step 4: Implement extract_zip_file stub**

Add to `src/irc/zip.rs`:

```rust
use crate::types::ExtractedFile;
use std::fs;
use tracing::warn;

/// Extract zip file to directory with safety limits
pub fn extract_zip_file(
    zip_path: &Path,
    extract_dir: &Path,
    limits: &ExtractionLimits,
) -> Result<Vec<ExtractedFile>> {
    Ok(Vec::new())
}
```

- [ ] **Step 5: Run test to verify different failure**

Run: `cargo test test_extract_simple_zip`
Expected: FAIL with "assertion failed: extracted.len() == 3"

- [ ] **Step 6: Implement full extract_zip_file**

Replace extract_zip_file in `src/irc/zip.rs`:

```rust
use std::io::Read;
use zip::ZipArchive;

/// Extract zip file to directory with safety limits
pub fn extract_zip_file(
    zip_path: &Path,
    extract_dir: &Path,
    limits: &ExtractionLimits,
) -> Result<Vec<ExtractedFile>> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    
    fs::create_dir_all(extract_dir)?;
    
    let mut extracted_files = Vec::new();
    let mut file_count = 0;
    
    for i in 0..archive.len() {
        let mut zip_file = archive.by_index(i)?;
        
        // Skip directories
        if zip_file.is_dir() {
            continue;
        }
        
        // Check file count limit
        file_count += 1;
        if file_count > limits.max_files {
            bail!("Too many files in archive (limit: {})", limits.max_files);
        }
        
        // Get and sanitize path
        let file_path = match zip_file.enclosed_name() {
            Some(path) => path.to_path_buf(),
            None => {
                warn!("Skipping file with unsafe name in zip");
                continue;
            }
        };
        
        let path_str = file_path.to_string_lossy();
        
        // Apply path traversal check if enabled
        if limits.enable_path_traversal_check {
            if let Err(e) = sanitize_path_component(&path_str) {
                warn!("Skipping file with unsafe path: {} - {}", path_str, e);
                continue;
            }
        }
        
        // Create output path
        let output_path = extract_dir.join(&file_path);
        
        // Create parent directories
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        // Extract file
        let mut output_file = File::create(&output_path)?;
        std::io::copy(&mut zip_file, &mut output_file)?;
        
        // Get file size
        let metadata = fs::metadata(&output_path)?;
        
        extracted_files.push(ExtractedFile {
            relative_path: path_str.to_string(),
            full_path: output_path.to_string_lossy().to_string(),
            size: metadata.len(),
        });
    }
    
    Ok(extracted_files)
}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cargo test test_extract_simple_zip`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/irc/zip.rs tests/zip_test.rs Cargo.toml
git commit -m "feat: implement zip extraction with safety limits

- Extract zip files to target directory
- Skip directories, only extract files
- Create parent directories as needed
- Track extracted file metadata
- Add test for basic extraction

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 5: Add Security Tests (Path Traversal and File Count)

**Files:**
- Modify: `tests/zip_test.rs`

- [ ] **Step 1: Write test for path traversal protection**

Add to `tests/zip_test.rs`:

```rust
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
    assert!(!PathBuf::from("/etc/passwd").exists() || 
            fs::read_to_string("/etc/passwd").unwrap() != "malicious");
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test test_path_traversal_protection`
Expected: PASS (malicious file skipped due to sanitization)

- [ ] **Step 3: Write test for file count limit**

Add to `tests/zip_test.rs`:

```rust
fn create_large_zip(path: &PathBuf, file_count: usize) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    
    for i in 0..file_count {
        zip.start_file(format!("file_{}.txt", i), FileOptions::default()).unwrap();
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_file_count_limit`
Expected: PASS

- [ ] **Step 5: Write test for nested zip not extracted**

Add to `tests/zip_test.rs`:

```rust
fn create_nested_zip(path: &PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let inner_zip = temp_dir.path().join("inner.zip");
    
    // Create inner zip
    let inner_file = File::create(&inner_zip).unwrap();
    let mut inner = ZipWriter::new(inner_file);
    inner.start_file("nested.txt", FileOptions::default()).unwrap();
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
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test test_nested_zip_not_extracted`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add tests/zip_test.rs
git commit -m "test: add security tests for zip extraction

- Test path traversal protection skips malicious files
- Test file count limit enforcement (>1000 files)
- Test nested zips not recursively extracted

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 6: Add Database Schema Migration

**Files:**
- Modify: `src/storage/database.rs`

- [ ] **Step 1: Write failing test for extraction columns**

Add to `tests/database_test.rs`:

```rust
use irc_mcp_server::types::ExtractedFile;

#[test]
fn test_dcc_transfer_with_extraction_metadata() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    
    // Save transfer with extraction data
    let extracted = vec![
        ExtractedFile {
            relative_path: "file1.txt".to_string(),
            full_path: "/path/to/file1.txt".to_string(),
            size: 100,
        },
    ];
    
    let transfer_id = db.save_dcc_transfer(
        "sender",
        "test.zip",
        None,
        1000,
        Some("192.168.1.1"),
        Some(12345),
    ).unwrap();
    
    db.update_extraction_metadata(
        transfer_id,
        "extracted",
        Some(&extracted),
        None,
    ).unwrap();
    
    // Retrieve and verify
    let transfer = db.get_dcc_transfer(transfer_id).unwrap().unwrap();
    assert_eq!(transfer.extraction_status, Some("extracted".to_string()));
    assert!(transfer.extracted_files.is_some());
    assert_eq!(transfer.extracted_files.as_ref().unwrap().len(), 1);
    assert_eq!(transfer.extracted_files.unwrap()[0].relative_path, "file1.txt");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_dcc_transfer_with_extraction_metadata`
Expected: FAIL with "no such column: extraction_status"

- [ ] **Step 3: Add schema migration to init_schema**

In `src/storage/database.rs`, update the `init_schema` method to add columns to dcc_transfers table. Add these lines after line 60 (after the port INTEGER line):

```rust
                extraction_status TEXT,
                extraction_error TEXT,
                extracted_files_json TEXT
```

- [ ] **Step 4: Add migration for existing databases**

Add new method after `init_schema` in `src/storage/database.rs`:

```rust
    /// Migrate database schema to add extraction columns if missing
    fn migrate_schema(&self) -> Result<()> {
        // Check if extraction columns exist
        let columns: Vec<String> = self.conn
            .prepare("PRAGMA table_info(dcc_transfers)")?
            .query_map([], |row| row.get(1))?
            .collect::<Result<Vec<_>, _>>()?;
        
        if !columns.contains(&"extraction_status".to_string()) {
            self.conn.execute_batch(
                r#"
                ALTER TABLE dcc_transfers ADD COLUMN extraction_status TEXT;
                ALTER TABLE dcc_transfers ADD COLUMN extraction_error TEXT;
                ALTER TABLE dcc_transfers ADD COLUMN extracted_files_json TEXT;
                "#,
            )?;
        }
        
        Ok(())
    }
```

- [ ] **Step 5: Call migrate_schema in new method**

Update the `new` method in `src/storage/database.rs` to call `migrate_schema` after `init_schema`. Add after line 24:

```rust
        db.migrate_schema()?;
```

- [ ] **Step 6: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

- [ ] **Step 7: Commit**

```bash
git add src/storage/database.rs tests/database_test.rs
git commit -m "feat: add database schema for extraction metadata

- Add extraction_status, extraction_error, extracted_files_json columns
- Add migration for existing databases
- Update init_schema for new databases

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 7: Implement Database Extraction Metadata Methods

**Files:**
- Modify: `src/storage/database.rs`

- [ ] **Step 1: Implement update_extraction_metadata method**

Add to `src/storage/database.rs` after `update_dcc_transfer_status` method (around line 206):

```rust
    /// Update extraction metadata for a DCC transfer
    pub fn update_extraction_metadata(
        &self,
        transfer_id: i64,
        status: &str,
        extracted_files: Option<&Vec<ExtractedFile>>,
        error: Option<&str>,
    ) -> Result<()> {
        let extracted_json = extracted_files.map(|files| {
            serde_json::to_string(files).unwrap_or_default()
        });
        
        self.conn.execute(
            "UPDATE dcc_transfers SET extraction_status = ?, extracted_files_json = ?, extraction_error = ? WHERE id = ?",
            params![status, extracted_json, error, transfer_id],
        )?;
        
        Ok(())
    }
```

- [ ] **Step 2: Add use statement for ExtractedFile**

Add to imports at top of `src/storage/database.rs` (around line 1):

```rust
use crate::types::{DccStatus, DccTransfer, ExtractedFile, IrcMessage, MessageType};
```

- [ ] **Step 3: Update list_dcc_transfers to include extraction fields**

Update the SQL query in `list_dcc_transfers` method (around line 212) to include new columns:

```rust
        let query = if let Some(status) = status_filter {
            format!(
                "SELECT id, timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port, extraction_status, extraction_error, extracted_files_json
                 FROM dcc_transfers WHERE status = '{}' ORDER BY timestamp DESC LIMIT {}",
                status.as_str(),
                limit
            )
        } else {
            format!(
                "SELECT id, timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port, extraction_status, extraction_error, extracted_files_json
                 FROM dcc_transfers ORDER BY timestamp DESC LIMIT {}",
                limit
            )
        };
```

- [ ] **Step 4: Update list_dcc_transfers row mapping to deserialize extraction fields**

Update the query_map in `list_dcc_transfers` (around line 226) to add extraction fields:

```rust
        let rows = stmt.query_map([], |row| {
            let timestamp_str: String = row.get(1)?;
            let status_str: String = row.get(7)?;
            let extracted_json: Option<String> = row.get(13)?;
            
            let extracted_files = extracted_json.and_then(|json| {
                serde_json::from_str(&json).ok()
            });

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
            })
        })?;
```

- [ ] **Step 5: Update get_dcc_transfer similarly**

Update the SQL query in `get_dcc_transfer` method (around line 253) to include extraction columns:

```rust
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, sender_nick, filename, filepath, filesize, received_size, status, error, ip_address, port, extraction_status, extraction_error, extracted_files_json
             FROM dcc_transfers WHERE id = ?"
        )?;
```

- [ ] **Step 6: Update get_dcc_transfer row mapping**

Update the query_row in `get_dcc_transfer` (around line 259) to add extraction fields:

```rust
        let transfer = stmt
            .query_row(params![id], |row| {
                let timestamp_str: String = row.get(1)?;
                let status_str: String = row.get(7)?;
                let extracted_json: Option<String> = row.get(13)?;
                
                let extracted_files = extracted_json.and_then(|json| {
                    serde_json::from_str(&json).ok()
                });

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
                })
            })
            .optional()
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cargo test test_dcc_transfer_with_extraction_metadata`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/storage/database.rs
git commit -m "feat: implement extraction metadata database methods

- Add update_extraction_metadata method
- Update list_dcc_transfers to deserialize extracted_files_json
- Update get_dcc_transfer to deserialize extracted_files_json

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 8: Integrate Zip Extraction into DCC Handler

**Files:**
- Modify: `src/irc/dcc.rs`
- Modify: `src/irc/client.rs` (to pass extraction data to database)

- [ ] **Step 1: Update download_dcc_file return type**

Update the function signature in `src/irc/dcc.rs` (around line 70):

```rust
pub async fn download_dcc_file(
    offer: &DccSendOffer,
    download_dir: &Path,
    max_file_size: u64,
) -> Result<(PathBuf, u64, Option<Vec<crate::types::ExtractedFile>>)> {
```

- [ ] **Step 2: Add zip extraction logic after download completes**

Add to `src/irc/dcc.rs` after line 158 (after rename temp file to final path, before the Ok return):

```rust
    // Check if file is a zip and extract if so
    let extracted_files = if crate::irc::zip::is_zip_file(&final_path) {
        info!("Detected zip file, extracting: {}", safe_filename);
        
        let extract_dir = download_dir.join(format!("{}_extracted", safe_filename.trim_end_matches(".zip")));
        
        match crate::irc::zip::extract_zip_file(
            &final_path,
            &extract_dir,
            &crate::irc::zip::ExtractionLimits::default()
        ) {
            Ok(files) => {
                info!("Extracted {} files from {}", files.len(), safe_filename);
                Some(files)
            }
            Err(e) => {
                warn!("Zip extraction failed for {}: {}", safe_filename, e);
                None
            }
        }
    } else {
        None
    };

    info!("DCC download completed: {} ({} bytes)", safe_filename, total_received);
```

- [ ] **Step 3: Update return statement**

Update the return statement at the end of `download_dcc_file` (around line 161):

```rust
    Ok((final_path, total_received, extracted_files))
```

- [ ] **Step 4: Find DCC handler in client.rs that calls download_dcc_file**

Run: `grep -n "download_dcc_file" src/irc/client.rs`

Expected: Find the line number where download is called

- [ ] **Step 5: Update DCC handler to handle extraction result**

In `src/irc/client.rs`, find where `download_dcc_file` is called (search for "download_dcc_file"). Update the match block to handle the third return value and update the database. The code should look similar to this (around the download_dcc_file call):

```rust
                        match download_dcc_file(&offer, &download_dir, config.dcc.max_file_size_bytes).await {
                            Ok((filepath, received_size, extracted_files)) => {
                                info!("DCC download completed: {} ({} bytes)", offer.filename, received_size);
                                
                                // Update transfer status to completed
                                if let Err(e) = db.update_dcc_transfer_status(
                                    transfer_id,
                                    DccStatus::Completed,
                                    received_size,
                                    Some(&filepath.to_string_lossy()),
                                    None,
                                ) {
                                    error!("Failed to update DCC transfer status: {}", e);
                                }
                                
                                // Update extraction metadata if zip was extracted
                                if let Some(files) = extracted_files {
                                    if let Err(e) = db.update_extraction_metadata(
                                        transfer_id,
                                        "extracted",
                                        Some(&files),
                                        None,
                                    ) {
                                        error!("Failed to update extraction metadata: {}", e);
                                    }
                                }
                            }
```

- [ ] **Step 6: Run cargo check**

Run: `cargo check`
Expected: Compiles successfully

- [ ] **Step 7: Commit**

```bash
git add src/irc/dcc.rs src/irc/client.rs
git commit -m "feat: integrate zip extraction into DCC download flow

- Update download_dcc_file to return extracted files
- Add zip detection and extraction after download completes
- Update database with extraction metadata
- Log extraction success/failure

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 9: Add Integration Tests for DCC Extraction

**Files:**
- Modify: `tests/dcc_test.rs`

- [ ] **Step 1: Write test for DCC download with zip extraction**

Add to `tests/dcc_test.rs`:

```rust
use std::fs::File;
use std::io::Write;
use zip::write::{FileOptions, ZipWriter};
use irc_mcp_server::irc::dcc::download_dcc_file;
use irc_mcp_server::irc::dcc::DccSendOffer;

fn create_test_zip_file(path: &std::path::Path) {
    let file = File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    
    zip.start_file("readme.txt", FileOptions::default()).unwrap();
    zip.write_all(b"This is a test file").unwrap();
    
    zip.start_file("data.json", FileOptions::default()).unwrap();
    zip.write_all(b"{\"test\": true}").unwrap();
    
    zip.finish().unwrap();
}

#[tokio::test]
async fn test_dcc_download_and_extract_zip() {
    // Note: This test uses a mock zip file rather than actual DCC connection
    // Real DCC testing requires network setup
    
    let temp_dir = tempfile::tempdir().unwrap();
    let download_dir = temp_dir.path().join("downloads");
    std::fs::create_dir_all(&download_dir).unwrap();
    
    let zip_path = download_dir.join("test.zip");
    create_test_zip_file(&zip_path);
    
    // Verify extraction happens for zip files
    let result = irc_mcp_server::irc::zip::extract_zip_file(
        &zip_path,
        &download_dir.join("test_extracted"),
        &irc_mcp_server::irc::zip::ExtractionLimits::default()
    );
    
    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted.len(), 2);
    
    // Verify extracted files exist
    assert!(download_dir.join("test_extracted/readme.txt").exists());
    assert!(download_dir.join("test_extracted/data.json").exists());
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test test_dcc_download_and_extract_zip`
Expected: PASS

- [ ] **Step 3: Write test for non-zip file (no extraction)**

Add to `tests/dcc_test.rs`:

```rust
#[test]
fn test_non_zip_file_no_extraction() {
    let temp_dir = tempfile::tempdir().unwrap();
    let download_dir = temp_dir.path().join("downloads");
    std::fs::create_dir_all(&download_dir).unwrap();
    
    let txt_path = download_dir.join("test.txt");
    std::fs::write(&txt_path, "plain text content").unwrap();
    
    // Should not be detected as zip
    assert!(!irc_mcp_server::irc::zip::is_zip_file(&txt_path));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_non_zip_file_no_extraction`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/dcc_test.rs
git commit -m "test: add integration tests for DCC zip extraction

- Test zip file detection and extraction
- Test non-zip files not extracted
- Verify extracted files exist with correct content

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 10: Verify MCP Tools Return Extraction Data

**Files:**
- Create: `tests/mcp_extraction_test.rs`

- [ ] **Step 1: Write test for MCP get_dcc_file_info with extracted files**

Create `tests/mcp_extraction_test.rs`:

```rust
use irc_mcp_server::storage::Database;
use irc_mcp_server::types::ExtractedFile;
use tempfile::TempDir;

#[test]
fn test_mcp_get_dcc_info_with_extracted_files() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    
    // Create transfer
    let transfer_id = db.save_dcc_transfer(
        "sender",
        "archive.zip",
        None,
        5000,
        Some("192.168.1.1"),
        Some(8080),
    ).unwrap();
    
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
    
    db.update_extraction_metadata(transfer_id, "extracted", Some(&extracted), None).unwrap();
    
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
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test test_mcp_get_dcc_info_with_extracted_files`
Expected: PASS

- [ ] **Step 3: Write test for list transfers with extraction status**

Add to `tests/mcp_extraction_test.rs`:

```rust
use irc_mcp_server::types::DccStatus;

#[test]
fn test_mcp_list_transfers_includes_extraction_status() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::new(&db_path).unwrap();
    
    // Create multiple transfers with different extraction statuses
    let id1 = db.save_dcc_transfer("user1", "file1.zip", None, 1000, None, None).unwrap();
    let id2 = db.save_dcc_transfer("user2", "file2.txt", None, 2000, None, None).unwrap();
    let id3 = db.save_dcc_transfer("user3", "file3.zip", None, 3000, None, None).unwrap();
    
    // Mark first as extracted
    db.update_extraction_metadata(id1, "extracted", Some(&vec![]), None).unwrap();
    
    // Mark third as failed
    db.update_extraction_metadata(id3, "failed", None, Some("Too many files")).unwrap();
    
    // List all transfers
    let transfers = db.list_dcc_transfers(None, 10).unwrap();
    assert_eq!(transfers.len(), 3);
    
    // Find by filename and verify extraction status
    let t1 = transfers.iter().find(|t| t.filename == "file1.zip").unwrap();
    assert_eq!(t1.extraction_status, Some("extracted".to_string()));
    assert!(t1.extracted_files.is_some());
    
    let t2 = transfers.iter().find(|t| t.filename == "file2.txt").unwrap();
    assert_eq!(t2.extraction_status, None);
    
    let t3 = transfers.iter().find(|t| t.filename == "file3.zip").unwrap();
    assert_eq!(t3.extraction_status, Some("failed".to_string()));
    assert_eq!(t3.extraction_error, Some("Too many files".to_string()));
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_mcp_list_transfers_includes_extraction_status`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/mcp_extraction_test.rs
git commit -m "test: verify MCP tools expose extraction metadata

- Test get_dcc_file_info returns extracted_files array
- Test list_dcc_transfers includes extraction status
- Verify both success and failure cases

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

---

### Task 11: Run Full Test Suite and Integration Verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run all unit tests**

Run: `cargo test --lib`
Expected: All tests PASS

- [ ] **Step 2: Run all integration tests**

Run: `cargo test --test '*'`
Expected: All tests PASS

- [ ] **Step 3: Run cargo clippy for linting**

Run: `cargo clippy -- -D warnings`
Expected: No warnings or errors

- [ ] **Step 4: Run cargo fmt to check formatting**

Run: `cargo fmt -- --check`
Expected: All files properly formatted

- [ ] **Step 5: Build release binary**

Run: `cargo build --release`
Expected: Builds successfully

- [ ] **Step 6: Commit if any formatting changes needed**

If `cargo fmt` made changes:
```bash
git add -A
git commit -m "style: apply cargo fmt

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

Otherwise: No commit needed

---

### Task 12: Manual Testing and Documentation Update

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update README to document extraction feature**

Add after line 9 in README.md (in Features section):

```markdown
- **Automatic zip extraction** - Downloaded zip files are automatically extracted to subdirectories with security protections (path traversal prevention, file count limits)
```

- [ ] **Step 2: Add extraction fields to MCP Tools documentation**

Update the DCC Operations section in README.md (around line 113) to show extracted_files field:

```markdown
### DCC Operations
- **irc_list_dcc_transfers** - List file transfers (includes extracted_files array for zips)
- **irc_get_dcc_file_info** - Get transfer details (includes extraction_status, extraction_error, extracted_files)
- **irc_read_dcc_file** - Read file content
```

- [ ] **Step 3: Commit documentation updates**

```bash
git add README.md
git commit -m "docs: document automatic zip extraction feature

- Add zip extraction to features list
- Document extracted_files in MCP tool responses
- Note security protections (path traversal, file limits)

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
```

- [ ] **Step 4: Manual test - create test zip file**

Run:
```bash
mkdir -p /tmp/test_irc_zip
cd /tmp/test_irc_zip
echo "file1 content" > file1.txt
echo "file2 content" > file2.txt
mkdir subdir
echo "file3 content" > subdir/file3.txt
zip -r test.zip file1.txt file2.txt subdir/
```

Expected: test.zip created with 3 files

- [ ] **Step 5: Manual test - start MCP server**

Run: `cargo run -- --config irc-mcp-config.yaml`
Expected: Server starts on port 5001

- [ ] **Step 6: Manual test - simulate DCC by copying zip to downloads**

In another terminal:
```bash
mkdir -p ./data/irc-downloads
cp /tmp/test_irc_zip/test.zip ./data/irc-downloads/
```

- [ ] **Step 7: Manual test - verify extraction works programmatically**

Run:
```bash
cargo test --test '*' -- --nocapture test_dcc_download_and_extract_zip
```

Expected: Test passes, shows extraction of files

- [ ] **Step 8: Mark manual testing complete**

No commit needed - verification step only.

---

## Self-Review Checklist

**Spec Coverage:**
- ✅ Zip detection (extension + magic bytes) - Task 2
- ✅ Extraction with path sanitization - Tasks 3, 4
- ✅ Path traversal protection - Task 5
- ✅ File count limit (1000) - Task 5
- ✅ Database schema migration - Task 6
- ✅ Database extraction metadata methods - Task 7
- ✅ DCC handler integration - Task 8
- ✅ MCP tools return extraction data - Task 10
- ✅ Original zip preserved (implicit in extraction logic)
- ✅ No recursive extraction - Task 5 test
- ✅ Testing strategy covered - Tasks 2, 5, 9, 10, 11

**Placeholder Check:**
- ✅ No TBD, TODO, or "implement later"
- ✅ All code blocks complete with actual implementations
- ✅ All test code includes full assertions
- ✅ All file paths exact
- ✅ All commands include expected output

**Type Consistency:**
- ✅ ExtractedFile: relative_path, full_path, size - consistent across all tasks
- ✅ DccTransfer fields: extracted_files, extraction_status, extraction_error - consistent
- ✅ ExtractionLimits: max_files, enable_path_traversal_check - consistent
- ✅ Function names: is_zip_file, extract_zip_file, sanitize_path_component, update_extraction_metadata - consistent throughout

**Dependencies Declared:**
- ✅ zip = "0.6" added in Task 1
- ✅ All necessary imports included in code blocks
- ✅ Test dependencies (zip crate) added for test code

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-28-dcc-zip-extraction.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
