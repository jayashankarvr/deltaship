//! Cryptographic primitives for Deltaship (Verified Binary Distribution Protocol)
//!
//! This crate provides:
//! - Ed25519 signing and verification
//! - Blake3 hashing
//! - Key file operations with optional encryption
//!
//! # Security Model and Threat Assumptions
//!
//! This crate is designed to protect the integrity and authenticity of binary distributions.
//! The following security properties are provided:
//!
//! ## Cryptographic Choices
//!
//! - **Signing**: Ed25519 (via `ed25519-dalek`) provides 128-bit security level for signatures.
//!   Ed25519 is deterministic, avoiding nonce-related vulnerabilities common in ECDSA.
//!
//! - **Hashing**: Blake3 provides fast, secure hashing with 256-bit output (128-bit collision
//!   resistance). It is optimized for modern hardware and is suitable for file integrity checks.
//!
//! - **Key Encryption**: ChaCha20-Poly1305 AEAD with Argon2id key derivation. This combination
//!   provides authenticated encryption with memory-hard password hashing to resist brute-force
//!   attacks on encrypted keys.
//!
//! ## Threat Model
//!
//! This crate assumes:
//!
//! - **Attacker capabilities**: An attacker may have read access to encrypted key files and
//!   may attempt offline brute-force attacks on passphrases.
//!
//! - **Trust boundaries**: The passphrase is the primary secret for encrypted keys. Callers
//!   are responsible for secure passphrase handling (see [`save_signing_key`] documentation).
//!
//! - **Memory protection**: Sensitive key material is zeroized after use to limit exposure
//!   in memory. However, this is best-effort and does not protect against sophisticated
//!   memory forensics or compromised systems.
//!
//! ## What This Crate Does NOT Protect Against
//!
//! - Compromised systems with kernel-level access
//! - Side-channel attacks (timing, power analysis)
//! - Memory dumps or swap file forensics
//! - Weak passphrases chosen by users
//!
//! # Example: Proper Passphrase Handling
//!
//! ```rust,no_run
//! use zeroize::Zeroizing;
//!
//! // Read passphrase into a Zeroizing wrapper for automatic cleanup
//! let passphrase = Zeroizing::new(String::from("user-provided-passphrase"));
//!
//! // Pass reference to the Zeroizing wrapper (API requires &Zeroizing<String>)
//! // deltaship_crypto::save_signing_key(&key, &path, Some(&passphrase), SecurityLevel::Standard);
//! // The passphrase will be automatically zeroized when `passphrase` goes out of scope
//! ```

mod error;
mod hashing;
mod keys;
mod signing;

pub use error::{CryptoError, Result};
pub use hashing::{hash_bytes, hash_file, hash_file_with_progress, sha256_file, sha256_file_with_progress, Hash};
pub use keys::{load_signing_key, load_verifying_key, save_signing_key, save_verifying_key, SecurityLevel};
pub use signing::{signing_payload, Signature, SigningKey, VerifyingKey, SIGNING_DOMAIN_TAG};

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use zeroize::Zeroizing;

    fn temp_file(name: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir();
        temp_dir.join(format!("deltaship_integration_{}", name))
    }

    #[test]
    fn test_sign_verify_workflow() {
        let signing_key_path = temp_file("signing_key.pem");
        let verifying_key_path = temp_file("verifying_key.pub");
        let reimported_key_path = temp_file("reimported_verifying_key.pub");
        let passphrase = Zeroizing::new("integration_test_passphrase_123!".to_string());

        // 1. Generate a signing key
        let original_signing_key = SigningKey::generate();

        // 2. Save it encrypted
        save_signing_key(&original_signing_key, &signing_key_path, Some(&passphrase), SecurityLevel::Standard)
            .expect("Failed to save signing key");

        // Verify the file exists and is encrypted
        let content = fs::read_to_string(&signing_key_path).unwrap();
        assert!(
            content.contains("-----BEGIN DELTASHIP ENCRYPTED SIGNING KEY-----"),
            "Saved key should be encrypted"
        );
        assert!(content.contains("salt:"), "Encrypted key should have salt");
        assert!(content.contains("nonce:"), "Encrypted key should have nonce");

        // 3. Load it back
        let loaded_signing_key = load_signing_key(&signing_key_path, Some(&passphrase))
            .expect("Failed to load signing key");

        // Verify it's the same key
        assert_eq!(
            original_signing_key.to_bytes(),
            loaded_signing_key.to_bytes(),
            "Loaded key should match original"
        );

        // 4. Sign some data
        let test_data = b"This is important binary data that needs to be verified for integrity and authenticity.";
        let signature = loaded_signing_key.sign(test_data);

        // 5. Verify the signature
        let verifying_key = loaded_signing_key.verifying_key();
        verifying_key
            .verify(test_data, &signature)
            .expect("Signature verification should succeed");

        // Verify with wrong data fails
        let wrong_data = b"This is TAMPERED data!";
        assert!(
            verifying_key.verify(wrong_data, &signature).is_err(),
            "Verification should fail with wrong data"
        );

        // 6. Export verifying key
        save_verifying_key(&verifying_key, &verifying_key_path)
            .expect("Failed to save verifying key");

        // Verify the verifying key file format
        let pub_content = fs::read_to_string(&verifying_key_path).unwrap();
        assert!(
            pub_content.contains("-----BEGIN DELTASHIP VERIFYING KEY-----"),
            "Verifying key should have correct header"
        );
        assert!(
            pub_content.contains("-----END DELTASHIP VERIFYING KEY-----"),
            "Verifying key should have correct footer"
        );

        // 7. Reimport verifying key and verify again
        let reimported_verifying_key = load_verifying_key(&verifying_key_path)
            .expect("Failed to load verifying key");

        // Verify the reimported key matches
        assert_eq!(
            verifying_key.to_bytes(),
            reimported_verifying_key.to_bytes(),
            "Reimported verifying key should match original"
        );

        // Verify signature with reimported key
        reimported_verifying_key
            .verify(test_data, &signature)
            .expect("Verification with reimported key should succeed");

        // Save and load the verifying key again to test full roundtrip
        save_verifying_key(&reimported_verifying_key, &reimported_key_path)
            .expect("Failed to save reimported verifying key");
        let final_verifying_key = load_verifying_key(&reimported_key_path)
            .expect("Failed to load reimported verifying key");

        // Final verification
        final_verifying_key
            .verify(test_data, &signature)
            .expect("Final verification should succeed");

        // Clean up
        fs::remove_file(&signing_key_path).ok();
        fs::remove_file(&verifying_key_path).ok();
        fs::remove_file(&reimported_key_path).ok();
    }

    #[test]
    fn test_sign_verify_workflow_unencrypted() {
        // Same workflow but without encryption
        let signing_key_path = temp_file("unencrypted_signing_key.pem");
        let verifying_key_path = temp_file("unencrypted_verifying_key.pub");

        // 1. Generate and save without encryption
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &signing_key_path, None, SecurityLevel::Standard)
            .expect("Failed to save unencrypted signing key");

        // Verify it's unencrypted
        let content = fs::read_to_string(&signing_key_path).unwrap();
        assert!(
            content.contains("-----BEGIN DELTASHIP SIGNING KEY-----"),
            "Should be unencrypted"
        );
        assert!(
            !content.contains("salt:"),
            "Unencrypted key should not have salt"
        );

        // 2. Load, sign, verify
        let loaded_key = load_signing_key(&signing_key_path, None)
            .expect("Failed to load unencrypted key");

        let data = b"Test data for unencrypted key workflow";
        let sig = loaded_key.sign(data);

        let verify_key = loaded_key.verifying_key();
        save_verifying_key(&verify_key, &verifying_key_path)
            .expect("Failed to save verifying key");

        let loaded_verify_key = load_verifying_key(&verifying_key_path)
            .expect("Failed to load verifying key");

        loaded_verify_key
            .verify(data, &sig)
            .expect("Verification should succeed");

        // Clean up
        fs::remove_file(&signing_key_path).ok();
        fs::remove_file(&verifying_key_path).ok();
    }
}
