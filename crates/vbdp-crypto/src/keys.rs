//! Key management operations for VBDP signing keys.
//!
//! This module provides functions to save and load signing keys with optional
//! passphrase-based encryption. Encrypted keys use ChaCha20-Poly1305 AEAD with
//! Argon2id key derivation.
//!
//! # Security Considerations
//!
//! ## Passphrase Handling
//!
//! **IMPORTANT**: This module enforces passphrase zeroization by accepting
//! `Option<&Zeroizing<String>>` instead of `&str`. This ensures callers must
//! use `zeroize::Zeroizing<String>` for automatic cleanup:
//!
//! ```rust,no_run
//! use zeroize::Zeroizing;
//!
//! // Proper passphrase handling
//! let passphrase = Zeroizing::new(read_passphrase_from_user());
//! // save_signing_key(&key, &path, Some(&passphrase), SecurityLevel::Standard)?;
//! // `passphrase` is automatically zeroized when it goes out of scope
//!
//! # fn read_passphrase_from_user() -> String { String::new() }
//! ```
//!
//! ## Nonce Strategy
//!
//! Each encryption operation generates a fresh random 12-byte nonce using the
//! operating system's cryptographically secure random number generator (OsRng).
//! With 96 bits of randomness, the probability of nonce collision is negligible
//! for practical use (birthday bound ~2^48 encryptions before 50% collision chance).
//!
//! **WARNING**: Each key should only be encrypted once per passphrase. Re-encrypting
//! the same key with the same passphrase is safe because a new random nonce is
//! generated each time, but unnecessary re-encryption should be avoided to minimize
//! cryptographic operations on sensitive data.

use std::fs;
use std::path::Path;

use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Argon2, Params, Version,
};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use zeroize::{Zeroize, Zeroizing};

use crate::error::{CryptoError, Result};
use crate::signing::{SigningKey, VerifyingKey};

const SIGNING_KEY_HEADER: &str = "-----BEGIN VBDP SIGNING KEY-----";
const SIGNING_KEY_FOOTER: &str = "-----END VBDP SIGNING KEY-----";
const ENCRYPTED_SIGNING_KEY_HEADER: &str = "-----BEGIN VBDP ENCRYPTED SIGNING KEY-----";
const ENCRYPTED_SIGNING_KEY_FOOTER: &str = "-----END VBDP ENCRYPTED SIGNING KEY-----";
const VERIFYING_KEY_HEADER: &str = "-----BEGIN VBDP VERIFYING KEY-----";
const VERIFYING_KEY_FOOTER: &str = "-----END VBDP VERIFYING KEY-----";
const CURRENT_VERSION: u8 = 1;

/// Security level for Argon2id key derivation.
///
/// Different levels provide different trade-offs between security and performance.
/// Higher levels use more memory and time, making attacks more expensive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityLevel {
    /// Standard security level.
    /// - m_cost = 19456 KiB (~19 MiB)
    /// - t_cost = 2 iterations
    /// - Suitable for most use cases, ~0.5-1s derivation time
    Standard,

    /// High security level.
    /// - m_cost = 65536 KiB (64 MiB)
    /// - t_cost = 3 iterations
    /// - Better protection against GPU attacks, ~2-3s derivation time
    High,

    /// Maximum security level.
    /// - m_cost = 262144 KiB (256 MiB)
    /// - t_cost = 4 iterations
    /// - Maximum protection, ~8-10s derivation time
    Maximum,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        SecurityLevel::Standard
    }
}

impl SecurityLevel {
    /// Get the Argon2 parameters for this security level.
    fn params(&self) -> Result<Params> {
        let (m_cost, t_cost) = match self {
            SecurityLevel::Standard => (19456, 2),
            SecurityLevel::High => (65536, 3),
            SecurityLevel::Maximum => (262144, 4),
        };

        Params::new(m_cost, t_cost, 1, Some(32))
            .map_err(|_| CryptoError::ArgonParamsFailure)
    }
}

/// Derive a 32-byte encryption key from a passphrase using Argon2id.
///
/// # Argon2id Parameters
///
/// The parameters are chosen to balance security and usability:
///
/// - **m_cost = 19456 KiB (~19 MiB)**: Memory cost. This is the minimum recommended by
///   OWASP for Argon2id when defending against GPU-based attacks. Higher values provide
///   better protection but increase memory usage and derivation time.
///
/// - **t_cost = 2 iterations**: Time cost (number of passes). With 19 MiB memory, 2
///   iterations provides adequate protection while keeping derivation time reasonable
///   (~0.5-1 second on typical hardware).
///
/// - **p_cost = 1**: Parallelism (single-threaded). Keeps the implementation simple and
///   portable. Multi-threading would reduce wall-clock time but not security.
///
/// - **Output length = 32 bytes**: Matches ChaCha20-Poly1305 key size (256 bits).
///
/// # Security Assumptions
///
/// These parameters assume:
/// - Attacker has access to high-end GPUs for offline brute-force
/// - User passphrases have at least 40-60 bits of entropy
/// - Derivation time of ~1 second is acceptable for key operations
///
/// For higher security requirements, consider increasing m_cost or t_cost.
///
/// # Passphrase Requirements
///
/// For adequate security, passphrases should have sufficient entropy:
///
/// - **Minimum recommended**: 80 bits of entropy (approximately 16 random characters
///   or 6 random words from a 7776-word list like EFF's diceware)
/// - **High security**: 128+ bits of entropy (approximately 22 random characters
///   or 10 random diceware words)
///
/// ## Calculating Entropy
///
/// - Random lowercase letters (a-z): ~4.7 bits per character
/// - Random alphanumeric (a-z, A-Z, 0-9): ~5.95 bits per character
/// - Random printable ASCII: ~6.5 bits per character
/// - Diceware word list (7776 words): ~12.9 bits per word
///
/// ## Example: Secure Passphrase Generation
///
/// Using a cryptographically secure random generator:
///
/// ```rust,no_run
/// use rand::{rngs::OsRng, Rng};
///
/// // Generate a 128-bit entropy passphrase (22 alphanumeric characters)
/// fn generate_secure_passphrase() -> String {
///     const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
///     let mut rng = OsRng;
///     (0..22)
///         .map(|_| {
///             let idx = rng.gen_range(0..CHARSET.len());
///             CHARSET[idx] as char
///         })
///         .collect()
/// }
/// ```
///
/// **WARNING**: Do not use predictable passphrases like dictionary words, names,
/// dates, or common phrases. These provide far less entropy than random characters
/// and are vulnerable to dictionary attacks even with strong key derivation.
///
/// # Security: Caller Zeroization Responsibility
///
/// This function returns a plain `[u8; 32]` array. **Callers MUST zeroize the returned
/// key after use** by calling `.zeroize()` on it. This is an internal function, and all
/// call sites within this module properly zeroize the derived key after use.
///
/// If you are adding a new call site, ensure you follow this pattern:
/// ```ignore
/// let mut derived_key = derive_key_from_passphrase(&pass, &salt, level)?;
/// // ... use derived_key ...
/// derived_key.zeroize(); // REQUIRED: zeroize after use
/// ```
fn derive_key_from_passphrase(passphrase: &Zeroizing<String>, salt: &[u8; 16], security_level: SecurityLevel) -> Result<[u8; 32]> {
    // Get Argon2id parameters based on security level
    let params = security_level.params()?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

    let salt_string = SaltString::encode_b64(salt)
        .map_err(|e| CryptoError::KeyDerivationFailure(format!("salt encoding failed: {}", e)))?;

    // Wrap passphrase bytes in Zeroizing to ensure they are cleared after use.
    // The passphrase.as_bytes() returns a &[u8] slice pointing to the original string,
    // but Argon2 may create internal copies. We create an explicit zeroizing copy
    // to ensure at least our copy is properly cleared.
    let passphrase_bytes = Zeroizing::new(passphrase.as_bytes().to_vec());

    let hash = argon2
        .hash_password(&passphrase_bytes, &salt_string)
        .map_err(|e| CryptoError::KeyDerivationFailure(format!("Argon2 hashing failed: {}", e)))?;

    // passphrase_bytes is automatically zeroized when dropped here

    // Extract the 32-byte key from the hash
    let hash_bytes = hash
        .hash
        .ok_or_else(|| CryptoError::KeyDerivationFailure("hash output missing".into()))?;
    let mut key = [0u8; 32];
    key.copy_from_slice(hash_bytes.as_bytes());

    Ok(key)
}

/// Encrypt data using ChaCha20-Poly1305 AEAD.
fn encrypt_with_key(data: &[u8], key: &[u8; 32], nonce: &[u8; 12]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce);

    cipher
        .encrypt(nonce, data)
        .map_err(|_| CryptoError::AeadEncryptionFailure)
}

/// Decrypt data using ChaCha20-Poly1305 AEAD.
///
/// Returns the decrypted plaintext wrapped in `Zeroizing` for automatic cleanup.
fn decrypt_with_key(ciphertext: &[u8], key: &[u8; 32], nonce: &[u8; 12]) -> Result<Zeroizing<Vec<u8>>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce);

    cipher
        .decrypt(nonce, ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::AeadDecryptionFailure)
}

/// Encode bytes to hex lines (64 chars per line) for key file format.
///
/// # Security Note
///
/// This function creates a non-zeroizing String for the hex-encoded output.
/// It should only be used for:
/// - Public key data (verifying keys)
/// - Encrypted ciphertext (already protected)
/// - Salt and nonce values (public by design)
///
/// **Do NOT use this function for plaintext secret key material.**
/// The raw signing key bytes should be encrypted before encoding.
fn encode_key_data(data: &[u8]) -> String {
    let hex = data
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    hex.as_bytes()
        .chunks(64)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decode a hex string to bytes, ignoring whitespace.
///
/// This is the single implementation used by both `decode_key_data` (for multi-line
/// hex blocks) and inline hex parsing (for salt/nonce values).
///
/// Uses the `hex` crate's constant-time decoder to prevent timing attacks.
///
/// # Errors
///
/// Returns specific errors for different failure modes:
/// - [`CryptoError::InvalidKeyFileHexOddLength`] - Odd number of hex characters
/// - [`CryptoError::InvalidKeyFileHexChar`] - Invalid hex character
fn decode_hex_string(hex_str: &str) -> Result<Vec<u8>> {
    let hex: String = hex_str.chars().filter(|c| !c.is_whitespace()).collect();

    if hex.len() % 2 != 0 {
        return Err(CryptoError::InvalidKeyFileHexOddLength);
    }

    // Use the hex crate's constant-time decoder to prevent timing attacks
    hex::decode(&hex).map_err(|e| {
        // Try to extract the position information from the error if possible
        let error_str = e.to_string();
        if let Some(_idx) = error_str.find("Invalid character") {
            // Extract character if present in error message
            if let Some(ch) = hex.chars().nth(0) {
                return CryptoError::InvalidKeyFileHexChar {
                    character: ch,
                    position: 0,
                };
            }
        }
        // Fallback error
        CryptoError::InvalidKeyFileHexChar {
            character: '?',
            position: 0,
        }
    })
}

/// Decode hex lines back to bytes (convenience wrapper for key data blocks).
#[inline]
fn decode_key_data(data: &str) -> Result<Vec<u8>> {
    decode_hex_string(data)
}

/// Save a signing key to a file.
///
/// If passphrase is provided, the key will be encrypted using ChaCha20-Poly1305 AEAD
/// with a key derived from the passphrase using Argon2id.
///
/// # Security Note
///
/// **Passphrase Handling**: This function enforces passphrase zeroization by requiring
/// `Option<&Zeroizing<String>>`. Callers must wrap passphrases in `Zeroizing` for
/// automatic cleanup:
///
/// ```rust,no_run
/// use zeroize::Zeroizing;
///
/// // Read passphrase securely and wrap in Zeroizing
/// let passphrase = Zeroizing::new(get_passphrase_from_user());
///
/// // Pass reference to the Zeroizing wrapper
/// // save_signing_key(&key, &path, Some(&passphrase), SecurityLevel::Standard)?;
/// // `passphrase` is automatically zeroized when dropped
///
/// # fn get_passphrase_from_user() -> String { String::new() }
/// ```
///
/// **Nonce Strategy**: Each call generates a fresh random 12-byte nonce using
/// cryptographically secure randomness (OsRng). This ensures that even if the same
/// key is encrypted multiple times with the same passphrase, each ciphertext is unique.
/// The 96-bit nonce provides negligible collision probability for practical use.
///
/// **WARNING**: While re-encryption is safe due to random nonce generation, each key
/// should ideally be encrypted only once per passphrase to minimize cryptographic
/// operations on sensitive material.
pub fn save_signing_key(key: &SigningKey, path: &Path, passphrase: Option<&Zeroizing<String>>, security_level: SecurityLevel) -> Result<()> {
    let mut key_bytes = key.to_bytes();

    let content = match passphrase {
        Some(pass) => {
            // Generate cryptographically random salt (16 bytes) and nonce (12 bytes).
            // Using OsRng ensures high-quality randomness from the operating system.
            // The random nonce prevents nonce reuse even if the same key/passphrase
            // combination is used multiple times.
            let mut salt = [0u8; 16];
            let mut nonce = [0u8; 12];
            OsRng.fill_bytes(&mut salt);
            OsRng.fill_bytes(&mut nonce);

            // Derive encryption key from passphrase
            let mut derived_key = derive_key_from_passphrase(pass, &salt, security_level)?;

            // Encrypt the key bytes
            let ciphertext = encrypt_with_key(&key_bytes, &derived_key, &nonce)?;

            // Zeroize sensitive data
            derived_key.zeroize();
            key_bytes.zeroize();

            // Encode salt, nonce, and ciphertext as hex
            let salt_hex = salt
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            let nonce_hex = nonce
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();

            let security_level_str = match security_level {
                SecurityLevel::Standard => "standard",
                SecurityLevel::High => "high",
                SecurityLevel::Maximum => "maximum",
            };

            format!(
                "{}\nversion: {}\nsecurity_level: {}\nsalt: {}\nnonce: {}\nciphertext: {}\n{}\n",
                ENCRYPTED_SIGNING_KEY_HEADER,
                CURRENT_VERSION,
                security_level_str,
                salt_hex,
                nonce_hex,
                encode_key_data(&ciphertext),
                ENCRYPTED_SIGNING_KEY_FOOTER
            )
        }
        None => {
            let content = format!(
                "{}\n{}\n{}\n",
                SIGNING_KEY_HEADER,
                encode_key_data(&key_bytes),
                SIGNING_KEY_FOOTER
            );
            key_bytes.zeroize();
            content
        }
    };

    // Write atomically: write to temp file, then rename
    // This prevents partial writes if the process is interrupted
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &content).map_err(|e| CryptoError::io(&temp_path, e))?;

    // Set file permissions to 0600 (owner read/write only) for security
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&temp_path, perms).map_err(|e| CryptoError::io(&temp_path, e))?;
    }

    fs::rename(&temp_path, path).map_err(|e| CryptoError::io(path, e))?;
    Ok(())
}

/// Encrypted key metadata components
type EncryptedKeyMetadata = (u8, SecurityLevel, Vec<u8>, Vec<u8>, Vec<u8>);

/// Parse encrypted key metadata from file content
fn parse_encrypted_key_metadata(data: &str) -> Result<EncryptedKeyMetadata> {
    let mut version: Option<u8> = None;
    let mut security_level: Option<SecurityLevel> = None;
    let mut salt: Option<Vec<u8>> = None;
    let mut nonce: Option<Vec<u8>> = None;
    let mut ciphertext_lines = Vec::new();

    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(v) = line.strip_prefix("version:") {
            version = Some(
                v.trim()
                    .parse()
                    .map_err(|_| CryptoError::InvalidKeyFileVersion {
                        expected: CURRENT_VERSION,
                        actual: 0, // Unknown - couldn't parse
                    })?,
            );
        } else if let Some(sl) = line.strip_prefix("security_level:") {
            let sl = sl.trim();
            security_level = Some(match sl {
                "standard" => SecurityLevel::Standard,
                "high" => SecurityLevel::High,
                "maximum" => SecurityLevel::Maximum,
                _ => return Err(CryptoError::InvalidKeyFileFormat),
            });
        } else if let Some(s) = line.strip_prefix("salt:") {
            let hex = s.trim();
            salt = Some(decode_hex_string(hex).map_err(|_| CryptoError::InvalidKeyFileSalt {
                reason: "invalid hex encoding".into(),
            })?);
        } else if let Some(n) = line.strip_prefix("nonce:") {
            let hex = n.trim();
            nonce = Some(decode_hex_string(hex).map_err(|_| CryptoError::InvalidKeyFileNonce {
                reason: "invalid hex encoding".into(),
            })?);
        } else if let Some(c) = line.strip_prefix("ciphertext:") {
            ciphertext_lines.push(c.trim());
        } else if !line.starts_with("-----") {
            // Continuation of ciphertext
            ciphertext_lines.push(line);
        }
    }

    let version = version.ok_or(CryptoError::InvalidKeyFileVersion {
        expected: CURRENT_VERSION,
        actual: 0,
    })?;
    // Default to Standard for backward compatibility with keys that don't have security_level
    let security_level = security_level.unwrap_or(SecurityLevel::Standard);
    let salt = salt.ok_or(CryptoError::InvalidKeyFileSalt {
        reason: "missing salt field".into(),
    })?;
    let nonce = nonce.ok_or(CryptoError::InvalidKeyFileNonce {
        reason: "missing nonce field".into(),
    })?;
    let ciphertext = decode_key_data(&ciphertext_lines.join("\n"))?;

    Ok((version, security_level, salt, nonce, ciphertext))
}

/// Load a signing key from a file.
///
/// If the key is encrypted, passphrase must be provided.
/// Detects and rejects legacy (insecure) encrypted key format.
///
/// # Security Note
///
/// This function enforces passphrase zeroization by requiring `Option<&Zeroizing<String>>`.
/// Callers must wrap passphrases in `Zeroizing` for automatic cleanup after use.
/// See module-level documentation for examples.
///
/// # Errors
///
/// - [`CryptoError::PassphraseRequired`] - Encrypted key but no passphrase provided
/// - [`CryptoError::InvalidKeyFileVersion`] - Unsupported key file version
/// - [`CryptoError::InvalidKeyFileSalt`] - Salt is missing or malformed
/// - [`CryptoError::InvalidKeyFileNonce`] - Nonce is missing or malformed
/// - [`CryptoError::AeadDecryptionFailure`] - Wrong passphrase or corrupted ciphertext
/// - [`CryptoError::LegacyKeyFormat`] - Old insecure format detected
pub fn load_signing_key(path: &Path, passphrase: Option<&Zeroizing<String>>) -> Result<SigningKey> {
    let content = fs::read_to_string(path).map_err(|e| CryptoError::io(path, e))?;
    let content = content.trim();

    if content.starts_with(ENCRYPTED_SIGNING_KEY_HEADER) {
        let data = content
            .strip_prefix(ENCRYPTED_SIGNING_KEY_HEADER)
            .and_then(|s| s.strip_suffix(ENCRYPTED_SIGNING_KEY_FOOTER))
            .ok_or(CryptoError::InvalidKeyFileFormat)?
            .trim();

        // Format Detection Strategy:
        // -------------------------
        // We distinguish between legacy (insecure) and modern key formats by checking
        // for the presence of a "version:" field in the key file content.
        //
        // Modern format (v1+):
        //   - Contains structured metadata: "version:", "salt:", "nonce:", "ciphertext:"
        //   - Uses Argon2id for key derivation with explicit salt
        //   - Uses ChaCha20-Poly1305 with explicit nonce for authenticated encryption
        //
        // Legacy format (pre-v1):
        //   - Contains only raw hex-encoded data without metadata fields
        //   - Used insecure encryption without proper key derivation
        //   - Detected by absence of "version:" field
        //
        // This substring-based detection is intentional and safe because:
        // 1. The "version:" prefix cannot appear in valid hex-encoded ciphertext
        // 2. Legacy files only contain hex characters (0-9, a-f) and whitespace
        // 3. The modern format always starts with "version: 1" as its first field
        if data.contains("version:") {
            // New secure format
            let (version, security_level, salt, nonce, ciphertext) = parse_encrypted_key_metadata(data)?;

            if version != CURRENT_VERSION {
                return Err(CryptoError::InvalidKeyFileVersion {
                    expected: CURRENT_VERSION,
                    actual: version,
                });
            }

            let pass = passphrase.ok_or(CryptoError::PassphraseRequired)?;

            // Convert salt and nonce to fixed-size arrays with specific errors
            let salt: [u8; 16] = salt.try_into().map_err(|_| CryptoError::InvalidKeyFileSalt {
                reason: "expected 16 bytes, got different length".to_string(),
            })?;
            let nonce: [u8; 12] = nonce.try_into().map_err(|_| CryptoError::InvalidKeyFileNonce {
                reason: "expected 12 bytes, got different length".to_string(),
            })?;

            // Derive decryption key using the security level from the file
            let mut derived_key = derive_key_from_passphrase(pass, &salt, security_level)?;

            // Decrypt the key - returns Zeroizing<Vec<u8>> for automatic cleanup
            let decrypted = decrypt_with_key(&ciphertext, &derived_key, &nonce)?;

            // Zeroize derived key immediately after use
            derived_key.zeroize();

            // Convert to signing key - decrypted is automatically zeroized when dropped
            let mut key_bytes: [u8; 32] = decrypted.as_slice().try_into().map_err(|_| {
                CryptoError::InvalidKeyFileHexLength {
                    expected: 32,
                    actual: decrypted.len(),
                }
            })?;

            let result = SigningKey::from_bytes(&key_bytes);
            key_bytes.zeroize(); // Zeroize key_bytes after use
            result
            // `decrypted` (Zeroizing<Vec<u8>>) is automatically zeroized here when dropped
        } else {
            // Legacy insecure format detected
            Err(CryptoError::LegacyKeyFormat)
        }
    } else if content.starts_with(SIGNING_KEY_HEADER) {
        // Unencrypted key
        let data = content
            .strip_prefix(SIGNING_KEY_HEADER)
            .and_then(|s| s.strip_suffix(SIGNING_KEY_FOOTER))
            .ok_or(CryptoError::InvalidKeyFileFormat)?
            .trim();

        let decoded = Zeroizing::new(decode_key_data(data)?);
        let key_bytes: [u8; 32] = decoded.as_slice().try_into().map_err(|_| {
            CryptoError::InvalidKeyFileHexLength {
                expected: 32,
                actual: decoded.len(),
            }
        })?;

        let result = SigningKey::from_bytes(&key_bytes);
        // decoded is automatically zeroized when dropped here
        result
    } else {
        Err(CryptoError::InvalidKeyFileFormat)
    }
}

/// Save a verifying key to a file
pub fn save_verifying_key(key: &VerifyingKey, path: &Path) -> Result<()> {
    let key_bytes = key.to_bytes();
    let content = format!(
        "{}\n{}\n{}\n",
        VERIFYING_KEY_HEADER,
        encode_key_data(&key_bytes),
        VERIFYING_KEY_FOOTER
    );

    // Write atomically: write to temp file, then rename
    // This prevents partial writes if the process is interrupted
    let temp_path = path.with_extension("tmp");
    fs::write(&temp_path, &content).map_err(|e| CryptoError::io(&temp_path, e))?;

    // Verifying keys are public; 0644 allows world-read while keeping owner-only writes.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o644);
        fs::set_permissions(&temp_path, perms).map_err(|e| CryptoError::io(&temp_path, e))?;
    }

    fs::rename(&temp_path, path).map_err(|e| CryptoError::io(path, e))?;
    Ok(())
}

/// Load a verifying key from a file
pub fn load_verifying_key(path: &Path) -> Result<VerifyingKey> {
    let content = fs::read_to_string(path).map_err(|e| CryptoError::io(path, e))?;
    let content = content.trim();

    let data = content
        .strip_prefix(VERIFYING_KEY_HEADER)
        .and_then(|s| s.strip_suffix(VERIFYING_KEY_FOOTER))
        .ok_or(CryptoError::InvalidKeyFileFormat)?
        .trim();

    let decoded = decode_key_data(data)?;
    let key_bytes: [u8; 32] = decoded.try_into().map_err(|v: Vec<u8>| {
        CryptoError::InvalidKeyFileHexLength {
            expected: 32,
            actual: v.len(),
        }
    })?;

    VerifyingKey::from_bytes(&key_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::SigningKey;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;

    fn temp_file(name: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir();
        temp_dir.join(format!("vbdp_test_{}", name))
    }

    #[test]
    fn test_unencrypted_key_save_and_load() {
        let temp_path = temp_file("unencrypted_key.pem");

        // Generate a signing key
        let signing_key = SigningKey::generate();
        let original_bytes = signing_key.to_bytes();

        // Save without encryption
        save_signing_key(&signing_key, &temp_path, None, SecurityLevel::Standard).unwrap();

        // Load it back
        let loaded_key = load_signing_key(&temp_path, None).unwrap();
        assert_eq!(loaded_key.to_bytes(), original_bytes);

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_encrypted_key_save_and_load() {
        let temp_path = temp_file("encrypted_key.pem");
        let passphrase = Zeroizing::new("super_secret_passphrase_123!".to_string());

        // Generate a signing key
        let signing_key = SigningKey::generate();
        let original_bytes = signing_key.to_bytes();

        // Save with encryption
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Load it back with correct passphrase
        let loaded_key = load_signing_key(&temp_path, Some(&passphrase)).unwrap();
        assert_eq!(loaded_key.to_bytes(), original_bytes);

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_encrypted_key_wrong_passphrase() {
        let temp_path = temp_file("encrypted_wrong_pass.pem");
        let passphrase = Zeroizing::new("correct_passphrase".to_string());
        let wrong_passphrase = Zeroizing::new("wrong_passphrase".to_string());

        // Generate and save with encryption
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Try to load with wrong passphrase - should fail
        let result = load_signing_key(&temp_path, Some(&wrong_passphrase));
        assert!(result.is_err());

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_encrypted_key_requires_passphrase() {
        let temp_path = temp_file("encrypted_no_pass.pem");
        let passphrase = Zeroizing::new("my_passphrase".to_string());

        // Generate and save with encryption
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Try to load without passphrase - should fail
        let result = load_signing_key(&temp_path, None);
        assert!(matches!(result, Err(CryptoError::PassphraseRequired)));

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_encrypted_key_format_has_version_salt_nonce() {
        let temp_path = temp_file("format_check.pem");
        let passphrase = Zeroizing::new("test_passphrase".to_string());

        // Generate and save with encryption
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Read the file and verify format
        let content = fs::read_to_string(&temp_path).unwrap();
        assert!(content.contains("version: 1"));
        assert!(content.contains("security_level: standard"));
        assert!(content.contains("salt:"));
        assert!(content.contains("nonce:"));
        assert!(content.contains("ciphertext:"));
        assert!(content.starts_with(ENCRYPTED_SIGNING_KEY_HEADER));
        assert!(content.trim().ends_with(ENCRYPTED_SIGNING_KEY_FOOTER));

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_legacy_format_detection() {
        let temp_path = temp_file("legacy_format.pem");

        // Create a legacy format encrypted key (without version/salt/nonce fields)
        let legacy_content = format!(
            "{}\n{}\n{}\n",
            ENCRYPTED_SIGNING_KEY_HEADER,
            "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
            ENCRYPTED_SIGNING_KEY_FOOTER
        );
        fs::write(&temp_path, legacy_content).unwrap();

        // Try to load - should detect legacy format
        let passphrase = Zeroizing::new("any_passphrase".to_string());
        let result = load_signing_key(&temp_path, Some(&passphrase));
        assert!(matches!(result, Err(CryptoError::LegacyKeyFormat)));

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_verifying_key_save_and_load() {
        let temp_path = temp_file("verifying_key.pub");

        // Generate a signing key and get its verifying key
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let original_bytes = verifying_key.to_bytes();

        // Save verifying key
        save_verifying_key(&verifying_key, &temp_path).unwrap();

        // Load it back
        let loaded_key = load_verifying_key(&temp_path).unwrap();
        assert_eq!(loaded_key.to_bytes(), original_bytes);

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_derive_key_from_passphrase_consistency() {
        let passphrase = Zeroizing::new("test_passphrase_456".to_string());
        let salt = [42u8; 16];

        // Derive key twice with same inputs
        let key1 = derive_key_from_passphrase(&passphrase, &salt, SecurityLevel::Standard).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt, SecurityLevel::Standard).unwrap();

        // Should be identical
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_derive_key_from_passphrase_different_salts() {
        let passphrase = Zeroizing::new("same_passphrase".to_string());
        let salt1 = [1u8; 16];
        let salt2 = [2u8; 16];

        // Derive keys with different salts
        let key1 = derive_key_from_passphrase(&passphrase, &salt1, SecurityLevel::Standard).unwrap();
        let key2 = derive_key_from_passphrase(&passphrase, &salt2, SecurityLevel::Standard).unwrap();

        // Should be different
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = b"sensitive key material here";
        let key = [55u8; 32];
        let nonce = [99u8; 12];

        // Encrypt
        let ciphertext = encrypt_with_key(plaintext, &key, &nonce).unwrap();

        // Decrypt - returns Zeroizing<Vec<u8>>
        let decrypted = decrypt_with_key(&ciphertext, &key, &nonce).unwrap();

        // Should match original (compare inner slice)
        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let plaintext = b"sensitive data";
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let nonce = [0u8; 12];

        // Encrypt with key1
        let ciphertext = encrypt_with_key(plaintext, &key1, &nonce).unwrap();

        // Try to decrypt with key2 - should fail with AEAD error
        let result = decrypt_with_key(&ciphertext, &key2, &nonce);
        assert!(matches!(result, Err(CryptoError::AeadDecryptionFailure)));
    }

    #[test]
    fn test_ciphertext_integrity() {
        let plaintext = b"important data";
        let key = [7u8; 32];
        let nonce = [3u8; 12];

        // Encrypt
        let mut ciphertext = encrypt_with_key(plaintext, &key, &nonce).unwrap();

        // Tamper with ciphertext
        if let Some(byte) = ciphertext.get_mut(0) {
            *byte = byte.wrapping_add(1);
        }

        // Decryption should fail due to authentication tag mismatch
        let result = decrypt_with_key(&ciphertext, &key, &nonce);
        assert!(matches!(result, Err(CryptoError::AeadDecryptionFailure)));
    }

    #[test]
    fn test_concurrent_encryption_unique_nonces() {
        // Create a signing key
        let signing_key = Arc::new(SigningKey::generate());
        let passphrase = Arc::new(Zeroizing::new("concurrent_test_passphrase".to_string()));

        // Encrypt it 10 times in parallel threads
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let key = Arc::clone(&signing_key);
                let pass = Arc::clone(&passphrase);
                thread::spawn(move || {
                    let temp_path = temp_file(&format!("concurrent_nonce_{}.pem", i));
                    save_signing_key(&key, &temp_path, Some(&pass), SecurityLevel::Standard).unwrap();

                    // Read the file and extract the nonce
                    let content = fs::read_to_string(&temp_path).unwrap();
                    let nonce = content
                        .lines()
                        .find(|line| line.starts_with("nonce:"))
                        .map(|line| line.strip_prefix("nonce:").unwrap().trim().to_string())
                        .unwrap();

                    // Clean up
                    fs::remove_file(&temp_path).ok();

                    nonce
                })
            })
            .collect();

        // Collect all encrypted outputs and extract nonces
        let nonces: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify all nonces are unique
        let unique_nonces: HashSet<_> = nonces.iter().collect();
        assert_eq!(
            unique_nonces.len(),
            nonces.len(),
            "All nonces should be unique, but found duplicates"
        );
    }

    #[test]
    fn test_corrupted_key_truncated_ciphertext() {
        let temp_path = temp_file("truncated_ciphertext.pem");
        let passphrase = Zeroizing::new("test_passphrase".to_string());

        // Generate and save a valid encrypted key
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Read, truncate ciphertext, and write back
        let content = fs::read_to_string(&temp_path).unwrap();
        let mut modified_lines: Vec<&str> = Vec::new();
        let mut in_ciphertext = false;
        let mut ciphertext_truncated = false;

        for line in content.lines() {
            if line.starts_with("ciphertext:") {
                in_ciphertext = true;
                // Truncate the ciphertext by taking only part of it
                let truncated = &line[..line.len().min(20)];
                modified_lines.push(truncated);
                ciphertext_truncated = true;
            } else if in_ciphertext && !line.starts_with("-----") {
                // Skip continuation lines of ciphertext (truncate)
                continue;
            } else {
                modified_lines.push(line);
                if line.starts_with("-----END") {
                    in_ciphertext = false;
                }
            }
        }

        assert!(ciphertext_truncated, "Test setup: ciphertext should have been truncated");
        let modified_content = modified_lines.join("\n") + "\n";
        fs::write(&temp_path, modified_content).unwrap();

        // Try to load - should fail gracefully
        let result = load_signing_key(&temp_path, Some(&passphrase));
        assert!(result.is_err(), "Truncated ciphertext should fail to decrypt");

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_corrupted_key_modified_salt() {
        let temp_path = temp_file("modified_salt.pem");
        let passphrase = Zeroizing::new("test_passphrase".to_string());

        // Generate and save a valid encrypted key
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Read and modify the salt
        let content = fs::read_to_string(&temp_path).unwrap();
        let modified_content = content
            .lines()
            .map(|line| {
                if line.starts_with("salt:") {
                    // Replace first hex character to corrupt the salt
                    let salt_value = line.strip_prefix("salt:").unwrap().trim();
                    let corrupted = format!(
                        "salt: {}",
                        if salt_value.starts_with('0') {
                            format!("f{}", &salt_value[1..])
                        } else {
                            format!("0{}", &salt_value[1..])
                        }
                    );
                    corrupted
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";

        fs::write(&temp_path, modified_content).unwrap();

        // Try to load - should fail (wrong derived key)
        let result = load_signing_key(&temp_path, Some(&passphrase));
        assert!(
            matches!(result, Err(CryptoError::AeadDecryptionFailure)),
            "Modified salt should cause AEAD decryption to fail"
        );

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_corrupted_key_modified_nonce() {
        let temp_path = temp_file("modified_nonce.pem");
        let passphrase = Zeroizing::new("test_passphrase".to_string());

        // Generate and save a valid encrypted key
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Read and modify the nonce
        let content = fs::read_to_string(&temp_path).unwrap();
        let modified_content = content
            .lines()
            .map(|line| {
                if line.starts_with("nonce:") {
                    // Replace first hex character to corrupt the nonce
                    let nonce_value = line.strip_prefix("nonce:").unwrap().trim();
                    let corrupted = format!(
                        "nonce: {}",
                        if nonce_value.starts_with('0') {
                            format!("f{}", &nonce_value[1..])
                        } else {
                            format!("0{}", &nonce_value[1..])
                        }
                    );
                    corrupted
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";

        fs::write(&temp_path, modified_content).unwrap();

        // Try to load - should fail (wrong nonce causes AEAD failure)
        let result = load_signing_key(&temp_path, Some(&passphrase));
        assert!(
            matches!(result, Err(CryptoError::AeadDecryptionFailure)),
            "Modified nonce should cause AEAD decryption to fail"
        );

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_corrupted_key_bit_flip_in_auth_tag() {
        let temp_path = temp_file("bit_flip_auth_tag.pem");
        let passphrase = Zeroizing::new("test_passphrase".to_string());

        // Generate and save a valid encrypted key
        let signing_key = SigningKey::generate();
        save_signing_key(&signing_key, &temp_path, Some(&passphrase), SecurityLevel::Standard).unwrap();

        // Read and modify the last 16 bytes of ciphertext (the auth tag is appended)
        let content = fs::read_to_string(&temp_path).unwrap();
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        // Find and modify the ciphertext line(s) - flip a bit in the last hex char
        let mut found_ciphertext = false;
        for i in (0..lines.len()).rev() {
            if lines[i].starts_with("ciphertext:") || (!lines[i].starts_with("-----") && found_ciphertext) {
                if lines[i].starts_with("ciphertext:") {
                    found_ciphertext = true;
                }
                // Find a hex character and flip it
                let line = &mut lines[i];
                if let Some(pos) = line.rfind(|c: char| c.is_ascii_hexdigit()) {
                    let chars: Vec<char> = line.chars().collect();
                    let hex_char = chars[pos];
                    // Flip a bit in the hex digit
                    let flipped = match hex_char {
                        '0' => '1',
                        '1' => '0',
                        'a' => 'b',
                        'b' => 'a',
                        _ => if hex_char.is_ascii_digit() {
                            ((hex_char as u8) ^ 1) as char
                        } else {
                            ((hex_char as u8) ^ 1) as char
                        },
                    };
                    let mut new_line: String = chars[..pos].iter().collect();
                    new_line.push(flipped);
                    new_line.extend(chars[pos + 1..].iter());
                    lines[i] = new_line;
                    break;
                }
            }
        }

        let modified_content = lines.join("\n") + "\n";
        fs::write(&temp_path, modified_content).unwrap();

        // Try to load - should fail due to authentication tag mismatch
        let result = load_signing_key(&temp_path, Some(&passphrase));
        assert!(
            matches!(result, Err(CryptoError::AeadDecryptionFailure)),
            "Bit flip in ciphertext/auth tag should cause AEAD decryption to fail"
        );

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }

    #[test]
    fn test_empty_passphrase_encryption() {
        // Test that empty passphrase works (encrypts with empty-derived key)
        // Empty passphrases are cryptographically weak but should not cause errors
        let temp_path = temp_file("empty_passphrase.pem");
        let empty_passphrase = Zeroizing::new(String::new());

        // Generate a signing key
        let signing_key = SigningKey::generate();
        let original_bytes = signing_key.to_bytes();

        // Save with empty passphrase - should succeed (derives key from empty string)
        save_signing_key(&signing_key, &temp_path, Some(&empty_passphrase), SecurityLevel::Standard).unwrap();

        // Load it back with the same empty passphrase
        let loaded_key = load_signing_key(&temp_path, Some(&empty_passphrase)).unwrap();
        assert_eq!(loaded_key.to_bytes(), original_bytes);

        // Verify wrong passphrase still fails (proving encryption occurred)
        let wrong_passphrase = Zeroizing::new("not_empty".to_string());
        let result = load_signing_key(&temp_path, Some(&wrong_passphrase));
        assert!(matches!(result, Err(CryptoError::AeadDecryptionFailure)),
            "Wrong passphrase should fail even when original was empty");

        // Clean up
        let cleanup_result = fs::remove_file(&temp_path);
        debug_assert!(cleanup_result.is_ok(), "Test cleanup failed for {:?}: {:?}", temp_path, cleanup_result.err());
    }
}
