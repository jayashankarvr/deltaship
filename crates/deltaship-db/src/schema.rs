//! SQL schema definitions for Deltaship databases.
//!
//! This module contains the SQL strings for creating tables in both
//! the Publisher and Client SQLite databases.
//!
//! # Timestamp Format (P2 Issue 68 Fix)
//!
//! All timestamps in Deltaship databases are stored in **UTC ISO 8601 format** using
//! SQLite's `CURRENT_TIMESTAMP` function, which returns timestamps in the format:
//! `YYYY-MM-DD HH:MM:SS` (e.g., "2024-01-15 14:30:45").
//!
//! **Important consistency requirements:**
//! - All timestamp columns use `TEXT` type with `DEFAULT CURRENT_TIMESTAMP`
//! - All application code MUST store timestamps in UTC
//! - When inserting custom timestamps, use ISO 8601 format: `YYYY-MM-DDTHH:MM:SSZ`
//!   or SQLite's `datetime('now')` for current UTC time
//! - Never store local time or timezone-aware timestamps
//!
//! **Examples:**
//! ```sql
//! -- Correct: Using default (automatic UTC timestamp)
//! INSERT INTO table (data) VALUES ('value');  -- created_at filled automatically
//!
//! -- Correct: Explicit UTC timestamp
//! INSERT INTO table (data, created_at) VALUES ('value', datetime('now'));
//!
//! -- Correct: ISO 8601 UTC timestamp from application
//! INSERT INTO table (data, created_at) VALUES ('value', '2024-01-15T14:30:45Z');
//!
//! -- WRONG: Local time or non-UTC timestamp
//! INSERT INTO table (data, created_at) VALUES ('value', '2024-01-15 14:30:45-05:00');
//! ```
//!
//! **Rationale:**
//! - UTC eliminates timezone ambiguities in distributed systems
//! - ISO 8601 provides consistent string-based sorting and comparison
//! - SQLite's `CURRENT_TIMESTAMP` is always UTC, ensuring consistency

/// Current schema version for the publisher database.
pub const PUBLISHER_SCHEMA_VERSION: i32 = 1;

/// Current schema version for the client database.
pub const CLIENT_SCHEMA_VERSION: i32 = 1;

/// SQL to create the schema_version table (common to both databases).
pub const CREATE_SCHEMA_VERSION: &str = r#"
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    description TEXT NOT NULL
);
"#;

/// SQL to insert initial schema version using UPSERT pattern.
pub const INSERT_INITIAL_SCHEMA_VERSION: &str = r#"
INSERT INTO schema_version (version, description)
VALUES (1, 'Initial schema')
ON CONFLICT(version) DO UPDATE SET
    description = excluded.description;
"#;

// =============================================================================
// Publisher Database Tables
// =============================================================================

/// SQL to create the publisher_config table.
/// Stores publisher configuration (API keys, server URL, etc.).
pub const CREATE_PUBLISHER_CONFIG: &str = r#"
CREATE TABLE IF NOT EXISTS publisher_config (
    config_key TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    is_secret INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;

/// SQL to insert default publisher configuration using UPSERT pattern.
/// Only inserts if key doesn't exist; existing values are preserved.
pub const INSERT_DEFAULT_PUBLISHER_CONFIG: &str = r#"
INSERT INTO publisher_config (config_key, config_value, is_secret) VALUES
    ('server_url', 'https://updates.example.com', 0),
    ('publisher_name', '', 0),
    ('api_key', '', 1),
    ('public_key_path', '.deltaship/keys/public.key', 0),
    ('private_key_path', '.deltaship/keys/private.key', 1)
ON CONFLICT(config_key) DO UPDATE SET
    updated_at = CURRENT_TIMESTAMP
WHERE 0; -- Never actually update, just ignore conflicts
"#;

/// SQL to create the binaries table (publisher side).
/// Tracks binaries being published.
pub const CREATE_PUBLISHER_BINARIES: &str = r#"
CREATE TABLE IF NOT EXISTS binaries (
    binary_id TEXT PRIMARY KEY,
    binary_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    binary_path TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (binary_name, platform)
);
"#;

/// SQL to create index on publisher binaries.
pub const CREATE_PUBLISHER_BINARIES_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_publisher_binaries_name ON binaries(binary_name);
"#;

/// SQL to create the versions table (publisher side).
/// Tracks registered versions.
///
/// # Database Constraints
///
/// ## CHECK Constraint: Published State Consistency
///
/// The table includes a CHECK constraint that enforces consistency between
/// `is_published` and `published_at`:
/// - If `is_published = 1`, then `published_at` must be non-NULL
/// - If `is_published = 0`, then `published_at` must be NULL
///
/// This constraint is enforced at the database level for data integrity.
/// Application code should use [`PublisherDb::set_version_published()`] to
/// update these fields atomically, which sets both values correctly.
///
/// **Important**: Do not update `is_published` or `published_at` independently
/// via raw SQL queries, as this may violate the CHECK constraint. The
/// `set_version_published()` method handles this correctly by setting both
/// `is_published = 1` and `published_at = CURRENT_TIMESTAMP` together.
///
/// If you need to unpublish a version, use [`PublisherDb::set_version_unpublished()`],
/// which sets both `is_published = 0` and `published_at = NULL` in the same update.
///
/// ## Foreign Key CASCADE Behavior
///
/// **CRITICAL**: This table has `ON DELETE CASCADE` on the `binary_id` foreign key.
/// **Deleting a binary will automatically delete ALL versions associated with it.**
///
/// This is intentional to maintain referential integrity, but can result in data loss:
/// - All version records will be deleted
/// - All diff jobs referencing those versions will also be deleted (via cascading deletes)
/// - Version files on disk are NOT automatically deleted and become orphaned
///
/// **Before deleting a binary**, ensure you:
/// 1. Have backups of all version data if needed
/// 2. Manually clean up associated files on disk
/// 3. Understand that this operation cannot be undone
pub const CREATE_PUBLISHER_VERSIONS: &str = r#"
CREATE TABLE IF NOT EXISTS versions (
    version_id TEXT PRIMARY KEY,
    binary_id TEXT NOT NULL,
    version_string TEXT NOT NULL,
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    file_hash_blake3 BLOB NOT NULL,
    file_hash_sha256 BLOB NOT NULL,
    signature_ed25519 BLOB,
    signature_timestamp TEXT,
    registered_at TEXT,
    published_at TEXT,
    is_published INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (binary_id) REFERENCES binaries(binary_id) ON DELETE CASCADE,
    UNIQUE (binary_id, version_string),
    CHECK ((is_published = 0 AND published_at IS NULL) OR (is_published = 1 AND published_at IS NOT NULL))
);
"#;

/// SQL to create indexes on publisher versions.
pub const CREATE_PUBLISHER_VERSIONS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_publisher_versions_binary ON versions(binary_id);
CREATE INDEX IF NOT EXISTS idx_publisher_versions_published ON versions(is_published);
"#;

/// SQL to create the diff_jobs table.
/// Tracks diff computation jobs.
///
/// # Foreign Key CASCADE Behavior
///
/// **CRITICAL**: This table has `ON DELETE CASCADE` on both foreign keys.
/// **Deleting a version will automatically delete ALL diff jobs referencing it.**
///
/// This is intentional to maintain referential integrity, but can result in data loss:
/// - All diff job records (from or to the deleted version) will be deleted
/// - Computed diff files on disk are NOT automatically deleted and become orphaned
/// - Job status and computation history will be lost
///
/// **Cascade deletion happens when**:
/// - A version is deleted directly
/// - A binary is deleted (cascades to versions, then to diff jobs)
///
/// **Before deleting versions**, ensure you:
/// 1. Have backups of diff job data if needed for auditing
/// 2. Manually clean up associated diff files on disk
/// 3. Understand that this operation cannot be undone
pub const CREATE_PUBLISHER_DIFF_JOBS: &str = r#"
CREATE TABLE IF NOT EXISTS diff_jobs (
    job_id TEXT PRIMARY KEY,
    from_version_id TEXT NOT NULL,
    to_version_id TEXT NOT NULL,
    diff_algorithm TEXT NOT NULL DEFAULT 'bsdiff',
    status TEXT NOT NULL DEFAULT 'pending',
    diff_path TEXT,
    diff_size_bytes INTEGER,
    diff_hash_blake3 BLOB,
    computation_time_ms INTEGER,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    FOREIGN KEY (from_version_id) REFERENCES versions(version_id) ON DELETE CASCADE,
    FOREIGN KEY (to_version_id) REFERENCES versions(version_id) ON DELETE CASCADE,
    UNIQUE (from_version_id, to_version_id, diff_algorithm)
);
"#;

/// SQL to create indexes on diff_jobs.
pub const CREATE_PUBLISHER_DIFF_JOBS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_publisher_diff_jobs_status ON diff_jobs(status);
CREATE INDEX IF NOT EXISTS idx_publisher_diff_jobs_from ON diff_jobs(from_version_id);
CREATE INDEX IF NOT EXISTS idx_publisher_diff_jobs_to ON diff_jobs(to_version_id);
"#;

// =============================================================================
// Client Database Tables
// =============================================================================

/// SQL to create the client_config table.
/// Client configuration.
pub const CREATE_CLIENT_CONFIG: &str = r#"
CREATE TABLE IF NOT EXISTS client_config (
    config_key TEXT PRIMARY KEY,
    config_value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;

/// SQL to insert default client configuration using UPSERT pattern.
/// Only inserts if key doesn't exist; existing values are preserved.
pub const INSERT_DEFAULT_CLIENT_CONFIG: &str = r#"
INSERT INTO client_config (config_key, config_value) VALUES
    ('server_url', 'https://updates.example.com'),
    ('check_interval_seconds', '3600'),
    ('auto_update_enabled', '1'),
    ('verify_signatures', '1'),
    ('rollback_enabled', '1'),
    ('max_rollback_backups', '3')
ON CONFLICT(config_key) DO UPDATE SET
    updated_at = CURRENT_TIMESTAMP
WHERE 0; -- Never actually update, just ignore conflicts
"#;

/// SQL to create the managed_binaries table.
/// Binaries managed by this client.
pub const CREATE_CLIENT_MANAGED_BINARIES: &str = r#"
CREATE TABLE IF NOT EXISTS managed_binaries (
    binary_id TEXT PRIMARY KEY,
    binary_name TEXT NOT NULL UNIQUE,
    platform TEXT NOT NULL,
    install_path TEXT NOT NULL,
    current_version_id TEXT,
    current_version_string TEXT,
    publisher_public_key BLOB NOT NULL,
    auto_update INTEGER NOT NULL DEFAULT 1,
    last_check_at TEXT,
    last_update_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#;

/// SQL to create index on managed_binaries.
pub const CREATE_CLIENT_MANAGED_BINARIES_INDEX: &str = r#"
CREATE INDEX IF NOT EXISTS idx_client_binaries_name ON managed_binaries(binary_name);
"#;

/// SQL to create the installed_versions table.
/// History of installed versions.
///
/// # Constraints
///
/// ## Partial Unique Index: One Current Version Per Binary
///
/// The `is_current` column indicates which version is currently active for each binary.
/// A partial unique index ensures that at most one version per binary can have `is_current = 1`.
/// This is enforced by `CREATE_CLIENT_INSTALLED_VERSIONS_INDEXES`.
///
/// Application code in [`ClientDb::update_current_version()`] handles this correctly by:
/// 1. Clearing `is_current = 0` on all versions for the binary
/// 2. Setting `is_current = 1` on the new current version
/// 3. Both operations run in a transaction for atomicity
///
/// ## Foreign Key CASCADE Behavior
///
/// **CRITICAL**: This table has `ON DELETE CASCADE` on the `binary_id` foreign key.
/// **Deleting a managed binary will automatically delete ALL installed version records.**
///
/// This is intentional to maintain referential integrity, but results in data loss:
/// - All version installation history will be deleted
/// - You will lose the ability to rollback to previous versions
/// - The actual binary files on disk are NOT deleted automatically
///
/// **Before deleting a managed binary**, ensure you:
/// 1. No longer need version history or rollback capability
/// 2. Have backups if needed for auditing purposes
/// 3. Understand that this operation cannot be undone
pub const CREATE_CLIENT_INSTALLED_VERSIONS: &str = r#"
CREATE TABLE IF NOT EXISTS installed_versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    version_string TEXT NOT NULL,
    file_hash_blake3 BLOB NOT NULL,
    file_hash_sha256 BLOB NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_current INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE,
    UNIQUE (binary_id, version_id)
);
"#;

/// SQL to create indexes on installed_versions.
///
/// Includes a partial unique index that ensures only one version per binary can have
/// `is_current = 1`. SQLite supports partial indexes using WHERE clauses.
pub const CREATE_CLIENT_INSTALLED_VERSIONS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_client_versions_binary ON installed_versions(binary_id);
CREATE INDEX IF NOT EXISTS idx_client_versions_current ON installed_versions(binary_id, is_current);
CREATE INDEX IF NOT EXISTS idx_client_versions_version_id ON installed_versions(version_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_client_versions_one_current
    ON installed_versions(binary_id) WHERE is_current = 1;
"#;

/// SQL to create the update_history table.
/// Tracks all update attempts.
///
/// # Foreign Key CASCADE Behavior
///
/// **CRITICAL**: This table has `ON DELETE CASCADE` on the `binary_id` foreign key.
/// **Deleting a managed binary will automatically delete ALL update history records.**
///
/// This is intentional to maintain referential integrity, but results in data loss:
/// - All update attempt records will be deleted
/// - Download statistics and performance metrics will be lost
/// - Error history and diagnostics will be lost
///
/// **Before deleting a managed binary**, ensure you:
/// 1. Have exported any metrics or statistics you need
/// 2. Have backups if needed for auditing or debugging
/// 3. Understand that this operation cannot be undone
pub const CREATE_CLIENT_UPDATE_HISTORY: &str = r#"
CREATE TABLE IF NOT EXISTS update_history (
    update_id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,
    from_version_id TEXT,
    from_version_string TEXT,
    to_version_id TEXT NOT NULL,
    to_version_string TEXT NOT NULL,
    diff_id TEXT,
    diff_algorithm TEXT,
    diff_size_bytes INTEGER,
    full_size_bytes INTEGER,
    actual_downloaded_bytes INTEGER,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    status TEXT NOT NULL,
    success INTEGER,
    error_message TEXT,
    download_time_ms INTEGER,
    apply_time_ms INTEGER,
    verify_time_ms INTEGER,
    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);
"#;

/// SQL to create indexes on update_history.
///
/// Note: SQLite ignores DESC in index definitions, so we create the index
/// without it. Queries can still use ORDER BY DESC and SQLite will scan
/// the index in reverse order as needed.
pub const CREATE_CLIENT_UPDATE_HISTORY_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_client_update_history_binary ON update_history(binary_id, started_at);
CREATE INDEX IF NOT EXISTS idx_client_update_history_status ON update_history(status);
"#;

/// SQL to create the rollback_backups table.
/// Stores backup information for rollbacks.
///
/// # Foreign Key CASCADE Behavior
///
/// **CRITICAL**: This table has `ON DELETE CASCADE` on the `binary_id` foreign key.
/// **Deleting a managed binary will automatically delete ALL rollback backup records.**
///
/// This is intentional to maintain referential integrity, but results in data loss:
/// - All rollback backup records will be deleted
/// - You will lose tracking information for backup files
/// - **The actual backup files on disk are NOT deleted automatically and become orphaned**
///
/// **Before deleting a managed binary**, ensure you:
/// 1. Manually delete backup files from disk to prevent orphaned files
/// 2. Have other backups if needed for disaster recovery
/// 3. Understand that this operation cannot be undone
pub const CREATE_CLIENT_ROLLBACK_BACKUPS: &str = r#"
CREATE TABLE IF NOT EXISTS rollback_backups (
    backup_id INTEGER PRIMARY KEY AUTOINCREMENT,
    binary_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    version_string TEXT NOT NULL,
    backup_path TEXT NOT NULL,
    backup_hash_blake3 BLOB NOT NULL,
    backup_size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TEXT,
    FOREIGN KEY (binary_id) REFERENCES managed_binaries(binary_id) ON DELETE CASCADE
);
"#;

/// SQL to create indexes on rollback_backups.
pub const CREATE_CLIENT_ROLLBACK_BACKUPS_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_client_backups_binary ON rollback_backups(binary_id);
CREATE INDEX IF NOT EXISTS idx_client_backups_expires ON rollback_backups(expires_at);
"#;
