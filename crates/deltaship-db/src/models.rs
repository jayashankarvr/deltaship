//! Database model structs that map to table rows.
//!
//! These structs are used for reading from and writing to the database.

use sqlx::FromRow;
use std::fmt;
use std::str::FromStr;

use crate::error::DbError;

// =============================================================================
// Validation Constants and Functions
// =============================================================================

/// Known platform values that match the Platform enum in deltaship-core.
/// Both hyphen and underscore variants are included for flexible matching.
///
/// # P3 Issue #105 Fix: Platform List Centralized
///
/// This now references the centralized platform list from deltaship-core::Platform
/// instead of maintaining a separate hardcoded list.
pub const KNOWN_PLATFORMS: &[&str] = deltaship_core::Platform::all_platform_variants();

/// Canonical platform names for display purposes.
///
/// # P3 Issue #105 Fix: Platform List Centralized
///
/// This now references the centralized platform list from deltaship-core::Platform
/// instead of maintaining a separate hardcoded list.
pub const CANONICAL_PLATFORMS: &[&str] = deltaship_core::Platform::all_platforms();

/// Validate that a platform string matches a known Platform enum value.
///
/// Returns Ok(()) if valid, or Err with an InvalidData error if not recognized.
/// Accepts both hyphen and underscore separators (e.g., "linux-x86_64" or "linux_x86_64").
pub fn validate_platform(platform: &str) -> Result<(), DbError> {
    let normalized = platform.to_lowercase();
    for known in KNOWN_PLATFORMS {
        if *known == normalized.as_str() {
            return Ok(());
        }
    }
    Err(DbError::InvalidData(format!(
        "Unknown platform '{}'. Known platforms: {}",
        platform,
        CANONICAL_PLATFORMS.join(", ")
    )))
}

/// Known diff algorithm values.
///
/// # P1 Issue DB-P1-1 Fix: Diff Algorithm Validation
///
/// These are the valid diff algorithms that can be used for binary patching.
pub const KNOWN_DIFF_ALGORITHMS: &[&str] = &["bsdiff", "courgette", "xdelta3"];

/// Validate that a diff algorithm string is a known algorithm.
///
/// # P1 Issue DB-P1-1 Fix: Diff Algorithm Validation
///
/// Returns Ok(()) if valid, or Err with an InvalidData error if not recognized.
/// Valid algorithms: bsdiff, courgette, xdelta3
pub fn validate_diff_algorithm(algorithm: &str) -> Result<(), DbError> {
    let normalized = algorithm.to_lowercase();
    for known in KNOWN_DIFF_ALGORITHMS {
        if *known == normalized.as_str() {
            return Ok(());
        }
    }
    Err(DbError::InvalidData(format!(
        "Unknown diff algorithm '{}'. Valid algorithms: {}",
        algorithm,
        KNOWN_DIFF_ALGORITHMS.join(", ")
    )))
}

/// Normalize and validate a version string.
///
/// This function:
/// 1. Trims whitespace
/// 2. Strips optional 'v' or 'V' prefix
/// 3. Validates the result is a valid semver version
/// 4. Enforces length constraints and additional validation
///
/// # P2 Issue 84 Fix: Enhanced Semver Validation
///
/// Additional validations:
/// - Maximum length of 255 characters (database and filesystem compatibility)
/// - Rejects version "0.0.0" (invalid release version)
/// - Ensures version contains only ASCII characters (no Unicode confusables)
///
/// Returns an error if the version string is not valid semver after normalization.
pub fn normalize_version_string(version: &str) -> Result<String, DbError> {
    let trimmed = version.trim();

    // P2 Issue 84 Fix: Check for Unicode characters (only ASCII allowed)
    if !trimmed.is_ascii() {
        return Err(DbError::InvalidData(format!(
            "Version string must contain only ASCII characters, got: '{}'",
            trimmed
        )));
    }

    let normalized = if let Some(stripped) = trimmed.strip_prefix('v') {
        stripped.to_string()
    } else if let Some(stripped) = trimmed.strip_prefix('V') {
        stripped.to_string()
    } else {
        trimmed.to_string()
    };

    // P2 Issue 84 Fix: Enforce maximum length (255 chars for database/filesystem compatibility)
    if normalized.len() > 255 {
        return Err(DbError::InvalidData(format!(
            "Version string too long (max 255 chars): got {} chars",
            normalized.len()
        )));
    }

    // Validate the normalized string is valid semver
    let parsed_version = semver::Version::parse(&normalized).map_err(|e| {
        DbError::InvalidData(format!(
            "Invalid semver version '{}': {}",
            version, e
        ))
    })?;

    // P2 Issue 84 Fix: Reject 0.0.0 as it's not a valid release version
    // 0.0.0 is semantically meaningless in semver and often indicates an error
    if parsed_version.major == 0 && parsed_version.minor == 0 && parsed_version.patch == 0 {
        return Err(DbError::InvalidData(
            "Version 0.0.0 is not a valid release version".to_string()
        ));
    }

    Ok(normalized)
}

// =============================================================================
// Status Enums
// =============================================================================

/// Status of a diff computation job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffJobStatus {
    /// Job is queued, waiting to be processed.
    Pending,
    /// Job is currently being processed.
    Running,
    /// Job completed successfully.
    Completed,
    /// Job failed with an error.
    Failed,
}

impl DiffJobStatus {
    /// Convert to the string representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            DiffJobStatus::Pending => "pending",
            DiffJobStatus::Running => "running",
            DiffJobStatus::Completed => "completed",
            DiffJobStatus::Failed => "failed",
        }
    }
}

impl fmt::Display for DiffJobStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for DiffJobStatus {
    type Err = DbError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(DiffJobStatus::Pending),
            "running" => Ok(DiffJobStatus::Running),
            "completed" => Ok(DiffJobStatus::Completed),
            "failed" => Ok(DiffJobStatus::Failed),
            _ => Err(DbError::InvalidData(format!(
                "Unknown diff job status: '{}'",
                s
            ))),
        }
    }
}

/// Status of an update operation in the client's update history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateHistoryStatus {
    /// Update is being downloaded.
    Downloading,
    /// Update patch is being applied.
    Applying,
    /// Update is being verified.
    Verifying,
    /// Update completed successfully.
    Completed,
    /// Update failed.
    Failed,
    /// Update was rolled back.
    RolledBack,
}

impl UpdateHistoryStatus {
    /// Convert to the string representation stored in the database.
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateHistoryStatus::Downloading => "downloading",
            UpdateHistoryStatus::Applying => "applying",
            UpdateHistoryStatus::Verifying => "verifying",
            UpdateHistoryStatus::Completed => "completed",
            UpdateHistoryStatus::Failed => "failed",
            UpdateHistoryStatus::RolledBack => "rolled_back",
        }
    }
}

impl fmt::Display for UpdateHistoryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for UpdateHistoryStatus {
    type Err = DbError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "downloading" => Ok(UpdateHistoryStatus::Downloading),
            "applying" => Ok(UpdateHistoryStatus::Applying),
            "verifying" => Ok(UpdateHistoryStatus::Verifying),
            "completed" => Ok(UpdateHistoryStatus::Completed),
            "failed" => Ok(UpdateHistoryStatus::Failed),
            "rolled_back" => Ok(UpdateHistoryStatus::RolledBack),
            _ => Err(DbError::InvalidData(format!(
                "Unknown update history status: '{}'",
                s
            ))),
        }
    }
}

/// Validate that an expiration timestamp is in a parseable datetime format.
///
/// # DB-P2-3 Fix: Expiration Timestamp Validation
///
/// This function validates that `expires_at` timestamps are in a format that
/// SQLite can compare correctly with `CURRENT_TIMESTAMP`. Accepted formats:
///
/// - ISO 8601 with 'T' separator: `YYYY-MM-DDTHH:MM:SS` or `YYYY-MM-DDTHH:MM:SSZ`
/// - SQLite format with space separator: `YYYY-MM-DD HH:MM:SS`
/// - ISO 8601 with timezone: `YYYY-MM-DDTHH:MM:SS+HH:MM`
///
/// # Arguments
///
/// * `expires_at` - The timestamp string to validate
///
/// # Returns
///
/// Returns `Ok(())` if the timestamp is valid, or `Err(DbError::InvalidData)` if not.
pub fn validate_expires_at_timestamp(expires_at: &str) -> Result<(), DbError> {
    let trimmed = expires_at.trim();

    // Check minimum length for a datetime (YYYY-MM-DD HH:MM:SS = 19 chars)
    if trimmed.len() < 19 {
        return Err(DbError::InvalidData(format!(
            "Expiration timestamp too short, expected at least 'YYYY-MM-DD HH:MM:SS', got: '{}'",
            expires_at
        )));
    }

    // Extract date and time parts (first 19 chars)
    let datetime_part = &trimmed[..19];

    // Check for valid separator (either 'T' for ISO 8601 or space for SQLite format)
    let has_valid_separator = datetime_part.chars().nth(10).map(|c| c == 'T' || c == ' ').unwrap_or(false);
    if !has_valid_separator {
        return Err(DbError::InvalidData(format!(
            "Expiration timestamp missing valid separator at position 10 (expected 'T' or space), got: '{}'",
            expires_at
        )));
    }

    // Parse date components
    let year: i32 = trimmed[0..4].parse().map_err(|_| {
        DbError::InvalidData(format!("Invalid year in expiration timestamp: '{}'", expires_at))
    })?;

    let month: u32 = trimmed[5..7].parse().map_err(|_| {
        DbError::InvalidData(format!("Invalid month in expiration timestamp: '{}'", expires_at))
    })?;

    let day: u32 = trimmed[8..10].parse().map_err(|_| {
        DbError::InvalidData(format!("Invalid day in expiration timestamp: '{}'", expires_at))
    })?;

    // Parse time components
    let hour: u32 = trimmed[11..13].parse().map_err(|_| {
        DbError::InvalidData(format!("Invalid hour in expiration timestamp: '{}'", expires_at))
    })?;

    let minute: u32 = trimmed[14..16].parse().map_err(|_| {
        DbError::InvalidData(format!("Invalid minute in expiration timestamp: '{}'", expires_at))
    })?;

    let second: u32 = trimmed[17..19].parse().map_err(|_| {
        DbError::InvalidData(format!("Invalid second in expiration timestamp: '{}'", expires_at))
    })?;

    // Validate ranges
    if !(1970..=2100).contains(&year) {
        return Err(DbError::InvalidData(format!(
            "Year out of range (1970-2100) in expiration timestamp: '{}'",
            expires_at
        )));
    }
    if !(1..=12).contains(&month) {
        return Err(DbError::InvalidData(format!(
            "Month out of range (1-12) in expiration timestamp: '{}'",
            expires_at
        )));
    }
    if !(1..=31).contains(&day) {
        return Err(DbError::InvalidData(format!(
            "Day out of range (1-31) in expiration timestamp: '{}'",
            expires_at
        )));
    }
    if hour > 23 {
        return Err(DbError::InvalidData(format!(
            "Hour out of range (0-23) in expiration timestamp: '{}'",
            expires_at
        )));
    }
    if minute > 59 {
        return Err(DbError::InvalidData(format!(
            "Minute out of range (0-59) in expiration timestamp: '{}'",
            expires_at
        )));
    }
    if second > 59 {
        return Err(DbError::InvalidData(format!(
            "Second out of range (0-59) in expiration timestamp: '{}'",
            expires_at
        )));
    }

    Ok(())
}

/// Expected size of Blake3 hashes in bytes.
///
/// # P2 Issue 85 Fix: Blake3 Constant Mismatch Prevention
///
/// This constant is set to 32 bytes, which is the standard Blake3 hash output size.
/// Blake3 always outputs 32 bytes for the default hash function.
///
/// **Note:** The blake3 crate is not a dependency of deltaship-db (to keep the database
/// layer lightweight). The actual Blake3 hashing is done in the deltaship-crypto crate.
/// If you need to verify this constant matches blake3::OUT_LEN, add this assertion
/// to deltaship-crypto where blake3 is already a dependency:
///
/// ```ignore
/// const _: () = assert!(blake3::OUT_LEN == 32);
/// ```
pub const BLAKE3_HASH_SIZE: usize = 32;

/// Expected size of SHA-256 hashes in bytes.
pub const SHA256_HASH_SIZE: usize = 32;

/// Validate that a hash has the expected size.
pub fn validate_hash_size(hash: &[u8], expected_size: usize, hash_name: &str) -> Result<(), DbError> {
    if hash.len() != expected_size {
        return Err(DbError::InvalidData(format!(
            "{} hash must be exactly {} bytes, got {} bytes",
            hash_name,
            expected_size,
            hash.len()
        )));
    }
    Ok(())
}

/// Calculate diff efficiency ratio, handling empty file edge cases.
///
/// # P2 Issue 87 Fix: Diff Empty File Calculation
///
/// This function calculates the diff efficiency as `diff_size / target_size`,
/// handling all edge cases correctly:
///
/// - **Empty target file (target_size = 0)**:
///   - If diff_size = 0: Returns 0.0 (perfect efficiency, no change)
///   - If diff_size > 0: Returns infinity (cannot represent creating something from nothing)
///
/// - **Normal case (target_size > 0)**:
///   - Returns `diff_size / target_size` as a ratio (e.g., 0.5 = 50% of original size)
///
/// # Arguments
///
/// * `diff_size_bytes` - Size of the diff file in bytes
/// * `target_size_bytes` - Size of the target file in bytes
///
/// # Returns
///
/// Returns a ratio where:
/// - 0.0 = perfect efficiency (no diff needed)
/// - < 1.0 = diff is smaller than target (efficient)
/// - 1.0 = diff is same size as target (no benefit)
/// - > 1.0 = diff is larger than target (inefficient, should use full download)
/// - f64::INFINITY = invalid case (creating file from empty)
pub fn calculate_diff_efficiency(diff_size_bytes: i64, target_size_bytes: i64) -> f64 {
    // Handle empty target file
    if target_size_bytes == 0 {
        // If both are zero, perfect efficiency (no change)
        if diff_size_bytes == 0 {
            return 0.0;
        }
        // If diff size is positive but target is empty, this is invalid
        // (cannot create something from nothing with a diff)
        return f64::INFINITY;
    }

    // Normal case: calculate ratio
    // Convert to f64 for division to avoid integer division
    diff_size_bytes as f64 / target_size_bytes as f64
}

// =============================================================================
// Publisher Database Models
// =============================================================================

/// A binary being published.
#[derive(Debug, Clone, FromRow)]
pub struct DbBinary {
    /// Unique identifier (UUID).
    pub binary_id: String,
    /// Human-readable name (e.g., "myapp").
    pub binary_name: String,
    /// Target platform (e.g., "linux-x86_64").
    pub platform: String,
    /// Local file path to the binary.
    pub binary_path: String,
    /// Optional description.
    pub description: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last update timestamp (ISO 8601).
    pub updated_at: String,
}

/// A version of a binary.
#[derive(Debug, Clone, FromRow)]
pub struct DbVersion {
    /// Unique identifier (UUID from server after registration).
    pub version_id: String,
    /// Binary this version belongs to.
    pub binary_id: String,
    /// Version string (e.g., "1.0.0").
    pub version_string: String,
    /// Local path to binary file.
    pub file_path: String,
    /// File size in bytes.
    pub file_size_bytes: i64,
    /// Blake3 hash of the file (32 bytes).
    pub file_hash_blake3: Vec<u8>,
    /// SHA-256 hash of the file (32 bytes).
    pub file_hash_sha256: Vec<u8>,
    /// Ed25519 signature (64 bytes), optional until signed.
    pub signature_ed25519: Option<Vec<u8>>,
    /// Timestamp when signature was created (ISO 8601).
    pub signature_timestamp: Option<String>,
    /// Timestamp when registered with server (ISO 8601).
    pub registered_at: Option<String>,
    /// Timestamp when published (ISO 8601).
    pub published_at: Option<String>,
    /// Whether this version is published.
    pub is_published: bool,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
}

/// A diff computation job.
#[derive(Debug, Clone, FromRow)]
pub struct DbDiffJob {
    /// Unique job identifier (UUID).
    pub job_id: String,
    /// Source version ID.
    pub from_version_id: String,
    /// Target version ID.
    pub to_version_id: String,
    /// Diff algorithm used (bsdiff, courgette, xdelta3).
    pub diff_algorithm: String,
    /// Job status (pending, running, completed, failed).
    pub status: String,
    /// Local path to generated diff file.
    pub diff_path: Option<String>,
    /// Size of diff file in bytes.
    pub diff_size_bytes: Option<i64>,
    /// Blake3 hash of diff file (32 bytes).
    pub diff_hash_blake3: Option<Vec<u8>>,
    /// Time taken to compute diff in milliseconds.
    pub computation_time_ms: Option<i64>,
    /// Error message if job failed.
    pub error_message: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Timestamp when job started running (ISO 8601).
    /// DB-P2-5 Fix: Used by reset_stale_diff_jobs to detect stuck jobs.
    pub started_at: Option<String>,
    /// Completion timestamp (ISO 8601).
    pub completed_at: Option<String>,
}

// =============================================================================
// Client Database Models
// =============================================================================

/// A binary managed by the client patcher.
#[derive(Debug, Clone, FromRow)]
pub struct DbManagedBinary {
    /// Unique identifier (UUID from server).
    pub binary_id: String,
    /// Human-readable name.
    pub binary_name: String,
    /// Target platform.
    pub platform: String,
    /// Path to the installed binary.
    pub install_path: String,
    /// Currently installed version ID.
    pub current_version_id: Option<String>,
    /// Currently installed version string.
    pub current_version_string: Option<String>,
    /// Publisher's Ed25519 public key (32 bytes).
    pub publisher_public_key: Vec<u8>,
    /// Whether automatic updates are enabled.
    pub auto_update: bool,
    /// Last time update was checked (ISO 8601).
    pub last_check_at: Option<String>,
    /// Last time binary was updated (ISO 8601).
    pub last_update_at: Option<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last update timestamp (ISO 8601).
    pub updated_at: String,
}

/// An installed version record.
#[derive(Debug, Clone, FromRow)]
pub struct DbInstalledVersion {
    /// Auto-increment ID.
    pub id: i64,
    /// Binary this version belongs to.
    pub binary_id: String,
    /// Version ID from server.
    pub version_id: String,
    /// Version string.
    pub version_string: String,
    /// Blake3 hash of the file (32 bytes).
    pub file_hash_blake3: Vec<u8>,
    /// SHA-256 hash of the file (32 bytes).
    pub file_hash_sha256: Vec<u8>,
    /// File size in bytes.
    pub file_size_bytes: i64,
    /// Installation timestamp (ISO 8601).
    pub installed_at: String,
    /// Whether this is the current version.
    pub is_current: bool,
}

/// An update history record.
#[derive(Debug, Clone, FromRow)]
pub struct DbUpdateHistory {
    /// Auto-increment ID.
    pub update_id: i64,
    /// Binary being updated.
    pub binary_id: String,
    /// Source version ID (None for fresh install).
    pub from_version_id: Option<String>,
    /// Source version string.
    pub from_version_string: Option<String>,
    /// Target version ID.
    pub to_version_id: String,
    /// Target version string.
    pub to_version_string: String,
    /// Diff ID from server.
    pub diff_id: Option<String>,
    /// Diff algorithm used.
    pub diff_algorithm: Option<String>,
    /// Size of diff downloaded.
    pub diff_size_bytes: Option<i64>,
    /// Size of full binary (what would have been downloaded without diff).
    pub full_size_bytes: Option<i64>,
    /// Actual bytes downloaded (diff if used, full binary otherwise).
    pub actual_downloaded_bytes: Option<i64>,
    /// Start timestamp (ISO 8601).
    pub started_at: String,
    /// Completion timestamp (ISO 8601).
    pub completed_at: Option<String>,
    /// Status (downloading, applying, verifying, completed, failed, rolled_back).
    pub status: String,
    /// Whether update succeeded.
    pub success: Option<bool>,
    /// Error message if failed.
    pub error_message: Option<String>,
    /// Time spent downloading in milliseconds.
    pub download_time_ms: Option<i64>,
    /// Time spent applying patch in milliseconds.
    pub apply_time_ms: Option<i64>,
    /// Time spent verifying in milliseconds.
    pub verify_time_ms: Option<i64>,
}

/// A rollback backup record.
#[derive(Debug, Clone, FromRow)]
pub struct DbRollbackBackup {
    /// Auto-increment ID.
    pub backup_id: i64,
    /// Binary this backup is for.
    pub binary_id: String,
    /// Version that was backed up.
    pub version_id: String,
    /// Version string.
    pub version_string: String,
    /// Path to backed-up binary.
    pub backup_path: String,
    /// Blake3 hash of backup (32 bytes).
    pub backup_hash_blake3: Vec<u8>,
    /// Size of backup in bytes.
    pub backup_size_bytes: i64,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Expiration timestamp (ISO 8601), None if no expiry.
    pub expires_at: Option<String>,
}

// =============================================================================
// Input structs for inserting new records
// =============================================================================

/// Input for creating a new binary.
#[derive(Debug, Clone)]
pub struct NewBinary {
    pub binary_name: String,
    pub platform: String,
    pub binary_path: String,
    pub description: Option<String>,
}

/// Input for creating a new version.
#[derive(Debug, Clone)]
pub struct NewVersion {
    pub binary_id: String,
    pub version_string: String,
    pub file_path: String,
    pub file_size_bytes: i64,
    pub file_hash_blake3: Vec<u8>,
    pub file_hash_sha256: Vec<u8>,
}

/// Input for creating a new diff job.
#[derive(Debug, Clone)]
pub struct NewDiffJob {
    pub from_version_id: String,
    pub to_version_id: String,
    pub diff_algorithm: String,
}

/// Input for registering a managed binary.
#[derive(Debug, Clone)]
pub struct NewManagedBinary {
    pub binary_id: String,
    pub binary_name: String,
    pub platform: String,
    pub install_path: String,
    pub publisher_public_key: Vec<u8>,
}

/// Input for starting an update.
#[derive(Debug, Clone)]
pub struct NewUpdateRecord {
    pub binary_id: String,
    pub from_version_id: Option<String>,
    pub from_version_string: Option<String>,
    pub to_version_id: String,
    pub to_version_string: String,
}

/// Metrics for completing an update.
#[derive(Debug, Clone, Default)]
pub struct UpdateMetrics {
    pub diff_id: Option<String>,
    pub diff_algorithm: Option<String>,
    pub diff_size_bytes: Option<i64>,
    /// Size of full binary (what would have been downloaded without diff).
    pub full_size_bytes: Option<i64>,
    /// Actual bytes downloaded (diff if used, full binary otherwise).
    pub actual_downloaded_bytes: Option<i64>,
    pub download_time_ms: Option<i64>,
    pub apply_time_ms: Option<i64>,
    pub verify_time_ms: Option<i64>,
}

/// Database statistics for health monitoring.
///
/// # P3 Issue #112 Fix: Database Statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    /// Number of managed binaries (client) or registered binaries (publisher).
    pub managed_binaries: usize,
    /// Number of installed versions (client) or total versions (publisher).
    pub installed_versions: usize,
    /// Number of rollback backups (client) or diff jobs (publisher).
    pub rollback_backups: usize,
    /// Number of update history records.
    pub update_history_records: usize,
    /// Maximum connection pool size.
    pub pool_connections_max: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_platform_valid() {
        assert!(validate_platform("linux-x86_64").is_ok());
        assert!(validate_platform("linux-aarch64").is_ok());
        assert!(validate_platform("windows-x86_64").is_ok());
        assert!(validate_platform("macos-x86_64").is_ok());
        assert!(validate_platform("macos-aarch64").is_ok());
    }

    #[test]
    fn test_validate_platform_underscore() {
        assert!(validate_platform("linux_x86_64").is_ok());
    }

    #[test]
    fn test_validate_platform_case_insensitive() {
        assert!(validate_platform("LINUX-X86_64").is_ok());
        assert!(validate_platform("Linux-X86_64").is_ok());
    }

    #[test]
    fn test_validate_platform_invalid() {
        assert!(validate_platform("invalid-platform").is_err());
        assert!(validate_platform("").is_err());
    }

    #[test]
    fn test_validate_diff_algorithm_valid() {
        assert!(validate_diff_algorithm("bsdiff").is_ok());
        assert!(validate_diff_algorithm("courgette").is_ok());
        assert!(validate_diff_algorithm("xdelta3").is_ok());
    }

    #[test]
    fn test_validate_diff_algorithm_case_insensitive() {
        assert!(validate_diff_algorithm("BSDIFF").is_ok());
        assert!(validate_diff_algorithm("Courgette").is_ok());
        assert!(validate_diff_algorithm("XDelta3").is_ok());
    }

    #[test]
    fn test_validate_diff_algorithm_invalid() {
        assert!(validate_diff_algorithm("invalid-algorithm").is_err());
        assert!(validate_diff_algorithm("").is_err());
        assert!(validate_diff_algorithm("mydiff").is_err());
    }

    #[test]
    fn test_normalize_version_string() {
        assert_eq!(normalize_version_string("1.0.0").unwrap(), "1.0.0");
        assert_eq!(normalize_version_string("  1.0.0  ").unwrap(), "1.0.0");
        assert_eq!(normalize_version_string("v1.0.0").unwrap(), "1.0.0");
        assert_eq!(normalize_version_string("V1.0.0").unwrap(), "1.0.0");
        assert_eq!(normalize_version_string("  v1.0.0  ").unwrap(), "1.0.0");
    }

    #[test]
    fn test_normalize_version_string_invalid() {
        // Invalid semver strings should return an error
        assert!(normalize_version_string("not-a-version").is_err());
        assert!(normalize_version_string("").is_err());
        assert!(normalize_version_string("1.0").is_err());
        assert!(normalize_version_string("garbage").is_err());
        assert!(normalize_version_string("v").is_err());
    }

    #[test]
    fn test_normalize_version_string_valid_semver() {
        // Valid semver with prerelease and build metadata
        assert_eq!(normalize_version_string("1.2.3-alpha").unwrap(), "1.2.3-alpha");
        assert_eq!(normalize_version_string("v2.0.0-beta.1").unwrap(), "2.0.0-beta.1");
        assert_eq!(normalize_version_string("1.0.0+build.123").unwrap(), "1.0.0+build.123");
    }

    #[test]
    fn test_validate_hash_size() {
        let valid_hash = vec![0u8; 32];
        assert!(validate_hash_size(&valid_hash, 32, "Test").is_ok());

        let invalid_hash = vec![0u8; 16];
        assert!(validate_hash_size(&invalid_hash, 32, "Test").is_err());
    }

    #[test]
    fn test_diff_job_status_as_str() {
        assert_eq!(DiffJobStatus::Pending.as_str(), "pending");
        assert_eq!(DiffJobStatus::Running.as_str(), "running");
        assert_eq!(DiffJobStatus::Completed.as_str(), "completed");
        assert_eq!(DiffJobStatus::Failed.as_str(), "failed");
    }

    #[test]
    fn test_diff_job_status_from_str() {
        assert_eq!("pending".parse::<DiffJobStatus>().unwrap(), DiffJobStatus::Pending);
        assert_eq!("running".parse::<DiffJobStatus>().unwrap(), DiffJobStatus::Running);
        assert_eq!("completed".parse::<DiffJobStatus>().unwrap(), DiffJobStatus::Completed);
        assert_eq!("failed".parse::<DiffJobStatus>().unwrap(), DiffJobStatus::Failed);
        assert!("invalid".parse::<DiffJobStatus>().is_err());
    }

    #[test]
    fn test_update_history_status_as_str() {
        assert_eq!(UpdateHistoryStatus::Downloading.as_str(), "downloading");
        assert_eq!(UpdateHistoryStatus::Applying.as_str(), "applying");
        assert_eq!(UpdateHistoryStatus::Verifying.as_str(), "verifying");
        assert_eq!(UpdateHistoryStatus::Completed.as_str(), "completed");
        assert_eq!(UpdateHistoryStatus::Failed.as_str(), "failed");
        assert_eq!(UpdateHistoryStatus::RolledBack.as_str(), "rolled_back");
    }

    #[test]
    fn test_update_history_status_from_str() {
        assert_eq!("downloading".parse::<UpdateHistoryStatus>().unwrap(), UpdateHistoryStatus::Downloading);
        assert_eq!("applying".parse::<UpdateHistoryStatus>().unwrap(), UpdateHistoryStatus::Applying);
        assert_eq!("verifying".parse::<UpdateHistoryStatus>().unwrap(), UpdateHistoryStatus::Verifying);
        assert_eq!("completed".parse::<UpdateHistoryStatus>().unwrap(), UpdateHistoryStatus::Completed);
        assert_eq!("failed".parse::<UpdateHistoryStatus>().unwrap(), UpdateHistoryStatus::Failed);
        assert_eq!("rolled_back".parse::<UpdateHistoryStatus>().unwrap(), UpdateHistoryStatus::RolledBack);
        assert!("invalid".parse::<UpdateHistoryStatus>().is_err());
    }

    // DB-P2-3 Fix: Tests for expires_at timestamp validation
    #[test]
    fn test_validate_expires_at_timestamp_valid() {
        // ISO 8601 format with T separator
        assert!(validate_expires_at_timestamp("2024-12-31T23:59:59").is_ok());
        assert!(validate_expires_at_timestamp("2024-12-31T23:59:59Z").is_ok());
        assert!(validate_expires_at_timestamp("2024-01-15T14:30:45+00:00").is_ok());

        // SQLite format with space separator
        assert!(validate_expires_at_timestamp("2024-12-31 23:59:59").is_ok());
    }

    #[test]
    fn test_validate_expires_at_timestamp_invalid() {
        // Too short
        assert!(validate_expires_at_timestamp("2024-12-31").is_err());
        assert!(validate_expires_at_timestamp("").is_err());

        // Invalid separator
        assert!(validate_expires_at_timestamp("2024-12-31X23:59:59").is_err());

        // Invalid date components
        assert!(validate_expires_at_timestamp("2024-13-31T23:59:59").is_err()); // month > 12
        assert!(validate_expires_at_timestamp("2024-00-31T23:59:59").is_err()); // month = 0
        assert!(validate_expires_at_timestamp("2024-12-32T23:59:59").is_err()); // day > 31
        assert!(validate_expires_at_timestamp("2024-12-00T23:59:59").is_err()); // day = 0

        // Invalid time components
        assert!(validate_expires_at_timestamp("2024-12-31T24:59:59").is_err()); // hour > 23
        assert!(validate_expires_at_timestamp("2024-12-31T23:60:59").is_err()); // minute > 59
        assert!(validate_expires_at_timestamp("2024-12-31T23:59:60").is_err()); // second > 59

        // Year out of range
        assert!(validate_expires_at_timestamp("1969-12-31T23:59:59").is_err());
        assert!(validate_expires_at_timestamp("2101-12-31T23:59:59").is_err());

        // Non-numeric components
        assert!(validate_expires_at_timestamp("XXXX-12-31T23:59:59").is_err());
    }
}
