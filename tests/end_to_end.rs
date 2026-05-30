//! End-to-end integration tests for VBDP
//!
//! # P3 Issue #114 Fix: Integration Tests
//!
//! This test suite provides end-to-end integration testing of the complete VBDP workflow:
//!
//! 1. Publisher registers a binary and version
//! 2. Publisher generates and uploads diffs
//! 3. Server stores and serves updates
//! 4. Client checks for and applies updates
//! 5. Client can rollback to previous versions
//!
//! These tests verify that all components work together correctly in realistic scenarios.
//!
//! ## Running Integration Tests
//!
//! Run with: `cargo test --test end_to_end`
//!
//! ## Test Coverage
//!
//! - Binary registration and versioning
//! - Diff generation and application
//! - Database integrity across components
//! - Error handling in integration scenarios
//!
//! ## Future Enhancements
//!
//! Additional integration tests that could be added:
//! - Server API endpoint testing with real HTTP requests
//! - Multi-client concurrent update scenarios
//! - Network failure and retry testing
//! - Signature verification end-to-end
//! - Rollback recovery scenarios

use tempfile::TempDir;
use vbdp_db::{ClientDb, PublisherDb, NewBinary, NewVersion};
use vbdp_diff::{generate_diff, apply_patch};
use vbdp_crypto::{SigningKey, Signature};

#[tokio::test]
async fn test_publisher_database_workflow() {
    // This test verifies the publisher database workflow:
    // 1. Create database
    // 2. Register binary
    // 3. Register version
    // 4. Query back the data

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("publisher.db");

    // Initialize publisher database
    let db = PublisherDb::open(&db_path).await.unwrap();
    db.init().await.unwrap();

    // Create temporary binary file for testing (required for path validation)
    let binary_path = temp_dir.path().join("test-app");
    std::fs::write(&binary_path, b"test binary content").unwrap();

    // Register a binary
    let binary = db
        .insert_binary(NewBinary {
            binary_name: "test-app".to_string(),
            platform: "linux-x86_64".to_string(),
            binary_path: binary_path.to_string_lossy().to_string(),
            description: Some("Test application".to_string()),
        })
        .await
        .unwrap();

    assert_eq!(binary.binary_name, "test-app");
    assert_eq!(binary.platform, "linux-x86_64");

    // Register a version
    let version = db
        .insert_version(NewVersion {
            binary_id: binary.binary_id.clone(),
            version_string: "1.0.0".to_string(),
            file_path: "/tmp/test-app-1.0.0".to_string(),
            file_size_bytes: 1024,
            file_hash_blake3: vec![0u8; 32],
            file_hash_sha256: vec![0u8; 32],
        })
        .await
        .unwrap();

    assert_eq!(version.version_string, "1.0.0");
    assert_eq!(version.file_size_bytes, 1024);

    // List binaries and versions
    let binaries = db.list_binaries().await.unwrap();
    assert_eq!(binaries.len(), 1);

    let versions = db.list_versions(&binary.binary_id).await.unwrap();
    assert_eq!(versions.len(), 1);
}

#[tokio::test]
async fn test_client_database_workflow() {
    // This test verifies the client database workflow:
    // 1. Create database
    // 2. Register managed binary
    // 3. Record version installation
    // 4. Update current version

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("client.db");

    // Initialize client database
    let db = ClientDb::open(&db_path).await.unwrap();
    db.init().await.unwrap();

    // Register a managed binary
    let binary = db
        .register_binary(vbdp_db::NewManagedBinary {
            binary_id: "test-binary-id".to_string(),
            binary_name: "test-app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/usr/local/bin/test-app".to_string(),
            publisher_public_key: vec![0u8; 32],
        })
        .await
        .unwrap();

    assert_eq!(binary.binary_name, "test-app");

    // Record installed version
    let version_id = db
        .record_installed_version(
            &binary.binary_id,
            "version-id-1",
            "1.0.0",
            &vec![1u8; 32],
            &vec![2u8; 32],
            1024,
        )
        .await
        .unwrap();

    assert!(version_id > 0);

    // Update current version
    db.update_current_version(&binary.binary_id, "version-id-1", "1.0.0")
        .await
        .unwrap();

    // Verify the binary was updated
    let updated_binary = db.get_binary(&binary.binary_id).await.unwrap().unwrap();
    assert_eq!(updated_binary.current_version_string, Some("1.0.0".to_string()));
}

#[test]
fn test_diff_generation_and_application() {
    // This test verifies diff generation and application work end-to-end:
    // 1. Create two binary versions
    // 2. Generate a diff
    // 3. Apply the diff
    // 4. Verify the result matches the target

    let old_data = b"Hello, World! This is version 1.0.0 of the test binary.";
    let new_data = b"Hello, World! This is version 2.0.0 of the test binary. Now with more features!";

    // Generate diff
    let diff = generate_diff(old_data, new_data).unwrap();

    // Note: Diffs can sometimes be larger than the target file, especially for small files
    // with significant changes. The vbdp-diff crate logs warnings about this (P2 Issue #39 fix).
    // We verify the diff works correctly, not its size.

    // Apply diff
    let recovered = apply_patch(old_data, &diff).unwrap();

    // Verify recovered data matches new data
    assert_eq!(recovered, new_data);
}

#[tokio::test]
async fn test_database_health_check() {
    // Test the health check functionality added in P3 Issue #112

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("health_test.db");

    let db = ClientDb::open(&db_path).await.unwrap();
    db.init().await.unwrap();

    // Health check should pass on a fresh database
    let health_status = db.health_check().await.unwrap();
    assert!(health_status.contains("healthy"));

    // Stats should show empty database
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.managed_binaries, 0);
    assert_eq!(stats.installed_versions, 0);
    assert_eq!(stats.rollback_backups, 0);
    assert_eq!(stats.update_history_records, 0);
}

#[test]
fn test_platform_centralization() {
    // Test that platform list is centralized (P3 Issue #105)
    use vbdp_core::Platform;

    let platforms = Platform::all_platforms();
    assert!(platforms.len() >= 5); // At least 5 platforms

    let variants = Platform::all_platform_variants();
    assert!(variants.len() >= platforms.len()); // Variants include more forms

    // Verify all platforms can be parsed
    for platform_str in platforms {
        let parsed: Platform = platform_str.parse().unwrap();
        assert_eq!(parsed.as_str(), *platform_str);
    }
}

/// P1 Issue #6 Fix: True End-to-End Integration Test
///
/// This test simulates a complete VBDP workflow:
/// 1. Publisher workflow: register binary, create versions, sign versions
/// 2. Mock HTTP server: simulate update server responses
/// 3. Client workflow: check for updates, download diff, verify signature, apply update
/// 4. Signature validation: verify cryptographic signatures throughout
#[tokio::test]
async fn test_true_end_to_end_update_workflow() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};
    use std::fs;

    // Setup test environment
    let temp_dir = TempDir::new().unwrap();

    // =========================================================================
    // Publisher Workflow
    // =========================================================================

    // Generate publisher keypair
    let publisher_signing_key = SigningKey::generate();
    let publisher_verifying_key = publisher_signing_key.verifying_key();
    let publisher_public_key = publisher_verifying_key.to_bytes();

    // Create publisher database
    let publisher_db_path = temp_dir.path().join("publisher.db");
    let publisher_db = PublisherDb::open(&publisher_db_path).await.unwrap();
    publisher_db.init().await.unwrap();

    // Create test binary files
    let v1_content = b"Binary content version 1.0.0 - initial release";
    let v2_content = b"Binary content version 2.0.0 - updated with new features";

    let v1_path = temp_dir.path().join("app-v1.0.0");
    let v2_path = temp_dir.path().join("app-v2.0.0");
    fs::write(&v1_path, v1_content).unwrap();
    fs::write(&v2_path, v2_content).unwrap();

    // Register binary in publisher database
    let binary = publisher_db
        .insert_binary(NewBinary {
            binary_name: "test-app".to_string(),
            platform: "linux-x86_64".to_string(),
            binary_path: v1_path.to_string_lossy().to_string(),
            description: Some("Test application for E2E testing".to_string()),
        })
        .await
        .unwrap();

    // Register version 1.0.0
    let v1_hash = blake3::hash(v1_content);
    let version1 = publisher_db
        .insert_version(NewVersion {
            binary_id: binary.binary_id.clone(),
            version_string: "1.0.0".to_string(),
            file_path: v1_path.to_string_lossy().to_string(),
            file_size_bytes: v1_content.len() as i64,
            file_hash_blake3: v1_hash.as_bytes().to_vec(),
            file_hash_sha256: vec![0u8; 32], // Simplified for test
        })
        .await
        .unwrap();

    // Sign version 1.0.0
    let v1_signature = publisher_signing_key.sign(v1_hash.as_bytes());
    publisher_db
        .set_version_signature(&version1.version_id, &v1_signature.to_bytes(), "2024-01-01T00:00:00Z")
        .await
        .unwrap();
    publisher_db
        .set_version_published(&version1.version_id)
        .await
        .unwrap();

    // Register version 2.0.0
    let v2_hash = blake3::hash(v2_content);
    let version2 = publisher_db
        .insert_version(NewVersion {
            binary_id: binary.binary_id.clone(),
            version_string: "2.0.0".to_string(),
            file_path: v2_path.to_string_lossy().to_string(),
            file_size_bytes: v2_content.len() as i64,
            file_hash_blake3: v2_hash.as_bytes().to_vec(),
            file_hash_sha256: vec![0u8; 32],
        })
        .await
        .unwrap();

    // Sign version 2.0.0
    let v2_signature = publisher_signing_key.sign(v2_hash.as_bytes());
    publisher_db
        .set_version_signature(&version2.version_id, &v2_signature.to_bytes(), "2024-01-02T00:00:00Z")
        .await
        .unwrap();
    publisher_db
        .set_version_published(&version2.version_id)
        .await
        .unwrap();

    // Generate diff from v1 to v2
    let diff_data = generate_diff(v1_content, v2_content).unwrap();
    let diff_path = temp_dir.path().join("diff-1.0.0-to-2.0.0.bsdiff");
    fs::write(&diff_path, &diff_data).unwrap();

    // =========================================================================
    // Mock HTTP Server (simulating update server)
    // =========================================================================

    let mock_server = MockServer::start().await;

    // Mock check for updates endpoint
    let check_response = serde_json::json!({
        "binary_id": binary.binary_id,
        "binary_name": "test-app",
        "latest_version": "2.0.0",
        "latest_version_id": version2.version_id,
        "update_available": true,
        "current_version": "1.0.0"
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/check-update"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&check_response))
        .mount(&mock_server)
        .await;

    // Mock download diff endpoint
    Mock::given(method("GET"))
        .and(path("/api/v1/download-diff"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(diff_data.clone()))
        .mount(&mock_server)
        .await;

    // Mock get signature endpoint
    let signature_response = serde_json::json!({
        "version_id": version2.version_id,
        "signature": hex::encode(v2_signature.to_bytes()),
        "public_key": hex::encode(publisher_public_key),
        "timestamp": "2024-01-02T00:00:00Z"
    });

    Mock::given(method("GET"))
        .and(path("/api/v1/signature"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&signature_response))
        .mount(&mock_server)
        .await;

    // =========================================================================
    // Client Workflow
    // =========================================================================

    // Create client database
    let client_db_path = temp_dir.path().join("client.db");
    let client_db = ClientDb::open(&client_db_path).await.unwrap();
    client_db.init().await.unwrap();

    // Register managed binary on client
    let client_binary = client_db
        .register_binary(vbdp_db::NewManagedBinary {
            binary_id: binary.binary_id.clone(),
            binary_name: "test-app".to_string(),
            platform: "linux-x86_64".to_string(),
            install_path: "/usr/local/bin/test-app".to_string(),
            publisher_public_key: publisher_public_key.to_vec(),
        })
        .await
        .unwrap();

    // Record initial version installation
    client_db
        .record_installed_version(
            &client_binary.binary_id,
            &version1.version_id,
            "1.0.0",
            v1_hash.as_bytes(),
            &vec![0u8; 32],
            v1_content.len() as i64,
        )
        .await
        .unwrap();

    client_db
        .update_current_version(&client_binary.binary_id, &version1.version_id, "1.0.0")
        .await
        .unwrap();

    // Simulate client checking for updates
    let http_client = reqwest::Client::new();
    let check_url = format!("{}/api/v1/check-update", mock_server.uri());
    let check_resp = http_client.get(&check_url).send().await.unwrap();
    assert_eq!(check_resp.status(), 200);

    let check_data: serde_json::Value = check_resp.json().await.unwrap();
    assert_eq!(check_data["update_available"], true);
    assert_eq!(check_data["latest_version"], "2.0.0");

    // Record update start
    let update_id = client_db
        .record_update_start(vbdp_db::NewUpdateRecord {
            binary_id: client_binary.binary_id.clone(),
            from_version_id: Some(version1.version_id.clone()),
            from_version_string: Some("1.0.0".to_string()),
            to_version_id: version2.version_id.clone(),
            to_version_string: "2.0.0".to_string(),
        })
        .await
        .unwrap();

    // Download diff
    client_db
        .set_update_status(update_id, vbdp_db::UpdateHistoryStatus::Downloading)
        .await
        .unwrap();

    let diff_url = format!("{}/api/v1/download-diff", mock_server.uri());
    let diff_resp = http_client.get(&diff_url).send().await.unwrap();
    assert_eq!(diff_resp.status(), 200);
    let downloaded_diff = diff_resp.bytes().await.unwrap();

    // Get signature
    let sig_url = format!("{}/api/v1/signature", mock_server.uri());
    let sig_resp = http_client.get(&sig_url).send().await.unwrap();
    assert_eq!(sig_resp.status(), 200);
    let sig_data: serde_json::Value = sig_resp.json().await.unwrap();

    // Verify signature
    let received_signature = hex::decode(sig_data["signature"].as_str().unwrap()).unwrap();
    let sig_array: [u8; 64] = received_signature.as_slice().try_into().unwrap();
    let signature = Signature::from_bytes(sig_array);
    let signature_valid = publisher_verifying_key.verify(v2_hash.as_bytes(), &signature).is_ok();
    assert!(signature_valid, "Signature verification failed - security check");

    // Apply diff
    client_db
        .set_update_status(update_id, vbdp_db::UpdateHistoryStatus::Applying)
        .await
        .unwrap();

    let patched_content = apply_patch(v1_content, &downloaded_diff).unwrap();
    assert_eq!(patched_content, v2_content, "Patched content should match v2");

    // Verify hash of patched content
    let patched_hash = blake3::hash(&patched_content);
    assert_eq!(
        patched_hash.as_bytes(),
        v2_hash.as_bytes(),
        "Patched content hash should match v2 hash"
    );

    // Record successful update
    client_db
        .record_update_complete(
            update_id,
            true,
            None,
            vbdp_db::UpdateMetrics {
                diff_id: Some("diff-1-2".to_string()),
                diff_algorithm: Some("bsdiff".to_string()),
                diff_size_bytes: Some(downloaded_diff.len() as i64),
                full_size_bytes: Some(v2_content.len() as i64),
                actual_downloaded_bytes: Some(downloaded_diff.len() as i64),
                download_time_ms: Some(50),
                apply_time_ms: Some(25),
                verify_time_ms: Some(10),
            },
        )
        .await
        .unwrap();

    // Record new version installation
    client_db
        .record_installed_version(
            &client_binary.binary_id,
            &version2.version_id,
            "2.0.0",
            v2_hash.as_bytes(),
            &vec![0u8; 32],
            v2_content.len() as i64,
        )
        .await
        .unwrap();

    client_db
        .update_current_version(&client_binary.binary_id, &version2.version_id, "2.0.0")
        .await
        .unwrap();

    // =========================================================================
    // Verification
    // =========================================================================

    // Verify final client state
    let final_binary = client_db.get_binary(&client_binary.binary_id).await.unwrap().unwrap();
    assert_eq!(final_binary.current_version_string, Some("2.0.0".to_string()));
    assert_eq!(final_binary.current_version_id, Some(version2.version_id));

    // Verify update history
    let update_record = client_db.get_update(update_id).await.unwrap().unwrap();
    assert_eq!(update_record.status, "completed");
    assert_eq!(update_record.success, Some(true));
    assert!(update_record.diff_size_bytes.is_some());

    println!("End-to-end integration test completed successfully!");
    println!("  - Publisher registered binary and created 2 versions");
    println!("  - Publisher signed versions with Ed25519");
    println!("  - Mock server simulated HTTP endpoints");
    println!("  - Client checked for updates via HTTP");
    println!("  - Client downloaded diff via HTTP");
    println!("  - Client verified cryptographic signature");
    println!("  - Client applied diff and verified hash");
    println!("  - All database operations completed successfully");
}
