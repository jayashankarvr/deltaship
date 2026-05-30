//! Blake3 hashing utilities for VBDP.
//!
//! This module provides Blake3 hashing for file integrity verification.
//! Blake3 produces a **32-byte (256-bit) hash output**, providing 128-bit
//! collision resistance - sufficient for all practical integrity checking.

use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;

use crate::error::{CryptoError, Result};

/// Trait for incremental hash computation.
///
/// This trait abstracts over different hash algorithms (Blake3, SHA256, etc.)
/// to enable generic streaming file hashing without code duplication.
pub trait StreamHasher {
    /// The output type of the hash (e.g., [u8; 32])
    type Output;

    /// Update the hasher with a chunk of data
    fn update(&mut self, data: &[u8]);

    /// Finalize the hash and return the result
    fn finalize(self) -> Self::Output;
}

/// Hash a file using any streaming hasher implementation.
///
/// This generic function eliminates code duplication by working with any
/// hasher that implements the `StreamHasher` trait. It handles:
/// - Memory-efficient streaming (8KB buffer)
/// - Progress reporting
/// - I/O error handling
///
/// # Example
///
/// ```ignore
/// // With Blake3
/// let hash = hash_file_generic(path, blake3::Hasher::new(), |bytes| {})?;
///
/// // With SHA256
/// use sha2::{Sha256, Digest};
/// let hash = hash_file_generic(path, Sha256::new(), |bytes| {})?;
/// ```
pub fn hash_file_generic<H, F>(
    path: &Path,
    mut hasher: H,
    mut progress: F,
) -> Result<H::Output>
where
    H: StreamHasher,
    F: FnMut(u64),
{
    let file = File::open(path).map_err(|e| CryptoError::io(path, e))?;
    let mut reader = BufReader::new(file);

    // 8192 bytes (8 KiB) is chosen as a balance between:
    // - Memory efficiency: Small enough to avoid excessive stack usage
    // - I/O efficiency: Large enough to minimize system call overhead
    // - Cache friendliness: Fits comfortably in L1 cache on most CPUs
    // This is a common buffer size used by BufReader's default and many I/O utilities.
    let mut buffer = [0u8; 8192];
    let mut total_bytes = 0u64;

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| CryptoError::io(path, e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        total_bytes += bytes_read as u64;
        progress(total_bytes);
    }

    Ok(hasher.finalize())
}

/// Implementation of `StreamHasher` for Blake3.
impl StreamHasher for blake3::Hasher {
    type Output = blake3::Hash;

    fn update(&mut self, data: &[u8]) {
        blake3::Hasher::update(self, data);
    }

    fn finalize(self) -> Self::Output {
        blake3::Hasher::finalize(&self)
    }
}

/// Implementation of `StreamHasher` for SHA256.
///
/// This enables SHA256 to use the same generic streaming logic as Blake3,
/// eliminating code duplication across the codebase.
impl StreamHasher for sha2::Sha256 {
    type Output = sha2::digest::Output<sha2::Sha256>;

    fn update(&mut self, data: &[u8]) {
        use sha2::Digest;
        Digest::update(self, data);
    }

    fn finalize(self) -> Self::Output {
        use sha2::Digest;
        Digest::finalize(self)
    }
}

// P2 Issue 85 Fix: Compile-time assertion that Blake3 output is 32 bytes
// This ensures consistency with BLAKE3_HASH_SIZE constant in vbdp-db
const _: () = assert!(
    blake3::OUT_LEN == 32,
    "Blake3 output length must be 32 bytes to match database expectations"
);

/// Blake3 hash (32 bytes / 256 bits).
///
/// Blake3 produces a fixed 32-byte output, providing:
/// - 256-bit preimage resistance
/// - 128-bit collision resistance (birthday bound)
///
/// This is the standard output length for Blake3 and matches common
/// cryptographic hash sizes (SHA-256, etc.).
///
/// # Security Note
///
/// The derived `PartialEq` implementation uses standard byte comparison which
/// may be vulnerable to timing attacks in security-sensitive contexts.
/// For cryptographic verification where timing attacks are a concern,
/// use the [`Hash::ct_eq`] method which provides constant-time comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hash(pub(crate) [u8; 32]);

impl Hash {
    /// Create hash from raw bytes
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Hash(bytes)
    }

    /// Get hash as raw bytes
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Convert hash to hexadecimal string
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut hex = String::with_capacity(64);
        for byte in &self.0 {
            hex.push_str(&format!("{:02x}", byte));
        }
        hex
    }

    /// Parse hash from hexadecimal string
    pub fn from_hex(s: &str) -> Result<Self> {
        if s.len() != 64 {
            return Err(CryptoError::InvalidHex(format!(
                "expected 64 hex chars, got {}",
                s.len()
            )));
        }

        let mut bytes = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hex_byte = std::str::from_utf8(chunk)
                .map_err(|_| CryptoError::InvalidHex("invalid UTF-8".into()))?;
            bytes[i] = u8::from_str_radix(hex_byte, 16)
                .map_err(|_| CryptoError::InvalidHex(format!("invalid hex: {}", hex_byte)))?;
        }

        Ok(Hash(bytes))
    }

    /// Constant-time comparison of two hashes.
    ///
    /// This method provides timing-attack resistant comparison using the `subtle` crate.
    /// Unlike the standard `==` operator (which may short-circuit on first mismatch),
    /// this method always compares all bytes in the same amount of time regardless
    /// of where (or if) they differ.
    ///
    /// Use this method when comparing hashes in security-sensitive contexts, such as:
    /// - Verifying file integrity from untrusted sources
    /// - MAC verification
    /// - Any scenario where timing side-channels could leak information
    ///
    /// # Example
    ///
    /// ```
    /// use vbdp_crypto::{hash_bytes, Hash};
    ///
    /// let hash1 = hash_bytes(b"data");
    /// let hash2 = hash_bytes(b"data");
    /// let hash3 = hash_bytes(b"different");
    ///
    /// assert!(hash1.ct_eq(&hash2));  // Same data, same hash
    /// assert!(!hash1.ct_eq(&hash3)); // Different data, different hash
    /// ```
    #[must_use]
    pub fn ct_eq(&self, other: &Hash) -> bool {
        use subtle::ConstantTimeEq;
        self.0.ct_eq(&other.0).into()
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl FromStr for Hash {
    type Err = CryptoError;

    fn from_str(s: &str) -> Result<Self> {
        Hash::from_hex(s)
    }
}

/// Hash arbitrary bytes
#[must_use]
pub fn hash_bytes(data: &[u8]) -> Hash {
    let hash = blake3::hash(data);
    Hash(*hash.as_bytes())
}

/// Hash a file in a streaming, memory-efficient manner.
///
/// Uses Blake3's incremental hashing API to process files of any size
/// without loading them entirely into memory.
pub fn hash_file(path: &Path) -> Result<Hash> {
    hash_file_with_progress(path, |_| {})
}

/// Hash a file with progress reporting.
///
/// The progress callback is called with the number of bytes processed so far.
/// This allows callers to display progress bars or other status indicators.
pub fn hash_file_with_progress<F>(path: &Path, progress: F) -> Result<Hash>
where
    F: FnMut(u64),
{
    let hasher = blake3::Hasher::new();
    let hash = hash_file_generic(path, hasher, progress)?;
    Ok(Hash(*hash.as_bytes()))
}

/// Compute SHA-256 hash of a file using streaming.
///
/// This is memory-efficient for large files, using the same 8KB buffered
/// streaming pattern as Blake3 hashing.
pub fn sha256_file(path: &Path) -> Result<[u8; 32]> {
    sha256_file_with_progress(path, |_| {})
}

/// Compute SHA-256 hash of a file with progress reporting.
///
/// The progress callback is called with the number of bytes processed so far.
/// Uses the same streaming logic as Blake3 to avoid code duplication.
pub fn sha256_file_with_progress<F>(path: &Path, progress: F) -> Result<[u8; 32]>
where
    F: FnMut(u64),
{
    use sha2::Digest;
    let hasher = sha2::Sha256::new();
    let hash = hash_file_generic(path, hasher, progress)?;
    Ok(hash.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_hash_bytes_consistency() {
        let data = b"test data for hashing";
        let hash1 = hash_bytes(data);
        let hash2 = hash_bytes(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_bytes_different_data() {
        let data1 = b"first data";
        let data2 = b"second data";
        let hash1 = hash_bytes(data1);
        let hash2 = hash_bytes(data2);
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_from_to_bytes() {
        let data = b"test data";
        let hash = hash_bytes(data);
        let bytes = hash.to_bytes();
        let recovered = Hash::from_bytes(bytes);
        assert_eq!(hash, recovered);
    }

    #[test]
    fn test_hash_to_hex() {
        let data = b"test data";
        let hash = hash_bytes(data);
        let hex = hash.to_hex();

        // Hex should be 64 characters long
        assert_eq!(hex.len(), 64);

        // All characters should be valid hex
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_from_hex() {
        let data = b"test data";
        let hash = hash_bytes(data);
        let hex = hash.to_hex();

        let recovered = Hash::from_hex(&hex).unwrap();
        assert_eq!(hash, recovered);
    }

    #[test]
    fn test_hash_from_hex_invalid_length() {
        let result = Hash::from_hex("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_from_hex_invalid_chars() {
        let invalid_hex = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        let result = Hash::from_hex(invalid_hex);
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_display() {
        let data = b"test data";
        let hash = hash_bytes(data);
        let display = format!("{}", hash);
        let hex = hash.to_hex();
        assert_eq!(display, hex);
    }

    #[test]
    fn test_hash_from_str() {
        let data = b"test data";
        let hash = hash_bytes(data);
        let hex = hash.to_hex();

        let recovered: Hash = hex.parse().unwrap();
        assert_eq!(hash, recovered);
    }

    #[test]
    fn test_hash_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_hash_file.bin");

        let data = b"test file content for hashing";
        std::fs::write(&temp_file, data).unwrap();

        let file_hash = hash_file(&temp_file).unwrap();
        let bytes_hash = hash_bytes(data);

        assert_eq!(file_hash, bytes_hash);

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_hash_large_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_large_file.bin");

        // Create a file larger than the buffer size (8192 bytes)
        let mut file = std::fs::File::create(&temp_file).unwrap();
        let chunk = vec![0xAB; 1024];
        for _ in 0..20 {
            file.write_all(&chunk).unwrap();
        }
        drop(file);

        let file_hash = hash_file(&temp_file).unwrap();

        // Hash the same data in memory
        let mut data = Vec::new();
        for _ in 0..20 {
            data.extend_from_slice(&chunk);
        }
        let bytes_hash = hash_bytes(&data);

        assert_eq!(file_hash, bytes_hash);

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_hash_empty_data() {
        let data = b"";
        let hash = hash_bytes(data);

        // Should produce a valid hash for empty data
        assert_eq!(hash.to_bytes().len(), 32);
    }

    #[test]
    fn test_hash_file_not_found() {
        let non_existent = std::path::Path::new("/non/existent/file");
        let result = hash_file(non_existent);
        assert!(result.is_err());
    }

    #[test]
    fn test_hash_binary_data() {
        // Test with various binary patterns
        let patterns = vec![
            vec![0x00; 100],                    // All zeros
            vec![0xFF; 100],                    // All ones
            (0..=255u8).cycle().take(1000).collect::<Vec<_>>(), // Sequential bytes
            vec![0xDE, 0xAD, 0xBE, 0xEF].repeat(250), // Repeated pattern
        ];

        for pattern in patterns {
            let hash1 = hash_bytes(&pattern);
            let hash2 = hash_bytes(&pattern);
            assert_eq!(hash1, hash2, "Hash should be deterministic for binary data");
            assert_eq!(hash1.to_bytes().len(), 32, "Hash should always be 32 bytes");
        }
    }

    #[test]
    fn test_hash_very_large_file() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_very_large_file.bin");

        // Create a 10MB file (larger than typical buffer sizes)
        let mut file = std::fs::File::create(&temp_file).unwrap();
        let chunk = vec![0x42; 1024 * 1024]; // 1MB chunk
        for _ in 0..10 {
            file.write_all(&chunk).unwrap();
        }
        drop(file);

        // Hash the file
        let start = std::time::Instant::now();
        let file_hash = hash_file(&temp_file).unwrap();
        let duration = start.elapsed();

        // Hash should complete in reasonable time
        assert!(duration.as_secs() < 5, "Large file hashing took too long");

        // Verify correctness by hashing in memory
        let mut data = Vec::new();
        for _ in 0..10 {
            data.extend_from_slice(&chunk);
        }
        let bytes_hash = hash_bytes(&data);
        assert_eq!(file_hash, bytes_hash);

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_hash_single_byte() {
        let data = vec![0x42];
        let hash = hash_bytes(&data);
        assert_eq!(hash.to_bytes().len(), 32);
    }

    #[test]
    fn test_hash_boundary_sizes() {
        // Test edge cases around buffer size (8192 bytes)
        let sizes = vec![
            8191, // Just under buffer size
            8192, // Exactly buffer size
            8193, // Just over buffer size
            16384, // Exactly 2x buffer size
            16385, // Just over 2x buffer size
        ];

        for size in sizes {
            let data = vec![0xAB; size];
            let hash = hash_bytes(&data);
            assert_eq!(hash.to_bytes().len(), 32, "Hash should be 32 bytes for size {}", size);

            // Verify file hashing matches byte hashing for this size
            let temp_dir = std::env::temp_dir();
            let temp_file = temp_dir.join(format!("vbdp_test_boundary_{}.bin", size));
            std::fs::write(&temp_file, &data).unwrap();
            let file_hash = hash_file(&temp_file).unwrap();
            assert_eq!(file_hash, hash, "File hash should match byte hash for size {}", size);
            std::fs::remove_file(&temp_file).ok();
        }
    }

    #[test]
    fn test_hash_non_utf8_data() {
        // Test with invalid UTF-8 sequences
        let invalid_utf8 = vec![
            0xFF, 0xFE, 0xFD, // Invalid UTF-8
            0x80, 0x81, 0x82, // Continuation bytes without start
            0xC0, 0xC1,       // Overlong encoding
        ];
        let hash = hash_bytes(&invalid_utf8);
        assert_eq!(hash.to_bytes().len(), 32);
    }

    #[test]
    fn test_hash_zero_bytes() {
        // Test with zero byte patterns
        let data = vec![0x00, 0x00, 0x00, 0x00];
        let hash = hash_bytes(&data);
        assert_eq!(hash.to_bytes().len(), 32);

        // Should be different from empty hash
        let empty_hash = hash_bytes(&[]);
        assert_ne!(hash, empty_hash);
    }

    #[test]
    fn test_hash_avalanche_effect() {
        // Small changes should produce completely different hashes (avalanche effect)
        let data1 = b"test data";
        let data2 = b"test datb"; // Changed last byte

        let hash1 = hash_bytes(data1);
        let hash2 = hash_bytes(data2);

        assert_ne!(hash1, hash2, "Single byte change should produce different hash");

        // Count different bytes in hashes
        let bytes1 = hash1.to_bytes();
        let bytes2 = hash2.to_bytes();
        let different_bytes = bytes1.iter()
            .zip(bytes2.iter())
            .filter(|(a, b)| a != b)
            .count();

        // At least half the bytes should be different (good avalanche)
        assert!(different_bytes >= 16, "Avalanche effect: {} bytes differ", different_bytes);
    }

    #[test]
    fn test_hash_file_with_special_content() {
        let temp_dir = std::env::temp_dir();

        // Test various special file contents
        let test_cases = vec![
            ("empty", vec![]),
            ("single_byte", vec![0x00]),
            ("null_bytes", vec![0x00; 1000]),
            ("high_entropy", (0..1000).map(|i| (i * 7 + 13) as u8).collect()),
        ];

        for (name, content) in test_cases {
            let temp_file = temp_dir.join(format!("vbdp_test_special_{}.bin", name));
            std::fs::write(&temp_file, &content).unwrap();

            let file_hash = hash_file(&temp_file).unwrap();
            let bytes_hash = hash_bytes(&content);

            assert_eq!(file_hash, bytes_hash, "Hashes should match for {}", name);
            std::fs::remove_file(&temp_file).ok();
        }
    }

    #[test]
    fn test_hash_concurrent_access() {
        // Test that hashing the same file multiple times concurrently works
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_concurrent.bin");
        let data = vec![0x42; 10000];
        std::fs::write(&temp_file, &data).unwrap();

        use std::sync::Arc;
        use std::thread;

        let path = Arc::new(temp_file.clone());
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let path = Arc::clone(&path);
                thread::spawn(move || hash_file(&*path))
            })
            .collect();

        let hashes: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect();

        // All hashes should be identical
        for hash in &hashes[1..] {
            assert_eq!(&hashes[0], hash, "Concurrent hashing should produce identical results");
        }

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_sha256_file_basic() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_sha256.bin");
        let data = b"test data for SHA256 hashing";
        std::fs::write(&temp_file, data).unwrap();

        let hash = sha256_file(&temp_file).unwrap();

        // Verify hash is 32 bytes
        assert_eq!(hash.len(), 32);

        // Compute expected hash using sha2 directly
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        Digest::update(&mut hasher, data);
        let expected: [u8; 32] = Digest::finalize(hasher).into();

        assert_eq!(hash, expected, "SHA256 hash should match direct computation");

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_sha256_file_with_progress() {
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_sha256_progress.bin");

        // Create file larger than buffer size
        let mut data = Vec::new();
        for _ in 0..20 {
            data.extend_from_slice(&vec![0xAB; 1024]);
        }
        std::fs::write(&temp_file, &data).unwrap();

        let mut progress_calls = 0u32;
        let mut last_bytes = 0u64;

        let hash = sha256_file_with_progress(&temp_file, |bytes| {
            progress_calls += 1;
            assert!(bytes >= last_bytes, "Progress should be monotonically increasing");
            last_bytes = bytes;
        }).unwrap();

        // Verify progress was called
        assert!(progress_calls > 0, "Progress callback should be called");
        assert_eq!(last_bytes, data.len() as u64, "Final progress should equal file size");

        // Verify hash is correct
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        Digest::update(&mut hasher, &data);
        let expected: [u8; 32] = Digest::finalize(hasher).into();
        assert_eq!(hash, expected);

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_sha256_and_blake3_consistency() {
        // Verify both hashers work on the same file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join("vbdp_test_dual_hash.bin");
        let data = b"data for dual hashing test";
        std::fs::write(&temp_file, data).unwrap();

        let blake3_hash = hash_file(&temp_file).unwrap();
        let sha256_hash = sha256_file(&temp_file).unwrap();

        // Both should succeed (different values, but both valid)
        assert_eq!(blake3_hash.to_bytes().len(), 32);
        assert_eq!(sha256_hash.len(), 32);

        // Verify they match direct computation
        let expected_blake3 = hash_bytes(data);
        assert_eq!(blake3_hash, expected_blake3);

        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        Digest::update(&mut hasher, data);
        let expected_sha256: [u8; 32] = Digest::finalize(hasher).into();
        assert_eq!(sha256_hash, expected_sha256);

        std::fs::remove_file(&temp_file).ok();
    }
}
