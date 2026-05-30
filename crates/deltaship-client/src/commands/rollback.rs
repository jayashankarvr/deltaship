//! Rollback command - rollback to a previous version.

use std::path::Path;

use anyhow::Context;
use deltaship_crypto::{hash_file, Hash};
use deltaship_db::{ClientDb, DbRollbackBackup};

use crate::audit::{log_audit_event, AuditEvent};
use crate::config::ClientConfig;
use crate::patcher::{sig_path_for, validate_install_path, verify_backup_signature};

/// Information about an available rollback.
// Used by the rollback command UI; fields read when displaying rollback options.
#[allow(dead_code)]
pub struct RollbackInfo {
    pub backup_id: i64,
    pub version_string: String,
    pub backup_path: String,
    pub created_at: String,
    pub size_bytes: i64,
}

impl From<DbRollbackBackup> for RollbackInfo {
    fn from(backup: DbRollbackBackup) -> Self {
        Self {
            backup_id: backup.backup_id,
            version_string: backup.version_string,
            backup_path: backup.backup_path,
            created_at: backup.created_at,
            size_bytes: backup.backup_size_bytes,
        }
    }
}

/// List available rollbacks for a binary.
pub async fn list_available_rollbacks(
    db: &ClientDb,
    binary_name: &str,
) -> anyhow::Result<Vec<RollbackInfo>> {
    let binary = db
        .get_binary_by_name(binary_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Binary '{}' not found", binary_name))?;

    let backups = db.list_backups(&binary.binary_id).await?;

    // Filter out backups whose files no longer exist
    let mut available = Vec::new();
    for backup in backups {
        if Path::new(&backup.backup_path).exists() {
            available.push(RollbackInfo::from(backup));
        }
    }

    Ok(available)
}

/// Perform a rollback to a previous version.
pub async fn perform_rollback(
    config: &ClientConfig,
    db: &ClientDb,
    binary_name: &str,
    to_version: Option<&str>,
) -> anyhow::Result<()> {
    let binary = db
        .get_binary_by_name(binary_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Binary '{}' not found", binary_name))?;

    // Get available backups
    let backups = db.list_backups(&binary.binary_id).await?;

    if backups.is_empty() {
        anyhow::bail!("No backups available for '{}'", binary_name);
    }

    // Find the target backup
    let target_backup = if let Some(version) = to_version {
        backups
            .iter()
            .find(|b| b.version_string == version)
            .ok_or_else(|| anyhow::anyhow!("No backup found for version '{}'", version))?
    } else {
        // Default to the most recent backup (first in the list, ordered by created_at DESC)
        &backups[0]
    };

    // Check if backup file exists
    let backup_path = Path::new(&target_backup.backup_path);
    if !backup_path.exists() {
        anyhow::bail!(
            "Backup file not found: {}. The backup may have been deleted.",
            target_backup.backup_path
        );
    }

    // Verify backup integrity (BLAKE3) and authenticity (Ed25519 sidecar).
    //
    // CLIENT-P1-1: Backups now carry the publisher's Ed25519 signature in a
    // `<backup>.sig` sidecar. Checksum proves the file is the bytes we wrote;
    // signature proves those bytes originally came from the trusted publisher,
    // so an attacker who tampers with a backup file cannot get it restored.
    //
    // Backwards compatibility: backups created before this change have no
    // sidecar. `verify_backup_signature` fails loudly in that case — a missing
    // sidecar is treated as an unverifiable backup and refused, rather than
    // silently restored. Users on legacy backups must reinstall the desired
    // version from the publisher.
    let expected_hash = Hash::from_bytes(
        target_backup
            .backup_hash_blake3
            .clone()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid backup hash stored in database"))?,
    );
    let actual_hash = hash_file(backup_path)?;

    if expected_hash != actual_hash {
        anyhow::bail!(
            "Backup file corrupted: checksum mismatch for version {}",
            target_backup.version_string
        );
    }

    verify_backup_signature(
        backup_path,
        &binary.publisher_public_key,
        binary_name,
        &target_backup.version_string,
    )?;
    tracing::info!(
        "Backup signature verified for version {}",
        target_backup.version_string
    );

    let current_version = binary
        .current_version_string
        .as_deref()
        .unwrap_or("(unknown)");

    println!(
        "Rolling back '{}': {} -> {}",
        binary_name, current_version, target_backup.version_string
    );

    // Create backup of current version before rollback (so user can undo)
    let install_path = Path::new(&binary.install_path);

    // Security: Validate install path early before any filesystem mutation.
    // Re-validated immediately before the atomic rename below to minimize the
    // TOCTOU race window. Mirrors the pattern used in patcher::execute_update.
    validate_install_path(install_path).with_context(|| {
        format!(
            "Install path validation failed for binary '{}' at '{}'",
            binary_name,
            install_path.display()
        )
    })?;

    if install_path.exists() {
        let backups_dir = config.backups_dir();
        tokio::fs::create_dir_all(&backups_dir).await?;

        let pre_rollback_backup_name = format!(
            "{}_{}_pre_rollback.backup",
            binary_name,
            current_version.replace(['/', '\\', ' '], "_")
        );
        let pre_rollback_path = backups_dir.join(&pre_rollback_backup_name);

        tracing::info!(
            "Creating pre-rollback backup at {}",
            pre_rollback_path.display()
        );
        tokio::fs::copy(install_path, &pre_rollback_path).await?;

        // CLIENT-P1-1: Carry the publisher signature forward to the pre-rollback
        // backup so a future "undo rollback" can re-verify authenticity.
        let install_sig = sig_path_for(install_path);
        let pre_rollback_sig = sig_path_for(&pre_rollback_path);
        if install_sig.exists() {
            tokio::fs::copy(&install_sig, &pre_rollback_sig).await.with_context(|| {
                format!(
                    "Failed to copy signature sidecar: {:?} -> {:?}",
                    install_sig, pre_rollback_sig
                )
            })?;
        } else {
            tracing::warn!(
                install_sig = %install_sig.display(),
                "No signature sidecar for current binary; pre-rollback backup will \
                 not be re-verifiable on undo. Likely installed before signature \
                 persistence was added."
            );
        }

        // Record this backup in the database
        if let (Some(version_id), Some(version_string)) = (
            binary.current_version_id.as_deref(),
            binary.current_version_string.as_deref(),
        ) {
            let backup_hash = hash_file(&pre_rollback_path)?;
            let backup_size = tokio::fs::metadata(&pre_rollback_path).await?.len() as i64;

            // Set expiration time for pre-rollback backup (default: 30 days from now)
            // These backups are temporary snapshots to allow undoing a rollback
            let expires_at = chrono::Utc::now() + chrono::Duration::days(30);
            let expires_at_str = expires_at.format("%Y-%m-%d %H:%M:%S").to_string();

            db.create_backup(
                &binary.binary_id,
                version_id,
                version_string,
                &pre_rollback_path.to_string_lossy(),
                &backup_hash.to_bytes(),
                backup_size,
                Some(&expires_at_str),
            )
            .await?;
        }
    }

    // Restore the backup atomically
    // Write to temp file in same directory, then atomically rename to prevent corruption
    tracing::info!(
        "Restoring {} to {}",
        target_backup.version_string,
        install_path.display()
    );

    // Get the parent directory for the temp file (must be same filesystem for atomic rename)
    let parent_dir = install_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Install path has no parent directory"))?;

    // Create temp file in same directory for atomic rename
    let temp_file = tempfile::NamedTempFile::new_in(parent_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create temp file for restore: {}", e))?;
    let temp_path = temp_file.path().to_path_buf();

    // Copy backup to temp file
    tokio::fs::copy(backup_path, &temp_path).await?;

    // Set executable permissions on Unix before atomic rename
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&temp_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&temp_path, perms).await?;
    }

    // CRITICAL: Validate install path immediately before atomic rename
    //
    // SECURITY NOTE: This validation prevents symlink attacks where an attacker
    // could replace install_path with a symlink to a privileged location (e.g., /etc/passwd).
    // The validation minimizes but does NOT eliminate the TOCTOU race window.
    // See validate_install_path() documentation in patcher.rs for full security discussion.
    validate_install_path(install_path).with_context(|| {
        format!(
            "Install path validation failed immediately before rollback write for '{}'",
            install_path.display()
        )
    })?;

    tracing::info!(
        install_path = %install_path.display(),
        "Security: Rollback path validation passed, performing atomic write"
    );

    // Atomically rename temp file to install path
    // persist() uses rename() which is atomic on the same filesystem
    if let Err(e) = temp_file.persist(install_path) {
        // Cleanup on failure
        let _ = tokio::fs::remove_file(&temp_path).await;
        anyhow::bail!(
            "Failed to atomically restore binary to {}: {}",
            install_path.display(),
            e.error
        );
    }

    // CLIENT-P1-1: Restore the sidecar signature so the rolled-back binary is
    // itself backup-and-rollback-safe going forward.
    let backup_sig = sig_path_for(backup_path);
    let install_sig = sig_path_for(install_path);
    if backup_sig.exists() {
        tokio::fs::copy(&backup_sig, &install_sig).await.with_context(|| {
            format!(
                "Failed to restore signature sidecar: {:?} -> {:?}",
                backup_sig, install_sig
            )
        })?;
    }

    // Update database with the rolled-back version
    db.update_current_version(
        &binary.binary_id,
        &target_backup.version_id,
        &target_backup.version_string,
    )
    .await?;

    // Record rollback in update history
    db.record_rollback(
        &binary.binary_id,
        binary.current_version_id.as_deref(),
        binary.current_version_string.as_deref(),
        &target_backup.version_id,
        &target_backup.version_string,
    )
    .await?;

    println!("Rollback completed successfully!");
    println!("  Binary: {}", binary_name);
    println!("  Previous version: {}", current_version);
    println!("  Current version: {}", target_backup.version_string);

    // Audit log rollback
    log_audit_event(&AuditEvent::Rollback {
        binary_name: binary_name.to_string(),
        from_version: current_version.to_string(),
        to_version: target_backup.version_string.clone(),
    });

    Ok(())
}

/// Execute the rollback command.
pub async fn rollback(
    config: &ClientConfig,
    db: &ClientDb,
    name: &str,
    to_version: Option<&str>,
    list: bool,
) -> anyhow::Result<()> {
    if list {
        // List available backups
        let rollbacks = list_available_rollbacks(db, name).await?;

        if rollbacks.is_empty() {
            println!("No backups available for '{}'.", name);
            println!("Backups are created automatically when updates are applied.");
            return Ok(());
        }

        println!("Available rollback versions for '{}':", name);
        println!();
        println!("{:<12} {:<20} {:<10} PATH", "VERSION", "CREATED", "SIZE");
        println!("{}", "-".repeat(70));

        for info in rollbacks {
            let size_str = format_size(info.size_bytes);
            // Truncate created_at to just the date/time portion
            let created = if info.created_at.len() > 19 {
                &info.created_at[..19]
            } else {
                &info.created_at
            };
            println!(
                "{:<12} {:<20} {:<10} {}",
                info.version_string, created, size_str, info.backup_path
            );
        }

        return Ok(());
    }

    // Perform the rollback
    perform_rollback(config, db, name, to_version).await
}

/// Format bytes as a human-readable size.
fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

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
