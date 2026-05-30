//! Cleanup and prune commands for Deltaship Client.

use std::fs;
use std::path::Path;

use deltaship_db::ClientDb;

use crate::config::ClientConfig;

/// Prefix for Deltaship temporary files to distinguish them from other files.
/// Only files with this prefix (or in deltaship-specific directories) will be cleaned up.
const DELTASHIP_TEMP_PREFIX: &str = "deltaship-";

/// Default age for old history entries in days.
const DEFAULT_HISTORY_AGE_DAYS: u64 = 90;

/// Format bytes as a human-readable size.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Cleanup result for tracking removed items.
#[derive(Default)]
struct CleanupResult {
    expired_backups: Vec<(String, u64)>,
    incomplete_downloads: Vec<(String, u64)>,
    old_history_entries: usize,
    total_space: u64,
}

/// Run the cleanup command.
pub async fn run_cleanup(
    config: &ClientConfig,
    db: &ClientDb,
    dry_run: bool,
    max_age_days: Option<u64>,
) -> anyhow::Result<()> {
    let max_age = max_age_days.unwrap_or(DEFAULT_HISTORY_AGE_DAYS);

    if dry_run {
        println!("Running cleanup in dry-run mode (no files will be removed)...\n");
    } else {
        println!("Running cleanup...\n");
    }

    let mut result = CleanupResult::default();

    // 1. Remove expired rollback backups
    cleanup_expired_backups(config, db, &mut result, dry_run).await?;

    // 2. Clean up incomplete downloads in temp directory
    cleanup_incomplete_downloads(config, &mut result, dry_run).await?;

    // 3. Remove old update history entries
    cleanup_old_history(db, &mut result, dry_run, max_age).await?;

    // Print summary
    println!("\nCleanup summary:");

    if !result.expired_backups.is_empty() {
        let total: u64 = result.expired_backups.iter().map(|(_, s)| s).sum();
        println!(
            "  - Removed {} expired backups ({})",
            result.expired_backups.len(),
            format_size(total)
        );
    }

    if !result.incomplete_downloads.is_empty() {
        let total: u64 = result.incomplete_downloads.iter().map(|(_, s)| s).sum();
        println!(
            "  - Cleaned {} incomplete downloads ({})",
            result.incomplete_downloads.len(),
            format_size(total)
        );
    }

    if result.old_history_entries > 0 {
        println!(
            "  - Pruned {} old history entries",
            result.old_history_entries
        );
    }

    if result.total_space > 0 {
        println!(
            "  - Total space recovered: {}",
            format_size(result.total_space)
        );
    } else {
        println!("  - No cleanup needed");
    }

    if dry_run && result.total_space > 0 {
        println!("\n(Dry run - no files were actually removed. Run without --dry-run to remove.)");
    }

    Ok(())
}

/// Clean up expired rollback backups.
///
/// # Timestamp Handling
///
/// Backup expiration timestamps (`expires_at`) are stored as naive datetimes in the database.
/// They are assumed to be in UTC for consistency with `chrono::Utc::now().naive_utc()`.
/// If backups were created with local time timestamps, there may be timezone mismatches
/// causing early or late expiration (by up to the local timezone offset from UTC).
///
/// # P3 Issue #110 Fix: Known Limitation - Timezone Handling
///
/// **Status**: Not implemented (tracked as P2 Issue #68 in TODO.md)
///
/// **Current behavior**: All timestamps are stored as naive UTC (no timezone info).
/// This works correctly as long as all Deltaship components use UTC consistently.
///
/// **Limitation**: If timestamps are compared across systems with different timezone settings,
/// or if local time is mistakenly stored, backups may expire at incorrect times.
///
/// **Recommended solution**: Migrate to RFC 3339 format (ISO 8601 with timezone)
/// for all timestamps, e.g., `"2024-01-15T10:30:00Z"` or `"2024-01-15T10:30:00+00:00"`.
///
/// **Workaround**: Ensure all systems running Deltaship use UTC for system time, or explicitly
/// document that Deltaship timestamps are always UTC.
async fn cleanup_expired_backups(
    config: &ClientConfig,
    db: &ClientDb,
    result: &mut CleanupResult,
    dry_run: bool,
) -> anyhow::Result<()> {
    let backups_dir = config.backups_dir();

    if !backups_dir.exists() {
        return Ok(());
    }

    // Get all binaries and their backups
    let binaries = db.list_binaries().await?;

    for binary in &binaries {
        let backups = db.list_backups(&binary.binary_id).await?;

        for backup in backups {
            // Check if the backup has expired
            if let Some(ref expires_at) = backup.expires_at {
                // Parse the expiration timestamp (assumed to be in UTC)
                // Note: If the timestamp was stored in local time, this comparison may be
                // inaccurate by the local timezone offset.
                if let Ok(expires) =
                    chrono::NaiveDateTime::parse_from_str(expires_at, "%Y-%m-%d %H:%M:%S")
                {
                    let now = chrono::Utc::now().naive_utc();
                    if expires < now {
                        let backup_path = Path::new(&backup.backup_path);
                        let size = backup.backup_size_bytes as u64;

                        if dry_run {
                            println!(
                                "  Would remove expired backup: {} v{} ({})",
                                binary.binary_name,
                                backup.version_string,
                                format_size(size)
                            );
                        } else {
                            // Delete from database first to avoid orphaned records
                            // If we delete the file first and crash before deleting the DB record,
                            // the database will reference a non-existent file
                            if let Err(e) = db.delete_backup(backup.backup_id).await {
                                eprintln!("  Failed to delete backup record: {}", e);
                                continue;
                            }

                            // Now delete the file
                            // If this fails, we have a dangling file but no DB reference (preferred)
                            if backup_path.exists() {
                                match fs::remove_file(backup_path) {
                                    Ok(_) => {
                                        println!(
                                            "  Removed expired backup: {} v{} ({})",
                                            binary.binary_name,
                                            backup.version_string,
                                            format_size(size)
                                        );
                                        result.total_space += size;
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "  Failed to remove backup file {}: {}",
                                            backup.backup_path, e
                                        );
                                        // DB record is already deleted, so we continue
                                    }
                                }
                            }
                        }

                        result
                            .expired_backups
                            .push((backup.backup_path.clone(), size));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if a file is a valid Deltaship temporary file that can be safely deleted.
///
/// A file is considered a Deltaship temp file if:
/// 1. It starts with the Deltaship prefix ("deltaship-"), OR
/// 2. It's located within the deltaship data directory structure
///
/// Additionally, the file must have a temporary file extension.
fn is_deltaship_temp_file(path: &Path, deltaship_data_dir: &Path) -> bool {
    let filename = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => return false,
    };

    // Check if it has a temporary file extension
    let has_temp_extension = filename.ends_with(".tmp")
        || filename.ends_with(".part")
        || filename.ends_with(".download");

    // Check if it starts with the Deltaship prefix
    let has_deltaship_prefix =
        filename.starts_with(DELTASHIP_TEMP_PREFIX) || filename.starts_with(".partial_deltaship-");

    // Check if the file is within the deltaship data directory
    let is_in_deltaship_dir = path.starts_with(deltaship_data_dir);

    // File must have a temp extension AND either have the deltaship prefix OR be in the deltaship directory
    has_temp_extension && (has_deltaship_prefix || is_in_deltaship_dir)
}

/// Clean up incomplete downloads in the deltaship downloads directory.
///
/// Only removes files that are:
/// 1. Located within the deltaship downloads directory, AND
/// 2. Have temporary file extensions (.tmp, .part, .download)
///
/// This prevents accidental deletion of unrelated files.
async fn cleanup_incomplete_downloads(
    config: &ClientConfig,
    result: &mut CleanupResult,
    dry_run: bool,
) -> anyhow::Result<()> {
    let downloads_dir = config.downloads_dir();
    let data_dir = &config.data_dir;

    if !downloads_dir.exists() {
        return Ok(());
    }

    // Security: Only walk within the deltaship downloads directory, not arbitrary paths
    // This ensures we never accidentally delete files outside our data directory
    for entry in walkdir::WalkDir::new(&downloads_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_file() {
            // Security check: Verify the file is a valid Deltaship temp file
            if !is_deltaship_temp_file(path, data_dir) {
                // Skip files that don't match our criteria
                // This could be a file that somehow got into the directory
                // but doesn't belong to deltaship
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let has_temp_ext = filename.ends_with(".tmp")
                    || filename.ends_with(".part")
                    || filename.ends_with(".download");

                if has_temp_ext {
                    tracing::debug!(
                        "Skipping non-Deltaship temp file: {} (missing '{}' prefix)",
                        path.display(),
                        DELTASHIP_TEMP_PREFIX
                    );
                }
                continue;
            }

            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Double-check the file has a temporary extension
            // (is_deltaship_temp_file already checks this, but being explicit for safety)
            if filename.ends_with(".tmp")
                || filename.ends_with(".part")
                || filename.ends_with(".download")
            {
                let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

                if dry_run {
                    println!(
                        "  Would remove incomplete download: {} ({})",
                        path.display(),
                        format_size(size)
                    );
                } else {
                    match fs::remove_file(path) {
                        Ok(_) => {
                            println!(
                                "  Removed incomplete download: {} ({})",
                                path.display(),
                                format_size(size)
                            );
                            result.total_space += size;
                        }
                        Err(e) => {
                            eprintln!("  Failed to remove {}: {}", path.display(), e);
                            continue;
                        }
                    }
                }

                result
                    .incomplete_downloads
                    .push((path.to_string_lossy().to_string(), size));
            }
        }
    }

    Ok(())
}

/// Clean up old update history entries.
///
/// # P2 Issue 66 Fix: Incomplete History Cleanup
///
/// This function deletes old update history entries from the database to prevent
/// unbounded growth. Only entries older than `max_age_days` are deleted.
async fn cleanup_old_history(
    db: &ClientDb,
    result: &mut CleanupResult,
    dry_run: bool,
    max_age_days: u64,
) -> anyhow::Result<()> {
    // Get all binaries and count old history entries
    let binaries = db.list_binaries().await?;
    let cutoff = chrono::Utc::now().naive_utc() - chrono::Duration::days(max_age_days as i64);

    for binary in &binaries {
        let updates = db.get_recent_updates(&binary.binary_id, 1000).await?;

        for update in updates {
            // Parse the started_at timestamp
            if let Ok(started) =
                chrono::NaiveDateTime::parse_from_str(&update.started_at, "%Y-%m-%d %H:%M:%S")
            {
                if started < cutoff {
                    // Safe date extraction with bounds check
                    let date_display = if update.started_at.len() >= 10 {
                        &update.started_at[..10]
                    } else {
                        &update.started_at
                    };

                    if dry_run {
                        println!(
                            "  Would prune history entry: {} {} -> {} ({})",
                            binary.binary_name,
                            update.from_version_string.as_deref().unwrap_or("?"),
                            &update.to_version_string,
                            date_display
                        );
                    }
                    result.old_history_entries += 1;
                }
            }
        }
    }

    // Delete old history entries (all at once for efficiency)
    if !dry_run && result.old_history_entries > 0 {
        let deleted = db.delete_update_history(max_age_days as i32).await?;
        if deleted > 0 {
            println!("  Deleted {} old history entries", deleted);
        }
    }

    Ok(())
}

/// Run the prune-backups command - keep only the last N backups.
pub async fn run_prune_backups(
    _config: &ClientConfig,
    db: &ClientDb,
    name: String,
    keep: usize,
    dry_run: bool,
) -> anyhow::Result<()> {
    if keep == 0 {
        anyhow::bail!("Cannot keep 0 backups. Use at least --keep 1.");
    }

    if dry_run {
        println!("Running prune-backups in dry-run mode (no changes will be made)...\n");
    }

    // Find the binary
    let binary = db
        .get_binary_by_name(&name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Binary '{}' not found", name))?;

    // Get all backups for this binary (ordered by created_at DESC)
    let backups = db.list_backups(&binary.binary_id).await?;

    if backups.len() <= keep {
        println!(
            "Binary '{}' has {} backups, keeping all (requested: keep {}).",
            name,
            backups.len(),
            keep
        );
        return Ok(());
    }

    let backups_to_remove = &backups[keep..];
    let mut total_removed = 0;
    let mut total_space: u64 = 0;

    println!("Pruning backups for '{}'...\n", name);

    for backup in backups_to_remove {
        let size = backup.backup_size_bytes as u64;

        if dry_run {
            println!(
                "  Would remove backup: v{} ({}) - {}",
                backup.version_string,
                format_size(size),
                backup.backup_path
            );
        } else {
            let backup_path = Path::new(&backup.backup_path);

            // Delete from database first to avoid orphaned records
            // If we delete the file first and crash before deleting the DB record,
            // the database will reference a non-existent file
            if let Err(e) = db.delete_backup(backup.backup_id).await {
                eprintln!("  Failed to delete backup record: {}", e);
                continue;
            }

            // Now delete the file
            // If this fails, we have a dangling file but no DB reference (preferred)
            if backup_path.exists() {
                match fs::remove_file(backup_path) {
                    Ok(_) => {
                        println!(
                            "  Removed backup: v{} ({}) - {}",
                            backup.version_string,
                            format_size(size),
                            backup.backup_path
                        );
                        total_space += size;
                    }
                    Err(e) => {
                        eprintln!("  Failed to remove file {}: {}", backup.backup_path, e);
                        // DB record is already deleted, so we continue
                    }
                }
            }
        }

        total_removed += 1;
    }

    // Print summary
    println!("\nPrune summary:");
    println!("  - Removed {} backups", total_removed);
    if total_space > 0 {
        println!("  - Total space recovered: {}", format_size(total_space));
    }
    println!("  - Kept {} most recent backups", keep);

    if dry_run {
        println!("\n(Dry run - no changes were actually made. Run without --dry-run to apply.)");
    }

    Ok(())
}
