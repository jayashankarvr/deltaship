//! Error types for binary diff/patch operations.
//!
//! # Error Handling Conventions
//!
//! This library crate uses custom error types with a type alias for results:
//! - `DiffError`: Custom error type with specific variants for diff/patch failures
//! - `Result<T>`: Type alias for `std::result::Result<T, DiffError>`
//!
//! ## When to Use This Pattern
//!
//! **Libraries** (like this crate) should use custom error types because:
//! - Provides structured, parseable errors for downstream consumers
//! - Enables pattern matching on specific error conditions (e.g., checksum mismatch)
//! - Maintains API stability and type safety
//!
//! **CLI binaries** (`vbdp-publisher`, `vbdp-client`) should use `anyhow::Result`:
//! - Simplifies error handling with `?` operator across different error types
//! - Provides good error messages with context chains for end users
//!
//! **Server routes** should use custom error types that implement `IntoResponse`:
//! - Maps errors to appropriate HTTP status codes
//! - Provides structured error responses for API clients

use std::io;
use thiserror::Error;

/// Result type for diff operations.
pub type Result<T> = std::result::Result<T, DiffError>;

/// Errors that can occur during diff/patch operations.
#[derive(Debug, Error)]
pub enum DiffError {
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Failed to generate a diff.
    #[error("failed to generate diff: {0}")]
    DiffGenerationFailure(String),

    /// Failed to apply a patch.
    #[error("failed to apply patch: {0}")]
    PatchApplicationFailure(String),

    /// The diff data is invalid or corrupted.
    #[error("invalid diff format")]
    InvalidDiff,

    /// Checksum verification failed.
    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// A size limit was exceeded while decompressing or applying untrusted data.
    ///
    /// This protects against decompression bombs and maliciously crafted patches
    /// that would otherwise expand to an unbounded amount of memory.
    #[error("size limit exceeded: output would exceed {limit} bytes")]
    SizeLimitExceeded { limit: usize },
}
