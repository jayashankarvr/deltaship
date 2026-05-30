//! Database error types for Deltaship.
//!
//! # Error Handling Conventions
//!
//! This library crate uses custom error types with a type alias for results:
//! - `DbError`: Custom error type with specific variants for different failure modes
//! - `Result<T>`: Type alias for `std::result::Result<T, DbError>`
//!
//! ## When to Use This Pattern
//!
//! **Libraries** (like this crate) should use custom error types because:
//! - Provides structured, parseable errors for downstream consumers
//! - Enables pattern matching on specific error conditions
//! - Maintains API stability and type safety
//! - Allows error context to be preserved with specific variants
//!
//! **CLI binaries** (`deltaship-publisher`, `deltaship-client`) should use `anyhow::Result`:
//! - Simplifies error handling with `?` operator across different error types
//! - Provides good error messages with context chains for end users
//! - No need for consumers to match on specific error types
//!
//! **Server routes** should use custom error types that implement `IntoResponse`:
//! - Maps errors to appropriate HTTP status codes
//! - Provides structured error responses for API clients
//!
//! # P3 Issue #106 Fix: Error Message Format Standard
//!
//! All "not found" error messages in this crate follow the standardized format:
//!
//! **Format**: `"{Entity} not found: '{id}'"`
//!
//! Where:
//! - `{Entity}` is the entity type (e.g., "Binary", "Version", "Update record")
//! - `{id}` is the identifier value, enclosed in single quotes
//!
//! Examples:
//! - `"Binary not found: 'abc-123'"`
//! - `"Version not found: 'v1.0.0'"`
//! - `"Update record not found: '42'"`
//!
//! This standard provides:
//! 1. Consistency across all database operations
//! 2. Clear identification of missing resources
//! 3. Quoted IDs prevent ambiguity with surrounding text

use thiserror::Error;

/// Database errors for Deltaship operations.
#[derive(Debug, Error)]
pub enum DbError {
    /// SQLx database error (not mapped to a more specific variant).
    #[error("Database error: {0}")]
    Sqlx(String),

    /// Migration error during schema setup.
    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// Database connection error.
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Database operation timeout.
    #[error("Operation timed out after {timeout_secs} seconds: {message}")]
    Timeout {
        timeout_secs: u64,
        message: String,
    },

    /// Database constraint violation (e.g., UNIQUE, FOREIGN KEY).
    #[error("Constraint violation: {0}")]
    ConstraintViolation(String),

    /// Record not found.
    #[error("Record not found: {0}")]
    NotFound(String),

    /// Duplicate record constraint violation.
    #[error("Duplicate record: {0}")]
    Duplicate(String),

    /// Invalid data format or value.
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// IO error (e.g., file operations).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// State transition error (invalid status change).
    #[error("State transition error: {0}")]
    StateTransition(String),

    /// Schema version mismatch (migration required).
    #[error("Schema version mismatch: {0}")]
    SchemaMismatch(String),
}

impl From<sqlx::Error> for DbError {
    fn from(err: sqlx::Error) -> Self {
        // Parse sqlx errors into specific DbError variants
        //
        // NOTE: String matching is SQLite-specific. The error message formats are defined
        // by SQLite and stable across SQLite versions. This is acceptable because:
        //
        // 1. Deltaship exclusively uses SQLite for local storage (both publisher and client)
        // 2. SQLite is an embedded database with a stable API contract
        // 3. The sqlx library doesn't provide a database-agnostic way to detect
        //    specific error types through structured error codes
        // 4. SQLite's error message format has been stable since SQLite 3.x
        //
        // If Deltaship ever needs to support other databases (PostgreSQL, MySQL, etc.),
        // this would need to be extended with database-specific error detection,
        // potentially using sqlx::Error::Database::code() for structured matching.

        match &err {
            // Connection-related errors
            sqlx::Error::PoolTimedOut => DbError::Timeout {
                timeout_secs: 30, // Default pool timeout from PublisherDb::open
                message: "Connection pool timed out waiting for available connection".to_string(),
            },
            sqlx::Error::PoolClosed => {
                DbError::ConnectionError("Connection pool has been closed".to_string())
            }
            sqlx::Error::Io(io_err) => {
                DbError::ConnectionError(format!("I/O error during connection: {}", io_err))
            }
            sqlx::Error::Tls(tls_err) => {
                DbError::ConnectionError(format!("TLS error: {}", tls_err))
            }

            // Database constraint violations (SQLite-specific)
            sqlx::Error::Database(ref db_err) => {
                let msg = db_err.message();

                // UNIQUE constraint violation
                if msg.contains("UNIQUE constraint failed") {
                    return DbError::Duplicate(msg.to_string());
                }

                // FOREIGN KEY constraint violation
                if msg.contains("FOREIGN KEY constraint failed") {
                    return DbError::ConstraintViolation(format!(
                        "Foreign key constraint violation: {}",
                        msg
                    ));
                }

                // CHECK constraint violation
                if msg.contains("CHECK constraint failed") {
                    return DbError::ConstraintViolation(format!(
                        "Check constraint violation: {}",
                        msg
                    ));
                }

                // NOT NULL constraint violation
                if msg.contains("NOT NULL constraint failed") {
                    return DbError::ConstraintViolation(format!(
                        "Not null constraint violation: {}",
                        msg
                    ));
                }

                // Generic database error with context preserved
                DbError::Sqlx(format!("Database error: {}", msg))
            }

            // Row not found (query expected data but got none)
            sqlx::Error::RowNotFound => {
                DbError::NotFound("Query returned no rows".to_string())
            }

            // Column not found or type mismatch
            sqlx::Error::ColumnNotFound(col) => {
                DbError::InvalidData(format!("Column not found: {}", col))
            }
            sqlx::Error::ColumnDecode { index, source } => DbError::InvalidData(format!(
                "Failed to decode column {} ({})",
                index, source
            )),

            // Migration errors are handled by #[from] attribute
            // All other errors fall through to generic Sqlx variant
            _ => DbError::Sqlx(err.to_string()),
        }
    }
}

/// Result type alias for database operations.
pub type Result<T> = std::result::Result<T, DbError>;
