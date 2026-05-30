//! Deltaship Database Layer
//!
//! This crate provides SQLite database implementations for both the
//! Deltaship Publisher Toolkit and Client Patcher.
//!
//! # Overview
//!
//! Deltaship uses SQLite for local storage in both the publisher and client components:
//!
//! - **Publisher Database**: Stores binaries, versions, and diff computation jobs
//! - **Client Database**: Stores managed binaries, update history, and rollback backups
//!
//! # Usage
//!
//! ## Publisher
//!
//! ```no_run
//! use std::path::Path;
//! use deltaship_db::{PublisherDb, NewBinary, NewVersion};
//!
//! # async fn example() -> deltaship_db::Result<()> {
//! // Open or create a publisher database
//! let db = PublisherDb::open(Path::new(".deltaship/publisher.db")).await?;
//! db.init().await?;
//!
//! // Register a binary
//! let binary = db.insert_binary(NewBinary {
//!     binary_name: "myapp".into(),
//!     platform: "linux-x86_64".into(),
//!     binary_path: "/path/to/myapp".into(),
//!     description: Some("My application".into()),
//! }).await?;
//!
//! // Register a version
//! let version = db.insert_version(NewVersion {
//!     binary_id: binary.binary_id.clone(),
//!     version_string: "1.0.0".into(),
//!     file_path: "/path/to/myapp-1.0.0".into(),
//!     file_size_bytes: 1024000,
//!     file_hash_blake3: vec![0u8; 32],
//!     file_hash_sha256: vec![0u8; 32],
//! }).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Client
//!
//! ```no_run
//! use std::path::Path;
//! use deltaship_db::{ClientDb, NewManagedBinary, NewUpdateRecord};
//!
//! # async fn example() -> deltaship_db::Result<()> {
//! // Open or create a client database
//! let db = ClientDb::open(Path::new("/var/lib/deltaship/client.db")).await?;
//! db.init().await?;
//!
//! // Register a managed binary
//! let binary = db.register_binary(NewManagedBinary {
//!     binary_id: "uuid-from-server".into(),
//!     binary_name: "myapp".into(),
//!     platform: "linux-x86_64".into(),
//!     install_path: "/usr/local/bin/myapp".into(),
//!     publisher_public_key: vec![0u8; 32],
//! }).await?;
//!
//! // Record an update
//! let update_id = db.record_update_start(NewUpdateRecord {
//!     binary_id: binary.binary_id.clone(),
//!     from_version_id: None,
//!     from_version_string: None,
//!     to_version_id: "version-uuid".into(),
//!     to_version_string: "1.0.0".into(),
//! }).await?;
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod models;
mod publisher;
mod schema;

// Re-export public types
pub use client::ClientDb;
pub use error::{DbError, Result};
pub use models::{
    calculate_diff_efficiency, normalize_version_string, validate_diff_algorithm,
    validate_hash_size, validate_platform, DatabaseStats, DbBinary, DbDiffJob, DbInstalledVersion,
    DbManagedBinary, DbRollbackBackup, DbUpdateHistory, DbVersion, DiffJobStatus, NewBinary,
    NewDiffJob, NewManagedBinary, NewUpdateRecord, NewVersion, UpdateHistoryStatus, UpdateMetrics,
    BLAKE3_HASH_SIZE, KNOWN_DIFF_ALGORITHMS, KNOWN_PLATFORMS, SHA256_HASH_SIZE,
};
pub use publisher::PublisherDb;
pub use schema::{CLIENT_SCHEMA_VERSION, PUBLISHER_SCHEMA_VERSION};
