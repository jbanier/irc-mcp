// src/storage/cleanup.rs
use crate::storage::Database;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tracing::{debug, error, info};

/// Delete old messages and files
pub fn cleanup_old_data(
    db: &Database,
    server_download_dirs: &[PathBuf],
    cutoff: DateTime<Utc>,
) -> Result<(usize, usize)> {
    // Delete old messages
    let deleted_messages = db
        .delete_messages_before(cutoff)
        .context("Failed to delete old messages")?;

    // Delete old files
    let mut deleted_files = 0;
    for dir in server_download_dirs {
        if !dir.exists() {
            debug!("Download directory does not exist: {}", dir.display());
            continue;
        }

        match delete_old_files_in_dir(dir, cutoff) {
            Ok(count) => deleted_files += count,
            Err(e) => error!("Error cleaning up {}: {}", dir.display(), e),
        }
    }

    Ok((deleted_messages, deleted_files))
}

/// Delete files older than cutoff in a directory (recursive)
fn delete_old_files_in_dir(dir: &PathBuf, cutoff: DateTime<Utc>) -> Result<usize> {
    let mut deleted = 0;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            deleted += delete_old_files_in_dir(&path, cutoff)?;
            // Try to remove empty directory
            let _ = std::fs::remove_dir(&path);
        } else if path.is_file() {
            let metadata = std::fs::metadata(&path)?;
            let modified = metadata.modified()?;
            let modified_dt: DateTime<Utc> = modified.into();

            if modified_dt < cutoff {
                match std::fs::remove_file(&path) {
                    Ok(_) => {
                        debug!("Deleted old file: {}", path.display());
                        deleted += 1;
                    }
                    Err(e) => error!("Failed to delete {}: {}", path.display(), e),
                }
            }
        }
    }

    Ok(deleted)
}

/// Start background cleanup task
pub async fn start_cleanup_loop(
    db: std::sync::Arc<tokio::sync::Mutex<Database>>,
    server_download_dirs: Vec<PathBuf>,
    cleanup_interval_hours: u64,
    retention_days: u32,
) {
    let interval = tokio::time::Duration::from_secs(cleanup_interval_hours * 3600);
    let mut ticker = tokio::time::interval(interval);

    loop {
        ticker.tick().await;

        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

        info!("Starting cleanup of data older than {}", cutoff);

        let db_guard = db.lock().await;
        match cleanup_old_data(&db_guard, &server_download_dirs, cutoff) {
            Ok((deleted_msgs, deleted_files)) => {
                info!(
                    "Cleanup complete: {} messages, {} files deleted",
                    deleted_msgs, deleted_files
                );
            }
            Err(e) => {
                error!("Cleanup error: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_delete_old_files_in_dir() {
        let temp_dir = TempDir::new().unwrap();

        // Create old file
        let old_file = temp_dir.path().join("old.txt");
        let mut f = std::fs::File::create(&old_file).unwrap();
        f.write_all(b"old").unwrap();
        drop(f);

        let old_time =
            std::time::SystemTime::now() - std::time::Duration::from_secs(100 * 24 * 3600);
        filetime::set_file_mtime(&old_file, filetime::FileTime::from_system_time(old_time))
            .unwrap();

        // Create new file
        let new_file = temp_dir.path().join("new.txt");
        let mut f = std::fs::File::create(&new_file).unwrap();
        f.write_all(b"new").unwrap();

        let cutoff = Utc::now() - Duration::days(90);
        let deleted = delete_old_files_in_dir(&temp_dir.path().to_path_buf(), cutoff).unwrap();

        assert_eq!(deleted, 1);
        assert!(!old_file.exists());
        assert!(new_file.exists());
    }
}
