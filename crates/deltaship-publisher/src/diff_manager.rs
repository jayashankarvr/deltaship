//! Diff generation manager for Deltaship Publisher.
//!
//! Handles generating diffs from previous versions to new versions.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Instant;

// The FileExt trait is used for try_lock_shared() method on File
#[allow(unused_imports)]
use fs2::FileExt;
use indicatif::{ProgressBar, ProgressStyle};
use deltaship_crypto::{hash_bytes, hash_file};
use deltaship_db::{DbDiffJob, DbVersion, DiffJobStatus, NewDiffJob, PublisherDb};
use deltaship_diff::{compress_diff, generate_diff, DiffStats};

use crate::config::DELTASHIP_DIR;

/// Default number of previous versions to generate diffs from.
pub const DEFAULT_DIFF_COUNT: usize = 3;

/// Default diff algorithm.
pub const DEFAULT_ALGORITHM: &str = "bsdiff";

/// Result of generating a diff.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Source version string.
    pub from_version: String,
    /// Target version string.
    pub to_version: String,
    /// Path to the generated diff file.
    pub diff_path: PathBuf,
    /// Size of the diff in bytes.
    pub diff_size: u64,
    /// Compression ratio (diff_size / target_size).
    pub compression_ratio: f64,
    /// Time taken to generate the diff in milliseconds.
    pub computation_time_ms: u64,
    /// Database job ID for this diff.
    pub job_id: String,
}

/// Manager for diff generation operations.
pub struct DiffManager {
    /// Number of previous versions to generate diffs from.
    diff_count: usize,
    /// Algorithm to use for diff generation.
    algorithm: String,
    /// Output directory for diffs.
    output_dir: PathBuf,
}

impl DiffManager {
    /// Create a new DiffManager with default settings.
    pub fn new() -> Self {
        Self {
            diff_count: DEFAULT_DIFF_COUNT,
            algorithm: DEFAULT_ALGORITHM.to_string(),
            output_dir: PathBuf::from(DELTASHIP_DIR).join("diffs"),
        }
    }

    /// Set the number of previous versions to generate diffs from.
    // Builder method for callers customizing diff generation (e.g., CI pipelines).
    #[allow(dead_code)]
    pub fn with_diff_count(mut self, count: usize) -> Self {
        self.diff_count = count;
        self
    }

    /// Set the output directory for diffs.
    // Builder method for callers customizing diff output location.
    #[allow(dead_code)]
    pub fn with_output_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.output_dir = dir.as_ref().to_path_buf();
        self
    }

    /// Generate diffs from previous versions to the new version.
    ///
    /// Finds the last N versions (configurable, default 3) and generates
    /// a diff from each to the new version.
    pub async fn generate_diff_for_version(
        &self,
        db: &PublisherDb,
        binary_name: &str,
        binary_id: &str,
        new_version: &DbVersion,
    ) -> Result<Vec<DiffResult>, Box<dyn std::error::Error>> {
        // Get all versions for this binary, ordered by creation date (newest first)
        let versions = db.list_versions(binary_id).await?;

        // Filter out the current version and take the last N
        let previous_versions: Vec<_> = versions
            .into_iter()
            .filter(|v| v.version_id != new_version.version_id)
            .take(self.diff_count)
            .collect();

        if previous_versions.is_empty() {
            tracing::info!(binary = %binary_name, "No previous versions found for diffing");
            return Ok(Vec::new());
        }

        // Create output directory if it doesn't exist
        let binary_diff_dir = self.output_dir.join(binary_name);
        fs::create_dir_all(&binary_diff_dir)?;

        let mut results = Vec::new();

        for old_version in previous_versions {
            match self
                .generate_single_diff(db, &old_version, new_version, &binary_diff_dir)
                .await
            {
                Ok(Some(result)) => {
                    tracing::info!(
                        from = %result.from_version,
                        to = %result.to_version,
                        diff_bytes = result.diff_size,
                        ratio_pct = result.compression_ratio * 100.0,
                        "Generated diff"
                    );
                    results.push(result);
                }
                Ok(None) => {
                    tracing::info!(
                        from = %old_version.version_string,
                        to = %new_version.version_string,
                        "Skipped diff — delta larger than full binary"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        from = %old_version.version_string,
                        to = %new_version.version_string,
                        error = %e,
                        "Failed to generate diff"
                    );
                }
            }
        }

        Ok(results)
    }

    /// Generate a single diff between two versions.
    ///
    /// Returns `None` when the compressed delta is not smaller than the full
    /// new binary — in that case the client should download the full binary.
    async fn generate_single_diff(
        &self,
        db: &PublisherDb,
        from_version: &DbVersion,
        to_version: &DbVersion,
        output_dir: &Path,
    ) -> Result<Option<DiffResult>, Box<dyn std::error::Error>> {
        // Create diff job in database
        let job = db
            .insert_diff_job(NewDiffJob {
                from_version_id: from_version.version_id.clone(),
                to_version_id: to_version.version_id.clone(),
                diff_algorithm: self.algorithm.clone(),
            })
            .await?;

        // Mark job as running
        db.set_diff_job_running(&job.job_id).await?;

        let diff_filename = format!(
            "{}-to-{}.patch",
            from_version.version_string, to_version.version_string
        );
        let diff_path = output_dir.join(&diff_filename);

        let old_path = Path::new(&from_version.file_path);
        let new_path = Path::new(&to_version.file_path);

        // Open files immediately to prevent TOCTOU race conditions.
        // This atomically verifies existence and acquires a handle.
        let mut old_file = match File::open(old_path) {
            Ok(f) => f,
            Err(e) => {
                let err = format!("Failed to open source file {}: {}", old_path.display(), e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
        };
        let mut new_file = match File::open(new_path) {
            Ok(f) => f,
            Err(e) => {
                let err = format!("Failed to open target file {}: {}", new_path.display(), e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
        };

        // Acquire shared read locks to prevent modification during diff generation.
        // This ensures the file contents remain consistent throughout the operation.
        // Use retry logic with exponential backoff since try_lock_shared may fail if
        // the file is temporarily locked by another process.
        const MAX_LOCK_RETRIES: u32 = 5;
        const INITIAL_BACKOFF_MS: u64 = 10;

        // Try to acquire lock on source file with retries
        let mut retry_count = 0;
        loop {
            match old_file.try_lock_shared() {
                Ok(_) => break,
                Err(_) if retry_count < MAX_LOCK_RETRIES => {
                    let backoff_ms = INITIAL_BACKOFF_MS * 2u64.pow(retry_count);
                    retry_count += 1;
                    std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                }
                Err(e) => {
                    let err = format!(
                        "Failed to acquire read lock on source file {} after {} retries: {}",
                        old_path.display(),
                        MAX_LOCK_RETRIES,
                        e
                    );
                    db.set_diff_job_failed(&job.job_id, &err).await?;
                    return Err(err.into());
                }
            }
        }

        // Try to acquire lock on target file with retries
        retry_count = 0;
        loop {
            match new_file.try_lock_shared() {
                Ok(_) => break,
                Err(_) if retry_count < MAX_LOCK_RETRIES => {
                    let backoff_ms = INITIAL_BACKOFF_MS * 2u64.pow(retry_count);
                    retry_count += 1;
                    std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
                }
                Err(e) => {
                    let err = format!(
                        "Failed to acquire read lock on target file {} after {} retries: {}",
                        new_path.display(),
                        MAX_LOCK_RETRIES,
                        e
                    );
                    db.set_diff_job_failed(&job.job_id, &err).await?;
                    return Err(err.into());
                }
            }
        }

        // Read file contents while holding the locks
        let mut old_data = Vec::new();
        let mut new_data = Vec::new();

        // Show progress for large files
        let old_size = from_version.file_size_bytes as u64;
        let new_size = to_version.file_size_bytes as u64;
        let show_progress = old_size > 10 * 1024 * 1024 || new_size > 10 * 1024 * 1024;

        if show_progress {
            let pb = ProgressBar::new(old_size + new_size);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] Reading files [{bar:40.cyan/blue}] {bytes}/{total_bytes}")
                    .unwrap()
                    .progress_chars("#>-")
            );

            if let Err(e) = old_file.read_to_end(&mut old_data) {
                pb.finish_and_clear();
                let err = format!("Failed to read source file {}: {}", old_path.display(), e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
            pb.set_position(old_size);

            if let Err(e) = new_file.read_to_end(&mut new_data) {
                pb.finish_and_clear();
                let err = format!("Failed to read target file {}: {}", new_path.display(), e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
            pb.finish_and_clear();
        } else {
            if let Err(e) = old_file.read_to_end(&mut old_data) {
                let err = format!("Failed to read source file {}: {}", old_path.display(), e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
            if let Err(e) = new_file.read_to_end(&mut new_data) {
                let err = format!("Failed to read target file {}: {}", new_path.display(), e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
        }

        // Integrity verification: verify both source and target binary hashes
        // This catches file corruption or unexpected modifications before generating the diff
        if !from_version.file_hash_blake3.is_empty() {
            let actual_hash = hash_bytes(&old_data);
            if actual_hash.to_bytes() != from_version.file_hash_blake3.as_slice() {
                let err = format!(
                    "Source binary integrity check failed for {}: hash mismatch (expected {}, got {})",
                    old_path.display(),
                    hex::encode(&from_version.file_hash_blake3),
                    actual_hash.to_hex()
                );
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
        }

        // Verify target binary hash
        if !to_version.file_hash_blake3.is_empty() {
            let actual_hash = hash_bytes(&new_data);
            if actual_hash.to_bytes() != to_version.file_hash_blake3.as_slice() {
                let err = format!(
                    "Target binary integrity check failed for {}: hash mismatch (expected {}, got {})",
                    new_path.display(),
                    hex::encode(&to_version.file_hash_blake3),
                    actual_hash.to_hex()
                );
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
        }

        // Generate diff with timing (using in-memory data to avoid TOCTOU)
        let start = Instant::now();
        let diff_data = if show_progress {
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} [{elapsed_precise}] {msg}")
                    .unwrap()
            );
            pb.set_message(format!(
                "Computing diff {} -> {}",
                from_version.version_string, to_version.version_string
            ));
            pb.enable_steady_tick(std::time::Duration::from_millis(100));

            let result = match generate_diff(&old_data, &new_data) {
                Ok(d) => d,
                Err(e) => {
                    pb.finish_and_clear();
                    let err = format!("Diff generation failed: {}", e);
                    db.set_diff_job_failed(&job.job_id, &err).await?;
                    return Err(err.into());
                }
            };
            pb.finish_and_clear();
            result
        } else {
            match generate_diff(&old_data, &new_data) {
                Ok(d) => d,
                Err(e) => {
                    let err = format!("Diff generation failed: {}", e);
                    db.set_diff_job_failed(&job.job_id, &err).await?;
                    return Err(err.into());
                }
            }
        };

        // Compress the raw diff before storing
        let compressed_diff = match compress_diff(&diff_data) {
            Ok(c) => c,
            Err(e) => {
                let err = format!("Diff compression failed: {}", e);
                db.set_diff_job_failed(&job.job_id, &err).await?;
                return Err(err.into());
            }
        };

        // If the compressed delta is not smaller than the full new binary, sending
        // it would cost MORE bandwidth than a plain full-binary download. Skip it.
        if compressed_diff.len() >= new_data.len() {
            let reason = format!(
                "Skipped: compressed delta ({} bytes) >= full binary ({} bytes); client will use full binary download",
                compressed_diff.len(),
                new_data.len()
            );
            tracing::info!(
                from = %from_version.version_string,
                to = %to_version.version_string,
                delta_bytes = compressed_diff.len(),
                binary_bytes = new_data.len(),
                "Delta larger than full binary — skipping diff"
            );
            db.set_diff_job_failed(&job.job_id, &reason).await?;
            return Ok(None);
        }

        // Hash and size are computed from the compressed bytes — that is what gets served.
        let expected_hash = hash_bytes(&compressed_diff);

        // Write the compressed diff to disk
        if let Err(e) = fs::write(&diff_path, &compressed_diff) {
            let err = format!("Failed to write diff file {}: {}", diff_path.display(), e);
            db.set_diff_job_failed(&job.job_id, &err).await?;
            return Err(err.into());
        }

        let computation_time_ms = start.elapsed().as_millis() as u64;

        // Compute stats (diff_size is the compressed on-disk size)
        let old_size = old_data.len() as u64;
        let new_size = new_data.len() as u64;
        let diff_size = compressed_diff.len() as u64;
        let compression_ratio = if new_size > 0 {
            diff_size as f64 / new_size as f64
        } else {
            0.0
        };
        let stats = DiffStats {
            old_size,
            new_size,
            diff_size,
            compression_ratio,
        };

        // Locks are automatically released when files go out of scope

        // Compute hash of the written file and verify it matches expected
        let diff_hash = hash_file(&diff_path)?;

        // Verify write succeeded by comparing hashes
        if diff_hash != expected_hash {
            let err = format!(
                "Diff file write verification failed: hash mismatch. Expected {}, got {}",
                expected_hash.to_hex(),
                diff_hash.to_hex()
            );
            db.set_diff_job_failed(&job.job_id, &err).await?;
            // Clean up the corrupted file
            let _ = fs::remove_file(&diff_path);
            return Err(err.into());
        }

        // Mark job as completed
        let diff_path_str = diff_path.to_str().ok_or_else(|| {
            format!(
                "Diff path contains invalid UTF-8: {}",
                diff_path.display()
            )
        })?;
        db.set_diff_job_completed(
            &job.job_id,
            diff_path_str,
            stats.diff_size as i64,
            &diff_hash.to_bytes(),
            computation_time_ms as i64,
        )
        .await?;

        Ok(Some(DiffResult {
            from_version: from_version.version_string.clone(),
            to_version: to_version.version_string.clone(),
            diff_path,
            diff_size: stats.diff_size,
            compression_ratio: stats.compression_ratio,
            computation_time_ms,
            job_id: job.job_id,
        }))
    }

    /// Generate a diff between two specific versions by version strings.
    pub async fn generate_diff_between(
        &self,
        db: &PublisherDb,
        binary_name: &str,
        platform: &str,
        from_version_str: &str,
        to_version_str: &str,
        output_path: Option<&Path>,
    ) -> Result<Option<DiffResult>, Box<dyn std::error::Error>> {
        // Find the binary
        let binary = db
            .get_binary_by_name(binary_name, platform)
            .await?
            .ok_or_else(|| format!("Binary not found: {} ({})", binary_name, platform))?;

        // Find the versions
        let from_version = db
            .get_version_by_string(&binary.binary_id, from_version_str)
            .await?
            .ok_or_else(|| format!("Version not found: {}", from_version_str))?;

        let to_version = db
            .get_version_by_string(&binary.binary_id, to_version_str)
            .await?
            .ok_or_else(|| format!("Version not found: {}", to_version_str))?;

        // Determine output directory
        let output_dir = match output_path {
            Some(p) => p.to_path_buf(),
            None => self.output_dir.join(binary_name),
        };
        fs::create_dir_all(&output_dir)?;

        self.generate_single_diff(db, &from_version, &to_version, &output_dir)
            .await
    }

    /// Get the list of pending diff jobs.
    // Public API for status/monitoring commands and admin tooling.
    #[allow(dead_code)]
    pub async fn get_pending_jobs(
        &self,
        db: &PublisherDb,
    ) -> Result<Vec<DbDiffJob>, Box<dyn std::error::Error>> {
        let jobs = db.list_diff_jobs_by_status(DiffJobStatus::Pending).await?;
        Ok(jobs)
    }

    /// Get the list of completed diff jobs for a version.
    pub async fn get_diffs_for_version(
        &self,
        db: &PublisherDb,
        version_id: &str,
    ) -> Result<Vec<DbDiffJob>, Box<dyn std::error::Error>> {
        let jobs = db.list_diff_jobs_by_status(DiffJobStatus::Completed).await?;
        let filtered: Vec<_> = jobs
            .into_iter()
            .filter(|j| j.to_version_id == version_id)
            .collect();
        Ok(filtered)
    }
}

impl Default for DiffManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DiffManager builder ──────────────────────────────────────────────────

    #[test]
    fn test_diff_manager_new_defaults() {
        let mgr = DiffManager::new();
        assert_eq!(mgr.diff_count, DEFAULT_DIFF_COUNT);
        assert_eq!(mgr.algorithm, DEFAULT_ALGORITHM);
        // Default output dir should contain the expected components
        assert!(mgr.output_dir.ends_with("diffs"));
    }

    #[test]
    fn test_diff_manager_default_matches_new() {
        let a = DiffManager::new();
        let b = DiffManager::default();
        assert_eq!(a.diff_count, b.diff_count);
        assert_eq!(a.algorithm, b.algorithm);
        assert_eq!(a.output_dir, b.output_dir);
    }

    #[test]
    fn test_diff_manager_with_diff_count() {
        let mgr = DiffManager::new().with_diff_count(7);
        assert_eq!(mgr.diff_count, 7);
    }

    #[test]
    fn test_diff_manager_with_diff_count_zero() {
        let mgr = DiffManager::new().with_diff_count(0);
        assert_eq!(mgr.diff_count, 0);
    }

    #[test]
    fn test_diff_manager_with_output_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = DiffManager::new().with_output_dir(tmp.path());
        assert_eq!(mgr.output_dir, tmp.path());
    }

    #[test]
    fn test_diff_manager_builder_chaining() {
        let tmp = tempfile::tempdir().unwrap();
        let mgr = DiffManager::new()
            .with_diff_count(5)
            .with_output_dir(tmp.path());
        assert_eq!(mgr.diff_count, 5);
        assert_eq!(mgr.output_dir, tmp.path());
        // Algorithm should still be the default
        assert_eq!(mgr.algorithm, DEFAULT_ALGORITHM);
    }

    // ── DiffResult fields ────────────────────────────────────────────────────

    #[test]
    fn test_diff_result_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let result = DiffResult {
            from_version: "1.0.0".to_string(),
            to_version: "1.1.0".to_string(),
            diff_path: tmp.path().join("patch.bin"),
            diff_size: 1024,
            compression_ratio: 0.5,
            computation_time_ms: 42,
            job_id: "job-abc".to_string(),
        };
        let cloned = result.clone();
        assert_eq!(cloned.from_version, result.from_version);
        assert_eq!(cloned.to_version, result.to_version);
        assert_eq!(cloned.diff_size, result.diff_size);
        assert_eq!(cloned.job_id, result.job_id);
    }

    // ── Round-trip via deltaship_diff ─────────────────────────────────────────────
    //
    // These tests exercise the same generate_diff / apply_patch functions that
    // DiffManager calls internally, verifying the bsdiff round-trip property.

    #[test]
    fn test_diff_roundtrip_small() {
        let old_data = b"hello world version 1.0";
        let new_data = b"hello world version 1.1 with extra bytes";
        let diff = deltaship_diff::generate_diff(old_data, new_data).unwrap();
        let recovered = deltaship_diff::apply_patch(old_data, &diff).unwrap();
        assert_eq!(recovered.as_slice(), new_data);
    }

    #[test]
    fn test_diff_roundtrip_identical() {
        let data = b"exactly the same data";
        let diff = deltaship_diff::generate_diff(data, data).unwrap();
        let recovered = deltaship_diff::apply_patch(data, &diff).unwrap();
        assert_eq!(recovered.as_slice(), data);
    }

    #[test]
    fn test_diff_roundtrip_larger_than_file() {
        // Completely adversarial data — diff may end up bigger than the target.
        // The operation should still succeed and round-trip correctly.
        let old_data: Vec<u8> = (0u8..=127u8).collect();
        let new_data: Vec<u8> = (128u8..=255u8).collect();
        let diff = deltaship_diff::generate_diff(&old_data, &new_data).unwrap();
        let recovered = deltaship_diff::apply_patch(&old_data, &diff).unwrap();
        assert_eq!(recovered, new_data);
    }

    #[test]
    fn test_diff_roundtrip_binary_like_data() {
        // Simulate a small binary that changes a few bytes
        let mut old_data = vec![0xAA_u8; 4096];
        old_data[100] = 0x01;
        old_data[200] = 0x02;

        let mut new_data = old_data.clone();
        new_data[100] = 0xFF;
        new_data[200] = 0xFE;
        new_data.extend_from_slice(b"new section");

        let diff = deltaship_diff::generate_diff(&old_data, &new_data).unwrap();
        let recovered = deltaship_diff::apply_patch(&old_data, &diff).unwrap();
        assert_eq!(recovered, new_data);
    }

    // ── Constants ────────────────────────────────────────────────────────────

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_DIFF_COUNT, 3);
        assert_eq!(DEFAULT_ALGORITHM, "bsdiff");
    }
}
