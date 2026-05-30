//! List registered binaries and versions

use std::path::Path;

use serde::Serialize;
use deltaship_db::PublisherDb;

use crate::config::DB_FILE;

/// JSON output structure for binaries
#[derive(Serialize)]
struct BinaryOutput {
    binary_id: String,
    name: String,
    platform: String,
    description: Option<String>,
    versions: Vec<VersionOutput>,
}

/// JSON output structure for versions
#[derive(Serialize)]
struct VersionOutput {
    version_id: String,
    version: String,
    file_path: String,
    size_bytes: i64,
    hash_blake3: String,
    signed: bool,
    signature_timestamp: Option<String>,
    created_at: String,
}

/// Run the list command
pub async fn run(name: Option<String>, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    // Get all binaries
    let binaries = db.list_binaries().await?;

    // Filter by name if specified
    let binaries: Vec<_> = if let Some(ref filter_name) = name {
        binaries
            .into_iter()
            .filter(|b| b.binary_name == *filter_name)
            .collect()
    } else {
        binaries
    };

    if binaries.is_empty() {
        if name.is_some() {
            return Err(format!("No binary found with name '{}'", name.unwrap()).into());
        }
        println!("No binaries registered. Use 'deltaship-publisher register' to register a binary.");
        return Ok(());
    }

    // Build output structure
    let mut output: Vec<BinaryOutput> = Vec::new();

    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;

        let version_outputs: Vec<VersionOutput> = versions
            .iter()
            .map(|v| VersionOutput {
                version_id: v.version_id.clone(),
                version: v.version_string.clone(),
                file_path: v.file_path.clone(),
                size_bytes: v.file_size_bytes,
                hash_blake3: hex_encode(&v.file_hash_blake3),
                signed: v.signature_ed25519.is_some(),
                signature_timestamp: v.signature_timestamp.clone(),
                created_at: v.created_at.clone(),
            })
            .collect();

        output.push(BinaryOutput {
            binary_id: binary.binary_id.clone(),
            name: binary.binary_name.clone(),
            platform: binary.platform.clone(),
            description: binary.description.clone(),
            versions: version_outputs,
        });
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_table(&output);
    }

    Ok(())
}

/// Print human-readable table output
fn print_table(binaries: &[BinaryOutput]) {
    for binary in binaries {
        println!("Binary: {} ({})", binary.name, binary.platform);
        println!("  ID: {}", binary.binary_id);
        if let Some(ref desc) = binary.description {
            println!("  Description: {}", desc);
        }
        println!();

        if binary.versions.is_empty() {
            println!("  No versions registered.");
        } else {
            println!("  Versions:");
            println!(
                "  {:<12} {:<10} {:<8} {:<20}",
                "VERSION", "SIGNED", "SIZE", "CREATED"
            );
            println!("  {:-<12} {:-<10} {:-<8} {:-<20}", "", "", "", "");

            for version in &binary.versions {
                let signed_status = if version.signed { "Yes" } else { "No" };
                let size = format_size(version.size_bytes);
                let created = &version.created_at[..19]; // Truncate to readable length

                println!(
                    "  {:<12} {:<10} {:<8} {:<20}",
                    version.version, signed_status, size, created
                );
            }
        }
        println!();
    }
}

/// Format file size in human-readable format
fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{}B", bytes)
    }
}

/// Simple hex encoding
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
