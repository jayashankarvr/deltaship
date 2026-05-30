//! Deltaship Core - Core types and models for Version-Based Differential Patching
//!
//! This crate provides the foundational types used across the Deltaship system.

pub mod binary;
pub mod diff;
pub mod error;
pub mod update;
pub mod version;

// Re-export primary types for convenience
pub use binary::{BinaryId, BinaryMetadata, BinaryName, Platform};
pub use diff::{CompressionFormat, DiffAlgorithm, DiffId, DiffMetadata};
pub use error::DeltashipError;
pub use update::{RolloutPercentage, UpdateCheckRequest, UpdateCheckResponse, UpdateStatus};
pub use version::Version;

/// Result type alias for Deltaship operations
pub type Result<T> = std::result::Result<T, DeltashipError>;
