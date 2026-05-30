//! Client database for Deltaship Client Patcher.
//!
//! Handles local storage of managed binaries, update history, and rollback backups.
//!
//! # P3 Issue #107 Fix: Database Metrics and Observability
//!
//! Key database operations are instrumented with `tracing` spans to enable observability:
//!
//! - Operation timing and duration tracking
//! - Record identifiers in span fields for correlation
//! - Error and success tracking
//! - Query performance monitoring
//!
//! To enable tracing output, configure a tracing subscriber in the calling application.
//! Example:
//!
//! ```ignore
//! use tracing_subscriber;
//! tracing_subscriber::fmt::init();
//! ```
//!
//! This will output structured logs showing database operation timing and context.

use std::path::Path;

use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};

use crate::error::{DbError, Result};
use crate::models::{
    DatabaseStats, DbManagedBinary, DbRollbackBackup, DbUpdateHistory, NewManagedBinary,
    NewUpdateRecord, UpdateMetrics,
};
use crate::models::{
    normalize_version_string, validate_expires_at_timestamp, validate_hash_size, validate_platform,
    UpdateHistoryStatus, BLAKE3_HASH_SIZE, SHA256_HASH_SIZE,
};
use crate::schema::{
    CLIENT_SCHEMA_VERSION, CREATE_CLIENT_CONFIG, CREATE_CLIENT_INSTALLED_VERSIONS,
    CREATE_CLIENT_INSTALLED_VERSIONS_INDEXES, CREATE_CLIENT_MANAGED_BINARIES,
    CREATE_CLIENT_MANAGED_BINARIES_INDEX, CREATE_CLIENT_ROLLBACK_BACKUPS,
    CREATE_CLIENT_ROLLBACK_BACKUPS_INDEXES, CREATE_CLIENT_UPDATE_HISTORY,
    CREATE_CLIENT_UPDATE_HISTORY_INDEXES, CREATE_SCHEMA_VERSION, INSERT_DEFAULT_CLIENT_CONFIG,
    INSERT_INITIAL_SCHEMA_VERSION,
};

/// Client database connection.
pub struct ClientDb {
    pool: Pool<Sqlite>,
}

impl ClientDb {
    /// Open or create a client database at the given path.
    ///
    /// # Connection Pool Configuration
    ///
    /// The pool is configured with:
    /// - **max_connections: 5** - Allows concurrent reads with SQLite WAL mode while
    ///   keeping resource usage reasonable. SQLite serializes writes via file-level
    ///   locking, so additional write connections don't improve throughput, but multiple
    ///   connections enable concurrent reads for better performance.
    /// - **acquire_timeout: 30s** - Appropriate timeout for legitimate SQLite operations.
    ///   Most database operations complete in seconds. Longer timeouts mask real problems
    ///   like deadlocks, incorrect locking patterns, or application bugs.
    ///
    /// For high-concurrency scenarios (e.g., managing many binaries simultaneously),
    /// consider increasing `max_connections` to 10-20. However, SQLite's write
    /// serialization means more connections primarily help with read operations.
    ///
    /// # P2 Issue 81 Fix: Pool Exhaustion Handling
    ///
    /// Connection acquisition failures now include pool statistics in the error message
    /// to help diagnose pool exhaustion issues. If you encounter "pool timed out" errors,
    /// check the reported idle/size/max_size values to understand pool saturation.
    pub async fn open(path: &Path) -> Result<Self> {
        use std::time::Duration;

        let db_url = format!("sqlite:{}?mode=rwc", path.display());

        // Pool sizing rationale:
        // - 5 connections allows concurrent reads with SQLite WAL mode
        // - SQLite serializes writes via file locking, so more write connections
        //   won't improve write throughput
        // - 30s timeout is appropriate for legitimate SQLite operations
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(&db_url)
            .await
            .map_err(|e| {
                // P2 Issue 81 Fix: Add context for connection pool errors
                DbError::Sqlx(format!(
                    "Failed to connect to database at '{}': {}",
                    path.display(),
                    e
                ))
            })?;

        // Enable foreign keys and WAL mode
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .map_err(|e| {
                // P2 Issue 81 Fix: Add pool stats on query failure
                let stats = pool.options().get_max_connections();
                DbError::Sqlx(format!(
                    "Failed to enable foreign keys (pool max: {}): {}",
                    stats, e
                ))
            })?;
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await
            .map_err(|e| {
                let stats = pool.options().get_max_connections();
                DbError::Sqlx(format!(
                    "Failed to enable WAL mode (pool max: {}): {}",
                    stats, e
                ))
            })?;

        // Verify foreign keys are enabled
        let enabled: i32 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                let stats = pool.options().get_max_connections();
                DbError::Sqlx(format!(
                    "Failed to verify foreign keys (pool max: {}): {}",
                    stats, e
                ))
            })?;
        if enabled != 1 {
            return Err(DbError::InvalidData("Foreign keys not enabled".into()));
        }

        Ok(Self { pool })
    }

    /// Initialize the database schema (create tables if not exist).
    ///
    /// # Schema Versioning
    ///
    /// This function checks the stored schema version against the expected version.
    /// If the database has a newer schema version than the code expects, an error is returned.
    /// Migrations are not yet supported - manual intervention is required for schema upgrades.
    ///
    /// # P2 Issue 88 Fix: Database Integrity Check
    ///
    /// This function now runs `PRAGMA integrity_check` to detect database corruption
    /// before performing schema operations. If corruption is detected, an error is returned.
    pub async fn init(&self) -> Result<()> {
        // P2 Issue 88 Fix: Check database integrity before proceeding
        let integrity_result: (String,) = sqlx::query_as("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                DbError::Sqlx(format!("Failed to check database integrity: {}", e))
            })?;

        // The integrity check returns "ok" if the database is not corrupt
        if integrity_result.0 != "ok" {
            return Err(DbError::InvalidData(format!(
                "Database corruption detected: {}",
                integrity_result.0
            )));
        }

        // P1 Issue DB-P1-2 Fix: Wrap schema creation in a transaction
        // This ensures atomic schema initialization - either all tables are created
        // or none are, preventing partial schema states on failure.
        let mut tx = self.pool.begin().await?;

        // Create schema version table first
        sqlx::query(CREATE_SCHEMA_VERSION)
            .execute(&mut *tx)
            .await?;

        // Check if there's an existing schema version
        let existing_version: Option<i32> =
            sqlx::query_scalar("SELECT MAX(version) FROM schema_version")
                .fetch_optional(&mut *tx)
                .await?
                .flatten();

        if let Some(version) = existing_version {
            // Version matches current - tables already exist, nothing to do
            if version == CLIENT_SCHEMA_VERSION {
                tx.rollback().await?;
                return Ok(());
            }
            // Version mismatch - handle both newer and older schemas
            // Since migrations are not supported, both cases are errors
            let mismatch_msg = if version > CLIENT_SCHEMA_VERSION {
                format!(
                    "Database schema version {} is newer than expected version {}. \
                     Please upgrade deltaship-db or use a compatible database.",
                    version, CLIENT_SCHEMA_VERSION
                )
            } else {
                format!(
                    "Database schema version {} is older than expected version {}. \
                     Please backup and recreate the database.",
                    version, CLIENT_SCHEMA_VERSION
                )
            };
            tx.rollback().await?;
            return Err(DbError::SchemaMismatch(format!(
                "{} Migrations are not yet supported.",
                mismatch_msg
            )));
        }

        // Insert initial schema version
        sqlx::query(INSERT_INITIAL_SCHEMA_VERSION)
            .execute(&mut *tx)
            .await?;

        // Create client config table with defaults
        sqlx::query(CREATE_CLIENT_CONFIG)
            .execute(&mut *tx)
            .await?;
        sqlx::query(INSERT_DEFAULT_CLIENT_CONFIG)
            .execute(&mut *tx)
            .await?;

        // Create managed_binaries table
        sqlx::query(CREATE_CLIENT_MANAGED_BINARIES)
            .execute(&mut *tx)
            .await?;
        sqlx::query(CREATE_CLIENT_MANAGED_BINARIES_INDEX)
            .execute(&mut *tx)
            .await?;

        // Create installed_versions table
        sqlx::query(CREATE_CLIENT_INSTALLED_VERSIONS)
            .execute(&mut *tx)
            .await?;
        sqlx::query(CREATE_CLIENT_INSTALLED_VERSIONS_INDEXES)
            .execute(&mut *tx)
            .await?;

        // Create update_history table
        sqlx::query(CREATE_CLIENT_UPDATE_HISTORY)
            .execute(&mut *tx)
            .await?;
        sqlx::query(CREATE_CLIENT_UPDATE_HISTORY_INDEXES)
            .execute(&mut *tx)
            .await?;

        // Create rollback_backups table
        sqlx::query(CREATE_CLIENT_ROLLBACK_BACKUPS)
            .execute(&mut *tx)
            .await?;
        sqlx::query(CREATE_CLIENT_ROLLBACK_BACKUPS_INDEXES)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    // =========================================================================
    // Health and Statistics operations
    // =========================================================================

    /// Check database health and return status information.
    ///
    /// # P3 Issue #112 Fix: Health Endpoint
    ///
    /// This method performs basic health checks on the database:
    /// - Verifies database connectivity
    /// - Checks database integrity
    /// - Returns connection pool statistics
    ///
    /// Returns `Ok(String)` with health status message if healthy,
    /// or `Err(DbError)` if any health check fails.
    pub async fn health_check(&self) -> Result<String> {
        // Test connectivity with a simple query
        let _: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                DbError::Sqlx(format!("Database connectivity check failed: {}", e))
            })?;

        // Run integrity check
        let integrity: (String,) = sqlx::query_as("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                DbError::Sqlx(format!("Database integrity check failed: {}", e))
            })?;

        if integrity.0 != "ok" {
            return Err(DbError::InvalidData(format!(
                "Database integrity check failed: {}",
                integrity.0
            )));
        }

        Ok("Database is healthy".to_string())
    }

    /// Get database statistics.
    ///
    /// # P3 Issue #112 Fix: Database Statistics
    ///
    /// Returns statistics about database usage:
    /// - Number of managed binaries
    /// - Number of installed versions
    /// - Number of rollback backups
    /// - Number of update records
    /// - Connection pool stats
    ///
    /// Useful for monitoring and debugging.
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let binary_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM managed_binaries")
                .fetch_one(&self.pool)
                .await?;

        let version_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM installed_versions")
                .fetch_one(&self.pool)
                .await?;

        let backup_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM rollback_backups")
                .fetch_one(&self.pool)
                .await?;

        let update_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM update_history")
                .fetch_one(&self.pool)
                .await?;

        Ok(DatabaseStats {
            managed_binaries: binary_count.0 as usize,
            installed_versions: version_count.0 as usize,
            rollback_backups: backup_count.0 as usize,
            update_history_records: update_count.0 as usize,
            pool_connections_max: self.pool.options().get_max_connections() as usize,
        })
    }

    // =========================================================================
    // Config operations
    // =========================================================================

    /// Get a configuration value by key.
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT config_value FROM client_config WHERE config_key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(|(v,)| v))
    }

    /// Set a configuration value.
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO client_config (config_key, config_value, updated_at)
             VALUES (?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(config_key) DO UPDATE SET
                config_value = excluded.config_value,
                updated_at = CURRENT_TIMESTAMP",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // =========================================================================
    // Managed binary operations
    // =========================================================================

    /// Register a new managed binary.
    ///
    /// Validates platform before insertion.
    #[tracing::instrument(skip(self, binary), fields(binary_id = %binary.binary_id, binary_name = %binary.binary_name))]
    pub async fn register_binary(&self, binary: NewManagedBinary) -> Result<DbManagedBinary> {
        // Validate platform
        validate_platform(&binary.platform)?;

        sqlx::query(
            "INSERT INTO managed_binaries (
                binary_id, binary_name, platform, install_path, publisher_public_key
             ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&binary.binary_id)
        .bind(&binary.binary_name)
        .bind(&binary.platform)
        .bind(&binary.install_path)
        .bind(&binary.publisher_public_key)
        .execute(&self.pool)
        .await?;

        self.get_binary(&binary.binary_id)
            .await?
            .ok_or_else(|| DbError::NotFound("Binary just registered not found".into()))
    }

    /// Get a managed binary by ID.
    pub async fn get_binary(&self, binary_id: &str) -> Result<Option<DbManagedBinary>> {
        let binary = sqlx::query_as::<_, DbManagedBinary>(
            "SELECT * FROM managed_binaries WHERE binary_id = ?",
        )
        .bind(binary_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(binary)
    }

    /// Get a managed binary by name.
    pub async fn get_binary_by_name(&self, binary_name: &str) -> Result<Option<DbManagedBinary>> {
        let binary = sqlx::query_as::<_, DbManagedBinary>(
            "SELECT * FROM managed_binaries WHERE binary_name = ?",
        )
        .bind(binary_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(binary)
    }

    /// List all managed binaries.
    pub async fn list_binaries(&self) -> Result<Vec<DbManagedBinary>> {
        let binaries = sqlx::query_as::<_, DbManagedBinary>(
            "SELECT * FROM managed_binaries ORDER BY binary_name",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(binaries)
    }

    /// List managed binaries with pagination support.
    ///
    /// # P2 Issue 86 Fix: Pagination Support
    ///
    /// This method returns a page of results with the specified limit and offset.
    /// Useful for displaying large lists in UI or CLI with paging.
    ///
    /// # Arguments
    ///
    /// * `limit` - Maximum number of records to return (must be > 0)
    /// * `offset` - Number of records to skip (must be >= 0)
    ///
    /// # Returns
    ///
    /// Returns up to `limit` records, starting after `offset` records.
    pub async fn list_binaries_paginated(
        &self,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<DbManagedBinary>> {
        if limit <= 0 {
            return Err(DbError::InvalidData(format!(
                "Limit must be positive, got {}",
                limit
            )));
        }
        if offset < 0 {
            return Err(DbError::InvalidData(format!(
                "Offset must be non-negative, got {}",
                offset
            )));
        }

        let binaries = sqlx::query_as::<_, DbManagedBinary>(
            "SELECT * FROM managed_binaries ORDER BY binary_name LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(binaries)
    }

    /// Update the current version of a managed binary.
    ///
    /// This operation runs in a transaction to ensure atomicity.
    ///
    /// # P0 Issue 15 Fix: Race Condition Prevention
    ///
    /// This function now verifies that the version exists in `installed_versions`
    /// before attempting to set it as current. If the UPDATE affects 0 rows,
    /// it means the version is not installed, and the transaction is rolled back.
    #[tracing::instrument(skip(self), fields(binary_id, version_id, version_string))]
    pub async fn update_current_version(
        &self,
        binary_id: &str,
        version_id: &str,
        version_string: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // First, clear the is_current flag on all versions for this binary
        sqlx::query("UPDATE installed_versions SET is_current = 0 WHERE binary_id = ?")
            .bind(binary_id)
            .execute(&mut *tx)
            .await?;

        // Update the managed_binaries table
        let result = sqlx::query(
            "UPDATE managed_binaries SET
                current_version_id = ?,
                current_version_string = ?,
                last_update_at = CURRENT_TIMESTAMP,
                updated_at = CURRENT_TIMESTAMP
             WHERE binary_id = ?",
        )
        .bind(version_id)
        .bind(version_string)
        .bind(binary_id)
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            // P3 Issue #106 Fix: Standardized error format
            return Err(DbError::NotFound(format!(
                "Binary not found: '{}'",
                binary_id
            )));
        }

        // Set is_current on the new version in installed_versions
        // P0 Issue 15 Fix: Verify this UPDATE affects exactly 1 row
        let result = sqlx::query(
            "UPDATE installed_versions SET is_current = 1
             WHERE binary_id = ? AND version_id = ?",
        )
        .bind(binary_id)
        .bind(version_id)
        .execute(&mut *tx)
        .await?;

        // If no rows were affected, the version doesn't exist in installed_versions
        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Err(DbError::NotFound(format!(
                "Version {} not found in installed_versions for binary {}. \
                 Call record_installed_version() before update_current_version().",
                version_id, binary_id
            )));
        }

        tx.commit().await?;
        Ok(())
    }

    /// Record a newly installed version in the installed_versions table.
    ///
    /// # P0 Issue 14 Fix: Missing Insert Function
    ///
    /// This function inserts a record into `installed_versions` to track version history.
    /// It validates hash sizes and normalizes the version string before insertion.
    ///
    /// # Arguments
    ///
    /// * `binary_id` - The binary this version belongs to
    /// * `version_id` - The version identifier from the server
    /// * `version_string` - The semantic version string (e.g., "1.0.0")
    /// * `file_hash_blake3` - Blake3 hash of the binary file (must be 32 bytes)
    /// * `file_hash_sha256` - SHA-256 hash of the binary file (must be 32 bytes)
    /// * `file_size_bytes` - Size of the binary file in bytes
    ///
    /// # Returns
    ///
    /// Returns the auto-generated ID of the inserted record.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The binary_id doesn't exist in managed_binaries (foreign key violation)
    /// - Hash sizes are invalid
    /// - Version string is not valid semver
    #[tracing::instrument(skip(self, file_hash_blake3, file_hash_sha256), fields(binary_id, version_id, version_string, file_size_bytes))]
    pub async fn record_installed_version(
        &self,
        binary_id: &str,
        version_id: &str,
        version_string: &str,
        file_hash_blake3: &[u8],
        file_hash_sha256: &[u8],
        file_size_bytes: i64,
    ) -> Result<i64> {
        // Validate hash sizes
        validate_hash_size(file_hash_blake3, BLAKE3_HASH_SIZE, "File Blake3")?;
        validate_hash_size(file_hash_sha256, SHA256_HASH_SIZE, "File SHA-256")?;

        // Normalize version string
        let normalized_version = normalize_version_string(version_string)?;

        // Validate file size
        if file_size_bytes <= 0 {
            return Err(DbError::InvalidData(format!(
                "File size must be positive, got {}",
                file_size_bytes
            )));
        }

        let result = sqlx::query(
            "INSERT INTO installed_versions (
                binary_id, version_id, version_string,
                file_hash_blake3, file_hash_sha256, file_size_bytes
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(binary_id)
        .bind(version_id)
        .bind(&normalized_version)
        .bind(file_hash_blake3)
        .bind(file_hash_sha256)
        .bind(file_size_bytes)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Record the last check time for a binary.
    pub async fn set_last_check(&self, binary_id: &str) -> Result<()> {
        let result = sqlx::query(
            "UPDATE managed_binaries SET last_check_at = CURRENT_TIMESTAMP WHERE binary_id = ?",
        )
        .bind(binary_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound(format!(
                "Binary not found: '{}'",
                binary_id
            )));
        }

        Ok(())
    }

    /// Set auto-update flag for a binary.
    pub async fn set_auto_update(&self, binary_id: &str, enabled: bool) -> Result<()> {
        let result = sqlx::query(
            "UPDATE managed_binaries SET auto_update = ?, updated_at = CURRENT_TIMESTAMP WHERE binary_id = ?",
        )
        .bind(enabled)
        .bind(binary_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound(format!(
                "Binary not found: '{}'",
                binary_id
            )));
        }

        Ok(())
    }

    /// Delete a managed binary and all its related records.
    ///
    /// This deletes the binary entry along with cascading deletes of:
    /// - installed_versions (via foreign key)
    /// - update_history (via foreign key)
    /// - rollback_backups (via foreign key)
    pub async fn delete_managed_binary(&self, binary_id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM managed_binaries WHERE binary_id = ?")
            .bind(binary_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound(format!(
                "Binary not found: '{}'",
                binary_id
            )));
        }

        Ok(())
    }

    // =========================================================================
    // Update history operations
    // =========================================================================

    /// Record the start of an update.
    pub async fn record_update_start(&self, update: NewUpdateRecord) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO update_history (
                binary_id, from_version_id, from_version_string,
                to_version_id, to_version_string, status
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&update.binary_id)
        .bind(&update.from_version_id)
        .bind(&update.from_version_string)
        .bind(&update.to_version_id)
        .bind(&update.to_version_string)
        .bind(UpdateHistoryStatus::Downloading.as_str())
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Update the status of an update.
    ///
    /// Uses UpdateHistoryStatus enum for type safety.
    pub async fn set_update_status(&self, update_id: i64, status: UpdateHistoryStatus) -> Result<()> {
        let result = sqlx::query("UPDATE update_history SET status = ? WHERE update_id = ?")
            .bind(status.as_str())
            .bind(update_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound(format!(
                "Update record not found: '{}'",
                update_id
            )));
        }

        Ok(())
    }

    /// Record the completion of an update (success or failure).
    pub async fn record_update_complete(
        &self,
        update_id: i64,
        success: bool,
        error_message: Option<&str>,
        metrics: UpdateMetrics,
    ) -> Result<()> {
        let status = if success {
            UpdateHistoryStatus::Completed
        } else {
            UpdateHistoryStatus::Failed
        };

        let result = sqlx::query(
            "UPDATE update_history SET
                status = ?,
                success = ?,
                error_message = ?,
                diff_id = ?,
                diff_algorithm = ?,
                diff_size_bytes = ?,
                full_size_bytes = ?,
                actual_downloaded_bytes = ?,
                download_time_ms = ?,
                apply_time_ms = ?,
                verify_time_ms = ?,
                completed_at = CURRENT_TIMESTAMP
             WHERE update_id = ?",
        )
        .bind(status.as_str())
        .bind(success)
        .bind(error_message)
        .bind(&metrics.diff_id)
        .bind(&metrics.diff_algorithm)
        .bind(metrics.diff_size_bytes)
        .bind(metrics.full_size_bytes)
        .bind(metrics.actual_downloaded_bytes)
        .bind(metrics.download_time_ms)
        .bind(metrics.apply_time_ms)
        .bind(metrics.verify_time_ms)
        .bind(update_id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound(format!(
                "Update record not found: '{}'",
                update_id
            )));
        }

        Ok(())
    }

    /// Get recent updates for a binary.
    pub async fn get_recent_updates(
        &self,
        binary_id: &str,
        limit: i32,
    ) -> Result<Vec<DbUpdateHistory>> {
        let updates = sqlx::query_as::<_, DbUpdateHistory>(
            "SELECT * FROM update_history
             WHERE binary_id = ?
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(binary_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(updates)
    }

    /// Get an update record by ID.
    pub async fn get_update(&self, update_id: i64) -> Result<Option<DbUpdateHistory>> {
        let update = sqlx::query_as::<_, DbUpdateHistory>(
            "SELECT * FROM update_history WHERE update_id = ?",
        )
        .bind(update_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(update)
    }

    // =========================================================================
    // Rollback backup operations
    // =========================================================================

    /// Create a rollback backup record.
    ///
    /// Validates hash size, expiration timestamp format, and normalizes version string before insertion.
    ///
    /// # Arguments
    ///
    /// * `expires_at` - Optional expiration timestamp in ISO 8601 format (e.g., `"2024-12-31T23:59:59Z"`
    ///   or `"2024-12-31T23:59:59+00:00"`). This is compared against `CURRENT_TIMESTAMP` in SQLite
    ///   for cleanup operations via [`delete_expired_backups()`]. SQLite's datetime comparison
    ///   works correctly with ISO 8601 strings as they sort lexicographically.
    ///
    ///   **Expected format**: `YYYY-MM-DDTHH:MM:SSZ` or `YYYY-MM-DDTHH:MM:SS+HH:MM`
    ///
    /// # DB-P2-3 Fix: Expiration Timestamp Validation
    ///
    /// The `expires_at` parameter is now validated to ensure it can be parsed as a datetime.
    /// If an invalid format is provided, the function returns an error instead of creating
    /// a backup with an unparseable expiration time.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_backup(
        &self,
        binary_id: &str,
        version_id: &str,
        version_string: &str,
        backup_path: &str,
        backup_hash_blake3: &[u8],
        backup_size_bytes: i64,
        expires_at: Option<&str>,
    ) -> Result<i64> {
        // Validate hash size
        validate_hash_size(backup_hash_blake3, BLAKE3_HASH_SIZE, "Backup Blake3")?;

        // DB-P2-3 Fix: Validate expires_at timestamp format if provided
        if let Some(expires) = expires_at {
            validate_expires_at_timestamp(expires)?;
        }

        // Normalize version string
        let normalized_version = normalize_version_string(version_string)?;

        let result = sqlx::query(
            "INSERT INTO rollback_backups (
                binary_id, version_id, version_string, backup_path,
                backup_hash_blake3, backup_size_bytes, expires_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(binary_id)
        .bind(version_id)
        .bind(&normalized_version)
        .bind(backup_path)
        .bind(backup_hash_blake3)
        .bind(backup_size_bytes)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Get backup by ID.
    pub async fn get_backup(&self, backup_id: i64) -> Result<Option<DbRollbackBackup>> {
        let backup = sqlx::query_as::<_, DbRollbackBackup>(
            "SELECT * FROM rollback_backups WHERE backup_id = ?",
        )
        .bind(backup_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(backup)
    }

    /// List backups for a binary.
    pub async fn list_backups(&self, binary_id: &str) -> Result<Vec<DbRollbackBackup>> {
        let backups = sqlx::query_as::<_, DbRollbackBackup>(
            "SELECT * FROM rollback_backups WHERE binary_id = ? ORDER BY created_at DESC",
        )
        .bind(binary_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(backups)
    }

    /// Delete a backup record.
    pub async fn delete_backup(&self, backup_id: i64) -> Result<()> {
        let result = sqlx::query("DELETE FROM rollback_backups WHERE backup_id = ?")
            .bind(backup_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(DbError::NotFound(format!(
                "Backup not found: '{}'",
                backup_id
            )));
        }

        Ok(())
    }

    /// Delete expired backups.
    pub async fn delete_expired_backups(&self) -> Result<u64> {
        let result = sqlx::query(
            "DELETE FROM rollback_backups
             WHERE expires_at IS NOT NULL AND expires_at < CURRENT_TIMESTAMP",
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Delete oldest backups keeping only the specified count per binary.
    ///
    /// This operation uses a transaction to ensure atomicity between the subquery
    /// (selecting backups to keep) and the delete operation. Without a transaction,
    /// concurrent insertions could cause the operation to delete incorrect backups.
    pub async fn cleanup_old_backups(&self, binary_id: &str, keep_count: i32) -> Result<u64> {
        let mut tx = self.pool.begin().await?;

        let result = sqlx::query(
            "DELETE FROM rollback_backups
             WHERE binary_id = ? AND backup_id NOT IN (
                SELECT backup_id FROM rollback_backups
                WHERE binary_id = ?
                ORDER BY created_at DESC
                LIMIT ?
             )",
        )
        .bind(binary_id)
        .bind(binary_id)
        .bind(keep_count)
        .execute(&mut *tx)
        .await?;

        let rows_affected = result.rows_affected();
        tx.commit().await?;

        Ok(rows_affected)
    }

    /// Get a backup by version string.
    pub async fn get_backup_by_version(
        &self,
        binary_id: &str,
        version_string: &str,
    ) -> Result<Option<DbRollbackBackup>> {
        let backup = sqlx::query_as::<_, DbRollbackBackup>(
            "SELECT * FROM rollback_backups WHERE binary_id = ? AND version_string = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(binary_id)
        .bind(version_string)
        .fetch_optional(&self.pool)
        .await?;

        Ok(backup)
    }

    /// Record a rollback operation in the update history.
    pub async fn record_rollback(
        &self,
        binary_id: &str,
        from_version_id: Option<&str>,
        from_version_string: Option<&str>,
        to_version_id: &str,
        to_version_string: &str,
    ) -> Result<i64> {
        let result = sqlx::query(
            "INSERT INTO update_history (
                binary_id, from_version_id, from_version_string,
                to_version_id, to_version_string, status, success, completed_at
             ) VALUES (?, ?, ?, ?, ?, ?, 1, CURRENT_TIMESTAMP)",
        )
        .bind(binary_id)
        .bind(from_version_id)
        .bind(from_version_string)
        .bind(to_version_id)
        .bind(to_version_string)
        .bind(UpdateHistoryStatus::RolledBack.as_str())
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    // =========================================================================
    // Cleanup operations
    // =========================================================================

    /// Delete update history records older than a specified number of days.
    ///
    /// # P2 Issue 66 Fix: Incomplete History Cleanup
    ///
    /// This function deletes old update history records to prevent unbounded
    /// database growth. Only records older than `days` are deleted.
    ///
    /// # Arguments
    ///
    /// * `days` - Delete records older than this many days
    ///
    /// # Returns
    ///
    /// Returns the number of records deleted.
    pub async fn delete_update_history(&self, days: i32) -> Result<u64> {
        if days <= 0 {
            return Err(DbError::InvalidData(format!(
                "Days must be positive, got {}",
                days
            )));
        }

        let result = sqlx::query(
            "DELETE FROM update_history
             WHERE started_at < datetime('now', ? || ' days')",
        )
        .bind(format!("-{}", days))
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_db() -> (ClientDb, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = ClientDb::open(&db_path).await.unwrap();
        db.init().await.unwrap();
        (db, temp_dir)
    }

    #[tokio::test]
    async fn test_client_db_init() {
        let (db, _temp) = create_test_db().await;
        assert!(db.pool.acquire().await.is_ok());
    }

    #[tokio::test]
    async fn test_register_and_get_binary() {
        let (db, _temp) = create_test_db().await;

        let new_binary = NewManagedBinary {
            binary_id: "test-binary-id".to_string(),
            binary_name: "testapp".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/usr/local/bin/testapp".to_string(),
            publisher_public_key: vec![0u8; 32],
        };

        let registered = db.register_binary(new_binary).await.unwrap();

        assert_eq!(registered.binary_id, "test-binary-id");
        assert_eq!(registered.binary_name, "testapp");
        assert_eq!(registered.platform, "linux-x86_64");

        let retrieved = db.get_binary("test-binary-id").await.unwrap().unwrap();
        assert_eq!(retrieved.binary_id, registered.binary_id);
    }

    #[tokio::test]
    async fn test_get_binary_by_name() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "myapp".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        let retrieved = db.get_binary_by_name("myapp").await.unwrap().unwrap();
        assert_eq!(retrieved.binary_name, "myapp");
    }

    #[tokio::test]
    async fn test_list_binaries() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "id1".to_string(),
            binary_name: "app1".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path1".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        db.register_binary(NewManagedBinary {
            binary_id: "id2".to_string(),
            binary_name: "app2".to_string(),
            platform: "windows-x86_64".to_string(),
            install_path: "/path2".to_string(),
            publisher_public_key: vec![1u8; 32],
        })
        .await
        .unwrap();

        let binaries = db.list_binaries().await.unwrap();
        assert_eq!(binaries.len(), 2);
    }

    #[tokio::test]
    async fn test_record_installed_version() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        let id = db
            .record_installed_version(
                "test-id",
                "v1",
                "1.0.0",
                &[0u8; 32], // blake3 hash
                &[1u8; 32], // sha256 hash
                10240,
            )
            .await
            .unwrap();

        assert!(id > 0);

        // Verify the version was recorded
        let versions: Vec<(String,)> = sqlx::query_as(
            "SELECT version_id FROM installed_versions WHERE id = ?",
        )
        .bind(id)
        .fetch_all(&db.pool)
        .await
        .unwrap();

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].0, "v1");
    }

    #[tokio::test]
    async fn test_record_installed_version_invalid_hash() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // Invalid blake3 hash size (16 bytes instead of 32)
        let result = db
            .record_installed_version(
                "test-id",
                "v1",
                "1.0.0",
                &[0u8; 16],
                &[1u8; 32],
                10240,
            )
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_current_version() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // P0 Issue 14 & 15 Fix: Must record installed version before updating current
        db.record_installed_version(
            "test-id",
            "v1",
            "1.0.0",
            &[0u8; 32],
            &[1u8; 32],
            10240,
        )
        .await
        .unwrap();

        db.update_current_version("test-id", "v1", "1.0.0")
            .await
            .unwrap();

        let binary = db.get_binary("test-id").await.unwrap().unwrap();
        assert_eq!(binary.current_version_id, Some("v1".to_string()));
        assert_eq!(binary.current_version_string, Some("1.0.0".to_string()));
    }

    #[tokio::test]
    async fn test_update_current_version_nonexistent_version() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // P0 Issue 15 Fix: Attempting to update to a version not in installed_versions
        // should fail and rollback
        let result = db.update_current_version("test-id", "v1", "1.0.0").await;

        assert!(result.is_err());
        match result {
            Err(DbError::NotFound(msg)) => {
                assert!(msg.contains("not found in installed_versions"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_set_auto_update() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        db.set_auto_update("test-id", true).await.unwrap();

        let binary = db.get_binary("test-id").await.unwrap().unwrap();
        assert!(binary.auto_update);
    }

    #[tokio::test]
    async fn test_delete_managed_binary() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        db.delete_managed_binary("test-id").await.unwrap();

        let binary = db.get_binary("test-id").await.unwrap();
        assert!(binary.is_none());
    }

    #[tokio::test]
    async fn test_record_update_lifecycle() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        let update_id = db
            .record_update_start(NewUpdateRecord {
                binary_id: "test-id".to_string(),
                from_version_id: Some("v1".to_string()),
                from_version_string: Some("1.0.0".to_string()),
                to_version_id: "v2".to_string(),
                to_version_string: "2.0.0".to_string(),
            })
            .await
            .unwrap();

        db.set_update_status(update_id, UpdateHistoryStatus::Applying)
            .await
            .unwrap();

        let metrics = UpdateMetrics {
            diff_id: Some("diff-id".to_string()),
            diff_algorithm: Some("bsdiff".to_string()),
            diff_size_bytes: Some(1024),
            full_size_bytes: Some(10240),
            actual_downloaded_bytes: Some(1024),
            download_time_ms: Some(500),
            apply_time_ms: Some(200),
            verify_time_ms: Some(100),
        };

        db.record_update_complete(update_id, true, None, metrics)
            .await
            .unwrap();

        let update = db.get_update(update_id).await.unwrap().unwrap();
        assert_eq!(update.status, "completed");
        assert_eq!(update.success, Some(true));
    }

    #[tokio::test]
    async fn test_backup_operations() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        let backup_id = db
            .create_backup(
                "test-id",
                "v1",
                "1.0.0",
                "/backup/path",
                &[0u8; 32],
                10240,
                None,
            )
            .await
            .unwrap();

        let backup = db.get_backup(backup_id).await.unwrap().unwrap();
        assert_eq!(backup.binary_id, "test-id");
        assert_eq!(backup.version_string, "1.0.0");

        let backups = db.list_backups("test-id").await.unwrap();
        assert_eq!(backups.len(), 1);

        db.delete_backup(backup_id).await.unwrap();

        let backup = db.get_backup(backup_id).await.unwrap();
        assert!(backup.is_none());
    }

    #[tokio::test]
    async fn test_config_operations() {
        let (db, _temp) = create_test_db().await;

        db.set_config("test_key", "test_value").await.unwrap();

        let value = db.get_config("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        db.set_config("test_key", "new_value").await.unwrap();
        let value = db.get_config("test_key").await.unwrap();
        assert_eq!(value, Some("new_value".to_string()));
    }

    // =========================================================================
    // P2 Issue 89 Fix: Error Scenario Tests
    // =========================================================================

    #[tokio::test]
    async fn test_foreign_key_violation() {
        let (db, _temp) = create_test_db().await;

        // Attempt to insert an installed_version for a non-existent binary
        let result = db
            .record_installed_version(
                "non-existent-binary-id",
                "v1",
                "1.0.0",
                &[0u8; 32],
                &[1u8; 32],
                10240,
            )
            .await;

        // Should fail due to foreign key constraint
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unique_constraint_violation() {
        let (db, _temp) = create_test_db().await;

        // Register a binary
        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "unique-app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // Attempt to register another binary with the same name (violates UNIQUE constraint)
        let result = db
            .register_binary(NewManagedBinary {
                binary_id: "different-id".to_string(),
                binary_name: "unique-app".to_string(), // Same name
                platform: "linux-x86_64".to_string(),
                install_path: "/different/path".to_string(),
                publisher_public_key: vec![1u8; 32],
            })
            .await;

        // Should fail due to unique constraint on binary_name
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_transaction_rollback() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // Record an installed version
        db.record_installed_version(
            "test-id",
            "v1",
            "1.0.0",
            &[0u8; 32],
            &[1u8; 32],
            10240,
        )
        .await
        .unwrap();

        // Test that update_current_version rolls back on error
        // (trying to set a non-existent version)
        let result = db.update_current_version("test-id", "v999", "999.0.0").await;

        // Should fail and rollback
        assert!(result.is_err());

        // Verify the binary state hasn't changed
        let binary = db.get_binary("test-id").await.unwrap().unwrap();
        assert_eq!(binary.current_version_id, None);
    }

    #[tokio::test]
    async fn test_cascade_delete() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // Create an installed version
        let version_id = db
            .record_installed_version(
                "test-id",
                "v1",
                "1.0.0",
                &[0u8; 32],
                &[1u8; 32],
                10240,
            )
            .await
            .unwrap();

        // Create a backup
        let backup_id = db
            .create_backup(
                "test-id",
                "v1",
                "1.0.0",
                "/backup/path",
                &[0u8; 32],
                10240,
                None,
            )
            .await
            .unwrap();

        // Delete the binary (should cascade delete installed_versions and backups)
        db.delete_managed_binary("test-id").await.unwrap();

        // Verify cascading deletes worked
        let binary = db.get_binary("test-id").await.unwrap();
        assert!(binary.is_none());

        let backup = db.get_backup(backup_id).await.unwrap();
        assert!(backup.is_none());
    }

    #[tokio::test]
    async fn test_invalid_version_string() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // P2 Issue 84 Fix test: Reject 0.0.0
        let result = db
            .record_installed_version(
                "test-id",
                "v1",
                "0.0.0",
                &[0u8; 32],
                &[1u8; 32],
                10240,
            )
            .await;
        assert!(result.is_err());

        // P2 Issue 84 Fix test: Reject non-ASCII
        let result = db
            .record_installed_version(
                "test-id",
                "v1",
                "1.0.0-βeta",
                &[0u8; 32],
                &[1u8; 32],
                10240,
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_hash_sizes() {
        let (db, _temp) = create_test_db().await;

        db.register_binary(NewManagedBinary {
            binary_id: "test-id".to_string(),
            binary_name: "app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/path".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

        // Blake3 hash too short
        let result = db
            .record_installed_version(
                "test-id",
                "v1",
                "1.0.0",
                &[0u8; 16], // Wrong size
                &[1u8; 32],
                10240,
            )
            .await;
        assert!(result.is_err());

        // SHA256 hash too long
        let result = db
            .record_installed_version(
                "test-id",
                "v1",
                "1.0.0",
                &[0u8; 32],
                &[1u8; 64], // Wrong size
                10240,
            )
            .await;
        assert!(result.is_err());
    }
}
