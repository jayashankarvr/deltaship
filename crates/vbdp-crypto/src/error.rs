//! Cryptographic error types for VBDP.
//!
//! # Error Handling Conventions
//!
//! This library crate uses custom error types with a type alias for results:
//! - `CryptoError`: Custom error type with detailed, actionable error messages
//! - `Result<T>`: Type alias for `std::result::Result<T, CryptoError>`
//!
//! ## When to Use This Pattern
//!
//! **Libraries** (like this crate) should use custom error types because:
//! - Provides structured, parseable errors for downstream consumers
//! - Enables pattern matching on specific error conditions
//! - Maintains API stability and type safety
//! - Allows embedding actionable guidance in error messages
//!
//! **CLI binaries** (`vbdp-publisher`, `vbdp-client`) should use `anyhow::Result`:
//! - Simplifies error handling with `?` operator across different error types
//! - Provides good error messages with context chains for end users
//!
//! **Server routes** should use custom error types that implement `IntoResponse`:
//! - Maps errors to appropriate HTTP status codes
//! - Provides structured error responses for API clients

use std::path::PathBuf;
use thiserror::Error;

/// Cryptographic operation errors with full context and actionable guidance
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Signature verification failed: The signature does not match the data.\n  Next steps:\n  - Verify you are using the correct public key\n  - Ensure the data was not modified after signing\n  - Check that the signature was generated for this exact data")]
    SignatureVerificationFailure,

    #[error("Invalid signing key: The key format is not recognized or corrupted.\n  Next steps:\n  - Verify the key file was not modified\n  - Ensure you are loading a signing key (not a verifying key)\n  - Try regenerating the key with 'vbdp-publisher keygen'")]
    InvalidSigningKey,

    #[error("Invalid verifying key: The key format is not recognized or corrupted.\n  Next steps:\n  - Verify the key file was not modified\n  - Ensure you are loading a verifying key (.pub file)\n  - Request a fresh copy from the publisher")]
    InvalidVerifyingKey,

    #[error("Invalid signature: The signature format is malformed.\n  Next steps:\n  - Verify the signature was correctly copied/transmitted\n  - Ensure the signature is exactly 64 bytes (128 hex characters)\n  - Check for truncation or corruption during transfer")]
    InvalidSignature,

    #[error("Invalid hex string: {0}\n  Next steps:\n  - Ensure the string contains only hexadecimal characters (0-9, a-f)\n  - Check that the string has even length (2 characters per byte)\n  - Verify no whitespace or special characters are present")]
    InvalidHex(String),

    #[error("Invalid key file format: File structure does not match expected format.\n  Next steps:\n  - Verify the file is a VBDP key file (not corrupted or truncated)\n  - Check that the BEGIN/END markers are intact\n  - Ensure the file was not modified or re-encoded\n  - Try regenerating the key if this is a signing key")]
    InvalidKeyFileFormat,

    #[error("Invalid hex in key file: Odd number of characters (must be even).\n  Next steps:\n  - The key file may be corrupted or truncated\n  - Try opening the key file in a text editor to inspect it\n  - If this is a signing key, regenerate it with 'vbdp-publisher keygen'")]
    InvalidKeyFileHexOddLength,

    #[error("Invalid hex in key file: Invalid character '{character}' at position {position}.\n  Next steps:\n  - The key file contains non-hexadecimal characters\n  - Check for accidental modifications to the file\n  - Ensure the file was not corrupted during transfer\n  - Valid characters are: 0-9, a-f, A-F")]
    InvalidKeyFileHexChar {
        character: char,
        position: usize,
    },

    #[error("Invalid hex in key file: Expected {expected} bytes, got {actual}.\n  Next steps:\n  - The key data is truncated or has extra data\n  - Ed25519 keys should be exactly 32 bytes (64 hex characters)\n  - Verify the key file was not partially copied or modified\n  - Try regenerating the key if possible")]
    InvalidKeyFileHexLength {
        expected: usize,
        actual: usize,
    },

    #[error("Invalid key file version: Expected version {expected}, got {actual}.\n  Next steps:\n  - This key file was created with a different version of VBDP\n  - If version {actual} is newer, update your VBDP tools\n  - If version {actual} is older, you may need to regenerate the key\n  - Contact the key provider for a compatible version")]
    InvalidKeyFileVersion {
        expected: u8,
        actual: u8,
    },

    #[error("Invalid key file salt: {reason}.\n  Next steps:\n  - The encryption salt in the key file is invalid\n  - This indicates file corruption or tampering\n  - You must regenerate this signing key (backups cannot be recovered)")]
    InvalidKeyFileSalt {
        reason: String,
    },

    #[error("Invalid key file nonce: {reason}.\n  Next steps:\n  - The encryption nonce in the key file is invalid\n  - This indicates file corruption or tampering\n  - You must regenerate this signing key (backups cannot be recovered)")]
    InvalidKeyFileNonce {
        reason: String,
    },

    #[error("Passphrase required but not provided.\n  Next steps:\n  - This key file is encrypted and requires a passphrase\n  - Provide the passphrase using the appropriate command option\n  - If you forgot the passphrase, you must regenerate the key")]
    PassphraseRequired,

    #[error("Incorrect passphrase: The passphrase does not decrypt this key.\n  Next steps:\n  - Verify you entered the correct passphrase\n  - Check for typos or caps lock\n  - Ensure you're using the right key file\n  - If you forgot the passphrase, you must regenerate the key")]
    IncorrectPassphrase,

    #[error("Failed to configure Argon2 parameters.\n  Next steps:\n  - This is an internal error in key derivation setup\n  - Report this issue with your system details\n  - Try updating to the latest version of VBDP")]
    ArgonParamsFailure,

    #[error("Key derivation failed: {0}.\n  Next steps:\n  - The key derivation process encountered an error\n  - This may indicate insufficient system resources\n  - Try closing other applications and retrying")]
    KeyDerivationFailure(String),

    #[error("AEAD encryption failed: Could not encrypt the key.\n  Next steps:\n  - This is an internal encryption error\n  - Ensure you have sufficient disk space\n  - Try regenerating the key\n  - Report this issue if it persists")]
    AeadEncryptionFailure,

    #[error("AEAD decryption failed: Could not decrypt the key.\n  Next steps:\n  - The key file may be corrupted\n  - Verify you're using the correct passphrase\n  - Check that the key file wasn't modified\n  - This error can occur with the wrong passphrase or corrupted file")]
    AeadDecryptionFailure,

    #[error("Encryption failed during key save.\n  Next steps:\n  - Ensure you have write permissions\n  - Check available disk space\n  - Verify the output directory exists")]
    EncryptionFailure,

    #[error("Decryption failed during key load.\n  Next steps:\n  - Verify the passphrase is correct\n  - Check that the key file is not corrupted\n  - Ensure the file is a valid encrypted VBDP key")]
    DecryptionFailure,

    #[error("Legacy key format detected.\n  Next steps:\n  - This key file uses an outdated format\n  - Re-encrypt the key using the current version: 'vbdp-publisher keygen --import'\n  - The old key will be backed up automatically\n  - Update any documentation referencing the old format")]
    LegacyKeyFormat,

    #[error("I/O error for path '{path}': {source}\n  Next steps:\n  - Check that the file/directory exists: {path}\n  - Verify you have appropriate permissions (read/write)\n  - Ensure the disk is not full\n  - Check that the path is correct and accessible")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl CryptoError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        CryptoError::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, CryptoError>;
