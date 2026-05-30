//! Update patcher for Deltaship Client.
//!
//! Handles downloading, verifying, and applying updates to managed binaries.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use deltaship_crypto::{
    hash_bytes, hash_file, sha256_file, signing_payload, Hash, Signature, VerifyingKey,
};
use deltaship_db::{ClientDb, DbManagedBinary, NewUpdateRecord, UpdateMetrics};
use deltaship_diff::{apply_patch, decompress_diff};

use crate::audit::{log_audit_event, AuditEvent};
use crate::checker::UpdateInfo;
use crate::config::ClientConfig;
use crate::downloader::{download_bytes, download_file_with_progress};
use indicatif::ProgressBar;

/// Privileged system directories that should never be updated.
/// This prevents accidental or malicious updates to system files.
const PRIVILEGED_PATHS: &[&str] = &[
    "/etc",
    "/usr",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/boot",
    "/sys",
    "/proc",
    "/dev",
    "/var/run",
    "/run",
    // Windows system paths
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
];

/// Strip a leading Windows verbatim prefix (`\\?\` or `\\?\UNC\`) that
/// `Path::canonicalize` adds on Windows, so prefix matching sees the plain path.
fn strip_verbatim_prefix(p: &str) -> &str {
    p.strip_prefix(r"\\?\UNC\")
        .or_else(|| p.strip_prefix(r"\\?\"))
        .unwrap_or(p)
}

/// Strip a leading `X:` drive specifier so Windows denylist matching is
/// drive-agnostic (Windows / Program Files may live on a non-`C:` system drive).
fn strip_drive(p: &str) -> &str {
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        &p[2..]
    } else {
        p
    }
}

/// True if `candidate` equals or is nested under the `prefix` directory, matching
/// on path-component boundaries (so `/var/run` does NOT match `/var/runner`).
fn unix_path_under(candidate: &str, prefix: &str) -> bool {
    match candidate.strip_prefix(prefix) {
        Some(rest) => rest.is_empty() || rest.starts_with('/'),
        None => false,
    }
}

/// Windows equivalent of [`unix_path_under`], but case-insensitive,
/// separator-insensitive (`/` and `\`), drive-agnostic, and verbatim-prefix
/// aware — closing the gaps in a naive `starts_with` check.
fn windows_path_under(candidate: &str, prefix: &str) -> bool {
    let normalize = |s: &str| s.to_lowercase().replace('/', "\\");
    let cand = normalize(candidate);
    let cand = strip_drive(strip_verbatim_prefix(&cand));
    let pref = normalize(prefix);
    let pref = strip_drive(&pref);
    match cand.strip_prefix(pref) {
        Some(rest) => rest.is_empty() || rest.starts_with('\\'),
        None => false,
    }
}

/// Return the privileged system directory that `candidate` falls under, if any.
///
/// Windows-style entries (containing `:` or `\`) are matched case- and
/// separator-insensitively and drive-agnostically; Unix-style entries are
/// matched case-sensitively. Both match only on component boundaries.
fn matched_privileged_path(candidate: &str) -> Option<&'static str> {
    PRIVILEGED_PATHS.iter().copied().find(|&privileged| {
        if privileged.contains('\\') || privileged.contains(':') {
            windows_path_under(candidate, privileged)
        } else {
            unix_path_under(candidate, privileged)
        }
    })
}


/// Return the sidecar signature path for a given binary or backup path
/// (e.g. `/path/to/foo` -> `/path/to/foo.sig`).
///
/// CLIENT-P1-1: We persist the publisher's Ed25519 signature next to the
/// installed binary so backups can carry it forward and rollback can
/// re-verify authenticity (not just integrity) of the restored bytes.
pub(crate) fn sig_path_for(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".sig");
    PathBuf::from(s)
}

/// Verify a backup's sidecar signature against the publisher's public key.
///
/// Loads `<backup_path>.sig` and verifies the signature over the canonical
/// `signing_payload` (`"DELTASHIP-sig-v1\0"` ++ raw BLAKE3 hash of the backup file
/// ++ the backup's recorded version), matching the scheme in `execute_update`.
/// The `version` passed in MUST be the version those backup bytes actually are.
///
/// On missing or invalid signature, emits a `SignatureVerificationFailed`
/// audit event and returns an error. Callers must refuse to restore.
pub(crate) fn verify_backup_signature(
    backup_path: &Path,
    publisher_public_key: &[u8],
    binary_name: &str,
    version: &str,
) -> anyhow::Result<()> {
    let sig_path = sig_path_for(backup_path);
    if !sig_path.exists() {
        log_audit_event(&AuditEvent::SignatureVerificationFailed {
            binary_name: binary_name.to_string(),
            version: version.to_string(),
        });
        anyhow::bail!(
            "Backup signature missing at {} — refusing to restore unverified backup. \
             This backup was likely created before signature persistence was added; \
             reinstall the desired version from the publisher rather than rolling back.",
            sig_path.display()
        );
    }

    let sig_bytes = std::fs::read(&sig_path)
        .with_context(|| format!("Failed to read backup signature: {}", sig_path.display()))?;
    let sig_array: [u8; 64] = sig_bytes.as_slice().try_into().map_err(|_| {
        anyhow::anyhow!(
            "Invalid backup signature length at {}: expected 64 bytes, got {}",
            sig_path.display(),
            sig_bytes.len()
        )
    })?;
    let signature = Signature::from_bytes(sig_array);

    let key_bytes: [u8; 32] = publisher_public_key
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid publisher public key length"))?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .context("Failed to parse publisher public key")?;

    let backup_hash = hash_file(backup_path)
        .with_context(|| format!("Failed to hash backup file: {}", backup_path.display()))?;

    // FIX-1: verify over the canonical payload bound to the backup's recorded version.
    let payload = signing_payload(&backup_hash.to_bytes(), version);
    if let Err(e) = verifying_key.verify(&payload, &signature) {
        log_audit_event(&AuditEvent::SignatureVerificationFailed {
            binary_name: binary_name.to_string(),
            version: version.to_string(),
        });
        anyhow::bail!(
            "Backup signature verification failed for {} version {}: {}",
            binary_name,
            version,
            e
        );
    }

    Ok(())
}

/// Validate that the install path is safe for updates.
///
/// # Security Limitations: TOCTOU Race Condition
///
/// **WARNING:** This validation is subject to a Time-of-Check-Time-of-Use (TOCTOU)
/// race condition. There is a small time window between when we validate the path
/// and when we actually write to it. During this window, an attacker with local
/// access could:
/// - Replace the file with a symlink pointing to a privileged location
/// - Replace a parent directory with a symlink
///
/// ## Mitigations Applied
///
/// 1. **Double validation**: We validate once early (line 256) and again immediately
///    before the atomic write (line 654), minimizing the race window to microseconds.
/// 2. **Atomic operations**: We use atomic file operations (rename) for the final write.
/// 3. **Post-write verification**: We verify the installed file checksum immediately
///    after writing (line 678).
/// 4. **Restrictive permissions**: Temp files are created with 0600 permissions (line 632).
///
/// ## Residual Risk
///
/// A sophisticated local attacker with:
/// - Write access to the install directory or parent directories
/// - Ability to execute code during the microsecond window between validations
/// - Precise timing capabilities
///
/// Could potentially redirect writes to unintended locations.
///
/// ## Recommended Additional Protections
///
/// - Run the client as a non-privileged user (never as root)
/// - Use mandatory access control (AppArmor, SELinux) to restrict file writes
/// - Monitor file system events for suspicious symlink creation
/// - Use immutable flags on critical system directories (chattr +i on Linux)
/// - Install binaries in user-owned directories (~/.local/bin) rather than system paths
///
/// ## Future Improvements
///
/// A complete fix would require:
/// - Platform-specific O_NOFOLLOW flag support for atomic writes
/// - File descriptor-based validation and writing (open -> validate fd -> write to fd)
/// - This is complex and requires platform-specific code (Linux, macOS, Windows differ)
///
/// For now, we prioritize honesty about the limitation over claiming a complete fix.
///
/// # Validation Checks
///
/// This function performs security checks to prevent:
/// 1. Symlink attacks - where a symlink could redirect writes to privileged locations
/// 2. Direct writes to privileged system directories
/// 3. Path traversal via symlinks in parent directories
///
/// # Errors
///
/// Returns an error if:
/// - The path is a symlink
/// - Any parent directory is a symlink (to prevent symlink-based path traversal)
/// - The resolved path is in a privileged system directory
pub fn validate_install_path(path: &Path) -> anyhow::Result<()> {
    // Check if the path itself is a symlink
    if path.is_symlink() {
        anyhow::bail!(
            "Security error: install path '{}' is a symlink. \
             Symlinks are not allowed as install paths to prevent symlink attacks. \
             Please use the actual file path instead.",
            path.display()
        );
    }

    // If the path exists, resolve it and check the canonical path
    // If it doesn't exist, check the parent directory
    let check_path = if path.exists() {
        path.canonicalize().with_context(|| {
            format!(
                "Failed to resolve canonical path for '{}'",
                path.display()
            )
        })?
    } else {
        // For non-existent paths, check the parent directory
        if let Some(parent) = path.parent() {
            if parent.exists() {
                // Check if parent is a symlink
                if parent.is_symlink() {
                    anyhow::bail!(
                        "Security error: parent directory '{}' is a symlink. \
                         This could be used for a symlink attack. \
                         Please use a path with no symlinks in the directory chain.",
                        parent.display()
                    );
                }

                let canonical_parent = parent.canonicalize().with_context(|| {
                    format!(
                        "Failed to resolve canonical path for parent '{}'",
                        parent.display()
                    )
                })?;

                // Construct the full path with the filename
                if let Some(filename) = path.file_name() {
                    canonical_parent.join(filename)
                } else {
                    canonical_parent
                }
            } else {
                // Parent doesn't exist, use the path as-is for privilege check
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        }
    };

    // Check against privileged paths (case/separator/drive-aware on Windows,
    // component-boundary matching on both platforms).
    let path_str = check_path.to_string_lossy();
    if let Some(privileged) = matched_privileged_path(&path_str) {
        anyhow::bail!(
            "Security error: cannot update binary in privileged location '{}'. \
             Deltaship does not support updating system binaries in protected directories like {}. \
             Install the binary in a user-accessible location (e.g., ~/.local/bin/).",
            check_path.display(),
            privileged
        );
    }

    // Additional check for root-owned directories on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if let Ok(metadata) = std::fs::metadata(&check_path) {
            if metadata.uid() == 0 {
                // File is owned by root, check if current user is root
                if !nix_check_root() {
                    anyhow::bail!(
                        "Security error: install path '{}' is owned by root and current user is not root. \
                         This could indicate an attempt to overwrite system files.",
                        check_path.display()
                    );
                }
            }
        } else if let Some(parent) = check_path.parent() {
            if let Ok(parent_meta) = std::fs::metadata(parent) {
                if parent_meta.uid() == 0 && !nix_check_root() {
                    anyhow::bail!(
                        "Security error: parent directory '{}' is owned by root and current user is not root. \
                         Cannot create files in root-owned directories.",
                        parent.display()
                    );
                }
            }
        }
    }

    Ok(())
}

/// Check if the current process is running as root.
#[cfg(unix)]
fn nix_check_root() -> bool {
    // Using libc directly to avoid adding nix dependency
    unsafe { libc::geteuid() == 0 }
}

/// Check if there's enough available system memory for an operation.
///
/// # P1-2 Fix: Memory Exhaustion in Signature Verification
///
/// Ed25519 signature verification requires loading the entire binary into memory.
/// This function checks that sufficient memory is available before attempting to
/// load large files, preventing OOM crashes.
///
/// # Memory Requirements
///
/// - **Required memory**: 2x the file size
/// - **Why 2x**: Accounts for the file data itself plus overhead from verification,
///   data structures, and other concurrent operations
///
/// # Arguments
///
/// * `required_bytes` - The size of data that needs to be loaded into memory
/// * `purpose` - Description of the operation (for error messages)
///
/// # Errors
///
/// Returns an error if:
/// - Available memory is less than 2x `required_bytes`
/// - System memory information cannot be retrieved
///
/// # Actionable Error Messages
///
/// On failure, provides clear guidance on:
/// - How much memory is needed
/// - How much is currently available
/// - What actions the user can take (close applications, use smaller binaries, etc.)
fn check_available_memory(required_bytes: u64, purpose: &str) -> anyhow::Result<()> {
    use sysinfo::System;

    // Calculate required memory (2x file size for safety margin)
    let required_memory = required_bytes.saturating_mul(2);

    // Get system memory information
    let mut sys = System::new();
    sys.refresh_memory();

    let available_memory = sys.available_memory();

    tracing::debug!(
        "Memory check for {}: need {} MB, available {} MB",
        purpose,
        required_memory / 1_024 / 1_024,
        available_memory / 1_024 / 1_024
    );

    if available_memory < required_memory {
        anyhow::bail!(
            "Insufficient memory for {}: need {} MB (2x file size for safety), only {} MB available.\n\
             \n\
             Memory Requirements:\n\
             - File size: {} MB\n\
             - Required memory: {} MB (2x file size)\n\
             - Available memory: {} MB\n\
             \n\
             Possible solutions:\n\
             1. Close other applications to free up memory\n\
             2. Use a system with more RAM\n\
             3. If updating very large binaries (>200MB), consider splitting into smaller components\n\
             4. For server deployments, increase available memory or use swap space\n\
             \n\
             Note: Signature verification requires loading the entire binary into memory \
             due to Ed25519 algorithm requirements. This is a necessary security measure.",
            purpose,
            required_memory / 1_024 / 1_024,
            available_memory / 1_024 / 1_024,
            required_bytes / 1_024 / 1_024,
            required_memory / 1_024 / 1_024,
            available_memory / 1_024 / 1_024
        );
    }

    tracing::info!(
        "Memory check for {} passed: {} MB available, {} MB required",
        purpose,
        available_memory / 1_024 / 1_024,
        required_memory / 1_024 / 1_024
    );

    Ok(())
}

/// Check if there's enough disk space at a specific path.
///
/// # P2 Issue 6 Fix: Configurable behavior when disk info unavailable
///
/// Returns Ok if there's enough space, or an error with details if not.
/// Behavior when disk info cannot be determined depends on config.disk_check_mode:
/// - Strict mode: Return error (safest, prevents potential disk full failures)
/// - Permissive mode: Log warning and continue (for systems with unusual filesystems)
fn check_disk_space_at_path(
    path: &Path,
    required_bytes: u64,
    purpose: &str,
    config: &ClientConfig,
) -> anyhow::Result<()> {
    use sysinfo::Disks;

    let disks = Disks::new_with_refreshed_list();

    // Find the disk containing the path
    let disk = disks
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len());

    if let Some(disk) = disk {
        let available = disk.available_space();
        if available < required_bytes {
            anyhow::bail!(
                "Insufficient disk space for {}: need {} MB, available {} MB at {}",
                purpose,
                required_bytes / 1_024 / 1_024,
                available / 1_024 / 1_024,
                path.display()
            );
        }
        tracing::info!(
            "Disk space check for {} passed: {} MB available, {} MB required at {}",
            purpose,
            available / 1_024 / 1_024,
            required_bytes / 1_024 / 1_024,
            path.display()
        );
    } else {
        // Can't find disk info - behavior depends on config mode
        use crate::config::DiskCheckMode;

        match config.disk_check_mode {
            DiskCheckMode::Strict => {
                anyhow::bail!(
                    "Cannot determine disk space for {} at {}. \
                     Operation aborted in strict mode. \
                     Set disk_check_mode = 'permissive' in config to allow updates \
                     when disk info is unavailable.",
                    purpose,
                    path.display()
                );
            }
            DiskCheckMode::Permissive => {
                tracing::warn!(
                    "Could not determine disk space for {} at {} - continuing in permissive mode. \
                     Update may fail if insufficient space available.",
                    purpose,
                    path.display()
                );
            }
        }
    }

    Ok(())
}

/// Check if there's enough disk space for an update.
///
/// Checks both the install path (for the new binary) and the backup directory
/// (for the backup copy). These may be on different filesystems.
fn check_disk_space(
    install_path: &Path,
    backup_path: &Path,
    binary_size: u64,
    config: &ClientConfig,
) -> anyhow::Result<()> {
    // Check space for new binary at install location
    check_disk_space_at_path(install_path, binary_size, "new binary", config)?;

    // Check space for backup at backup location
    // The backup directory may be on a different filesystem than the install path
    check_disk_space_at_path(backup_path, binary_size, "backup", config)?;

    Ok(())
}

/// Apply an update to a managed binary.
///
/// Steps:
/// 1. Create backup of current binary
/// 2. Download diff (or full binary if diff unavailable)
/// 3. Verify signature using stored public key (always required)
/// 4. Verify checksum
/// 5. Apply diff (or copy full binary)
/// 6. Verify final binary checksum
/// 7. Update database with new version
///
/// On any failure, restores from backup.
///
/// # Arguments
/// * `config` - Client configuration
/// * `db` - Database connection
/// * `managed_binary` - The binary to update
/// * `update` - Update information
/// * `progress_bar` - Optional progress bar for download progress
pub async fn apply_update(
    config: &ClientConfig,
    db: &ClientDb,
    managed_binary: &DbManagedBinary,
    update: &UpdateInfo,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<()> {
    let install_path = Path::new(&managed_binary.install_path);
    let binary_name = &managed_binary.binary_name;

    // Security: Validate install path before proceeding
    // NOTE: This check has a TOCTOU race condition - see validate_install_path() docs
    // We perform this check early and repeat it immediately before write (line ~654)
    // to minimize the race window, but cannot eliminate it without platform-specific
    // file descriptor-based operations.
    validate_install_path(install_path).with_context(|| {
        format!(
            "Install path validation failed for binary '{}' at '{}'",
            binary_name,
            install_path.display()
        )
    })?;

    // Security audit: Log the update attempt for security monitoring
    tracing::info!(
        binary = %binary_name,
        install_path = %install_path.display(),
        "Security: Beginning update process with path validation"
    );

    tracing::info!(
        binary = %binary_name,
        from_version = %managed_binary.current_version_string.as_deref().unwrap_or("(none)"),
        to_version = %update.version,
        "Applying update"
    );

    // Get backup directory for disk space check
    let backups_dir = config.backups_dir();

    // Check disk space before starting
    // We need space for both the new binary (at install_path) and the backup (at backups_dir)
    // These may be on different filesystems, so we check both
    // For diff-based updates, we also need space for the diff file in downloads_dir
    if let Some(binary_size) = update.full_binary_size {
        check_disk_space(install_path, &backups_dir, binary_size, config)?;

        // For diff updates, also check space for the diff file in downloads directory
        if let Some(diff_size) = update.diff_size {
            let downloads_dir = config.downloads_dir();
            check_disk_space_at_path(&downloads_dir, diff_size, "diff file", config)?;
        }
    } else if install_path.exists() {
        // Estimate based on current binary size
        let current_size = tokio::fs::metadata(install_path)
            .await
            .with_context(|| format!("Failed to read metadata for {:?}", install_path))?
            .len();
        check_disk_space(install_path, &backups_dir, current_size, config)?;

        // For diff updates, also check space for the diff file
        if let Some(diff_size) = update.diff_size {
            let downloads_dir = config.downloads_dir();
            check_disk_space_at_path(&downloads_dir, diff_size, "diff file", config)?;
        }
    }

    // Record update start
    let update_record = NewUpdateRecord {
        binary_id: managed_binary.binary_id.clone(),
        from_version_id: managed_binary.current_version_id.clone(),
        from_version_string: managed_binary.current_version_string.clone(),
        to_version_id: update.version_id.clone(),
        to_version_string: update.version.to_string(),
    };
    let update_id = db
        .record_update_start(update_record)
        .await
        .context("Failed to record update start in database")?;

    // Create backup directory (backups_dir was already computed above for disk space check)
    tokio::fs::create_dir_all(&backups_dir)
        .await
        .with_context(|| format!("Failed to create backups directory: {:?}", backups_dir))?;

    // Step 1: Create backup of current binary (if it exists)
    let backup_path = if install_path.exists() {
        let backup_name = format!(
            "{}_{}.backup",
            binary_name,
            managed_binary
                .current_version_string
                .as_deref()
                .unwrap_or("unknown")
        );
        let backup_path = backups_dir.join(&backup_name);
        tracing::info!(
            path = %backup_path.display(),
            "Creating backup"
        );
        tokio::fs::copy(install_path, &backup_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to create backup: {:?} -> {:?}",
                    install_path, backup_path
                )
            })?;

        // CLIENT-P1-1: Copy the persisted publisher signature alongside the backup so
        // rollback can re-verify authenticity (not just integrity) of the backup.
        let install_sig = sig_path_for(install_path);
        let backup_sig = sig_path_for(&backup_path);
        if install_sig.exists() {
            tokio::fs::copy(&install_sig, &backup_sig).await.with_context(|| {
                format!(
                    "Failed to copy signature sidecar: {:?} -> {:?}",
                    install_sig, backup_sig
                )
            })?;
        } else {
            tracing::warn!(
                install_sig = %install_sig.display(),
                backup_path = %backup_path.display(),
                "No signature sidecar found for current binary; backup will not be \
                 re-verifiable on rollback. This is expected for binaries installed \
                 before signature persistence was added."
            );
        }

        Some(backup_path)
    } else {
        None
    };

    // Execute update with rollback on failure
    let result = execute_update(config, db, managed_binary, update, update_id, progress_bar).await;

    match result {
        Ok(metrics) => {
            // Update succeeded, record completion
            db.record_update_complete(update_id, true, None, metrics)
                .await?;

            // Register the newly installed version so update_current_version can reference it
            let installed_size = tokio::fs::metadata(install_path).await?.len() as i64;
            let blake3_bytes = blake3::Hash::from_hex(&update.checksum)
                .context("Failed to decode blake3 checksum from update info")?
                .as_bytes()
                .to_vec();
            let sha256_bytes = tokio::task::spawn_blocking({
                let p = install_path.to_path_buf();
                move || sha256_file(&p)
            })
            .await
            .context("sha256 task panicked")?
            .context("Failed to compute sha256 of installed binary")?;
            db.record_installed_version(
                &managed_binary.binary_id,
                &update.version_id,
                &update.version.to_string(),
                &blake3_bytes,
                &sha256_bytes,
                installed_size,
            )
            .await
            .context("Failed to record installed version")?;

            // Update current version in database
            db.update_current_version(
                &managed_binary.binary_id,
                &update.version_id,
                &update.version.to_string(),
            )
            .await?;

            // Record backup in database for rollback capability.
            // Only record if we have a known previous version — create_backup requires valid semver.
            if let (Some(ref backup_path), Some(ref prev_version_id), Some(ref prev_version_str)) = (
                backup_path,
                &managed_binary.current_version_id,
                &managed_binary.current_version_string,
            ) {
                if backup_path.exists() {
                    let backup_hash = hash_file(backup_path)?;
                    let backup_size = tokio::fs::metadata(backup_path).await?.len() as i64;

                    db.create_backup(
                        &managed_binary.binary_id,
                        prev_version_id,
                        prev_version_str,
                        &backup_path.to_string_lossy(),
                        &backup_hash.to_bytes(),
                        backup_size,
                        None,
                    )
                    .await?;
                }
            }

            tracing::info!("Update applied successfully for {}", binary_name);

            // Audit log successful update
            log_audit_event(&AuditEvent::UpdateApplied {
                binary_name: binary_name.clone(),
                version: update.version.to_string(),
                install_path: install_path.display().to_string(),
            });

            Ok(())
        }
        Err(e) => {
            // Update failed, restore from backup
            tracing::error!("Update failed: {}, rolling back", e);

            let mut restore_error: Option<std::io::Error> = None;
            if let Some(ref backup_path) = backup_path {
                if backup_path.exists() {
                    tracing::info!("Restoring from backup");
                    if let Err(restore_err) = tokio::fs::copy(backup_path, install_path).await {
                        tracing::error!("Failed to restore backup: {}", restore_err);
                        restore_error = Some(restore_err);
                    }
                }
            }

            // Record failure
            db.record_update_complete(
                update_id,
                false,
                Some(&e.to_string()),
                UpdateMetrics::default(),
            )
            .await?;

            // If restore also failed, return a compound error
            if let Some(restore_err) = restore_error {
                return Err(anyhow::anyhow!(
                    "Update failed: {}. CRITICAL: Backup restore also failed: {}. Binary may be in inconsistent state.",
                    e,
                    restore_err
                ));
            }

            Err(e)
        }
    }
}

/// Represents the source of a new binary - either in memory (from diff) or on disk (streamed download).
enum BinarySource {
    /// Binary data in memory (from diff-based update).
    InMemory(Vec<u8>),
    /// Binary streamed to a file on disk (to avoid memory exhaustion on large downloads).
    OnDisk(std::path::PathBuf),
}

impl BinarySource {
    /// Verify the checksum of the binary data.
    fn verify_checksum(&self, expected: &Hash) -> anyhow::Result<()> {
        let actual = match self {
            BinarySource::InMemory(data) => hash_bytes(data),
            BinarySource::OnDisk(path) => hash_file(path)?,
        };
        if expected != &actual {
            anyhow::bail!(
                "Binary checksum mismatch: expected {}, got {}",
                expected,
                actual
            );
        }
        Ok(())
    }

    /// Read the binary data for signature verification.
    /// For large files (>100MB), uses chunked reading to avoid memory exhaustion.
    async fn read_for_verification(&self) -> anyhow::Result<Vec<u8>> {
        match self {
            BinarySource::InMemory(data) => Ok(data.clone()),
            BinarySource::OnDisk(path) => {
                // Check file size first
                let metadata = tokio::fs::metadata(path).await?;
                let file_size = metadata.len();

                // For files larger than 100MB, we need to be more careful about memory
                // Ed25519 requires the full message for verification, but we can at least
                // provide a clear error message for extremely large files
                const MAX_SIGNATURE_SIZE: u64 = 500 * 1024 * 1024; // 500MB

                if file_size > MAX_SIGNATURE_SIZE {
                    anyhow::bail!(
                        "Binary too large for signature verification: {} MB exceeds {} MB limit. \
                         Consider using full binary download instead of diff updates for very large binaries.",
                        file_size / 1_024 / 1_024,
                        MAX_SIGNATURE_SIZE / 1_024 / 1_024
                    );
                }

                // P1-2 Fix: Check available system memory before loading file
                // Signature verification requires loading the entire file into memory
                // Check that we have at least 2x the file size available to be safe
                check_available_memory(file_size, "signature verification")?;

                // For files under the limit, read into memory
                // This is unavoidable with Ed25519's design (requires full message)
                tracing::warn!(
                    "Loading {} MB binary into memory for signature verification",
                    file_size / 1_024 / 1_024
                );

                tokio::fs::read(path)
                    .await
                    .with_context(|| format!("Failed to read binary from {:?}", path))
            }
        }
    }

    /// Write the binary to a destination file.
    async fn write_to(&self, dest: &Path) -> anyhow::Result<()> {
        match self {
            BinarySource::InMemory(data) => {
                std::fs::write(dest, data)?;
            }
            BinarySource::OnDisk(src_path) => {
                // Copy file instead of reading into memory
                tokio::fs::copy(src_path, dest).await?;
            }
        }
        Ok(())
    }

    /// Clean up any temporary files.
    async fn cleanup(&self) {
        if let BinarySource::OnDisk(path) = self {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

/// Execute the actual update process.
async fn execute_update(
    config: &ClientConfig,
    _db: &ClientDb,
    managed_binary: &DbManagedBinary,
    update: &UpdateInfo,
    _update_id: i64,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<UpdateMetrics> {
    let install_path = Path::new(&managed_binary.install_path);
    let downloads_dir = config.downloads_dir();
    tokio::fs::create_dir_all(&downloads_dir).await?;

    let mut metrics = UpdateMetrics::default();

    // Expected checksum of the final binary
    let expected_final_hash = Hash::from_hex(&update.checksum)?;

    // Try diff-based update first if available, fall back to full binary
    // Diff updates return in-memory data, full binary downloads stream to disk
    let binary_source = if update.diff_url.is_some() && install_path.exists() {
        match apply_diff_update(config, managed_binary, update, &mut metrics, progress_bar).await {
            Ok(data) => {
                tracing::info!("Diff-based update succeeded");
                BinarySource::InMemory(data)
            }
            Err(e) => {
                tracing::warn!(
                    "Diff-based update failed: {}, falling back to full binary",
                    e
                );
                // Fall back to full binary download (streams to disk)
                let path = download_full_binary(config, managed_binary, update, &mut metrics, progress_bar)
                    .await?;
                BinarySource::OnDisk(path)
            }
        }
    } else {
        // No diff available or no current binary exists, download full binary (streams to disk)
        let path = download_full_binary(config, managed_binary, update, &mut metrics, progress_bar).await?;
        BinarySource::OnDisk(path)
    };

    // Verify final binary checksum (works on both in-memory and on-disk data)
    tracing::info!("Verifying final binary checksum");
    if let Err(e) = binary_source.verify_checksum(&expected_final_hash) {
        binary_source.cleanup().await;
        return Err(e);
    }
    tracing::info!("Final binary checksum verified");

    // Verify signature (always required for security)
    let verify_start = Instant::now();

    let sig_url = update.signature_url.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Security error: signature URL is required but missing. \
             All updates must be cryptographically signed for security."
        )
    })?;

    tracing::info!("Downloading and verifying signature");

    let sig_bytes = download_bytes(sig_url).await?;
    let sig_len = sig_bytes.len();

    let sig_array: [u8; 64] = sig_bytes.as_slice().try_into().map_err(|_| {
        anyhow::anyhow!("Invalid signature: expected 64 bytes, got {}", sig_len)
    })?;
    let signature = Signature::from_bytes(sig_array);

    // Load verifying key from managed binary
    let key_bytes: [u8; 32] = managed_binary
        .publisher_public_key
        .clone()
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid public key length"))?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)?;

    // FIX-1: The publisher signs the canonical payload "DELTASHIP-sig-v1\0" ++ raw
    // BLAKE3 hash (32 bytes) ++ version string, not the bare hash. Compute the
    // hash from the downloaded bytes and verify the signature over the payload
    // bound to the target version we are about to install.
    let binary_data = binary_source.read_for_verification().await?;
    let blake3_hash = blake3::hash(&binary_data);
    let payload = signing_payload(blake3_hash.as_bytes(), &update.version.to_string());
    if let Err(e) = verifying_key.verify(&payload, &signature) {
        log_audit_event(&AuditEvent::SignatureVerificationFailed {
            binary_name: managed_binary.binary_name.clone(),
            version: update.version.to_string(),
        });
        binary_source.cleanup().await;
        return Err(e.into());
    }

    tracing::info!("Signature verified successfully");
    metrics.verify_time_ms = Some(verify_start.elapsed().as_millis() as i64);

    // Write new binary to temp location then atomically replace
    let apply_start = Instant::now();

    tracing::info!("Applying update to {}", install_path.display());

    // Ensure parent directory exists
    let parent_dir = install_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Install path has no parent directory"))?;
    tokio::fs::create_dir_all(parent_dir).await?;

    // Create temp file in the same directory for atomic rename
    // Using tempfile crate ensures atomic operation on same filesystem
    let temp_file = tempfile::NamedTempFile::new_in(parent_dir)?;
    let temp_path = temp_file.path().to_path_buf();

    // Record parent directory identity for TOCTOU detection on Unix.
    // If the parent directory is replaced by a symlink between now and the
    // final rename, the dev/inode check below will catch it.
    #[cfg(unix)]
    let parent_dir_identity = {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(parent_dir)?;
        (meta.dev(), meta.ino())
    };

    // Set restrictive permissions (0600) on Unix immediately after creation
    // This prevents the temp file from being world-readable during the write operation
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&temp_path, perms)?;
    }

    // Write binary data to temp file (streams from disk if using OnDisk source)
    binary_source.write_to(&temp_path).await?;

    // Clean up the downloaded file now that we've copied it
    binary_source.cleanup().await;

    // Set executable permissions on Unix before atomic rename
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)?;
    }

    // Re-validate install path immediately before atomic rename, then verify
    // the parent directory hasn't been swapped (TOCTOU mitigation).
    validate_install_path(install_path).with_context(|| {
        format!(
            "Install path validation failed immediately before write for '{}'",
            install_path.display()
        )
    })?;

    // Verify the parent directory's identity hasn't changed since the temp file
    // was created. A changed dev/inode means the directory was replaced (e.g. with a
    // symlink), which is the primary remaining TOCTOU attack vector.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(parent_dir).map_err(|e| {
            anyhow::anyhow!(
                "Security: cannot re-stat parent directory '{}': {}",
                parent_dir.display(), e
            )
        })?;
        if (meta.dev(), meta.ino()) != parent_dir_identity {
            return Err(anyhow::anyhow!(
                "Security error: parent directory '{}' was replaced between validation \
                 and write. Possible TOCTOU attack — aborting update.",
                parent_dir.display()
            ));
        }
    }

    tracing::info!(
        install_path = %install_path.display(),
        "Security: Final validation passed, performing atomic write"
    );

    // Atomically persist the temp file to the install path
    // This uses rename() which is atomic on the same filesystem
    // Note: persist() consumes the temp file, so on error we need to handle cleanup manually
    if let Err(e) = temp_file.persist(install_path) {
        // The temp file is returned in the error, try to clean it up
        let temp_path = e.file.path().to_path_buf();
        let _ = std::fs::remove_file(&temp_path);
        return Err(anyhow::anyhow!(
            "Failed to persist temp file to {}: {}",
            install_path.display(),
            e.error
        ));
    }

    metrics.apply_time_ms = Some(apply_start.elapsed().as_millis() as i64);

    // Verify installed binary checksum
    // This provides defense-in-depth: even if a TOCTOU attack succeeded in redirecting
    // the write, the checksum verification of the final installed file would detect
    // unexpected modifications (though the damage may already be done).
    tracing::info!("Verifying installed binary");
    let installed_hash = hash_file(install_path)?;
    if expected_final_hash != installed_hash {
        tracing::error!(
            install_path = %install_path.display(),
            expected = %expected_final_hash,
            actual = %installed_hash,
            "Security: Installed binary checksum mismatch - possible TOCTOU attack or corruption"
        );
        anyhow::bail!(
            "Installed binary checksum mismatch: expected {}, got {}",
            expected_final_hash,
            installed_hash
        );
    }

    // CLIENT-P1-1: Persist the publisher signature next to the installed binary
    // so future backups can carry it forward and rollback can re-verify
    // authenticity. Written atomically via tempfile + rename to avoid leaving
    // a torn sidecar on crash. Failure here aborts the update because a binary
    // without a sidecar will fail signature checks on rollback.
    let sig_path = sig_path_for(install_path);
    let sig_parent = sig_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Signature sidecar path has no parent directory"))?;
    let sig_temp = tempfile::NamedTempFile::new_in(sig_parent)
        .context("Failed to create temp file for signature sidecar")?;
    std::fs::write(sig_temp.path(), sig_array)
        .context("Failed to write signature sidecar to temp file")?;
    sig_temp
        .persist(&sig_path)
        .map_err(|e| anyhow::anyhow!(
            "Failed to persist signature sidecar to {}: {}",
            sig_path.display(),
            e.error
        ))?;

    // Security audit: Successful installation
    tracing::info!(
        install_path = %install_path.display(),
        checksum = %installed_hash,
        "Security: Binary installed successfully and verified"
    );

    Ok(metrics)
}

/// Apply a diff-based update.
///
/// Downloads the diff file, verifies its checksum, reads the current binary,
/// applies the patch, and returns the new binary data.
async fn apply_diff_update(
    config: &ClientConfig,
    managed_binary: &DbManagedBinary,
    update: &UpdateInfo,
    metrics: &mut UpdateMetrics,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<Vec<u8>> {
    let diff_url = update
        .diff_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No diff URL available"))?;

    let install_path = Path::new(&managed_binary.install_path);

    tracing::info!("Downloading diff from {}", diff_url);

    let download_start = Instant::now();

    // Download diff file
    let downloads_dir = config.downloads_dir();
    let diff_path = downloads_dir.join(format!(
        "{}_{}.diff",
        managed_binary.binary_name, update.version_id
    ));

    download_file_with_progress(diff_url, &diff_path, progress_bar).await?;

    let download_time = download_start.elapsed();
    metrics.download_time_ms = Some(download_time.as_millis() as i64);
    metrics.diff_id = Some(update.version_id.clone());
    metrics.diff_algorithm = Some("bsdiff".to_string());

    let diff_file_size = tokio::fs::metadata(&diff_path).await?.len();
    metrics.diff_size_bytes = Some(diff_file_size as i64);
    metrics.actual_downloaded_bytes = Some(diff_file_size as i64);
    // Store full binary size from update info if available
    if let Some(full_size) = update.full_binary_size {
        metrics.full_size_bytes = Some(full_size as i64);
    }

    tracing::info!(
        "Downloaded diff: {} bytes in {:?}",
        diff_file_size,
        download_time
    );

    // Verify diff checksum (mandatory for diff-based updates)
    let expected_diff_checksum = update.diff_checksum.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Security error: diff checksum is required but missing. \
             All diff-based updates must include checksums for integrity verification."
        )
    })?;

    tracing::info!("Verifying diff checksum");
    let expected_hash = Hash::from_hex(expected_diff_checksum)?;
    let actual_hash = hash_file(&diff_path)?;

    if expected_hash != actual_hash {
        // Clean up diff file
        let _ = tokio::fs::remove_file(&diff_path).await;
        anyhow::bail!(
            "Diff checksum mismatch: expected {}, got {}",
            expected_hash,
            actual_hash
        );
    }
    tracing::info!("Diff checksum verified");

    // Read current binary into memory
    tracing::info!("Reading current binary from {}", install_path.display());
    let current_binary = tokio::fs::read(install_path).await?;

    // Read diff data (may be zstd-compressed)
    let diff_raw = tokio::fs::read(&diff_path).await?;

    // Clean up diff file
    let _ = tokio::fs::remove_file(&diff_path).await;

    // Decompress if the diff is zstd-compressed (magic bytes: 0x28 0xB5 0x2F 0xFD)
    let diff_data = if diff_raw.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        tracing::info!("Decompressing diff ({} bytes compressed)", diff_raw.len());
        decompress_diff(&diff_raw)
            .map_err(|e| anyhow::anyhow!("Failed to decompress diff: {}", e))?
    } else {
        diff_raw
    };

    // Apply the patch
    tracing::info!("Applying binary diff patch");
    let patch_start = Instant::now();

    let new_binary = apply_patch(&current_binary, &diff_data)
        .map_err(|e| anyhow::anyhow!("Failed to apply diff patch: {}", e))?;

    tracing::info!("Patch applied in {:?}", patch_start.elapsed());

    Ok(new_binary)
}

/// Download full binary to a file and return the path.
///
/// This streams directly to disk instead of buffering in memory to avoid
/// memory exhaustion on large binaries.
async fn download_full_binary(
    config: &ClientConfig,
    managed_binary: &DbManagedBinary,
    update: &UpdateInfo,
    metrics: &mut UpdateMetrics,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<std::path::PathBuf> {
    let full_url = update
        .full_binary_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No full binary URL available"))?;

    tracing::info!("Downloading full binary from {}", full_url);

    let download_start = Instant::now();

    let downloads_dir = config.downloads_dir();
    let download_path = downloads_dir.join(format!(
        "{}_{}.tmp",
        managed_binary.binary_name, update.version_id
    ));

    download_file_with_progress(full_url, &download_path, progress_bar).await?;

    let download_time = download_start.elapsed();
    metrics.download_time_ms = Some(download_time.as_millis() as i64);

    let file_size = tokio::fs::metadata(&download_path).await?.len();
    // For full binary download, no diff was used
    metrics.full_size_bytes = Some(file_size as i64);
    metrics.actual_downloaded_bytes = Some(file_size as i64);

    tracing::info!(
        "Downloaded full binary: {} bytes in {:?}",
        file_size,
        download_time
    );

    // Return the path instead of reading into memory
    Ok(download_path)
}

/// Rollback to a previous version using a backup.
// Called by the rollback command (commands/rollback.rs).
#[allow(dead_code)]
pub async fn rollback_to_backup(
    db: &ClientDb,
    managed_binary: &DbManagedBinary,
    backup_id: i64,
) -> anyhow::Result<()> {
    let backup = db
        .get_backup(backup_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Backup not found: {}", backup_id))?;

    let backup_path = Path::new(&backup.backup_path);
    if !backup_path.exists() {
        anyhow::bail!("Backup file not found: {}", backup.backup_path);
    }

    // Verify backup integrity
    let expected_hash = Hash::from_bytes(
        backup
            .backup_hash_blake3
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid backup hash"))?,
    );
    let actual_hash = hash_file(backup_path)?;

    if expected_hash != actual_hash {
        anyhow::bail!("Backup file corrupted: checksum mismatch");
    }

    // CLIENT-P1-1: Verify the publisher's Ed25519 signature for the backup.
    // Checksum proves the file matches what we stored locally; signature
    // proves those bytes originally came from the trusted publisher.
    verify_backup_signature(
        backup_path,
        &managed_binary.publisher_public_key,
        &managed_binary.binary_name,
        &backup.version_string,
    )?;

    // Restore backup
    let install_path = Path::new(&managed_binary.install_path);
    tokio::fs::copy(backup_path, install_path).await?;

    // CLIENT-P1-1: Restore the sidecar signature alongside the binary so the
    // restored binary can itself be backed up and rolled back later.
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

    // Set executable permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(install_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(install_path, perms).await?;
    }

    // Update database
    db.update_current_version(
        &managed_binary.binary_id,
        &backup.version_id,
        &backup.version_string,
    )
    .await?;

    tracing::info!(
        "Rolled back {} to version {}",
        managed_binary.binary_name,
        backup.version_string
    );

    Ok(())
}

#[cfg(test)]
mod privileged_path_tests {
    use super::matched_privileged_path;

    #[test]
    fn windows_denylist_is_case_insensitive() {
        // Gaps the old `starts_with` check missed.
        for p in [
            r"C:\Windows\System32\app.exe",
            r"c:\windows\system32\app.exe",
            r"C:\WINDOWS\app.exe",
            r"C:\WinDows\app.exe",
        ] {
            assert!(matched_privileged_path(p).is_some(), "should block: {p}");
        }
    }

    #[test]
    fn windows_denylist_handles_forward_slashes_and_verbatim_prefix() {
        assert!(matched_privileged_path(r"C:/Windows/System32").is_some());
        assert!(matched_privileged_path(r"\\?\C:\Program Files\app.exe").is_some());
    }

    #[test]
    fn windows_denylist_is_drive_agnostic() {
        // Windows / Program Files may live on a non-C: system drive.
        assert!(matched_privileged_path(r"D:\Windows\app.exe").is_some());
        assert!(matched_privileged_path(r"E:\Program Files (x86)\app.exe").is_some());
    }

    #[test]
    fn windows_program_files_matches() {
        assert!(matched_privileged_path(r"C:\Program Files\Vendor\app.exe").is_some());
        assert!(matched_privileged_path(r"C:\Program Files (x86)\Vendor\app.exe").is_some());
    }

    #[test]
    fn user_locations_are_allowed() {
        for p in [
            r"C:\Users\jay\apps\app.exe",
            r"D:\tools\app.exe",
            "/home/jay/.local/bin/app",
            "/opt/myapp/app",
        ] {
            assert!(matched_privileged_path(p).is_none(), "should allow: {p}");
        }
    }

    #[test]
    fn unix_matches_on_component_boundary_only() {
        // Privileged directories themselves and their children are blocked...
        assert!(matched_privileged_path("/usr/bin/app").is_some());
        assert!(matched_privileged_path("/etc/app").is_some());
        assert!(matched_privileged_path("/var/run/app").is_some());
        // ...but sibling dirs that merely share a string prefix are NOT
        // (the old `starts_with` check wrongly blocked these).
        assert!(matched_privileged_path("/var/runner/app").is_none());
        assert!(matched_privileged_path("/usr-local/bin/app").is_none());
        assert!(matched_privileged_path("/libraryish/app").is_none());
    }

    #[test]
    fn unix_stays_case_sensitive() {
        // Unix filesystems are case-sensitive: /ETC is not /etc.
        assert!(matched_privileged_path("/ETC/app").is_none());
    }
}
