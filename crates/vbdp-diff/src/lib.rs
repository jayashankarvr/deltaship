//! Binary diff/patch operations using the bsdiff algorithm.
//!
//! This crate provides a simple API for generating and applying binary diffs.
//! It wraps the `bsdiff` crate with a clean interface.
//!
//! # Features
//!
//! - `compression` - Enable zstd compression for diffs (optional)
//!
//! # P3 Issue #108 Fix: Bsdiff Limitations and Performance Characteristics
//!
//! ## Memory Usage
//!
//! **The bsdiff algorithm requires approximately 8x the size of the input file in memory.**
//!
//! - For a 100MB binary: ~800MB RAM required for diff generation
//! - For a 500MB binary: ~4GB RAM required for diff generation
//! - For a 1GB binary: ~8GB RAM required for diff generation
//!
//! This is due to the suffix array construction that bsdiff uses for finding matching blocks.
//! The memory is primarily used during diff generation; patch application is much more memory-efficient.
//!
//! **Recommendation**: For binaries larger than 100MB, consider:
//! 1. Running diff generation on machines with sufficient RAM
//! 2. Using chunked/streaming diff approaches if available
//! 3. Falling back to full binary downloads for very large files
//!
//! ## Speed and Performance
//!
//! **Bsdiff is slow for files larger than ~100MB.**
//!
//! Typical performance characteristics:
//! - Small files (< 10MB): Fast, usually < 1 second
//! - Medium files (10-50MB): Moderate, 1-10 seconds
//! - Large files (50-100MB): Slow, 10-60 seconds
//! - Very large files (> 100MB): Very slow, minutes to hours
//!
//! The algorithm complexity is roughly O(n log n) where n is the file size,
//! but with significant constant factors due to suffix array construction.
//!
//! **Recommendation**: Set reasonable timeouts for diff generation (e.g., 5 minutes)
//! and fall back to full downloads if diff generation takes too long.
//!
//! ## Effectiveness
//!
//! **Bsdiff works best for binaries with small, localized changes.**
//!
//! Effectiveness varies by file type and change pattern:
//! - **Excellent** (90%+ reduction):
//!   - Small code changes with mostly identical functions
//!   - Minor version bumps with few modified functions
//!   - Bug fixes that touch limited code paths
//!
//! - **Good** (50-90% reduction):
//!   - Moderate refactoring with function reordering
//!   - Dependency updates that change linked library code
//!   - Feature additions with new code sections
//!
//! - **Poor** (< 50% reduction or larger than original):
//!   - Complete rewrites or major refactors
//!   - Changes to compressed/encrypted sections (diff can't exploit similarities)
//!   - Randomized or ASLR-affected binaries (address space changes)
//!   - Files with embedded timestamps or build UUIDs that change everything
//!
//! **Recommendation**: Measure diff effectiveness and fall back to full downloads
//! when the diff size exceeds 80% of the target file size.
//!
//! # Example
//!
//! ```no_run
//! use vbdp_diff::{generate_diff, apply_patch};
//!
//! let old_data = b"hello world";
//! let new_data = b"hello rust world";
//!
//! // Generate a diff
//! let diff = generate_diff(old_data, new_data).unwrap();
//!
//! // Apply the diff to recover the new data
//! let recovered = apply_patch(old_data, &diff).unwrap();
//! assert_eq!(recovered, new_data);
//! ```

pub mod apply;
#[cfg(feature = "compression")]
pub mod compress;
pub mod error;
pub mod generate;

// Re-export public API
pub use apply::{
    apply_patch, apply_patch_bounded, apply_patch_files, apply_patch_verified, MAX_OUTPUT_SIZE,
};
#[cfg(feature = "compression")]
pub use compress::{
    compress_diff, compress_diff_with_level, decompress_diff, decompress_diff_bounded,
    MAX_DECOMPRESSED_SIZE,
};
pub use error::{DiffError, Result};
pub use generate::{generate_diff, generate_diff_files, DiffStats};
