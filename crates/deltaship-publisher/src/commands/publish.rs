//! Publish a signed version to the update server.

use std::fs;
use std::path::Path;

use anyhow::Context;
use blake3::Hasher as Blake3Hasher;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::{debug, error, info};
use deltaship_db::PublisherDb;

use crate::api_client::{ApiClient, PublishDiffRequest, PublishRequest};
use crate::config::{CONFIG_SERVER_URL, DB_FILE, DEFAULT_SERVER_URL};
use crate::diff_manager::DiffManager;

/// Run the publish command.
pub async fn run(
    name: String,
    version: String,
    server_url: Option<String>,
    api_key: Option<String>,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        binary = %name,
        version = %version,
        "Publishing version"
    );
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err(format!(
            "Deltaship project not initialized.\n\nNext steps:\n  - Run 'deltaship-publisher init' in the project root directory\n  - This will create the database at: {}\n  - Then retry this publish command",
            DB_FILE
        ).into());
    }

    // Open database
    let db = PublisherDb::open(db_path)
        .await
        .with_context(|| format!("Failed to open database at {:?}", db_path))?;

    // Find the binary
    let binaries = db.list_binaries().await?;
    let binary = binaries
        .iter()
        .find(|b| b.binary_name == name)
        .ok_or_else(|| format!(
            "Binary '{}' not found in database.\n\nNext steps:\n  - List all binaries with: deltaship-publisher list\n  - Register this binary with: deltaship-publisher register --name {} --version <VERSION> --file <PATH> --platform <PLATFORM>\n  - Check for typos in the binary name",
            name, name
        ))?;

    // Find the version
    let db_version = db
        .get_version_by_string(&binary.binary_id, &version)
        .await?
        .ok_or_else(|| format!(
            "Version {} not found for binary '{}'.\n\nNext steps:\n  - List versions for this binary: deltaship-publisher list --name {}\n  - Register the version: deltaship-publisher register --name {} --version {} --file <PATH> --platform {}\n  - Check for typos in the version number",
            version, name, name, name, version, binary.platform
        ))?;

    // Check if version is signed
    let signature = db_version.signature_ed25519.as_ref().ok_or_else(|| {
        format!(
            "Version {} is not signed yet.\n\nNext steps:\n  - Sign the version with: deltaship-publisher sign --name {} --version {}\n  - You will need your signing key (default: .deltaship/keys/signing_key.pem)\n  - After signing, retry this publish command",
            version, name, version
        )
    })?;

    // Check if already published
    if db_version.is_published {
        println!(
            "Warning: Version {} is already published. Re-publishing...",
            version
        );
    }

    // Get the server URL (priority: CLI flag > config > default)
    let base_url = match server_url {
        Some(url) => url,
        None => db
            .get_config(CONFIG_SERVER_URL)
            .await?
            .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string()),
    };

    // Validate and confirm non-default server URL
    if base_url != DEFAULT_SERVER_URL {
        // Validate URL format
        if !base_url.starts_with("http://") && !base_url.starts_with("https://") {
            return Err(format!(
                "Invalid server URL: '{}'. Must start with http:// or https://",
                base_url
            )
            .into());
        }

        // Warn about http (unencrypted)
        if base_url.starts_with("http://") {
            println!("\n⚠️  WARNING: Using unencrypted HTTP connection!");
            println!("  Server URL: {}", base_url);
            println!("  API keys and data will be transmitted without encryption.");
        }

        // Require confirmation for non-default server (skip with --yes)
        if !yes {
            println!("\n⚠️  Publishing to NON-DEFAULT server:");
            println!("  Server URL: {}", base_url);
            println!("  Default URL: {}", DEFAULT_SERVER_URL);
            println!("\nType 'YES' to confirm publishing to this server:");

            use std::io::{self, BufRead};
            let stdin = io::stdin();
            let mut line = String::new();
            stdin.lock().read_line(&mut line)?;

            if line.trim() != "YES" {
                return Err("Publish cancelled. Server URL not confirmed.".into());
            }
            println!();
        }
    }

    // Read binary file
    let file_path = Path::new(&db_version.file_path);
    if !file_path.exists() {
        return Err(format!(
            "Binary file not found at: {}\n\nNext steps:\n  - The file may have been moved or deleted since registration\n  - Verify the file exists: ls -l {}\n  - If the file was moved, update the path or re-register the version\n  - Ensure you're running this command from the correct directory",
            db_version.file_path, db_version.file_path
        )
        .into());
    }

    println!("Reading binary file...");
    let binary_data = fs::read(file_path)
        .with_context(|| format!("Failed to read binary file from {:?}", file_path))?;

    // Verify file size matches (use safe comparison to avoid overflow)
    let actual_size = binary_data.len();
    let expected_size = if db_version.file_size_bytes >= 0 {
        db_version.file_size_bytes as u64
    } else {
        return Err(format!(
            "Invalid file size in database: {} (negative value)",
            db_version.file_size_bytes
        )
        .into());
    };

    if actual_size as u64 != expected_size {
        return Err(format!(
            "File size mismatch. Expected {} bytes, found {} bytes. The file may have been modified.",
            expected_size,
            actual_size
        )
        .into());
    }

    // Create API client and publish request
    let client = ApiClient::new(base_url.clone(), api_key);

    let request = PublishRequest {
        app_name: name.clone(),
        version: version.clone(),
        platform: binary.platform.clone(),
        binary_data,
        signature: signature.clone(),
        checksum: db_version.file_hash_blake3.clone(),
    };

    println!("Uploading to {}...", base_url);
    println!("  Binary: {} ({}) v{}", name, binary.platform, version);
    println!("  Size: {} bytes", db_version.file_size_bytes);

    // Publish the binary with progress indicator for large files
    debug!(
        server_url = %base_url,
        "Sending publish request to server"
    );

    let response = if db_version.file_size_bytes > 1024 * 1024 {
        // Show spinner for files > 1MB
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap()
        );
        pb.set_message(format!("Uploading binary ({} bytes)", db_version.file_size_bytes));
        pb.enable_steady_tick(std::time::Duration::from_millis(100));

        let result = client.publish_version(request).await;
        pb.finish_and_clear();
        result?
    } else {
        client.publish_version(request).await?
    };

    if !response.success {
        error!("Publish failed: {}", response.message);
        return Err(format!("Publish failed: {}", response.message).into());
    }

    println!("\nBinary uploaded successfully!");
    if let Some(vid) = response.version_id {
        println!("  Server Version ID: {}", vid);
    }

    // Find and upload diffs for this version
    let diff_manager = DiffManager::new();
    let diffs = diff_manager
        .get_diffs_for_version(&db, &db_version.version_id)
        .await?;

    if !diffs.is_empty() {
        println!("\nUploading {} diff(s)...", diffs.len());

        let mut diff_failures: Vec<String> = Vec::new();

        for diff_job in diffs {
            if let Some(ref diff_path) = diff_job.diff_path {
                let diff_path = Path::new(diff_path);
                if !diff_path.exists() {
                    let msg = format!("Diff file not found: {}", diff_path.display());
                    error!("{}", msg);
                    diff_failures.push(msg);
                    continue;
                }

                let from_version = match db.get_version(&diff_job.from_version_id).await? {
                    Some(v) => v,
                    None => {
                        let msg = format!(
                            "Source version not found for diff job {}",
                            diff_job.job_id
                        );
                        error!("{}", msg);
                        diff_failures.push(msg);
                        continue;
                    }
                };

                let diff_data = fs::read(diff_path)?;
                let checksum = diff_job.diff_hash_blake3.clone().unwrap_or_default();

                // Verify diff checksum before upload
                let computed_hash = Blake3Hasher::new().update(&diff_data).finalize();
                let computed_hash_bytes = computed_hash.as_bytes().to_vec();

                if computed_hash_bytes != checksum {
                    let msg = format!(
                        "Diff checksum mismatch for {} -> {}: computed {:?}, expected {:?}",
                        from_version.version_string, version,
                        computed_hash_bytes, checksum
                    );
                    error!("{}", msg);
                    diff_failures.push(msg);
                    continue;
                }

                let diff_request = PublishDiffRequest {
                    app_name: name.clone(),
                    from_version: from_version.version_string.clone(),
                    to_version: version.clone(),
                    platform: binary.platform.clone(),
                    diff_data,
                    checksum,
                };

                match client.publish_diff(diff_request).await {
                    Ok(resp) if resp.success => {
                        println!(
                            "  Uploaded diff {} -> {}",
                            from_version.version_string, version
                        );
                    }
                    Ok(resp) => {
                        let msg = format!(
                            "Failed to upload diff {} -> {}: {}",
                            from_version.version_string, version, resp.message
                        );
                        error!("{}", msg);
                        diff_failures.push(msg);
                    }
                    Err(e) => {
                        let msg = format!(
                            "Failed to upload diff {} -> {}: {}",
                            from_version.version_string, version, e
                        );
                        error!("{}", msg);
                        diff_failures.push(msg);
                    }
                }
            }
        }

        if !diff_failures.is_empty() {
            // Do NOT mark the version as published if diff uploads failed.
            // The publisher should fix the issues and re-publish to ensure all diffs are uploaded.
            let failure_summary = diff_failures.join("\n  - ");
            return Err(format!(
                "Binary uploaded successfully, but {} diff upload(s) failed:\n  - {}\n\nVersion not marked as published. Fix the issues and re-run publish.",
                diff_failures.len(),
                failure_summary
            )
            .into());
        }
    }

    // Update local database with published status
    db.set_version_published(&db_version.version_id).await?;

    println!("\nVersion published successfully!");
    println!("  Binary:  {} ({})", name, binary.platform);
    println!("  Version: {}", version);
    println!("  Server:  {}", base_url);

    Ok(())
}
