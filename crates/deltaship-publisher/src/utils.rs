//! Shared utilities for the publisher crate.

use serde::{Deserialize, Serialize};

/// Canonical Ed25519 signing payload ("DELTASHIP-sig-v1").
///
/// Thin re-export of [`deltaship_crypto::signing_payload`], the single source of
/// truth shared by the publisher, server, and client. Kept here so existing
/// call sites continue to use `crate::utils::signing_payload`.
pub use deltaship_crypto::signing_payload;

/// Version manifest structure for signature verification.
/// This structure is used for both signing (in sign.rs) and verification (in verify.rs).
/// The field order and types must remain stable to ensure consistent JSON serialization
/// across signing and verification operations.
///
/// IMPORTANT: Do not reorder fields or change types without understanding the impact on
/// signature verification. The JSON serialization must be identical between signing and
/// verification for Ed25519 signatures to validate correctly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionManifest {
    pub version_id: String,
    pub binary_id: String,
    pub version_string: String,
    pub file_hash_blake3: String,
    pub file_hash_sha256: String,
    pub file_size_bytes: i64,
    pub timestamp: String,
    pub signature_algorithm: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_payload_layout() {
        let hash = [0xABu8; 32];
        let payload = signing_payload(&hash, "1.2.3");

        // 17-byte domain tag + 32-byte hash + 5-byte version
        assert_eq!(payload.len(), 17 + 32 + 5);
        assert_eq!(&payload[..17], b"DELTASHIP-sig-v1\x00");
        assert_eq!(&payload[17..49], &hash);
        assert_eq!(&payload[49..], b"1.2.3");
    }

    #[test]
    fn test_signing_payload_binds_version() {
        let hash = [0x01u8; 32];
        assert_ne!(
            signing_payload(&hash, "1.0.0"),
            signing_payload(&hash, "2.0.0"),
            "payload must differ when only the version differs"
        );
    }

    #[test]
    fn test_version_manifest_roundtrip() {
        // Create a manifest
        let manifest = VersionManifest {
            version_id: "test-version-id".to_string(),
            binary_id: "test-binary-id".to_string(),
            version_string: "1.0.0".to_string(),
            file_hash_blake3: "abc123".to_string(),
            file_hash_sha256: "def456".to_string(),
            file_size_bytes: 1024,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            signature_algorithm: "ed25519".to_string(),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&manifest).expect("Failed to serialize");

        // Deserialize back
        let deserialized: VersionManifest =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Verify they match
        assert_eq!(manifest, deserialized);
    }

    #[test]
    fn test_version_manifest_field_order_stability() {
        // This test ensures the field order is stable for signature verification
        let manifest = VersionManifest {
            version_id: "v1".to_string(),
            binary_id: "b1".to_string(),
            version_string: "1.0.0".to_string(),
            file_hash_blake3: "blake3hash".to_string(),
            file_hash_sha256: "sha256hash".to_string(),
            file_size_bytes: 2048,
            timestamp: "2024-01-01T12:00:00Z".to_string(),
            signature_algorithm: "ed25519".to_string(),
        };

        let json = serde_json::to_string(&manifest).expect("Failed to serialize");

        // Verify the field order in the JSON matches the expected order
        // This is critical for signature verification
        let expected = r#"{"version_id":"v1","binary_id":"b1","version_string":"1.0.0","file_hash_blake3":"blake3hash","file_hash_sha256":"sha256hash","file_size_bytes":2048,"timestamp":"2024-01-01T12:00:00Z","signature_algorithm":"ed25519"}"#;
        assert_eq!(json, expected, "Field order must remain stable for signatures");
    }

    #[test]
    fn test_version_manifest_deserialize_with_extra_fields() {
        // Test that we can deserialize JSON with extra fields (forward compatibility)
        let json = r#"{
            "version_id": "v1",
            "binary_id": "b1",
            "version_string": "1.0.0",
            "file_hash_blake3": "blake3",
            "file_hash_sha256": "sha256",
            "file_size_bytes": 1024,
            "timestamp": "2024-01-01T00:00:00Z",
            "signature_algorithm": "ed25519",
            "extra_field": "ignored"
        }"#;

        let manifest: VersionManifest =
            serde_json::from_str(json).expect("Failed to deserialize with extra fields");

        assert_eq!(manifest.version_id, "v1");
        assert_eq!(manifest.binary_id, "b1");
    }
}
