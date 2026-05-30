//! Verify registered versions have valid signatures and checksums

use std::fs;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use deltaship_crypto::{Signature, VerifyingKey};
use deltaship_db::{DbVersion, PublisherDb};

use crate::config::{ConfigKey, DB_FILE};
use crate::utils::signing_payload;

/// Result of verifying a single version
#[derive(Debug, Clone, Serialize)]
struct VersionVerifyResult {
    version_id: String,
    version_string: String,
    binary_name: String,
    platform: String,
    file_exists: bool,
    file_size: Option<String>,
    blake3_matches: Option<bool>,
    sha256_matches: Option<bool>,
    is_signed: bool,
    signature_valid: Option<bool>,
    signature_timestamp: Option<String>,
    timestamp_reasonable: Option<bool>,
    passed: bool,
    warnings: Vec<String>,
    errors: Vec<String>,
}

/// Summary of all verification results
#[derive(Debug, Clone, Serialize)]
struct VerifySummary {
    total: usize,
    passed: usize,
    failed: usize,
    warnings: usize,
}

/// JSON output structure
#[derive(Debug, Clone, Serialize)]
struct JsonOutput {
    versions: Vec<VersionVerifyResult>,
    summary: VerifySummary,
}

/// Run the verify command
pub async fn run(
    name: String,
    version: Option<String>,
    fix: bool,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    // Require confirmation for --fix flag (unless in JSON mode)
    if fix && !json {
        println!("\n⚠️  WARNING: --fix will UPDATE stored hashes in the database!");
        println!("  This will modify the database to match the current file contents.");
        println!("  Only use this if you trust the current files are correct.");
        println!("\nType 'YES' to confirm:");

        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;

        if line.trim() != "YES" {
            return Err("Fix operation cancelled. No changes were made.".into());
        }
        println!();
    }

    // Find binaries by name
    let binaries = db.list_binaries().await?;
    let matching_binaries: Vec<_> = binaries.iter().filter(|b| b.binary_name == name).collect();

    if matching_binaries.is_empty() {
        return Err(format!("No binary found with name '{}'", name).into());
    }

    // Load public key for signature verification
    let public_key = load_public_key(&db).await?;

    // Collect versions to verify
    let mut versions_to_verify: Vec<(DbVersion, String, String)> = Vec::new();

    for binary in &matching_binaries {
        let versions = db.list_versions(&binary.binary_id).await?;

        if let Some(ref ver_filter) = version {
            // Filter to specific version
            for v in versions {
                if v.version_string == *ver_filter {
                    versions_to_verify.push((
                        v,
                        binary.binary_name.clone(),
                        binary.platform.clone(),
                    ));
                }
            }
        } else {
            // All versions for this binary
            for v in versions {
                versions_to_verify.push((v, binary.binary_name.clone(), binary.platform.clone()));
            }
        }
    }

    if versions_to_verify.is_empty() {
        if let Some(ver) = version {
            return Err(format!("Version {} not found for binary '{}'", ver, name).into());
        }
        return Err(format!("No versions found for binary '{}'", name).into());
    }

    // Verify each version
    let mut results: Vec<VersionVerifyResult> = Vec::new();

    for (db_version, binary_name, platform) in &versions_to_verify {
        let result = verify_version(
            &db,
            db_version,
            binary_name,
            platform,
            public_key.as_ref(),
            fix,
        )
        .await?;
        results.push(result);
    }

    // Calculate summary
    let total = results.len();
    let passed = results
        .iter()
        .filter(|r| r.passed && r.warnings.is_empty())
        .count();
    let failed = results.iter().filter(|r| !r.passed).count();
    let warnings = results
        .iter()
        .filter(|r| r.passed && !r.warnings.is_empty())
        .count();

    let summary = VerifySummary {
        total,
        passed,
        failed,
        warnings,
    };

    // Output results
    if json {
        let output = JsonOutput {
            versions: results,
            summary,
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_results(&results, &summary);
    }

    // Return error if any verifications failed
    if failed > 0 {
        return Err(format!("{} verification(s) failed", failed).into());
    }

    Ok(())
}

/// Verify a single version
async fn verify_version(
    db: &PublisherDb,
    version: &DbVersion,
    binary_name: &str,
    platform: &str,
    public_key: Option<&VerifyingKey>,
    fix: bool,
) -> Result<VersionVerifyResult, Box<dyn std::error::Error>> {
    let mut result = VersionVerifyResult {
        version_id: version.version_id.clone(),
        version_string: version.version_string.clone(),
        binary_name: binary_name.to_string(),
        platform: platform.to_string(),
        file_exists: false,
        file_size: None,
        blake3_matches: None,
        sha256_matches: None,
        is_signed: version.signature_ed25519.is_some(),
        signature_valid: None,
        signature_timestamp: version.signature_timestamp.clone(),
        timestamp_reasonable: None,
        passed: true,
        warnings: Vec::new(),
        errors: Vec::new(),
    };

    let file_path = Path::new(&version.file_path);

    // Check 1: Binary file exists
    if !file_path.exists() {
        result.file_exists = false;
        result.passed = false;
        result
            .errors
            .push(format!("Binary file not found: {}", version.file_path));
        return Ok(result);
    }
    result.file_exists = true;

    // Get file size
    let metadata = fs::metadata(file_path)?;
    let file_size = metadata.len();
    result.file_size = Some(format_size(file_size as i64));

    // Check 2: Blake3 hash matches
    // SECURITY FIX: Read file once to avoid TOCTOU and compute both hashes
    let file_data = fs::read(file_path)?;

    // Compute Blake3 from in-memory data
    let computed_blake3 = blake3::hash(&file_data);
    let stored_blake3: [u8; 32] = version
        .file_hash_blake3
        .clone()
        .try_into()
        .map_err(|_| "Invalid stored Blake3 hash length")?;

    if computed_blake3.as_bytes() == &stored_blake3 {
        result.blake3_matches = Some(true);
    } else {
        result.blake3_matches = Some(false);
        if fix {
            // Compute SHA256 from same data (will be done below)
            // Update hash in database
            let computed_sha256 = {
                let mut hasher = Sha256::new();
                hasher.update(&file_data);
                let result = hasher.finalize();
                let hash_array: [u8; 32] = result.into();
                hash_array
            };
            db.update_version_hashes(
                &version.version_id,
                computed_blake3.as_bytes(),
                &computed_sha256,
                file_size as i64,
            )
            .await?;
            result
                .warnings
                .push("Blake3 hash mismatch - FIXED".to_string());
        } else {
            result.passed = false;
            result.errors.push("Blake3 hash mismatch".to_string());
        }
    }

    // Check 3: SHA256 hash matches (compute from same in-memory data)
    let computed_sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&file_data);
        let result = hasher.finalize();
        let hash_array: [u8; 32] = result.into();
        hash_array
    };
    let stored_sha256: [u8; 32] = version
        .file_hash_sha256
        .clone()
        .try_into()
        .map_err(|_| "Invalid stored SHA256 hash length")?;

    if computed_sha256 == stored_sha256 {
        result.sha256_matches = Some(true);
    } else {
        result.sha256_matches = Some(false);
        if fix {
            // Already fixed above with blake3
            if result.blake3_matches == Some(true) {
                // Only SHA256 was wrong, fix it
                db.update_version_hashes(
                    &version.version_id,
                    &stored_blake3,
                    &computed_sha256,
                    file_size as i64,
                )
                .await?;
            }
            result
                .warnings
                .push("SHA256 hash mismatch - FIXED".to_string());
        } else {
            result.passed = false;
            result.errors.push("SHA256 hash mismatch".to_string());
        }
    }

    // Check 4: Signature validity (if signed)
    // SECURITY FIX: Use computed hashes from actual file, not stored hashes
    if let Some(ref sig_bytes) = version.signature_ed25519 {
        if let Some(pk) = public_key {
            // Verify the canonical "DELTASHIP-sig-v1" payload, identical to sign.rs and
            // the client: domain tag || raw BLAKE3 hash || version string.
            // CRITICAL: Use the COMPUTED hash from the actual file content (not the
            // stored hash) so a tampered binary cannot pass with a stale signature.
            let message =
                signing_payload(computed_blake3.as_bytes(), &version.version_string);

            let sig_array: [u8; 64] = sig_bytes
                .clone()
                .try_into()
                .map_err(|_| "Invalid signature length")?;
            let signature = Signature::from_bytes(sig_array);

            match pk.verify(&message, &signature) {
                Ok(()) => {
                    result.signature_valid = Some(true);
                }
                Err(_) => {
                    result.signature_valid = Some(false);
                    result.passed = false;
                    result
                        .errors
                        .push("Signature verification failed".to_string());
                }
            }
        } else {
            result
                .warnings
                .push("Cannot verify signature: no public key configured".to_string());
        }
    } else {
        result.warnings.push("Not signed".to_string());
    }

    // Check 5: Signature timestamp is reasonable
    if let Some(ref timestamp_str) = version.signature_timestamp {
        match DateTime::parse_from_rfc3339(timestamp_str) {
            Ok(timestamp) => {
                let now = Utc::now();
                let timestamp_utc = timestamp.with_timezone(&Utc);

                // Check not in future
                if timestamp_utc > now + Duration::hours(1) {
                    result.timestamp_reasonable = Some(false);
                    result.passed = false;
                    result
                        .errors
                        .push("Signature timestamp is in the future".to_string());
                }
                // Check not too old (more than 10 years)
                else if timestamp_utc < now - Duration::days(3650) {
                    result.timestamp_reasonable = Some(false);
                    result
                        .warnings
                        .push("Signature timestamp is very old (>10 years)".to_string());
                } else {
                    result.timestamp_reasonable = Some(true);
                }
            }
            Err(_) => {
                result.timestamp_reasonable = Some(false);
                result
                    .warnings
                    .push("Invalid signature timestamp format".to_string());
            }
        }
    }

    Ok(result)
}

/// Load public key from database config
async fn load_public_key(
    db: &PublisherDb,
) -> Result<Option<VerifyingKey>, Box<dyn std::error::Error>> {
    if let Some(pk_hex) = db.get_config(ConfigKey::PublicKey.as_db_key()).await? {
        if !pk_hex.is_empty() {
            let pk_bytes = hex_decode(&pk_hex)?;
            let pk_array: [u8; 32] = pk_bytes
                .try_into()
                .map_err(|_| "Invalid public key length")?;
            let pk = VerifyingKey::from_bytes(&pk_array)?;
            return Ok(Some(pk));
        }
    }
    Ok(None)
}

/// Print results in human-readable format
fn print_results(results: &[VersionVerifyResult], summary: &VerifySummary) {
    for result in results {
        println!(
            "Verifying {} v{}...",
            result.binary_name, result.version_string
        );

        // File exists
        if result.file_exists {
            let size = result.file_size.as_deref().unwrap_or("?");
            println!("  \u{2713} Binary file exists ({})", size);
        } else {
            println!("  \u{2717} Binary file missing");
        }

        // Blake3 checksum
        if let Some(matches) = result.blake3_matches {
            if matches {
                println!("  \u{2713} Blake3 checksum matches");
            } else {
                println!("  \u{2717} Blake3 checksum mismatch");
            }
        }

        // SHA256 checksum
        if let Some(matches) = result.sha256_matches {
            if matches {
                println!("  \u{2713} SHA256 checksum matches");
            } else {
                println!("  \u{2717} SHA256 checksum mismatch");
            }
        }

        // Signature
        if result.is_signed {
            if let Some(valid) = result.signature_valid {
                if valid {
                    println!("  \u{2713} Signature valid");
                } else {
                    println!("  \u{2717} Signature invalid");
                }
            } else {
                println!("  ? Signature not verified");
            }

            // Timestamp
            if let Some(ref ts) = result.signature_timestamp {
                if result.timestamp_reasonable == Some(true) {
                    println!("  \u{2713} Signed at {}", ts);
                } else {
                    println!("  \u{2717} Signed at {} (unreasonable)", ts);
                }
            }
        } else {
            println!("  \u{2717} Not signed");
        }

        // Warnings
        for warning in &result.warnings {
            if !warning.contains("Not signed") {
                println!("  ! {}", warning);
            }
        }

        println!();
    }

    // Summary
    let mut summary_parts: Vec<String> = Vec::new();
    if summary.passed > 0 {
        summary_parts.push(format!("{} passed", summary.passed));
    }
    if summary.warnings > 0 {
        summary_parts.push(format!("{} warning(s)", summary.warnings));
    }
    if summary.failed > 0 {
        summary_parts.push(format!("{} failed", summary.failed));
    }

    // Count unsigned versions as warnings
    let unsigned_count = results.iter().filter(|r| !r.is_signed && r.passed).count();
    if unsigned_count > 0 && !summary_parts.iter().any(|s| s.contains("warning")) {
        summary_parts.push(format!("{} warning(s) (unsigned)", unsigned_count));
    }

    println!("Summary: {}", summary_parts.join(", "));
}

/// Format file size in human-readable format
fn format_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.1} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Hex decoding
fn hex_decode(s: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if s.len() % 2 != 0 {
        return Err("Invalid hex string length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.into()))
        .collect()
}
