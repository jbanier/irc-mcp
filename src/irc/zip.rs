use crate::types::ExtractedFile;
use anyhow::{bail, Result};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use tracing::warn;
use zip::ZipArchive;

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
