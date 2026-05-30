//! Daemon loop for Deltaship Client.
//!
//! Runs in the background, periodically checking for updates
//! and applying them automatically.
//!
//! # Concurrency Safety
//!
//! This daemon implements file-based locking to prevent race conditions:
//! 1. A PID file with exclusive lock ensures only one daemon instance runs
//! 2. An update lock file is held during update operations to prevent conflicts
//!    with manual update commands

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use fs2::FileExt;
use deltaship_core::Version;
use deltaship_db::ClientDb;

use crate::checker::UpdateChecker;
use crate::config::ClientConfig;
use crate::patcher::apply_update;

/// Name of the PID lock file for single-instance enforcement.
const PID_LOCK_FILE: &str = "deltaship-daemon.pid";

/// Name of the update lock file for operation synchronization.
const UPDATE_LOCK_FILE: &str = "deltaship-update.lock";

/// Guard that holds the daemon PID lock. When dropped, releases the lock.
pub struct DaemonLockGuard {
    _pid_file: File,
    pid_path: PathBuf,
}

impl Drop for DaemonLockGuard {
    fn drop(&mut self) {
        // Clean up PID file on exit
        let _ = std::fs::remove_file(&self.pid_path);
    }
}

/// Acquire an exclusive lock for the daemon to ensure single-instance operation.
///
/// Creates a PID file and acquires an exclusive lock on it. If another daemon
/// instance is already running, this will fail immediately.
///
/// Returns a guard that releases the lock when dropped.
pub fn acquire_daemon_lock(config: &ClientConfig) -> anyhow::Result<DaemonLockGuard> {
    let pid_path = config.data_dir.join(PID_LOCK_FILE);

    // Ensure the data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    // Open or create the PID file
    let mut pid_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&pid_path)?;

    // Try to acquire an exclusive lock (non-blocking)
    match pid_file.try_lock_exclusive() {
        Ok(()) => {
            // We got the lock, write our PID
            pid_file.set_len(0)?;
            write!(pid_file, "{}", std::process::id())?;
            pid_file.sync_all()?;

            tracing::info!("Acquired daemon lock, PID: {}", std::process::id());
            Ok(DaemonLockGuard {
                _pid_file: pid_file,
                pid_path,
            })
        }
        Err(_) => {
            // Another instance is running, try to read its PID
            let mut existing_pid = String::new();
            pid_file.read_to_string(&mut existing_pid).ok();
            let pid_info = existing_pid.trim();

            anyhow::bail!(
                "Another Deltaship daemon instance is already running (PID: {}). \
                 If you believe this is an error, remove the lock file at: {}",
                if pid_info.is_empty() {
                    "unknown"
                } else {
                    pid_info
                },
                pid_path.display()
            )
        }
    }
}

/// Guard that holds the update operation lock. When dropped, releases the lock.
pub struct UpdateLockGuard {
    _lock_file: File,
}

// NOTE: Version Comparison Design Decision
//
// Version comparison is intentionally performed server-side, not client-side.
// The client sends its current version to the server, and the server determines
// if an update is available by comparing versions according to its own logic.
//
// This design provides several benefits:
// 1. Consistent version comparison logic across all clients
// 2. Server can implement complex version policies (e.g., channel-based updates,
//    staged rollouts, version pinning)
// 3. No need to update clients when version comparison logic changes
// 4. Server can return "no update" even for older versions (e.g., during rollback periods)
//
// Trade-off: Requires network connectivity to determine if updates are available.

/// Acquire a lock for update operations.
///
/// This prevents concurrent update operations from conflicting with each other.
/// Uses a blocking lock - will wait if another update is in progress.
pub fn acquire_update_lock(config: &ClientConfig) -> anyhow::Result<UpdateLockGuard> {
    let lock_path = config.data_dir.join(UPDATE_LOCK_FILE);

    // Ensure the data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    // Acquire exclusive lock (blocking)
    lock_file.lock_exclusive()?;

    tracing::debug!("Acquired update lock");
    Ok(UpdateLockGuard {
        _lock_file: lock_file,
    })
}

/// Try to acquire a lock for update operations (non-blocking).
///
/// Returns None if the lock is already held by another operation.
pub fn try_acquire_update_lock(config: &ClientConfig) -> anyhow::Result<Option<UpdateLockGuard>> {
    let lock_path = config.data_dir.join(UPDATE_LOCK_FILE);

    // Ensure the data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    // Try to acquire exclusive lock (non-blocking)
    match lock_file.try_lock_exclusive() {
        Ok(()) => {
            tracing::debug!("Acquired update lock");
            Ok(Some(UpdateLockGuard {
                _lock_file: lock_file,
            }))
        }
        Err(_) => {
            tracing::debug!("Update lock is held by another operation");
            Ok(None)
        }
    }
}

/// Maximum consecutive failures before entering extended backoff mode.
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

/// Maximum backoff multiplier (caps exponential growth).
const MAX_BACKOFF_MULTIPLIER: u32 = 16;

/// Database key for storing circuit breaker failure count.
/// P3 Issue 94 Fix: Make circuit breaker persistent across daemon restarts.
const CIRCUIT_BREAKER_FAILURES_KEY: &str = "daemon_consecutive_failures";

/// Database key for storing the timestamp when circuit breaker failures were last recorded.
/// P2 Issue 4 Fix: Add 24-hour expiration to prevent stale failure counts.
const CIRCUIT_BREAKER_TIMESTAMP_KEY: &str = "daemon_failures_timestamp";

/// Circuit breaker failure expiration time (24 hours in seconds).
/// P2 Issue 4 Fix: After this time, persisted failures are cleared to allow recovery.
const CIRCUIT_BREAKER_EXPIRATION_SECS: i64 = 24 * 60 * 60;

/// Run the daemon loop.
///
/// This function runs forever, periodically checking for updates
/// for all managed binaries and applying them if available.
///
/// # Single Instance
///
/// This function acquires an exclusive daemon lock before starting.
/// If another daemon instance is already running, this will return an error.
///
/// # Error Handling
///
/// The daemon implements a circuit breaker pattern:
/// - On consecutive failures, delay increases exponentially (up to 16x the base interval)
/// - After MAX_CONSECUTIVE_FAILURES (5), the daemon enters extended backoff mode
/// - A successful cycle resets the failure counter and backoff
pub async fn run_daemon(config: ClientConfig, db: ClientDb) -> anyhow::Result<()> {
    // Acquire daemon lock to ensure single instance
    let _daemon_lock = acquire_daemon_lock(&config)?;

    let checker = UpdateChecker::new(config.clone())?;
    let base_interval = Duration::from_secs(config.check_interval_secs);

    tracing::info!(
        "Starting Deltaship client daemon, checking every {} seconds",
        config.check_interval_secs
    );

    // P3 Issue 94 Fix: Load circuit breaker state from database for persistence
    // P2 Issue 4 Fix: Check expiration timestamp to prevent stale failures from persisting forever
    let mut consecutive_failures: u32 = match db.get_config(CIRCUIT_BREAKER_FAILURES_KEY).await {
        Ok(Some(value)) => {
            let failures = value.parse().unwrap_or(0);

            // Check if failures have expired (24-hour window)
            if failures > 0 {
                match db.get_config(CIRCUIT_BREAKER_TIMESTAMP_KEY).await {
                    Ok(Some(timestamp_str)) => {
                        if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                            let current_time = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .expect("system clock predates UNIX epoch")
                                .as_secs() as i64;

                            let age_secs = current_time - timestamp;

                            if age_secs > CIRCUIT_BREAKER_EXPIRATION_SECS {
                                tracing::info!(
                                    "Circuit breaker failures expired after {} hours, resetting from {} to 0",
                                    age_secs / 3600,
                                    failures
                                );
                                // Clear the expired failure count
                                let _ = db.set_config(CIRCUIT_BREAKER_FAILURES_KEY, "0").await;
                                let _ = db.set_config(CIRCUIT_BREAKER_TIMESTAMP_KEY, "0").await;
                                0
                            } else {
                                tracing::info!(
                                    "Loaded circuit breaker state: {} consecutive failures from {} hours ago",
                                    failures,
                                    age_secs / 3600
                                );
                                failures
                            }
                        } else {
                            tracing::warn!("Invalid circuit breaker timestamp, resetting failures");
                            0
                        }
                    }
                    _ => {
                        tracing::warn!("No timestamp found for circuit breaker failures, resetting");
                        0
                    }
                }
            } else {
                failures
            }
        }
        _ => 0,
    };

    if consecutive_failures > 0 {
        tracing::info!(
            "Circuit breaker active: {} consecutive failures",
            consecutive_failures
        );
    }

    loop {
        let mut cycle_failed = false;

        // Acquire update lock before running update cycle
        // This prevents conflicts with manual update commands
        match acquire_update_lock(&config) {
            Ok(_update_lock) => {
                // Run update check cycle while holding the lock
                if let Err(e) = check_and_apply_updates(&config, &db, &checker).await {
                    tracing::error!("Update cycle failed: {}", e);
                    cycle_failed = true;
                }
                // Lock is released here when _update_lock goes out of scope
            }
            Err(e) => {
                tracing::warn!("Failed to acquire update lock: {}", e);
                cycle_failed = true;
            }
        }

        // Clean up expired backups
        if let Err(e) = db.delete_expired_backups().await {
            tracing::warn!("Failed to clean up expired backups: {}", e);
            // Don't count backup cleanup failures toward the circuit breaker
        }

        // Update circuit breaker state
        if cycle_failed {
            consecutive_failures = consecutive_failures.saturating_add(1);

            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                tracing::warn!(
                    "Circuit breaker: {} consecutive failures, entering extended backoff mode",
                    consecutive_failures
                );
            }

            // P3 Issue 94 Fix: Persist failure count to database
            // P2 Issue 4 Fix: Also persist timestamp for expiration tracking
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock predates UNIX epoch")
                .as_secs() as i64;

            if let Err(e) = db
                .set_config(CIRCUIT_BREAKER_FAILURES_KEY, &consecutive_failures.to_string())
                .await
            {
                tracing::warn!("Failed to save circuit breaker state: {}", e);
            }

            if let Err(e) = db
                .set_config(CIRCUIT_BREAKER_TIMESTAMP_KEY, &current_time.to_string())
                .await
            {
                tracing::warn!("Failed to save circuit breaker timestamp: {}", e);
            }
        } else {
            if consecutive_failures > 0 {
                tracing::info!(
                    "Update cycle succeeded, resetting failure counter (was {})",
                    consecutive_failures
                );

                // P3 Issue 94 Fix: Clear persisted failure count on success
                // P2 Issue 4 Fix: Also clear timestamp
                if let Err(e) = db.set_config(CIRCUIT_BREAKER_FAILURES_KEY, "0").await {
                    tracing::warn!("Failed to clear circuit breaker state: {}", e);
                }
                if let Err(e) = db.set_config(CIRCUIT_BREAKER_TIMESTAMP_KEY, "0").await {
                    tracing::warn!("Failed to clear circuit breaker timestamp: {}", e);
                }
            }
            consecutive_failures = 0;
        }

        // Calculate sleep duration with exponential backoff
        let backoff_multiplier = if consecutive_failures == 0 {
            1
        } else {
            // Exponential backoff: 2^(failures-1), capped at MAX_BACKOFF_MULTIPLIER
            std::cmp::min(
                2u32.saturating_pow(consecutive_failures - 1),
                MAX_BACKOFF_MULTIPLIER,
            )
        };

        let sleep_duration = base_interval * backoff_multiplier;

        if backoff_multiplier > 1 {
            tracing::info!(
                "Backing off: sleeping for {} seconds ({}x normal interval, {} consecutive failures)",
                sleep_duration.as_secs(),
                backoff_multiplier,
                consecutive_failures
            );
        } else {
            tracing::debug!("Sleeping for {} seconds", sleep_duration.as_secs());
        }

        tokio::time::sleep(sleep_duration).await;
    }
}

/// Run a single update check cycle.
///
/// Acquires an update lock before proceeding to prevent conflicts with
/// the daemon or other concurrent operations.
pub async fn run_check_once(config: &ClientConfig, db: &ClientDb) -> anyhow::Result<()> {
    // Acquire update lock to prevent conflicts
    let _update_lock = acquire_update_lock(config)?;

    let checker = UpdateChecker::new(config.clone())?;
    check_and_apply_updates(config, db, &checker).await
}

/// Check all managed binaries for updates and apply them.
async fn check_and_apply_updates(
    config: &ClientConfig,
    db: &ClientDb,
    checker: &UpdateChecker,
) -> anyhow::Result<()> {
    tracing::info!("Starting update check cycle");

    // Get all managed binaries
    let binaries = db.list_binaries().await?;

    if binaries.is_empty() {
        tracing::info!("No managed binaries registered");
        return Ok(());
    }

    tracing::info!("Checking {} managed binaries", binaries.len());

    for binary in binaries {
        // Skip if auto-update is disabled
        if !binary.auto_update {
            tracing::debug!("Skipping {} (auto-update disabled)", binary.binary_name);
            continue;
        }

        tracing::info!("Checking for updates: {}", binary.binary_name);

        // Record check time
        if let Err(e) = db.set_last_check(&binary.binary_id).await {
            tracing::warn!(
                "Failed to record check time for {}: {}",
                binary.binary_name,
                e
            );
        }

        // Check for updates
        match checker.check_for_updates(&binary).await {
            Ok(Some(update)) => {
                tracing::info!(
                    "Update available for {}: {} -> {}",
                    binary.binary_name,
                    binary.current_version_string.as_deref().unwrap_or("(none)"),
                    update.version
                );

                // FIX-2: Downgrade protection. The daemon NEVER applies a target
                // version that is not strictly newer than what is installed — there
                // is no interactive opt-out in the unattended path. This blocks a
                // malicious/compromised server from rolling clients back to an older,
                // vulnerable build even when it sets update_available == true.
                if let Some(ref current_str) = binary.current_version_string {
                    match current_str.parse::<Version>() {
                        Ok(current) => {
                            let target = &update.version;
                            if target <= &current {
                                tracing::error!(
                                    "Refusing downgrade for {}: server offered {} but {} is installed \
                                     (daemon never downgrades)",
                                    binary.binary_name,
                                    target,
                                    current
                                );
                                crate::audit::log_audit_event(
                                    &crate::audit::AuditEvent::UpdateCheckFailed {
                                        binary_name: binary.binary_name.clone(),
                                        error: format!(
                                            "downgrade refused: target {} <= installed {}",
                                            target, current
                                        ),
                                    },
                                );
                                continue;
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Cannot parse installed version '{}' for {}: {} — refusing update \
                                 (cannot prove it is not a downgrade)",
                                current_str,
                                binary.binary_name,
                                e
                            );
                            continue;
                        }
                    }
                }

                // Apply the update (no progress bar in daemon mode)
                match apply_update(config, db, &binary, &update, None).await {
                    Ok(()) => {
                        tracing::info!(
                            "Successfully updated {} to {}",
                            binary.binary_name,
                            update.version
                        );
                    }
                    Err(e) => {
                        tracing::error!("Failed to update {}: {}", binary.binary_name, e);
                    }
                }
            }
            Ok(None) => {
                tracing::debug!("No update available for {}", binary.binary_name);
            }
            Err(e) => {
                tracing::error!("Failed to check updates for {}: {}", binary.binary_name, e);
                crate::audit::log_audit_event(&crate::audit::AuditEvent::UpdateCheckFailed {
                    binary_name: binary.binary_name.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    tracing::info!("Update check cycle completed");
    Ok(())
}
