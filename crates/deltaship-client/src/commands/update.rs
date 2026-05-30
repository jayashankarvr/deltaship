//! Update command - manually trigger updates for binaries.
//!
//! # P3 Issue #110 Fix: Known Limitation - Audit Logging
//!
//! **Status**: Not implemented (tracked as P3 Issue #91 in TODO.md)
//!
//! This module performs security-sensitive operations (applying binary updates) that
//! would benefit from audit logging. Consider integrating syslog for production deployments:
//!
//! - Log successful updates: binary name, old version, new version, timestamp
//! - Log failed updates: binary name, error reason, timestamp
//! - Log signature verification results
//!
//! **Workaround**: Currently, tracing logs provide some observability when a tracing subscriber
//! is configured, but these are not persistent audit logs.
//! - Log rollback operations
//!
//! Example syslog integration (using the `syslog` crate):
//! ```rust,ignore
//! use syslog::{Facility, Formatter3164};
//! let formatter = Formatter3164 { facility: Facility::LOG_DAEMON, ... };
//! syslog::unix(formatter)?.info("deltaship-client: Updated myapp 1.0.0 -> 1.1.0");
//! ```
//!
//! This would enable centralized security monitoring and compliance auditing.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tracing::{debug, error, info};
use deltaship_db::{ClientDb, DbManagedBinary};

use deltaship_core::Version;

use crate::checker::{UpdateChecker, UpdateInfo};
use crate::config::ClientConfig;
use crate::patcher::apply_update;

/// Result of applying an update.
enum UpdateResult {
    /// Update was applied successfully.
    Updated,
    /// Binary is already up to date.
    AlreadyCurrent,
    /// Update was skipped (dry-run or download-only).
    Skipped,
    /// Update failed.
    Failed,
}

/// Summary of the update operation.
struct UpdateSummary {
    updated: usize,
    already_current: usize,
    skipped: usize,
    failed: usize,
}

impl UpdateSummary {
    fn new() -> Self {
        Self {
            updated: 0,
            already_current: 0,
            skipped: 0,
            failed: 0,
        }
    }

    fn record(&mut self, result: &UpdateResult) {
        match result {
            UpdateResult::Updated => self.updated += 1,
            UpdateResult::AlreadyCurrent => self.already_current += 1,
            UpdateResult::Skipped => self.skipped += 1,
            UpdateResult::Failed => self.failed += 1,
        }
    }
}

/// Perform updates on binaries.
///
/// # Arguments
/// * `config` - Client configuration
/// * `db` - Database connection
/// * `name` - Optional specific binary name to update
/// * `force` - Re-download even if on latest version
/// * `dry_run` - Only check, don't apply updates
/// * `download_only` - Download updates but don't apply them
/// * `quiet` - Suppress progress bars
#[allow(clippy::too_many_arguments)]
async fn perform_update(
    config: &ClientConfig,
    db: &ClientDb,
    name: Option<&str>,
    force: bool,
    dry_run: bool,
    download_only: bool,
    quiet: bool,
    allow_downgrade: bool,
) -> anyhow::Result<Vec<UpdateResult>> {
    println!("Checking for updates...");
    println!();

    let checker = UpdateChecker::new(config.clone())?;
    let mut results = Vec::new();

    // Get binaries to update
    let binaries: Vec<DbManagedBinary> = if let Some(binary_name) = name {
        match db.get_binary_by_name(binary_name).await? {
            Some(binary) => vec![binary],
            None => {
                println!("{}: Binary not found", binary_name);
                results.push(UpdateResult::Failed);
                return Ok(results);
            }
        }
    } else {
        db.list_binaries().await?
    };

    if binaries.is_empty() {
        println!("No managed binaries registered.");
        return Ok(results);
    }

    // Create multi-progress for multiple binaries
    let multi = if !quiet && binaries.len() > 1 {
        Some(MultiProgress::new())
    } else {
        None
    };

    for binary in binaries {
        let result = process_binary_update(
            config,
            db,
            &checker,
            &binary,
            force,
            dry_run,
            download_only,
            quiet,
            allow_downgrade,
            multi.as_ref(),
        )
        .await;
        results.push(result);
    }

    Ok(results)
}

/// Create a progress bar for downloading.
fn create_download_progress_bar(
    multi: Option<&MultiProgress>,
    binary_name: &str,
    size_hint: Option<u64>,
) -> ProgressBar {
    let total = size_hint.unwrap_or(0);
    let pb = if let Some(m) = multi {
        m.add(ProgressBar::new(total))
    } else {
        ProgressBar::new(total)
    };

    let template = if size_hint.is_some() {
        format!(
            "{{spinner:.green}} {} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{bytes}}/{{total_bytes}} ({{bytes_per_sec}}, {{eta}})",
            binary_name
        )
    } else {
        format!(
            "{{spinner:.green}} {} [{{elapsed_precise}}] {{bytes}} ({{bytes_per_sec}})",
            binary_name
        )
    };

    let style = ProgressStyle::default_bar()
        .template(&template)
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("#>-");
    pb.set_style(style);
    pb
}

/// Process update for a single binary.
#[allow(clippy::too_many_arguments)]
async fn process_binary_update(
    config: &ClientConfig,
    db: &ClientDb,
    checker: &UpdateChecker,
    binary: &DbManagedBinary,
    force: bool,
    dry_run: bool,
    download_only: bool,
    quiet: bool,
    allow_downgrade: bool,
    multi: Option<&MultiProgress>,
) -> UpdateResult {
    let binary_name = &binary.binary_name;
    let current_version = binary.current_version_string.clone();

    // Record check time
    if let Err(e) = db.set_last_check(&binary.binary_id).await {
        tracing::warn!(
            "Failed to record check time for {}: {}",
            binary_name,
            e
        );
    }

    // Check for updates
    debug!("Checking for updates for binary: {}", binary_name);
    let update_info = match checker.check_for_updates(binary).await {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to check updates for {}: {}", binary_name, e);
            println!("{}: Check failed ({})", binary_name, e);
            return UpdateResult::Failed;
        }
    };

    // Determine if we should update
    // Note: force flag is handled separately below - we only proceed if there's actual update info
    let should_update = update_info.is_some();

    if !should_update {
        println!(
            "{}: Already up to date ({})",
            binary_name,
            current_version.as_deref().unwrap_or("unknown")
        );
        return UpdateResult::AlreadyCurrent;
    }

    // If no update available and force is set, we need to re-fetch current version
    let update = match update_info {
        Some(info) => info,
        None => {
            if force {
                // P2 Issue 67 Fix: Force Re-Download Limitation Documentation
                //
                // Force re-download of the current version is not implemented because:
                // 1. The server API only returns update info when a newer version exists
                // 2. Implementing this requires a new API endpoint:
                //    GET /api/v1/apps/{name}/versions/{version}/artifacts
                // 3. This endpoint would need to return full binary URL, signature, etc. for ANY version
                //
                // P3 Issue #110 Fix: Known Limitation - Force Re-download
                //
                // **Status**: Not implemented (tracked as P2 Issue #67 in TODO.md)
                //
                // Current behavior: --force is ignored when already on the latest version.
                // Workaround: To force re-download, temporarily unregister and re-add the binary.
                //
                // To implement force re-download:
                // - Add server endpoint for fetching specific version artifacts
                // - Update checker to support fetching current version metadata
                // - Implement download and verify logic for same-version updates
                println!(
                    "{}: Already on latest version ({}).",
                    binary_name,
                    current_version.as_deref().unwrap_or("unknown")
                );
                println!(
                    "  Note: --force re-download is not currently supported when already on the latest version."
                );
                println!(
                    "  The --force flag only works when a newer version is available."
                );
                println!(
                    "  Workaround: To force re-download the current version, remove and re-add the binary."
                );
            } else {
                println!(
                    "{}: Already up to date ({})",
                    binary_name,
                    current_version.as_deref().unwrap_or("unknown")
                );
            }
            return UpdateResult::AlreadyCurrent;
        }
    };

    let target_version = update.version.to_string();

    println!(
        "{}: Update available ({} -> {})",
        binary_name,
        current_version.as_deref().unwrap_or("none"),
        target_version
    );

    // FIX-2: Downgrade protection. Compare the server's target_version to the
    // installed version as semver and refuse to apply unless target > current,
    // unless the operator explicitly passed --allow-downgrade. This is enforced
    // before any download/apply (and before dry-run/download-only) so a
    // compromised server cannot roll the client back to an older, vulnerable
    // build by simply setting update_available == true.
    if let Some(ref current_str) = current_version {
        match current_str.parse::<Version>() {
            Ok(current) => {
                if update.version <= current && !allow_downgrade {
                    println!(
                        "  Refusing downgrade: target {} is not newer than installed {}.",
                        update.version, current
                    );
                    println!(
                        "  Pass --allow-downgrade to override (only if you trust this server \
                         and intend to install an older version)."
                    );
                    error!(
                        "Refusing downgrade for {}: target {} <= installed {}",
                        binary_name, update.version, current
                    );
                    return UpdateResult::Failed;
                }
                if update.version <= current && allow_downgrade {
                    println!(
                        "  WARNING: --allow-downgrade set; installing {} over newer/equal {}.",
                        update.version, current
                    );
                }
            }
            Err(e) => {
                // Cannot prove this is not a downgrade — refuse unless overridden.
                if !allow_downgrade {
                    println!(
                        "  Cannot parse installed version '{}' as semver ({}); refusing update.",
                        current_str, e
                    );
                    println!("  Pass --allow-downgrade to override.");
                    error!(
                        "Cannot parse installed version '{}' for {}: {}",
                        current_str, binary_name, e
                    );
                    return UpdateResult::Failed;
                }
            }
        }
    }

    // Dry run - just show what would happen
    if dry_run {
        if let Some(diff_size) = update.diff_size {
            println!("  Would download diff ({} KB)", diff_size / 1024);
        } else if update.diff_url.is_some() {
            println!("  Would download diff");
        } else {
            println!("  Would download full binary");
        }
        println!("  Would update to {}", target_version);
        return UpdateResult::Skipped;
    }

    // Download only - download but don't apply
    if download_only {
        match download_update_only(config, binary, &update).await {
            Ok(path) => {
                println!("  Downloaded to: {}", path);
                return UpdateResult::Skipped;
            }
            Err(e) => {
                println!("  Download failed: {}", e);
                return UpdateResult::Failed;
            }
        }
    }

    // Apply the update with progress bar
    let progress_bar = if !quiet {
        // Get size hint from diff_size or full_binary_size
        let size_hint = update.diff_size.or(update.full_binary_size);
        let pb = create_download_progress_bar(multi, binary_name, size_hint);

        // Show what we're downloading
        if update.diff_url.is_some() {
            if let Some(diff_size) = update.diff_size {
                println!("  Downloading diff ({} KB)...", diff_size / 1024);
            } else {
                println!("  Downloading diff...");
            }
        } else {
            println!("  Downloading full binary...");
        }

        Some(pb)
    } else {
        // Quiet mode - no progress bar
        if let Some(diff_size) = update.diff_size {
            print!("  Downloading diff ({} KB)...", diff_size / 1024);
        } else if update.diff_url.is_some() {
            print!("  Downloading diff...");
        } else {
            print!("  Downloading full binary...");
        }
        std::io::Write::flush(&mut std::io::stdout()).ok();
        None
    };

    match apply_update(config, db, binary, &update, progress_bar.as_ref()).await {
        Ok(()) => {
            if let Some(pb) = progress_bar {
                pb.finish_and_clear();
            }
            info!("Successfully updated {} to {}", binary_name, target_version);
            if quiet {
                println!(" done");
            }
            println!("  Applying patch... done");
            println!("  Verifying... done");
            println!("  [OK] Updated to {}", target_version);
            UpdateResult::Updated
        }
        Err(e) => {
            if let Some(pb) = progress_bar {
                pb.abandon_with_message("failed");
            }
            error!("Failed to update {}: {}", binary_name, e);
            if quiet {
                println!(" failed");
            }
            println!("  Error: {}", e);
            UpdateResult::Failed
        }
    }
}

/// Download an update without applying it.
///
/// Downloads the update file and verifies its checksum. When a full binary is
/// downloaded and a signature URL is advertised, the Ed25519 signature is also
/// verified against the publisher's public key. Diff downloads are not
/// individually signed (only the reconstructed binary is), so they are
/// checksum-verified only and the user is warned to verify before use.
///
/// On any verification failure the downloaded file is deleted before returning
/// an error.
async fn download_update_only(
    config: &ClientConfig,
    binary: &DbManagedBinary,
    update: &UpdateInfo,
) -> anyhow::Result<String> {
    use crate::downloader::download_file;
    use deltaship_crypto::{hash_file, Hash};

    let downloads_dir = config.downloads_dir();
    tokio::fs::create_dir_all(&downloads_dir).await?;

    // Prefer diff if available, and determine expected checksum
    let (url, filename, expected_checksum, is_full_binary) =
        if let Some(ref diff_url) = update.diff_url {
            let filename = format!("{}_{}.diff", binary.binary_name, update.version);
            let checksum = update.diff_checksum.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Security error: diff checksum is required but missing. \
                     Cannot download unverified diff files."
                )
            })?;
            (diff_url.as_str(), filename, checksum.clone(), false)
        } else if let Some(ref full_url) = update.full_binary_url {
            let filename = format!("{}_{}", binary.binary_name, update.version);
            (full_url.as_str(), filename, update.checksum.clone(), true)
        } else {
            anyhow::bail!("No download URL available for update");
        };

    let dest_path = downloads_dir.join(&filename);

    print!("  Downloading...");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    download_file(url, &dest_path).await?;

    println!(" done");

    // Verify checksum after download
    print!("  Verifying checksum...");
    std::io::Write::flush(&mut std::io::stdout()).ok();

    let expected_hash = match Hash::from_hex(&expected_checksum) {
        Ok(h) => h,
        Err(e) => {
            let _ = tokio::fs::remove_file(&dest_path).await;
            anyhow::bail!("Invalid checksum format from server: {}", e);
        }
    };

    let actual_hash = match hash_file(&dest_path) {
        Ok(h) => h,
        Err(e) => {
            let _ = tokio::fs::remove_file(&dest_path).await;
            anyhow::bail!("Failed to compute checksum of downloaded file: {}", e);
        }
    };

    if expected_hash != actual_hash {
        let _ = tokio::fs::remove_file(&dest_path).await;
        anyhow::bail!(
            "Checksum verification failed: expected {}, got {}. \
             Downloaded file has been deleted for security.",
            expected_hash,
            actual_hash
        );
    }

    println!(" verified");

    // Attempt signature verification for full binaries when a signature URL is
    // advertised. Diffs are not individually signed by the publisher.
    let signature_verified = if is_full_binary {
        if let Some(ref sig_url) = update.signature_url {
            print!("  Verifying signature...");
            std::io::Write::flush(&mut std::io::stdout()).ok();

            match verify_downloaded_signature(
                sig_url,
                binary,
                &actual_hash,
                &update.version.to_string(),
            )
            .await
            {
                Ok(()) => {
                    println!(" verified");
                    true
                }
                Err(e) => {
                    let _ = tokio::fs::remove_file(&dest_path).await;
                    anyhow::bail!(
                        "Signature verification failed: {}. \
                         Downloaded file has been deleted for security.",
                        e
                    );
                }
            }
        } else {
            false
        }
    } else {
        false
    };

    if !signature_verified {
        // Loud, multi-line warning so it isn't lost in scrollback when the
        // downloaded artifact cannot be authenticity-checked here (diffs, or
        // a server response missing signature_url).
        println!();
        println!("  ============================================================");
        println!("  SECURITY WARNING: Signature was NOT verified for this file.");
        if !is_full_binary {
            println!("  Reason: diff artifacts are not individually signed; only");
            println!("  the reconstructed binary's signature is checked during a");
            println!("  normal `update` run.");
        } else {
            println!("  Reason: server did not advertise a signature URL.");
        }
        println!("  Do NOT install this file until you have manually verified");
        println!("  its authenticity against the publisher's public key.");
        println!("  ============================================================");
    }

    Ok(dest_path.to_string_lossy().to_string())
}

/// Fetch and verify the Ed25519 signature for a downloaded full binary.
///
/// FIX-1: The publisher signs the canonical payload `"DELTASHIP-sig-v1\0"` ++ raw
/// BLAKE3 hash of the binary (32 bytes) ++ the version string, not the bare
/// hash. We pass the already-computed hash to avoid re-reading the file, plus
/// the target version those bytes are, and verify the signature over that
/// canonical, version-bound payload.
async fn verify_downloaded_signature(
    sig_url: &str,
    binary: &DbManagedBinary,
    binary_hash: &deltaship_crypto::Hash,
    version: &str,
) -> anyhow::Result<()> {
    use crate::downloader::download_bytes;
    use deltaship_crypto::{signing_payload, Signature, VerifyingKey};

    let sig_bytes = download_bytes(sig_url).await?;
    let sig_len = sig_bytes.len();
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid signature length: expected 64 bytes, got {}", sig_len))?;
    let signature = Signature::from_bytes(sig_array);

    let key_bytes: [u8; 32] = binary
        .publisher_public_key
        .clone()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid publisher public key length"))?;
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)?;

    // FIX-1: build the canonical, version-bound signing payload.
    let payload = signing_payload(&binary_hash.to_bytes(), version);

    verifying_key
        .verify(&payload, &signature)
        .map_err(|e| anyhow::anyhow!("{}", e))
}

/// Print a summary of the update results.
fn print_summary(results: &[UpdateResult]) {
    let mut summary = UpdateSummary::new();
    for result in results {
        summary.record(result);
    }

    println!();
    print!("Summary: ");

    let mut parts = Vec::new();
    if summary.updated > 0 {
        parts.push(format!("{} updated", summary.updated));
    }
    if summary.already_current > 0 {
        parts.push(format!("{} already current", summary.already_current));
    }
    if summary.skipped > 0 {
        parts.push(format!("{} skipped", summary.skipped));
    }
    if summary.failed > 0 {
        parts.push(format!("{} failed", summary.failed));
    }

    if parts.is_empty() {
        println!("No binaries processed");
    } else {
        println!("{}", parts.join(", "));
    }
}

/// Execute the update command.
///
/// # Arguments
/// * `config` - Client configuration
/// * `db` - Database connection
/// * `name` - Optional specific binary name to update
/// * `force` - Re-download even if on latest version
/// * `dry_run` - Only check, don't apply updates
/// * `download_only` - Download updates but don't apply them
/// * `quiet` - Suppress progress bars (use for non-interactive mode)
/// * `allow_downgrade` - Permit installing a target version that is not strictly
///   newer than the installed version. Defaults to false at every call site;
///   when false, the update is refused (FIX-2 downgrade protection).
#[allow(clippy::too_many_arguments)]
pub async fn update(
    config: &ClientConfig,
    db: &ClientDb,
    name: Option<&str>,
    force: bool,
    dry_run: bool,
    download_only: bool,
    quiet: bool,
    allow_downgrade: bool,
) -> anyhow::Result<()> {
    let results = perform_update(
        config,
        db,
        name,
        force,
        dry_run,
        download_only,
        quiet,
        allow_downgrade,
    )
    .await?;
    print_summary(&results);
    Ok(())
}
