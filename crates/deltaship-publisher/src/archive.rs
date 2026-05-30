//! Archive utilities for export/import functionality.
//!
//! Provides helpers for creating and extracting tar.gz archives.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::Utc;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use tar::{Archive, Builder};
use thiserror::Error;

/// Current archive format version.
///
/// # Versioning Strategy
///
/// The archive format version follows semantic versioning principles:
///
/// - **Major version** (X.0): Incremented when making incompatible changes that
///   would prevent older tools from reading the archive (e.g., changing the
///   archive structure, removing required fields).
///
/// - **Minor version** (1.X): Incremented when adding backward-compatible features
///   (e.g., new optional metadata fields that older tools can safely ignore).
///
/// When reading archives, tools should:
/// - Reject archives with a higher major version than supported
/// - Accept archives with the same major version but higher minor version
///   (new optional fields will be ignored)
///
/// # History
///
/// - `1.0`: Initial release format with metadata.json, publisher.db, and optional
///   keys/binaries directories.
const ARCHIVE_FORMAT_VERSION: &str = "1.0";

/// Maximum total uncompressed size we will extract from a single archive.
///
/// gzip can achieve very high compression ratios, so a tiny `.tar.gz` can
/// expand to many gigabytes ("decompression bomb"). We cap the cumulative
/// uncompressed output at 4 GiB, which comfortably exceeds a legitimate
/// publisher export (a database plus a handful of binaries/diffs) while
/// preventing an attacker-supplied archive from filling the disk.
const MAX_TOTAL_UNCOMPRESSED: u64 = 4 * 1024 * 1024 * 1024;

/// Maximum uncompressed size for any single entry in the archive.
///
/// No legitimate file in an export (binaries, diffs, the SQLite database)
/// should exceed 1 GiB. Enforcing this per entry stops a single declared-huge
/// member from blowing the budget and bounds the work done per entry.
const MAX_ENTRY_SIZE: u64 = 1024 * 1024 * 1024;

/// Maximum number of entries we will process from an archive.
///
/// Guards against archives with an enormous number of tiny entries, which
/// could exhaust inodes or CPU even while staying under the byte ceilings.
const MAX_ENTRIES: u64 = 100_000;

/// Archive-related errors.
#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid archive: {0}")]
    InvalidArchive(String),

    #[error("Missing required file: {0}")]
    MissingFile(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Archive validation failed: {0}")]
    ValidationFailed(String),
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Export options for creating an archive.
///
/// This struct is part of the stable public API for archive exports.
/// It is kept for future use when implementing the full export command.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Public API - kept for future export command implementation
pub struct ExportOptions {
    /// Include signing keys (dangerous - security risk).
    pub include_keys: bool,
    /// Include binary files.
    pub include_binaries: bool,
    /// Publisher name for metadata.
    pub publisher_name: Option<String>,
}

/// Metadata stored in the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveMetadata {
    /// Deltaship export format version.
    pub format_version: String,
    /// Export timestamp (ISO 8601).
    pub exported_at: String,
    /// Deltaship publisher version.
    pub publisher_version: String,
    /// Optional publisher name.
    pub publisher_name: Option<String>,
    /// Whether signing keys are included.
    pub includes_keys: bool,
    /// Whether binary files are included.
    pub includes_binaries: bool,
    /// Number of binaries in export.
    pub binary_count: usize,
    /// Number of versions in export.
    pub version_count: usize,
}

impl ArchiveMetadata {
    /// Create new metadata for an export.
    pub fn new(
        publisher_name: Option<String>,
        includes_keys: bool,
        includes_binaries: bool,
        binary_count: usize,
        version_count: usize,
    ) -> Self {
        Self {
            format_version: ARCHIVE_FORMAT_VERSION.to_string(),
            exported_at: Utc::now().to_rfc3339(),
            publisher_version: env!("CARGO_PKG_VERSION").to_string(),
            publisher_name,
            includes_keys,
            includes_binaries,
            binary_count,
            version_count,
        }
    }
}

/// Builder for creating tar.gz archives.
pub struct ArchiveBuilder {
    builder: Builder<GzEncoder<File>>,
    base_dir: String,
}

impl ArchiveBuilder {
    /// Create a new archive builder.
    pub fn new(output_path: &Path) -> Result<Self> {
        let file = File::create(output_path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let builder = Builder::new(encoder);

        Ok(Self {
            builder,
            base_dir: "deltaship-export".to_string(),
        })
    }

    /// Add metadata.json to the archive.
    pub fn add_metadata(&mut self, metadata: &ArchiveMetadata) -> Result<()> {
        let json = serde_json::to_string_pretty(metadata)?;
        let path = format!("{}/metadata.json", self.base_dir);
        self.add_data(path.as_ref(), json.as_bytes())?;
        Ok(())
    }

    /// Add a file to the archive.
    pub fn add_file(&mut self, source_path: &Path, archive_path: &str) -> Result<()> {
        let full_archive_path = format!("{}/{}", self.base_dir, archive_path);
        let mut file = File::open(source_path)?;
        self.builder.append_file(&full_archive_path, &mut file)?;
        Ok(())
    }

    /// Add raw data to the archive.
    pub fn add_data(&mut self, archive_path: &str, data: &[u8]) -> Result<()> {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                // A clock set before the Unix epoch would make `duration_since`
                // return an error. Default to 0 (epoch) rather than panicking on
                // a misconfigured system clock.
                .unwrap_or_default()
                .as_secs(),
        );
        header.set_cksum();

        self.builder.append_data(&mut header, archive_path, data)?;
        Ok(())
    }

    /// Add a directory entry (for structure).
    pub fn add_directory(&mut self, archive_path: &str) -> Result<()> {
        let full_path = format!("{}/{}", self.base_dir, archive_path);
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o755);
        header.set_entry_type(tar::EntryType::Directory);
        header.set_mtime(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                // See `add_data`: avoid panicking if the system clock predates
                // the Unix epoch.
                .unwrap_or_default()
                .as_secs(),
        );
        header.set_cksum();

        self.builder
            .append_data(&mut header, &full_path, &[] as &[u8])?;
        Ok(())
    }

    /// Finish writing the archive.
    pub fn finish(self) -> Result<()> {
        let encoder = self.builder.into_inner()?;
        encoder.finish()?;
        Ok(())
    }
}

/// Validate that an archive entry path is safe (no symlinks, no path traversal).
///
/// Security checks performed:
/// - No null bytes or control characters in path
/// - No parent directory traversal (..)
/// - No absolute paths
/// - Path stays within target directory
fn validate_archive_entry_path(entry_path: &Path, target_dir: &Path) -> Result<()> {
    // Check for null bytes and control characters in the path string
    // These could be used to exploit vulnerabilities in file system operations
    let path_str = entry_path.to_string_lossy();
    for (idx, ch) in path_str.chars().enumerate() {
        if ch == '\0' {
            return Err(ArchiveError::ValidationFailed(format!(
                "Null byte detected in archive entry path at position {}: {}",
                idx,
                entry_path.display()
            )));
        }
        // Control characters are ASCII 0x00-0x1F (excluding newline/tab which are caught by filesystem)
        // and 0x7F (DEL). These should not appear in valid file paths.
        if ch.is_ascii_control() {
            return Err(ArchiveError::ValidationFailed(format!(
                "Control character (0x{:02x}) detected in archive entry path at position {}: {}",
                ch as u32,
                idx,
                entry_path.display()
            )));
        }
    }

    // Check for path traversal components (../)
    for component in entry_path.components() {
        match component {
            std::path::Component::ParentDir => {
                return Err(ArchiveError::ValidationFailed(format!(
                    "Path traversal detected in archive entry: {}",
                    entry_path.display()
                )));
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(ArchiveError::ValidationFailed(format!(
                    "Absolute path in archive entry: {}",
                    entry_path.display()
                )));
            }
            _ => {}
        }
    }

    // Build the full target path and verify it stays within target_dir
    let full_path = target_dir.join(entry_path);

    // Normalize the path to resolve any remaining traversal attempts
    // We use a manual normalization since the file doesn't exist yet
    let mut normalized = target_dir.to_path_buf();
    for component in entry_path.components() {
        match component {
            std::path::Component::Normal(c) => normalized.push(c),
            std::path::Component::CurDir => {} // Skip "."
            std::path::Component::ParentDir => {
                // Already checked above, but be extra safe
                return Err(ArchiveError::ValidationFailed(format!(
                    "Path traversal detected: {}",
                    entry_path.display()
                )));
            }
            _ => {
                return Err(ArchiveError::ValidationFailed(format!(
                    "Invalid path component in archive: {}",
                    entry_path.display()
                )));
            }
        }
    }

    // Verify the normalized path starts with target_dir
    if !normalized.starts_with(target_dir) {
        return Err(ArchiveError::ValidationFailed(format!(
            "Archive entry would escape target directory: {} -> {}",
            entry_path.display(),
            full_path.display()
        )));
    }

    Ok(())
}

/// Extract a tar.gz archive to a temporary directory.
pub fn extract_archive(input_path: &Path) -> Result<tempfile::TempDir> {
    let file = File::open(input_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let temp_dir = tempfile::tempdir()?;
    let target_path = temp_dir.path();

    // Running totals used to enforce decompression-bomb / resource limits.
    let mut total_written: u64 = 0;
    let mut entry_count: u64 = 0;

    // Manually extract each entry with validation instead of using unpack()
    for entry_result in archive.entries()? {
        let mut entry = entry_result?;

        // Bound the number of entries we are willing to process.
        entry_count += 1;
        if entry_count > MAX_ENTRIES {
            return Err(ArchiveError::ValidationFailed(format!(
                "Archive contains too many entries (limit: {})",
                MAX_ENTRIES
            )));
        }

        let entry_path = entry.path()?.into_owned();

        // Reject symlinks
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(ArchiveError::ValidationFailed(format!(
                "Symlinks/hardlinks not allowed in archive: {}",
                entry_path.display()
            )));
        }

        // Validate the path is safe
        validate_archive_entry_path(&entry_path, target_path)?;

        // Now extract the entry
        let full_path = target_path.join(&entry_path);

        if entry_type.is_dir() {
            fs::create_dir_all(&full_path)?;
        } else if entry_type.is_file() {
            // Reject entries whose declared size alone exceeds the per-entry
            // ceiling or would overflow the total budget. The header size is
            // attacker-controlled, so we *also* enforce the limit while copying
            // below — this check is a cheap early rejection.
            let declared_size = entry.header().size().unwrap_or(0);
            if declared_size > MAX_ENTRY_SIZE {
                return Err(ArchiveError::ValidationFailed(format!(
                    "Archive entry exceeds per-entry size limit ({} > {}): {}",
                    declared_size,
                    MAX_ENTRY_SIZE,
                    entry_path.display()
                )));
            }

            // Create parent directories if needed
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Copy the entry contents ourselves through a size-limited reader so
            // that a lying tar header cannot cause us to write more than the
            // per-entry budget. `take(MAX_ENTRY_SIZE + 1)` lets us detect when
            // the actual stream exceeds the limit.
            let mut out = File::create(&full_path)?;
            let mut limited = entry.by_ref().take(MAX_ENTRY_SIZE + 1);
            let written = std::io::copy(&mut limited, &mut out)?;

            if written > MAX_ENTRY_SIZE {
                let _ = fs::remove_file(&full_path);
                return Err(ArchiveError::ValidationFailed(format!(
                    "Archive entry exceeds per-entry size limit ({}): {}",
                    MAX_ENTRY_SIZE,
                    entry_path.display()
                )));
            }

            total_written = total_written.saturating_add(written);
            if total_written > MAX_TOTAL_UNCOMPRESSED {
                let _ = fs::remove_file(&full_path);
                return Err(ArchiveError::ValidationFailed(format!(
                    "Archive exceeds total uncompressed size limit ({})",
                    MAX_TOTAL_UNCOMPRESSED
                )));
            }

            // After extraction, verify the file is still within target_path
            // (handles edge cases where the path might resolve outside).
            if full_path.exists() {
                let canonical = full_path.canonicalize()?;
                let canonical_target = target_path.canonicalize()?;
                if !canonical.starts_with(&canonical_target) {
                    // Remove the file and return error
                    let _ = fs::remove_file(&full_path);
                    return Err(ArchiveError::ValidationFailed(format!(
                        "Extracted file escaped target directory: {}",
                        entry_path.display()
                    )));
                }
            }
        }
        // Skip other entry types (devices, etc.)
    }

    Ok(temp_dir)
}

/// Validate an archive and return its metadata.
///
/// This function is part of the stable public API for archive validation.
/// It is kept for use by import/validation commands and external tooling.
#[allow(dead_code)] // Public API - kept for validation commands and external tooling
pub fn validate_archive(input_path: &Path) -> Result<ArchiveMetadata> {
    let file = File::open(input_path)?;
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);

    let mut metadata: Option<ArchiveMetadata> = None;
    let mut has_db = false;

    for entry in archive.entries()? {
        let entry = entry?;
        let path = entry.path()?;
        let path_str = path.to_string_lossy();

        if path_str.ends_with("metadata.json") {
            let mut content = String::new();
            let mut entry = entry;
            entry.read_to_string(&mut content)?;
            metadata = Some(serde_json::from_str(&content)?);
        } else if path_str.ends_with("publisher.db") {
            has_db = true;
        }
    }

    let metadata = metadata.ok_or_else(|| {
        ArchiveError::MissingFile("metadata.json not found in archive".to_string())
    })?;

    if !has_db {
        return Err(ArchiveError::MissingFile(
            "publisher.db not found in archive".to_string(),
        ));
    }

    Ok(metadata)
}

/// Find the deltaship-export directory inside extracted archive.
pub fn find_export_dir(temp_dir: &Path) -> Result<PathBuf> {
    // Look for deltaship-export directory
    for entry in fs::read_dir(temp_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir()
            && path
                .file_name()
                .map(|n| n == "deltaship-export")
                .unwrap_or(false)
        {
            return Ok(path);
        }
    }

    // Fallback: check if files are directly in temp_dir
    if temp_dir.join("metadata.json").exists() {
        return Ok(temp_dir.to_path_buf());
    }

    Err(ArchiveError::InvalidArchive(
        "Could not find deltaship-export directory in archive".to_string(),
    ))
}

/// Read metadata from extracted archive.
pub fn read_extracted_metadata(export_dir: &Path) -> Result<ArchiveMetadata> {
    let metadata_path = export_dir.join("metadata.json");
    let content = fs::read_to_string(&metadata_path)
        .map_err(|_| ArchiveError::MissingFile("metadata.json not found".to_string()))?;
    let metadata: ArchiveMetadata = serde_json::from_str(&content)?;
    Ok(metadata)
}

/// Check if a path exists in the extracted archive.
///
/// This utility function is part of the stable public API for archive inspection.
/// It is kept for use by import commands and external tooling.
#[allow(dead_code)] // Public API - kept for import commands and external tooling
pub fn archive_has_file(export_dir: &Path, relative_path: &str) -> bool {
    export_dir.join(relative_path).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── ArchiveMetadata ──────────────────────────────────────────────────────

    #[test]
    fn test_archive_metadata_new_sets_format_version() {
        let meta = ArchiveMetadata::new(None, false, false, 0, 0);
        assert_eq!(meta.format_version, ARCHIVE_FORMAT_VERSION);
    }

    #[test]
    fn test_archive_metadata_new_publisher_name() {
        let meta = ArchiveMetadata::new(Some("my-publisher".to_string()), false, false, 1, 2);
        assert_eq!(meta.publisher_name.as_deref(), Some("my-publisher"));
        assert_eq!(meta.binary_count, 1);
        assert_eq!(meta.version_count, 2);
    }

    #[test]
    fn test_archive_metadata_includes_flags() {
        let meta = ArchiveMetadata::new(None, true, true, 3, 5);
        assert!(meta.includes_keys);
        assert!(meta.includes_binaries);
    }

    #[test]
    fn test_archive_metadata_roundtrip_json() {
        let meta = ArchiveMetadata::new(Some("test".to_string()), false, true, 2, 4);
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: ArchiveMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.format_version, meta.format_version);
        assert_eq!(decoded.publisher_name, meta.publisher_name);
        assert_eq!(decoded.binary_count, meta.binary_count);
        assert_eq!(decoded.version_count, meta.version_count);
        assert_eq!(decoded.includes_keys, meta.includes_keys);
        assert_eq!(decoded.includes_binaries, meta.includes_binaries);
    }

    // ── ArchiveBuilder + extract_archive round-trip ──────────────────────────

    /// Build a minimal archive (metadata + data file) and extract it.
    ///
    /// Note: `add_data` takes the *full* archive path without any base_dir
    /// prefix (that is applied manually by `add_metadata`).  We therefore
    /// store the helper file under `deltaship-export/extra/hello.txt` so that
    /// `find_export_dir` can resolve it relative to the export root.
    fn build_test_archive(dir: &Path) -> PathBuf {
        let archive_path = dir.join("test.tar.gz");
        let mut builder = ArchiveBuilder::new(&archive_path).unwrap();

        let meta = ArchiveMetadata::new(None, false, false, 0, 0);
        builder.add_metadata(&meta).unwrap();

        // add_data expects the full path inside the archive — no automatic
        // base_dir prefix is applied (unlike add_file/add_directory).
        builder
            .add_data("deltaship-export/extra/hello.txt", b"hello from archive")
            .unwrap();

        builder.finish().unwrap();
        archive_path
    }

    #[test]
    fn test_archive_builder_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = build_test_archive(tmp.path());
        assert!(archive_path.exists());
        assert!(archive_path.metadata().unwrap().len() > 0);
    }

    #[test]
    fn test_extract_archive_produces_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = build_test_archive(tmp.path());
        let extracted = extract_archive(&archive_path).unwrap();
        assert!(extracted.path().is_dir());
    }

    #[test]
    fn test_archive_roundtrip_metadata_json() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = build_test_archive(tmp.path());
        let extracted = extract_archive(&archive_path).unwrap();

        // find_export_dir should locate "deltaship-export/"
        let export_dir = find_export_dir(extracted.path()).unwrap();
        let meta = read_extracted_metadata(&export_dir).unwrap();

        assert_eq!(meta.format_version, ARCHIVE_FORMAT_VERSION);
    }

    #[test]
    fn test_archive_roundtrip_data_file() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = build_test_archive(tmp.path());
        let extracted = extract_archive(&archive_path).unwrap();
        let export_dir = find_export_dir(extracted.path()).unwrap();

        let content = fs::read(export_dir.join("extra/hello.txt")).unwrap();
        assert_eq!(content, b"hello from archive");
    }

    #[test]
    fn test_archive_add_file() {
        let tmp = tempfile::tempdir().unwrap();

        // Write a real file to add
        let source_file = tmp.path().join("source.bin");
        fs::write(&source_file, b"binary content here").unwrap();

        let archive_path = tmp.path().join("with_file.tar.gz");
        let mut builder = ArchiveBuilder::new(&archive_path).unwrap();
        let meta = ArchiveMetadata::new(None, false, true, 0, 0);
        builder.add_metadata(&meta).unwrap();
        builder.add_file(&source_file, "binaries/source.bin").unwrap();
        builder.finish().unwrap();

        let extracted = extract_archive(&archive_path).unwrap();
        let export_dir = find_export_dir(extracted.path()).unwrap();
        let recovered = fs::read(export_dir.join("binaries/source.bin")).unwrap();
        assert_eq!(recovered, b"binary content here");
    }

    #[test]
    fn test_archive_add_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("with_dir.tar.gz");
        let mut builder = ArchiveBuilder::new(&archive_path).unwrap();
        let meta = ArchiveMetadata::new(None, false, false, 0, 0);
        builder.add_metadata(&meta).unwrap();
        builder.add_directory("keys").unwrap();
        builder.finish().unwrap();

        // Should create without error — extraction should succeed
        let extracted = extract_archive(&archive_path).unwrap();
        assert!(extracted.path().is_dir());
    }

    // ── archive_has_file ─────────────────────────────────────────────────────

    #[test]
    fn test_archive_has_file_present() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("present.txt"), b"data").unwrap();
        assert!(archive_has_file(tmp.path(), "present.txt"));
    }

    #[test]
    fn test_archive_has_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!archive_has_file(tmp.path(), "missing.txt"));
    }

    // ── validate_archive_entry_path (path traversal) ─────────────────────────

    #[test]
    fn test_path_traversal_rejected() {
        // Build a raw tar.gz that embeds a `..` path component.
        // The `tar` crate itself may reject writing `..` paths; if so, the
        // archive cannot even be created and our safety guarantee still holds
        // (no archive with path-traversal entries can be crafted via these APIs).
        // When the archive *can* be written (e.g., by an external tool), our
        // `extract_archive` must reject it.
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("evil.tar.gz");

        let file = fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = tar::Builder::new(enc);

        let data = b"oops";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        // The `tar` crate rejects `..` paths at write time, so append_data
        // may fail.  Either outcome — a write error or an extract error —
        // satisfies the security invariant.
        let write_result = tar.append_data(&mut header, "../evil.txt", data.as_ref());
        if write_result.is_err() {
            // The crate-level protection kicked in at write time — test passes.
            return;
        }

        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        // If the archive was written, extraction must still reject it.
        let result = extract_archive(&archive_path);
        assert!(
            result.is_err(),
            "Expected path traversal to be rejected at extract time but got Ok"
        );
    }

    // ── resource limits (decompression bomb) ─────────────────────────────────

    #[test]
    fn test_extract_rejects_too_many_entries() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("many.tar.gz");

        let file = fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::fast());
        let mut tar = tar::Builder::new(enc);

        // Write one more entry than the limit allows. Each entry is empty, so
        // this stays tiny on disk but trips the entry-count ceiling.
        let data: &[u8] = b"";
        for i in 0..(MAX_ENTRIES + 1) {
            let mut header = tar::Header::new_gnu();
            header.set_size(0);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, format!("deltaship-export/f{}.txt", i), data)
                .unwrap();
        }
        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let result = extract_archive(&archive_path);
        assert!(
            result.is_err(),
            "Expected too-many-entries archive to be rejected"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("too many entries"),
            "Unexpected error message: {}",
            msg
        );
    }

    #[test]
    fn test_extract_rejects_oversized_entry_header() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let tmp = tempfile::tempdir().unwrap();
        let archive_path = tmp.path().join("bigheader.tar.gz");

        let file = fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::fast());
        let mut tar = tar::Builder::new(enc);

        // Declare a size far larger than MAX_ENTRY_SIZE in the header while
        // providing only a little actual data. The early header check should
        // reject this before we read the (truncated) body.
        let data = b"small body";
        let mut header = tar::Header::new_gnu();
        header.set_size(MAX_ENTRY_SIZE + 1);
        header.set_mode(0o644);
        header.set_cksum();
        // append_data uses the provided reader length, so build the header
        // manually via append with an explicit oversized size.
        tar.append(&header, &data[..]).unwrap();
        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap();

        let result = extract_archive(&archive_path);
        assert!(
            result.is_err(),
            "Expected oversized-header entry to be rejected"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("per-entry size limit"),
            "Unexpected error message: {}",
            msg
        );
    }

    // ── find_export_dir ──────────────────────────────────────────────────────

    #[test]
    fn test_find_export_dir_missing_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        // No deltaship-export dir and no metadata.json directly in tmp
        let result = find_export_dir(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_find_export_dir_fallback_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        // Place metadata.json directly in root (fallback path)
        fs::write(tmp.path().join("metadata.json"), b"{}").unwrap();
        let dir = find_export_dir(tmp.path()).unwrap();
        assert_eq!(dir, tmp.path());
    }

    #[test]
    fn test_find_export_dir_finds_deltaship_export() {
        let tmp = tempfile::tempdir().unwrap();
        let export_dir = tmp.path().join("deltaship-export");
        fs::create_dir(&export_dir).unwrap();
        let found = find_export_dir(tmp.path()).unwrap();
        assert_eq!(found, export_dir);
    }

    // ── read_extracted_metadata error path ───────────────────────────────────

    #[test]
    fn test_read_extracted_metadata_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = read_extracted_metadata(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_extracted_metadata_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("metadata.json"), b"not json at all").unwrap();
        let result = read_extracted_metadata(tmp.path());
        assert!(result.is_err());
    }
}
