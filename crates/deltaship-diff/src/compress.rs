//! Optional zstd compression for diffs.
//!
//! This module is only available when the `compression` feature is enabled.

use std::io::Read;

use crate::error::{DiffError, Result};

/// Default compression level for zstd.
const DEFAULT_COMPRESSION_LEVEL: i32 = 3;

/// Maximum size, in bytes, that any single decompression is allowed to produce.
///
/// Compressed (zstd) data is untrusted network input: a few-kilobyte blob can
/// expand to many gigabytes ("decompression bomb"), which would exhaust memory
/// and crash the process. Decompression is performed through a bounded reader
/// and aborted as soon as the output would exceed this ceiling.
///
/// 1 GiB is a defensible ceiling: it is far larger than any legitimate diff this
/// crate is expected to handle (diffs are typically kilobytes to a few megabytes),
/// while still being small enough to fail fast instead of OOMing a host.
pub const MAX_DECOMPRESSED_SIZE: usize = 1024 * 1024 * 1024;

/// Compress diff data using zstd with the default compression level.
pub fn compress_diff(diff: &[u8]) -> Result<Vec<u8>> {
    compress_diff_with_level(diff, DEFAULT_COMPRESSION_LEVEL)
}

/// Compress diff data using zstd with a configurable compression level.
///
/// # Arguments
///
/// * `diff` - The diff data to compress
/// * `level` - Compression level (1-22, where higher = better compression but slower)
///   - 1-3: Fast compression, lower ratio
///   - 3-9: Balanced (3 is default)
///   - 10-22: Maximum compression, slower
///
/// # Returns
///
/// Compressed diff data
pub fn compress_diff_with_level(diff: &[u8], level: i32) -> Result<Vec<u8>> {
    zstd::encode_all(std::io::Cursor::new(diff), level)
        .map_err(DiffError::IoError)
}

/// Decompress zstd-compressed diff data.
///
/// The compressed input is treated as untrusted: decompression is capped at
/// [`MAX_DECOMPRESSED_SIZE`] to protect against decompression bombs. If the
/// decompressed output would exceed that ceiling, this returns
/// [`DiffError::SizeLimitExceeded`] instead of allocating unbounded memory.
pub fn decompress_diff(compressed: &[u8]) -> Result<Vec<u8>> {
    decompress_diff_bounded(compressed, MAX_DECOMPRESSED_SIZE)
}

/// Decompress zstd-compressed diff data with an explicit output-size ceiling.
///
/// Behaves like [`decompress_diff`] but lets the caller choose the maximum
/// number of bytes the decompressed output may occupy. This is useful when a
/// caller knows a tighter bound (e.g. the expected target file size). If the
/// decompressed output would exceed `max_size`, returns
/// [`DiffError::SizeLimitExceeded`].
pub fn decompress_diff_bounded(compressed: &[u8], max_size: usize) -> Result<Vec<u8>> {
    let decoder = zstd::Decoder::new(std::io::Cursor::new(compressed)).map_err(DiffError::IoError)?;

    // Read at most `max_size + 1` bytes. If we manage to read more than
    // `max_size`, the stream is larger than allowed and we abort. Using a
    // bounded reader means we never allocate more than `max_size + 1` bytes,
    // regardless of how large the (untrusted) stream claims to be.
    let mut out = Vec::new();
    let read = max_size as u64 + 1;
    decoder
        .take(read)
        .read_to_end(&mut out)
        .map_err(DiffError::IoError)?;

    if out.len() > max_size {
        return Err(DiffError::SizeLimitExceeded { limit: max_size });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let data = b"the quick brown fox jumps over the lazy dog".repeat(100);
        let compressed = compress_diff(&data).unwrap();
        let recovered = decompress_diff(&compressed).unwrap();
        assert_eq!(recovered, data);
    }

    #[test]
    fn test_decompress_bomb_rejected() {
        // A tiny compressed blob that expands far beyond a small ceiling.
        // 16 MiB of zeros compresses to a few KB but blows past a 1 MiB cap.
        let bomb_plaintext = vec![0u8; 16 * 1024 * 1024];
        let compressed = compress_diff(&bomb_plaintext).unwrap();
        assert!(
            compressed.len() < 64 * 1024,
            "expected a small compressed blob, got {} bytes",
            compressed.len()
        );

        let max = 1024 * 1024; // 1 MiB ceiling
        let err = decompress_diff_bounded(&compressed, max).unwrap_err();
        match err {
            DiffError::SizeLimitExceeded { limit } => assert_eq!(limit, max),
            other => panic!("expected SizeLimitExceeded, got {other:?}"),
        }
    }

    #[test]
    fn test_decompress_at_limit_ok() {
        // Output exactly at the limit must succeed.
        let data = vec![7u8; 1000];
        let compressed = compress_diff(&data).unwrap();
        let recovered = decompress_diff_bounded(&compressed, 1000).unwrap();
        assert_eq!(recovered, data);
    }
}
