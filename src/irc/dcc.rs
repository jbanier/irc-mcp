use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

/// Parsed DCC SEND offer
#[derive(Debug, Clone)]
pub struct DccSendOffer {
    pub filename: String,
    pub ip_address: String,
    pub port: u16,
    pub filesize: u64,
}

/// Parse DCC SEND message from CTCP
/// Format: DCC SEND filename ipaddr port filesize
pub fn parse_dcc_send(ctcp_message: &str) -> Result<DccSendOffer> {
    // CTCP messages are wrapped in \x01
    let msg = ctcp_message.trim_matches('\x01');

    if !msg.starts_with("DCC SEND ") {
        bail!("Not a DCC SEND message");
    }

    let parts: Vec<&str> = msg.split_whitespace().collect();
    if parts.len() < 5 {
        bail!("Invalid DCC SEND format: expected 'DCC SEND filename ipaddr port filesize'");
    }

    let filename = parts[2].to_string();
    let ip_numeric: u32 = parts[3].parse().context("Failed to parse IP address")?;
    let port: u16 = parts[4].parse().context("Failed to parse port")?;
    let filesize: u64 = parts[5].parse().context("Failed to parse filesize")?;

    // Convert numeric IP to dotted decimal
    let ip_address = format!(
        "{}.{}.{}.{}",
        (ip_numeric >> 24) & 0xFF,
        (ip_numeric >> 16) & 0xFF,
        (ip_numeric >> 8) & 0xFF,
        ip_numeric & 0xFF
    );

    Ok(DccSendOffer {
        filename,
        ip_address,
        port,
        filesize,
    })
}

/// Sanitize filename to prevent directory traversal
pub fn sanitize_filename(filename: &str) -> String {
    // Remove path separators and parent directory references
    filename
        .replace("../", ".._")
        .replace("..\\", ".._")
        .replace(['/', '\\'], "_")
        .trim()
        .to_string()
}

/// Download a file via DCC SEND protocol
pub async fn download_dcc_file(
    offer: &DccSendOffer,
    download_dir: &Path,
    max_file_size: u64,
) -> Result<(PathBuf, u64, Option<Vec<crate::types::ExtractedFile>>)> {
    // Validate file size
    if offer.filesize > max_file_size {
        bail!(
            "File size {} exceeds maximum allowed size {}",
            offer.filesize,
            max_file_size
        );
    }

    // Sanitize and create destination path
    let safe_filename = sanitize_filename(&offer.filename);
    if safe_filename.is_empty() {
        bail!("Invalid filename after sanitization");
    }

    std::fs::create_dir_all(download_dir).context("Failed to create download directory")?;

    let temp_path = download_dir.join(format!("{}.part", safe_filename));
    let final_path = download_dir.join(&safe_filename);

    info!(
        "Starting DCC download: {} from {}:{} ({} bytes)",
        safe_filename, offer.ip_address, offer.port, offer.filesize
    );

    // Connect to sender
    let mut stream = TcpStream::connect(format!("{}:{}", offer.ip_address, offer.port))
        .await
        .context("Failed to connect to DCC sender")?;

    // Open temp file for writing
    let mut file = File::create(&temp_path)
        .await
        .context("Failed to create temporary file")?;

    // Download with progress tracking
    let mut total_received: u64 = 0;
    let mut buffer = vec![0u8; 8192];

    loop {
        let bytes_read = stream
            .read(&mut buffer)
            .await
            .context("Failed to read from DCC connection")?;

        if bytes_read == 0 {
            break; // EOF
        }

        file.write_all(&buffer[..bytes_read])
            .await
            .context("Failed to write to file")?;

        total_received += bytes_read as u64;

        // Send acknowledgement (DCC protocol requires this)
        let ack = (total_received as u32).to_be_bytes();
        stream
            .write_all(&ack)
            .await
            .context("Failed to send DCC acknowledgement")?;

        // Check if we've exceeded expected size
        if total_received > offer.filesize {
            warn!("Received more bytes than advertised filesize");
            break;
        }

        debug!("DCC progress: {}/{} bytes", total_received, offer.filesize);
    }

    // Validate received size
    if total_received != offer.filesize {
        bail!(
            "Size mismatch: expected {} bytes, received {} bytes",
            offer.filesize,
            total_received
        );
    }

    // Rename temp file to final name
    tokio::fs::rename(&temp_path, &final_path)
        .await
        .context("Failed to rename completed download")?;

    // Check if file is a zip and extract if so
    let extracted_files = if crate::irc::zip::is_zip_file(&final_path) {
        info!("Detected zip file, extracting: {}", safe_filename);

        let extract_dir = download_dir.join(format!(
            "{}_extracted",
            safe_filename.trim_end_matches(".zip")
        ));

        match crate::irc::zip::extract_zip_file(
            &final_path,
            &extract_dir,
            &crate::irc::zip::ExtractionLimits::default(),
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

    info!(
        "DCC download completed: {} ({} bytes)",
        safe_filename, total_received
    );

    Ok((final_path, total_received, extracted_files))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dcc_send() {
        let ctcp = "\x01DCC SEND document.pdf 3232235777 12345 2048576\x01";
        let offer = parse_dcc_send(ctcp).unwrap();

        assert_eq!(offer.filename, "document.pdf");
        assert_eq!(offer.ip_address, "192.168.1.1");
        assert_eq!(offer.port, 12345);
        assert_eq!(offer.filesize, 2048576);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(sanitize_filename("normal.txt"), "normal.txt");
        assert_eq!(sanitize_filename("path/to/file.txt"), "path_to_file.txt");
        assert_eq!(
            sanitize_filename("C:\\Windows\\file.exe"),
            "C:_Windows_file.exe"
        );
    }

    #[test]
    fn test_parse_invalid_dcc() {
        let result = parse_dcc_send("\x01INVALID MESSAGE\x01");
        assert!(result.is_err());
    }
}
