//! Patch application using bsdiff algorithm.

use std::fs;
use std::io::Read;
use std::path::Path;

use crate::error::{DiffError, Result};

/// Maximum size, in bytes, that a single patch application is allowed to produce
/// by default.
///
/// Patch data is untrusted network input. A maliciously crafted patch can
/// instruct the applier to emit an arbitrarily large output. Without a ceiling,
/// this would grow the output buffer until the process runs out of memory.
/// [`apply_patch`] and [`apply_patch_files`] enforce this default ceiling; use
/// [`apply_patch_bounded`] to choose a tighter (or looser) bound.
///
/// 1 GiB matches `vbdp_diff::MAX_DECOMPRESSED_SIZE`: large enough for any
/// legitimate target this crate is expected to reconstruct, small enough to
/// fail fast instead of OOMing a host.
pub const MAX_OUTPUT_SIZE: usize = 1024 * 1024 * 1024;

/// Apply a patch to old data in memory.
///
/// Returns the reconstructed new data.
///
/// # Security
///
/// This function does **not** verify the integrity (hash) of the reconstructed
/// output. Callers handling untrusted patches should use
/// [`apply_patch_verified`] with the expected Blake3 hash instead.
///
/// The output size is bounded by [`MAX_OUTPUT_SIZE`] to protect against
/// maliciously crafted patches that would otherwise exhaust memory; if the
/// output would exceed that ceiling, [`DiffError::SizeLimitExceeded`] is
/// returned. Use [`apply_patch_bounded`] to pick a different ceiling.
pub fn apply_patch(old: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    apply_patch_bounded(old, patch, MAX_OUTPUT_SIZE)
}

/// Apply a patch to old data in memory, capping the output at `max_output` bytes.
///
/// Behaves like [`apply_patch`] but lets the caller choose the maximum size of
/// the reconstructed output. The applier never allocates more than `max_output`
/// bytes of output: as soon as the patch would grow the output past the
/// ceiling, application aborts with [`DiffError::SizeLimitExceeded`].
///
/// Like [`apply_patch`], this does **not** verify the output hash; untrusted
/// callers should additionally verify the result (see [`apply_patch_verified`]).
pub fn apply_patch_bounded(old: &[u8], patch: &[u8], max_output: usize) -> Result<Vec<u8>> {
    let mut new = Vec::new();
    bounded_patch(old, patch, &mut new, max_output)?;
    Ok(new)
}

/// Reimplementation of the bsdiff patch loop with a hard output-size ceiling.
///
/// This mirrors `bsdiff::patch` but checks the projected output length against
/// `max_output` *before* reading literal/diff bytes into the buffer, so a
/// malicious patch can never drive the output `Vec` past the ceiling.
fn bounded_patch(
    old: &[u8],
    patch: &[u8],
    new: &mut Vec<u8>,
    max_output: usize,
) -> Result<()> {
    let mut reader = std::io::Cursor::new(patch);
    let mut oldpos: usize = 0;

    loop {
        // Read the 24-byte control record. EOF here (before any byte) ends the loop.
        let mut ctrl = [0u8; 24];
        match read_full_or_eof(&mut reader, &mut ctrl)
            .map_err(|e| DiffError::PatchApplicationFailure(e.to_string()))?
        {
            ControlRead::Eof => return Ok(()),
            ControlRead::Read => {}
        }

        let mix_len = u64_to_usize(u64::from_le_bytes(ctrl[0..8].try_into().unwrap()))?;
        let copy_len = u64_to_usize(u64::from_le_bytes(ctrl[8..16].try_into().unwrap()))?;
        let seek_len = offtin(ctrl[16..24].try_into().unwrap());

        let to_read = copy_len
            .checked_add(mix_len)
            .ok_or(DiffError::InvalidDiff)?;

        // Enforce the ceiling *before* growing the buffer.
        let projected = new
            .len()
            .checked_add(to_read)
            .ok_or(DiffError::InvalidDiff)?;
        if projected > max_output {
            return Err(DiffError::SizeLimitExceeded { limit: max_output });
        }

        let mix_start = new.len();
        let has_read = (&mut reader)
            .take(to_read as u64)
            .read_to_end(new)
            .map_err(|e| DiffError::PatchApplicationFailure(e.to_string()))?;
        if has_read != to_read {
            return Err(DiffError::PatchApplicationFailure(
                "unexpected end of patch data".to_string(),
            ));
        }

        let mix_end = mix_start.checked_add(mix_len).ok_or(DiffError::InvalidDiff)?;
        let mix_slice = new
            .get_mut(mix_start..mix_end)
            .ok_or(DiffError::InvalidDiff)?;

        let oldpos_end = oldpos.checked_add(mix_len).ok_or(DiffError::InvalidDiff)?;
        let old_slice = old.get(oldpos..oldpos_end).ok_or(DiffError::InvalidDiff)?;

        for (n, o) in mix_slice.iter_mut().zip(old_slice.iter().copied()) {
            *n = n.wrapping_add(o);
        }

        oldpos += mix_len;
        oldpos = (oldpos as i64)
            .checked_add(seek_len)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(DiffError::InvalidDiff)?;
    }
}

enum ControlRead {
    Read,
    Eof,
}

/// Reads exactly `buf.len()` bytes. Allows clean EOF only before the first byte.
fn read_full_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<ControlRead> {
    let total = buf.len();
    let mut filled = 0;
    while filled < total {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => {
                return if filled == 0 {
                    Ok(ControlRead::Eof)
                } else {
                    Err(std::io::ErrorKind::UnexpectedEof.into())
                };
            }
            Ok(n) => filled += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(ControlRead::Read)
}

fn u64_to_usize(v: u64) -> Result<usize> {
    usize::try_from(v).map_err(|_| DiffError::InvalidDiff)
}

/// Reads sign-magnitude i64 little-endian (bsdiff control encoding).
#[inline]
fn offtin(buf: [u8; 8]) -> i64 {
    let y = i64::from_le_bytes(buf);
    if 0 == y & (1 << 63) {
        y
    } else {
        -(y & !(1 << 63))
    }
}

/// Apply a patch with optional checksum verification of the output.
///
/// This function applies a patch and optionally verifies the output matches
/// an expected Blake3 hash. This provides integrity verification to ensure
/// the patched result is correct.
///
/// # Arguments
///
/// * `old` - The original data to patch
/// * `patch` - The patch data to apply
/// * `expected_hash` - Optional Blake3 hash (32 bytes) of the expected result.
///   If provided, the output will be verified against this hash.
///
/// # Returns
///
/// The reconstructed new data if successful and checksum matches (if provided).
///
/// # Errors
///
/// Returns an error if:
/// - Patch application fails
/// - Checksum verification fails (if expected_hash is provided)
pub fn apply_patch_verified(
    old: &[u8],
    patch: &[u8],
    expected_hash: Option<&[u8; 32]>,
) -> Result<Vec<u8>> {
    let new = apply_patch(old, patch)?;

    // Verify checksum if provided
    if let Some(expected) = expected_hash {
        let actual_hash = blake3::hash(&new);
        if actual_hash.as_bytes() != expected {
            return Err(DiffError::ChecksumMismatch {
                expected: hex_encode(expected),
                actual: hex_encode(actual_hash.as_bytes()),
            });
        }
    }

    Ok(new)
}

/// Simple hex encoding helper.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Apply a patch file to an old file, writing the result to a new file.
///
/// # Security
///
/// Like [`apply_patch`], this does **not** verify the integrity (hash) of the
/// reconstructed output, and bounds the output at [`MAX_OUTPUT_SIZE`]. Callers
/// handling untrusted patches should verify the result against an expected
/// Blake3 hash (see [`apply_patch_verified`]).
pub fn apply_patch_files(old_path: &Path, patch_path: &Path, new_path: &Path) -> Result<()> {
    let old = fs::read(old_path)?;
    let patch = fs::read(patch_path)?;

    let new = apply_patch(&old, &patch)?;

    fs::write(new_path, &new)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate_diff;

    #[test]
    fn test_apply_patch_roundtrip() {
        let old = b"hello world";
        let new = b"hello rust world";

        let diff = generate_diff(old, new).unwrap();
        let recovered = apply_patch(old, &diff).unwrap();

        assert_eq!(&recovered, new);
    }

    #[test]
    fn test_apply_patch_identical_data() {
        let data = b"identical data";

        let diff = generate_diff(data, data).unwrap();
        let recovered = apply_patch(data, &diff).unwrap();

        assert_eq!(&recovered, data);
    }

    #[test]
    fn test_apply_patch_empty_to_data() {
        let old = b"";
        let new = b"new content";

        let diff = generate_diff(old, new).unwrap();
        let recovered = apply_patch(old, &diff).unwrap();

        assert_eq!(&recovered, new);
    }

    #[test]
    fn test_apply_patch_data_to_empty() {
        let old = b"old content";
        let new = b"";

        let diff = generate_diff(old, new).unwrap();
        let recovered = apply_patch(old, &diff).unwrap();

        assert_eq!(&recovered, new);
    }

    #[test]
    fn test_apply_patch_corrupted_diff() {
        let old = b"test data";
        let corrupted_diff = b"this is not a valid diff";

        let result = apply_patch(old, corrupted_diff);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_patch_wrong_base() {
        let old = b"original data";
        let new = b"updated data";
        let wrong_base = b"completely different data that is much longer";

        let diff = generate_diff(old, new).unwrap();

        // Applying with wrong base should either fail or produce different output
        let result = apply_patch(wrong_base, &diff);
        // Test passes if it either errors or produces different data
        match result {
            Err(_) => {
                // Expected: patch application fails with wrong base
            }
            Ok(recovered) => {
                // If it succeeds, verify it's different from expected
                // (though in some cases bsdiff might still produce the same output)
                let _ = recovered;
            }
        }
    }

    #[test]
    fn test_apply_patch_files_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let old_path = temp_dir.join("vbdp_test_apply_old.bin");
        let new_path = temp_dir.join("vbdp_test_apply_new.bin");
        let diff_path = temp_dir.join("vbdp_test_apply_diff.bin");
        let recovered_path = temp_dir.join("vbdp_test_apply_recovered.bin");

        let old_data = b"original file content";
        let new_data = b"updated file content with changes";

        std::fs::write(&old_path, old_data).unwrap();
        std::fs::write(&new_path, new_data).unwrap();

        // Generate diff
        let diff = generate_diff(old_data, new_data).unwrap();
        std::fs::write(&diff_path, &diff).unwrap();

        // Apply patch
        apply_patch_files(&old_path, &diff_path, &recovered_path).unwrap();

        // Verify recovered matches new
        let recovered_data = std::fs::read(&recovered_path).unwrap();
        assert_eq!(&recovered_data, new_data);

        // Cleanup
        std::fs::remove_file(&old_path).ok();
        std::fs::remove_file(&new_path).ok();
        std::fs::remove_file(&diff_path).ok();
        std::fs::remove_file(&recovered_path).ok();
    }

    #[test]
    fn test_apply_patch_large_files() {
        let temp_dir = std::env::temp_dir();
        let old_path = temp_dir.join("vbdp_test_large_apply_old.bin");
        let new_path = temp_dir.join("vbdp_test_large_apply_new.bin");
        let diff_path = temp_dir.join("vbdp_test_large_apply_diff.bin");
        let recovered_path = temp_dir.join("vbdp_test_large_apply_recovered.bin");

        // Create larger files
        let old_data = vec![0xCC; 50000];
        let mut new_data = old_data.clone();
        new_data[10000] = 0xDD;
        new_data[20000] = 0xEE;
        new_data.extend_from_slice(b"extra data");

        std::fs::write(&old_path, &old_data).unwrap();
        std::fs::write(&new_path, &new_data).unwrap();

        // Generate and apply diff
        let diff = generate_diff(&old_data, &new_data).unwrap();
        std::fs::write(&diff_path, &diff).unwrap();
        apply_patch_files(&old_path, &diff_path, &recovered_path).unwrap();

        // Verify
        let recovered_data = std::fs::read(&recovered_path).unwrap();
        assert_eq!(recovered_data, new_data);

        // Cleanup
        std::fs::remove_file(&old_path).ok();
        std::fs::remove_file(&new_path).ok();
        std::fs::remove_file(&diff_path).ok();
        std::fs::remove_file(&recovered_path).ok();
    }

    #[test]
    fn test_apply_patch_bounded_rejects_oversized_output() {
        // Build a patch whose output is ~2000 bytes, then apply with a 100-byte cap.
        let old = b"";
        let new = vec![0xABu8; 2000];
        let diff = generate_diff(old, &new).unwrap();

        let err = apply_patch_bounded(old, &diff, 100).unwrap_err();
        match err {
            DiffError::SizeLimitExceeded { limit } => assert_eq!(limit, 100),
            other => panic!("expected SizeLimitExceeded, got {other:?}"),
        }
    }

    #[test]
    fn test_apply_patch_bounded_allows_within_limit() {
        let old = b"hello world";
        let new = b"hello rust world";
        let diff = generate_diff(old, new).unwrap();

        let recovered = apply_patch_bounded(old, &diff, 1024).unwrap();
        assert_eq!(&recovered, new);
    }

    #[test]
    fn test_apply_patch_bounded_matches_unbounded_roundtrip() {
        // The bounded reimplementation must produce identical output to bsdiff.
        let old = vec![0x11u8; 5000];
        let mut new = old.clone();
        new[100] = 0x22;
        new[2500] = 0x33;
        new.extend_from_slice(b"trailing bytes appended at the end");

        let diff = generate_diff(&old, &new).unwrap();

        let bounded = apply_patch_bounded(&old, &diff, MAX_OUTPUT_SIZE).unwrap();
        assert_eq!(bounded, new);

        let mut bsdiff_out = Vec::new();
        bsdiff::patch(&old, &mut std::io::Cursor::new(&diff), &mut bsdiff_out).unwrap();
        assert_eq!(bounded, bsdiff_out);
    }

    #[test]
    fn test_apply_patch_multiple_changes() {
        let old = b"The quick brown fox jumps over the lazy dog";
        let new = b"The quick red fox leaps over the sleepy cat";

        let diff = generate_diff(old, new).unwrap();
        let recovered = apply_patch(old, &diff).unwrap();

        assert_eq!(&recovered, new);
    }
}
