//! Core error types for Deltaship.
//!
//! # Error Handling Conventions
//!
//! This library crate uses custom error types with an implicit result pattern:
//! - `DeltashipError`: Custom error type for core domain operations
//! - No type alias defined - consumers use `Result<T, DeltashipError>` directly
//!
//! ## When to Use This Pattern
//!
//! **Libraries** (like this crate) should use custom error types because:
//! - Provides structured, parseable errors for downstream consumers
//! - Enables pattern matching on specific error conditions
//! - Maintains API stability and type safety
//!
//! **CLI binaries** (`deltaship-publisher`, `deltaship-client`) should use `anyhow::Result`:
//! - Simplifies error handling with `?` operator across different error types
//! - Provides good error messages with context chains for end users
//!
//! **Server routes** should use custom error types that implement `IntoResponse`:
//! - Maps errors to appropriate HTTP status codes
//! - Provides structured error responses for API clients

use thiserror::Error;

/// Error types for Deltaship operations
#[derive(Debug, Error)]
pub enum DeltashipError {
    #[error("invalid version: {0}")]
    InvalidVersion(String),

    #[error("invalid binary name: {0}")]
    InvalidBinaryName(String),

    #[error("invalid binary id: {0}")]
    InvalidBinaryId(String),

    #[error("invalid platform: {0}")]
    InvalidPlatform(String),

    #[error("invalid diff algorithm: {0}")]
    InvalidDiffAlgorithm(String),

    #[error("invalid diff id: {0}")]
    InvalidDiffId(String),

    #[error("invalid compression format: {0}")]
    InvalidCompressionFormat(String),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    SerializationError(String),

    #[error("diff generation failed: {0}")]
    DiffGenerationError(String),

    #[error("patch application failed: {0}")]
    PatchError(String),

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("signature verification failed: {0}")]
    SignatureError(String),

    #[error("version not found: {0}")]
    VersionNotFound(String),

    #[error("binary not found: {0}")]
    BinaryNotFound(String),

    #[error("invalid data: {0}")]
    InvalidData(String),

    #[error("invalid rollout percentage: {0}")]
    InvalidRolloutPercentage(String),
}

impl From<serde_json::Error> for DeltashipError {
    fn from(err: serde_json::Error) -> Self {
        DeltashipError::SerializationError(err.to_string())
    }
}
