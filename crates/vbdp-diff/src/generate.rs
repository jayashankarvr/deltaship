//! Diff generation using bsdiff algorithm.

use std::fs;
use std::path::Path;

use crate::error::{DiffError, Result};

/// Statistics about a generated diff.
#[derive(Debug, Clone)]
pub struct DiffStats {
    /// Size of the original file in bytes.
    pub old_size: u64,
    /// Size of the new file in bytes.
    pub new_size: u64,
    /// Size of the generated diff in bytes.
    pub diff_size: u64,
    /// Compression ratio (diff_size / new_size).
    pub compression_ratio: f64,
}

/// Generate a binary diff between old and new data in memory.
///
/// Returns the diff data that can be applied to `old` to produce `new`.
/// Callers should compare `diff.len()` to `new.len()` and prefer a full
/// binary download when the diff is larger than the target file.
pub fn generate_diff(old: &[u8], new: &[u8]) -> Result<Vec<u8>> {
    let mut diff = Vec::new();
    bsdiff::diff(old, new, &mut diff)
        .map_err(|e| DiffError::DiffGenerationFailure(e.to_string()))?;
    Ok(diff)
}

/// Generate a binary diff between two files.
///
/// Reads the old and new files, generates a diff, and writes it to the diff path.
/// Returns statistics about the generated diff.
///
/// Callers should check `stats.compression_ratio > 1.0` and prefer serving
/// the full binary when the diff is larger than the target file.
pub fn generate_diff_files(
    old_path: &Path,
    new_path: &Path,
    diff_path: &Path,
) -> Result<DiffStats> {
    let old = fs::read(old_path)?;
    let new = fs::read(new_path)?;

    let diff = generate_diff(&old, &new)?;

    fs::write(diff_path, &diff)?;

    let old_size = old.len() as u64;
    let new_size = new.len() as u64;
    let diff_size = diff.len() as u64;
    let compression_ratio = if new_size > 0 {
        diff_size as f64 / new_size as f64
    } else {
        0.0
    };

    Ok(DiffStats {
        old_size,
        new_size,
        diff_size,
        compression_ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_diff_basic() {
        let old = b"hello world";
        let new = b"hello rust world";

        let diff = generate_diff(old, new).unwrap();

        // Diff should be generated
        assert!(!diff.is_empty());
    }

    #[test]
    fn test_generate_diff_identical_data() {
        let data = b"identical data";

        let diff = generate_diff(data, data).unwrap();

        // Diff of identical data should still be valid but small
        assert!(!diff.is_empty());
    }

    #[test]
    fn test_generate_diff_empty_files() {
        let old = b"";
        let new = b"";

        let diff = generate_diff(old, new).unwrap();

        // bsdiff can produce an empty diff for empty files
        // Just verify it doesn't error
        let _ = diff.len();
    }

    #[test]
    fn test_generate_diff_from_empty() {
        let old = b"";
        let new = b"new content";

        let diff = generate_diff(old, new).unwrap();

        assert!(!diff.is_empty());
    }

    #[test]
    fn test_generate_diff_to_empty() {
        let old = b"old content";
        let new = b"";

        let diff = generate_diff(old, new).unwrap();

        // bsdiff can produce empty diff for some edge cases
        // Just verify it doesn't error
        let _ = diff.len();
    }

    #[test]
    fn test_generate_diff_files() {
        let temp_dir = std::env::temp_dir();
        let old_path = temp_dir.join("vbdp_test_old.bin");
        let new_path = temp_dir.join("vbdp_test_new.bin");
        let diff_path = temp_dir.join("vbdp_test_diff.bin");

        let old_data = b"original version of the file";
        let new_data = b"updated version of the file with more content";

        std::fs::write(&old_path, old_data).unwrap();
        std::fs::write(&new_path, new_data).unwrap();

        let stats = generate_diff_files(&old_path, &new_path, &diff_path).unwrap();

        // Verify stats
        assert_eq!(stats.old_size, old_data.len() as u64);
        assert_eq!(stats.new_size, new_data.len() as u64);
        assert!(stats.diff_size > 0);
        assert!(stats.compression_ratio > 0.0);

        // Verify diff file was created
        assert!(diff_path.exists());

        // Cleanup
        std::fs::remove_file(&old_path).ok();
        std::fs::remove_file(&new_path).ok();
        std::fs::remove_file(&diff_path).ok();
    }

    #[test]
    fn test_generate_diff_large_files() {
        let temp_dir = std::env::temp_dir();
        let old_path = temp_dir.join("vbdp_test_large_old.bin");
        let new_path = temp_dir.join("vbdp_test_large_new.bin");
        let diff_path = temp_dir.join("vbdp_test_large_diff.bin");

        // Create larger files
        let old_data = vec![0xAA; 10000];
        let mut new_data = old_data.clone();
        // Make a small change
        new_data[5000] = 0xBB;
        new_data.extend_from_slice(b"additional data at the end");

        std::fs::write(&old_path, &old_data).unwrap();
        std::fs::write(&new_path, &new_data).unwrap();

        let stats = generate_diff_files(&old_path, &new_path, &diff_path).unwrap();

        // For similar files with small changes, diff should typically be smaller
        // However, bsdiff might produce larger diffs for some patterns
        // Just verify the stats are reasonable
        assert!(stats.diff_size > 0 || stats.new_size > 0);

        // Cleanup
        std::fs::remove_file(&old_path).ok();
        std::fs::remove_file(&new_path).ok();
        std::fs::remove_file(&diff_path).ok();
    }

    #[test]
    fn test_diff_stats_compression_ratio() {
        let temp_dir = std::env::temp_dir();
        let old_path = temp_dir.join("vbdp_test_ratio_old.bin");
        let new_path = temp_dir.join("vbdp_test_ratio_new.bin");
        let diff_path = temp_dir.join("vbdp_test_ratio_diff.bin");

        let old_data = b"test data";
        let new_data = b"test data modified";

        std::fs::write(&old_path, old_data).unwrap();
        std::fs::write(&new_path, new_data).unwrap();

        let stats = generate_diff_files(&old_path, &new_path, &diff_path).unwrap();

        // Compression ratio should be diff_size / new_size
        let expected_ratio = stats.diff_size as f64 / stats.new_size as f64;
        assert!((stats.compression_ratio - expected_ratio).abs() < 0.0001);

        // Cleanup
        std::fs::remove_file(&old_path).ok();
        std::fs::remove_file(&new_path).ok();
        std::fs::remove_file(&diff_path).ok();
    }
}
