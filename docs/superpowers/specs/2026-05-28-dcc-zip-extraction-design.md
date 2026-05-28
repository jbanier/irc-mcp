# DCC Zip File Automatic Extraction Design

**Date:** 2026-05-28  
**Status:** Approved  
**Author:** Claude Code

## Problem Statement

When DCC file transfers complete for zip files, agents must handle the zip extraction themselves, which slows down the workflow. The MCP server should extract zip files automatically during download and report the uncompressed file paths to the agent.

## Goals

- Automatically detect and extract zip files during DCC transfer completion
- Store extracted file metadata in the database
- Expose extracted file paths through existing MCP tools
- Maintain security (path traversal protection, file count limits)
- Keep original zip file intact after extraction

## Non-Goals

- Recursive extraction of nested zip files
- Extraction size limits (rely on DCC max_file_size)
- Support for other archive formats (tar, rar, 7z)
- Background/async extraction
- Configurable auto-extract behavior

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────┐
│ DCC Download Handler (src/irc/dcc.rs)              │
│  - download_dcc_file()                              │
│  - Post-download zip detection                      │
│  - Orchestrates extraction                          │
└─────────────┬───────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────┐
│ Zip Module (src/irc/zip.rs)                        │
│  - is_zip_file(path) -> bool                        │
│  - extract_zip_file(zip_path, extract_dir, limits)  │
│  - ExtractionLimits struct                          │
└─────────────┬───────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────┐
│ Database Layer (src/storage/database.rs)            │
│  - Schema: extraction_status, extraction_error,     │
│             extracted_files_json columns            │
│  - Stores extracted file metadata                   │
└─────────────┬───────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────┐
│ MCP Tools (src/mcp/tools.rs)                       │
│  - irc_get_dcc_file_info returns extracted_files    │
│  - irc_list_dcc_transfers includes extraction data  │
└─────────────────────────────────────────────────────┘
```

### Data Structures

**ExtractedFile** (`src/types.rs`):
```rust
pub struct ExtractedFile {
    pub relative_path: String,  // Path relative to extraction dir
    pub full_path: String,       // Absolute path on disk
    pub size: u64,
}
```

**DccTransfer Updates** (`src/types.rs`):
```rust
pub struct DccTransfer {
    // ... existing fields ...
    pub extracted_files: Option<Vec<ExtractedFile>>,
    pub extraction_status: Option<String>,  // null, "extracted", "failed"
    pub extraction_error: Option<String>,
}
```

**ExtractionLimits** (`src/irc/zip.rs`):
```rust
pub struct ExtractionLimits {
    pub max_files: usize,              // Default: 1000
    pub enable_path_traversal_check: bool,  // Default: true
}
```

### Database Schema Changes

Add columns to `dcc_transfers` table:
- `extraction_status` TEXT NULL - Values: null, "extracted", "failed"
- `extraction_error` TEXT NULL - Error message if extraction failed
- `extracted_files_json` TEXT NULL - JSON array of ExtractedFile objects

Migration SQL:
```sql
ALTER TABLE dcc_transfers ADD COLUMN extraction_status TEXT;
ALTER TABLE dcc_transfers ADD COLUMN extraction_error TEXT;
ALTER TABLE dcc_transfers ADD COLUMN extracted_files_json TEXT;
```

## Data Flow

### Download and Extraction Flow

1. **DCC Transfer Initiated**
   - IRC CTCP DCC SEND message parsed
   - `download_dcc_file()` called with offer details

2. **File Download**
   - Connect to sender's IP:port
   - Stream bytes to `{filename}.part` temporary file
   - Verify size matches advertised filesize
   - Rename to final filename
   - Database record created: `status=Completed`

3. **Post-Download Zip Detection**
   - Call `is_zip_file(filepath)`
   - Check extension (.zip)
   - Verify magic bytes (PK\x03\x04 at file start)

4. **Extraction (if zip detected)**
   - Create extraction directory: `{download_dir}/{filename}_extracted/`
   - Call `extract_zip_file()` with:
     - `zip_path`: Path to downloaded zip
     - `extract_dir`: Created extraction directory
     - `limits`: ExtractionLimits { max_files: 1000, enable_path_traversal_check: true }
   - For each file in zip:
     - Sanitize path (check for `..`, absolute paths)
     - Increment file counter, fail if > max_files
     - Extract to `{extract_dir}/{sanitized_path}`
     - Record ExtractedFile { relative_path, full_path, size }
   - Return `Vec<ExtractedFile>`

5. **Database Update**
   - **Success case:**
     - Set `extraction_status = "extracted"`
     - Serialize extracted files to `extracted_files_json`
   - **Failure case:**
     - Set `extraction_status = "failed"`
     - Store error message in `extraction_error`
   - **Non-zip case:**
     - Leave extraction columns as NULL

6. **Original Zip File**
   - Remains on disk at `{download_dir}/{filename}.zip`
   - Not deleted or modified

### Agent Discovery Flow

1. **Agent queries transfer info**
   - Calls `irc_get_dcc_file_info(transfer_id)` or `irc_list_dcc_transfers()`

2. **MCP tool reads database**
   - Load DccTransfer record
   - Deserialize `extracted_files_json` into `Vec<ExtractedFile>`
   - Populate `extracted_files` field

3. **Agent receives response**
   - DccTransfer object includes `extracted_files` array
   - Agent sees full paths to all extracted files
   - Agent can read files directly using filesystem tools

## Security

### Path Traversal Protection

Malicious zip files may contain entries like:
- `../../../etc/passwd`
- `/etc/shadow`
- `C:\Windows\System32\config\sam`

**Mitigation:**
- Reject any path component containing `..`
- Reject absolute paths (starting with `/` or drive letter)
- Normalize paths before extraction
- Log security warnings when blocked

### File Count Limit

Zip bombs may contain thousands of small files to exhaust inodes.

**Mitigation:**
- Count extracted files during extraction
- Stop extraction when count exceeds 1000
- Mark extraction as failed with descriptive error
- Log warning about excessive file count

### Existing Protections (unchanged)

- DCC `max_file_size_bytes` config limits downloaded zip size
- Filename sanitization prevents directory traversal in download path
- Server binds to localhost (127.0.0.1) - no external exposure

## Error Handling

### Zip Detection Errors

| Error | Behavior |
|-------|----------|
| File not found during magic byte check | Log warning, skip extraction (treat as non-zip) |
| File unreadable (permissions) | Log warning, skip extraction |
| Extension .zip but invalid magic bytes | Log warning, skip extraction |

### Extraction Errors

| Error | Behavior |
|-------|----------|
| Path traversal detected in zip entry | Skip malicious file, log security warning, continue with other files |
| File count exceeds 1000 | Stop extraction, set `extraction_status="failed"`, error: "Too many files in archive (limit: 1000)" |
| Individual file extraction fails | Log error for that file, continue extracting remaining files |
| Extraction directory creation fails | Set `extraction_status="failed"`, store error message |
| Zip corruption/invalid format | Set `extraction_status="failed"`, error: "Invalid or corrupted zip file" |

### Database Update Errors

| Error | Behavior |
|-------|----------|
| Failed to update extraction metadata | Log error, but DCC transfer status remains "completed" (download succeeded) |

### Principles

- **Download success is independent of extraction success** - If download completes but extraction fails, transfer status = "completed"
- **Partial extraction is allowed** - If 50/100 files extract successfully, record those 50
- **Fail safely** - Extraction errors never crash the server or corrupt the database

## Implementation Details

### Zip Module API

**Location:** `src/irc/zip.rs`

```rust
/// Check if file is a zip based on extension and magic bytes
pub fn is_zip_file(path: &Path) -> bool;

/// Extract zip file to directory with safety limits
pub fn extract_zip_file(
    zip_path: &Path,
    extract_dir: &Path,
    limits: &ExtractionLimits,
) -> Result<Vec<ExtractedFile>>;

/// Safety limits for extraction
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
```

### DCC Handler Integration

**Location:** `src/irc/dcc.rs`

```rust
pub async fn download_dcc_file(
    offer: &DccSendOffer,
    download_dir: &Path,
    max_file_size: u64,
) -> Result<(PathBuf, u64, Option<Vec<ExtractedFile>>)> {
    // ... existing download logic ...
    
    // After successful download
    let extracted_files = if is_zip_file(&final_path) {
        let extract_dir = download_dir.join(format!("{}_extracted", safe_filename));
        match extract_zip_file(&final_path, &extract_dir, &ExtractionLimits::default()) {
            Ok(files) => Some(files),
            Err(e) => {
                warn!("Zip extraction failed: {}", e);
                None
            }
        }
    } else {
        None
    };
    
    Ok((final_path, total_received, extracted_files))
}
```

### Database Integration

**Location:** `src/storage/database.rs`

```rust
impl Database {
    pub fn save_dcc_transfer(
        &self,
        transfer: &DccTransfer,
    ) -> Result<i64>;
    
    pub fn update_extraction_metadata(
        &self,
        transfer_id: i64,
        status: &str,
        extracted_files: Option<&Vec<ExtractedFile>>,
        error: Option<&str>,
    ) -> Result<()>;
    
    // Existing methods updated to deserialize extracted_files_json
    pub fn get_dcc_transfer(&self, id: i64) -> Result<Option<DccTransfer>>;
    pub fn list_dcc_transfers(&self, ...) -> Result<Vec<DccTransfer>>;
}
```

### Dependencies

Add to `Cargo.toml`:
```toml
[dependencies]
zip = "0.6"  # Zip archive reading/extraction
```

## Testing Strategy

### Unit Tests (`tests/zip_test.rs`)

- ✅ `test_is_zip_file_by_extension_and_magic` - Verify .zip extension + PK magic bytes detection
- ✅ `test_is_zip_file_rejects_non_zip` - Ensure .txt files return false
- ✅ `test_extract_simple_zip` - Extract basic zip with 2-3 files, verify all present
- ✅ `test_path_traversal_protection` - Malicious paths like `../../../etc/passwd` blocked
- ✅ `test_file_count_limit` - Zip with >1000 files triggers limit error
- ✅ `test_nested_zip_not_extracted` - Nested zips remain as files (no recursion)
- ✅ `test_extraction_to_subdirectory` - Files go to `{filename}_extracted/`
- ✅ `test_corrupted_zip_handling` - Invalid zip fails gracefully with error message

### Integration Tests (`tests/dcc_test.rs`)

- ✅ `test_dcc_download_and_extract_zip` - Mock DCC transfer of zip, verify download + extraction
- ✅ `test_dcc_non_zip_file_no_extraction` - .txt file downloaded without extraction attempt
- ✅ `test_mcp_get_dcc_info_with_extracted_files` - MCP tool returns extracted_files array
- ✅ `test_mcp_list_transfers_includes_extraction_status` - Transfer list includes extraction metadata

### Manual Testing Checklist

- [ ] Connect to IRC, receive real DCC zip transfer from another client
- [ ] Verify extraction directory created: `./data/irc-downloads/{filename}_extracted/`
- [ ] Verify all files extracted with correct content
- [ ] Call `irc_get_dcc_file_info` via curl, confirm extracted_files array populated
- [ ] Verify original zip file still exists at `./data/irc-downloads/{filename}.zip`
- [ ] Test zip containing subdirectories (e.g., `folder1/file1.txt`, `folder2/file2.txt`)
- [ ] Test zip with special characters in filenames

## Performance Considerations

### Extraction Timing

- Typical zip (10-50 files, 1-10MB): ~50-200ms extraction time
- Large zip (1000 files, 50MB): ~1-2 seconds extraction time
- Extraction happens synchronously during DCC download handler
- MCP tool call blocks until extraction completes

**Acceptable trade-off:** Extraction is fast enough that synchronous behavior is reasonable. If future use cases involve larger archives, can migrate to async extraction (Approach 2 from design phase).

### Disk Space

- Extracted files stored alongside zip file
- No compression, so extracted files consume full uncompressed size
- No automatic cleanup (manual deletion required)

**Mitigation:** Existing DCC `max_file_size_bytes` config limits downloaded zip size, indirectly limiting extracted size.

## Future Enhancements (out of scope)

- Support for other archive formats (tar.gz, rar, 7z)
- Configurable `auto_extract_zips` option in config.yaml
- Automatic cleanup of old extracted files
- Async/background extraction for very large archives
- Size limit for total extracted content (independent of zip size)
- MCP tool to manually trigger extraction for existing transfers
- Recursive extraction with depth limit

## Migration Path

### Database Migration

Run on startup if columns don't exist:
```sql
ALTER TABLE dcc_transfers ADD COLUMN extraction_status TEXT;
ALTER TABLE dcc_transfers ADD COLUMN extraction_error TEXT;
ALTER TABLE dcc_transfers ADD COLUMN extracted_files_json TEXT;
```

Existing records will have NULL values for new columns (correct behavior - no extraction attempted).

### Backward Compatibility

- Existing DCC transfers without extraction data: `extracted_files = None`
- Old MCP clients: Ignore new fields (additive change)
- Configuration: No new config required (feature always enabled)

## Success Criteria

- ✅ Zip files automatically extracted during DCC download
- ✅ Extracted file paths returned via `irc_get_dcc_file_info` and `irc_list_dcc_transfers`
- ✅ Path traversal attacks blocked
- ✅ File count limit (1000) enforced
- ✅ Original zip file preserved
- ✅ Extraction failures don't break DCC transfer completion
- ✅ All tests pass (unit + integration)
- ✅ Manual testing confirms real-world IRC DCC zip transfers work correctly
