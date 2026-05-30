use ed25519_dalek::{self, Signer};
use rand::rngs::OsRng;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{CryptoError, Result};

/// Ed25519 signature (64 bytes)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature(pub(crate) [u8; 64]);

impl Signature {
    /// Create signature from bytes
    #[must_use]
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Signature(bytes)
    }

    /// Get signature as bytes
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 64] {
        self.0
    }
}

/// Ed25519 signing key (private key)
///
/// Security: ed25519-dalek v2.1+ implements Zeroize by default for SigningKey.
/// The inner key material is automatically zeroized when dropped.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SigningKey {
    #[zeroize(skip)] // ed25519_dalek::SigningKey handles its own zeroization
    inner: ed25519_dalek::SigningKey,
}

impl SigningKey {
    /// Generate a new random signing key
    #[must_use]
    pub fn generate() -> Self {
        let inner = ed25519_dalek::SigningKey::generate(&mut OsRng);
        SigningKey { inner }
    }

    /// Create signing key from raw bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let inner = ed25519_dalek::SigningKey::from_bytes(bytes);
        Ok(SigningKey { inner })
    }

    /// Export signing key as raw bytes
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Get the corresponding verifying key
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }

    /// Sign a message
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.inner.sign(message);
        Signature(sig.to_bytes())
    }
}

/// Ed25519 verifying key (public key)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyingKey {
    inner: ed25519_dalek::VerifyingKey,
}

impl VerifyingKey {
    /// Create verifying key from raw bytes
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        let inner = ed25519_dalek::VerifyingKey::from_bytes(bytes)
            .map_err(|_| CryptoError::InvalidVerifyingKey)?;
        Ok(VerifyingKey { inner })
    }

    /// Export verifying key as raw bytes
    #[must_use]
    pub fn to_bytes(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Verify a signature on a message.
    ///
    /// # Security: Strict, Constant-Time Verification
    ///
    /// This method delegates to `ed25519-dalek`'s **strict** verification
    /// (`verify_strict`), which performs constant-time signature verification and
    /// additionally rejects non-canonical / malleable signatures. The underlying
    /// implementation:
    ///
    /// - Uses constant-time field arithmetic from the `curve25519-dalek` crate
    /// - Performs verification in a way that does not leak timing information
    ///   about the signature or message through early-exit or data-dependent branching
    /// - Rejects signatures with a non-canonical `R` or `s` component and rejects
    ///   small-order public keys, preventing signature-malleability attacks where a
    ///   second distinct-but-valid signature can be derived for the same message
    /// - Returns the same error type regardless of where verification fails,
    ///   preventing information leakage through error differentiation
    ///
    /// The error mapping to `CryptoError::SignatureVerificationFailure` preserves
    /// this property by using a single, uniform error variant.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        let sig = ed25519_dalek::Signature::from_bytes(&signature.0);
        self.inner
            .verify_strict(message, &sig)
            .map_err(|_| CryptoError::SignatureVerificationFailure)
    }
}

/// Domain-separation tag for the canonical VBDP signing payload (12 bytes:
/// the ASCII string `VBDP-sig-v1` followed by a single `0x00` byte).
pub const SIGNING_DOMAIN_TAG: &[u8; 12] = b"VBDP-sig-v1\x00";

/// Build the canonical Ed25519 signing payload ("VBDP-sig-v1").
///
/// This is the **single source of truth** for what gets signed/verified across
/// the whole system (publisher, server, and client all call this function). The
/// signed message is EXACTLY these bytes concatenated, in order:
///
///   1. The 12-byte domain tag [`SIGNING_DOMAIN_TAG`].
///   2. The raw 32 bytes of the BLAKE3 hash of the binary (NOT hex).
///   3. The UTF-8 bytes of the version string.
///
/// The domain tag is fixed at 12 bytes and the BLAKE3 hash is fixed at 32 bytes,
/// so the version string is the unambiguous remainder — no length prefixes or
/// separators are needed.
///
/// Binding the version into the signed message prevents signature-substitution
/// and downgrade attacks: a signature issued for one version cannot be replayed
/// against a different version.
#[must_use]
pub fn signing_payload(blake3_hash: &[u8], version_string: &str) -> Vec<u8> {
    let mut msg =
        Vec::with_capacity(SIGNING_DOMAIN_TAG.len() + blake3_hash.len() + version_string.len());
    msg.extend_from_slice(SIGNING_DOMAIN_TAG);
    msg.extend_from_slice(blake3_hash);
    msg.extend_from_slice(version_string.as_bytes());
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_payload_layout() {
        let hash = [0xABu8; 32];
        let payload = signing_payload(&hash, "1.2.3");
        // domain (12) + hash (32) + "1.2.3" (5)
        assert_eq!(payload.len(), 12 + 32 + 5);
        assert_eq!(&payload[..12], b"VBDP-sig-v1\x00");
        assert_eq!(&payload[12..44], &hash);
        assert_eq!(&payload[44..], b"1.2.3");
    }

    #[test]
    fn test_signing_payload_golden_vector() {
        // Frozen byte vector — changing the payload format MUST break this test
        // so all three crates are forced to stay in lockstep.
        let hash = [0u8; 32];
        let payload = signing_payload(&hash, "0.1.0");
        let mut expected = Vec::new();
        expected.extend_from_slice(b"VBDP-sig-v1\x00");
        expected.extend_from_slice(&[0u8; 32]);
        expected.extend_from_slice(b"0.1.0");
        assert_eq!(payload, expected);
    }

    #[test]
    fn test_signing_payload_binds_version() {
        let hash = [0x11u8; 32];
        assert_ne!(signing_payload(&hash, "1.0.0"), signing_payload(&hash, "1.0.1"));
    }

    #[test]
    fn test_signature_generation_and_verification() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let message = b"test message for signing";

        let signature = signing_key.sign(message);

        // Verification should succeed
        assert!(verifying_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_signature_verification_fails_with_wrong_message() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let message = b"original message";
        let wrong_message = b"tampered message";

        let signature = signing_key.sign(message);

        // Verification with wrong message should fail
        assert!(verifying_key.verify(wrong_message, &signature).is_err());
    }

    #[test]
    fn test_signature_verification_fails_with_wrong_key() {
        let signing_key1 = SigningKey::generate();
        let signing_key2 = SigningKey::generate();
        let verifying_key2 = signing_key2.verifying_key();
        let message = b"test message";

        let signature = signing_key1.sign(message);

        // Verification with wrong key should fail
        assert!(verifying_key2.verify(message, &signature).is_err());
    }

    #[test]
    fn test_signing_key_to_bytes_roundtrip() {
        let signing_key = SigningKey::generate();
        let bytes = signing_key.to_bytes();
        let recovered = SigningKey::from_bytes(&bytes).unwrap();

        // Both keys should produce the same signature
        let message = b"test message";
        let sig1 = signing_key.sign(message);
        let sig2 = recovered.sign(message);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_verifying_key_to_bytes_roundtrip() {
        let signing_key = SigningKey::generate();
        let verifying_key = signing_key.verifying_key();
        let bytes = verifying_key.to_bytes();
        let recovered = VerifyingKey::from_bytes(&bytes).unwrap();

        // Both keys should verify the same signature
        let message = b"test message";
        let signature = signing_key.sign(message);
        assert!(verifying_key.verify(message, &signature).is_ok());
        assert!(recovered.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_signature_from_to_bytes() {
        let signing_key = SigningKey::generate();
        let message = b"test message";
        let signature = signing_key.sign(message);

        let bytes = signature.to_bytes();
        let recovered = Signature::from_bytes(bytes);

        assert_eq!(signature, recovered);
    }

    #[test]
    fn test_multiple_signatures_are_deterministic() {
        let signing_key = SigningKey::generate();
        let message = b"deterministic test";

        let sig1 = signing_key.sign(message);
        let sig2 = signing_key.sign(message);

        // Ed25519 signatures should be deterministic
        assert_eq!(sig1, sig2);
    }
}
