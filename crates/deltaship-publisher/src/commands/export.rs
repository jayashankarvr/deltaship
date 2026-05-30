//! Export project data to an archive for backup or migration.

use std::fs;
use std::path::Path;

use deltaship_db::{DiffJobStatus, PublisherDb};

use crate::archive::{ArchiveBuilder, ArchiveMetadata};
use crate::config::{ConfigKey, DB_FILE, PUBLIC_KEY_FILE, SIGNING_KEY_FILE};

/// Run the export command.
pub async fn run(
    output: String,
    include_keys: bool,
    include_binaries: bool,
    format: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Validate format
    if format != "tar" {
        return Err(format!(
            "Unsupported export format '{}'. Only 'tar' (tar.gz) is supported.",
            format
        )
        .into());
    }

    // Add .tar.gz extension if needed
    let output_path = if output.ends_with(".tar.gz") || output.ends_with(".tgz") {
        output.clone()
    } else {
        format!("{}.tar.gz", output)
    };

    // Print security warning for key export
    if include_keys {
        print_key_security_warning();
    }

    println!("Exporting Deltaship project...");
    println!("  Output: {}", output_path);
    println!("  Include keys: {}", include_keys);
    println!("  Include binaries: {}", include_binaries);
    println!();

    // Open database
    let db = PublisherDb::open(db_path).await?;

    // Get project stats
    let binaries = db.list_binaries().await?;
    let mut version_count = 0;
    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;
        version_count += versions.len();
    }

    // Get publisher name for metadata
    let publisher_name = db.get_config(ConfigKey::PublisherName.as_db_key()).await?;

    // Create metadata
    let metadata = ArchiveMetadata::new(
        publisher_name.clone(),
        include_keys,
        include_binaries,
        binaries.len(),
        version_count,
    );

    // Create archive
    let archive_path = Path::new(&output_path);
    let mut builder = ArchiveBuilder::new(archive_path)?;

    // Add metadata
    println!("  Adding metadata.json...");
    builder.add_metadata(&metadata)?;

    // Add database
    println!("  Adding publisher.db...");
    builder.add_file(db_path, "publisher.db")?;

    // Add keys directory
    builder.add_directory("keys")?;

    // Always add public key
    let public_key_path = Path::new(PUBLIC_KEY_FILE);
    if public_key_path.exists() {
        println!("  Adding keys/public.key...");
        builder.add_file(public_key_path, "keys/public.key")?;
    } else {
        println!(
            "  Warning: Public key file not found at {}",
            PUBLIC_KEY_FILE
        );
    }

    // Optionally add signing key
    if include_keys {
        let signing_key_path = Path::new(SIGNING_KEY_FILE);
        if signing_key_path.exists() {
            println!("  Adding keys/signing.key...");
            builder.add_file(signing_key_path, "keys/signing.key")?;
        } else {
            println!(
                "  Warning: Signing key file not found at {}",
                SIGNING_KEY_FILE
            );
        }
    }

    // Optionally add binary files
    if include_binaries {
        export_binaries(&db, &mut builder).await?;
        export_diffs(&db, &mut builder).await?;
    }

    // Finish archive
    builder.finish()?;

    // Get archive size
    let archive_size = fs::metadata(archive_path)?.len();

    println!();
    println!("Export complete!");
    println!("  Archive: {}", output_path);
    println!("  Size: {}", format_size(archive_size));
    println!("  Binaries: {}", binaries.len());
    println!("  Versions: {}", version_count);

    if include_keys {
        println!();
        println!("WARNING: This archive contains your signing key.");
        println!("         Store it securely and delete after transfer.");
    }

    Ok(())
}

/// Export binary files to the archive.
async fn export_binaries(
    db: &PublisherDb,
    builder: &mut ArchiveBuilder,
) -> Result<(), Box<dyn std::error::Error>> {
    let binaries = db.list_binaries().await?;

    if binaries.is_empty() {
        return Ok(());
    }

    builder.add_directory("binaries")?;

    let mut total_files = 0;
    let mut total_size: u64 = 0;

    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;

        for version in &versions {
            let file_path = Path::new(&version.file_path);

            if !file_path.exists() {
                println!(
                    "  Warning: Binary file not found: {} ({})",
                    version.file_path, version.version_string
                );
                continue;
            }

            // Archive path: binaries/{name}/{version}/{platform}
            let archive_path = format!(
                "binaries/{}/{}/{}",
                binary.binary_name, version.version_string, binary.platform
            );

            // Create directory structure
            builder.add_directory(&format!(
                "binaries/{}/{}",
                binary.binary_name, version.version_string
            ))?;

            println!(
                "  Adding {} v{} ({})...",
                binary.binary_name, version.version_string, binary.platform
            );
            builder.add_file(file_path, &archive_path)?;

            total_files += 1;
            total_size += version.file_size_bytes.max(0) as u64;
        }
    }

    println!(
        "  Added {} binary files ({})",
        total_files,
        format_size(total_size)
    );

    Ok(())
}

/// Export diff files to the archive.
async fn export_diffs(
    db: &PublisherDb,
    builder: &mut ArchiveBuilder,
) -> Result<(), Box<dyn std::error::Error>> {
    let completed_jobs = db.list_diff_jobs_by_status(DiffJobStatus::Completed).await?;

    if completed_jobs.is_empty() {
        return Ok(());
    }

    builder.add_directory("diffs")?;

    let mut total_files = 0;
    let mut total_size: u64 = 0;

    for job in &completed_jobs {
        if let Some(ref diff_path) = job.diff_path {
            let file_path = Path::new(diff_path);

            if !file_path.exists() {
                println!("  Warning: Diff file not found: {}", diff_path);
                continue;
            }

            // Get version info for naming
            let from_version = db.get_version(&job.from_version_id).await?;
            let to_version = db.get_version(&job.to_version_id).await?;

            if from_version.is_none() || to_version.is_none() {
                println!(
                    "  Warning: Could not find versions for diff job {}",
                    job.job_id
                );
                continue;
            }

            let from_v = from_version.unwrap();
            let to_v = to_version.unwrap();

            // Get binary info
            let binary = db.get_binary(&from_v.binary_id).await?;
            if binary.is_none() {
                continue;
            }
            let binary = binary.unwrap();

            // Archive path: diffs/{name}/{from}-to-{to}-{platform}
            let archive_path = format!(
                "diffs/{}/{}-to-{}-{}",
                binary.binary_name, from_v.version_string, to_v.version_string, binary.platform
            );

            // Create directory structure
            builder.add_directory(&format!("diffs/{}", binary.binary_name))?;

            println!(
                "  Adding diff {} -> {} ({})...",
                from_v.version_string, to_v.version_string, binary.platform
            );
            builder.add_file(file_path, &archive_path)?;

            total_files += 1;
            if let Some(size) = job.diff_size_bytes {
                total_size += size as u64;
            }
        }
    }

    if total_files > 0 {
        println!(
            "  Added {} diff files ({})",
            total_files,
            format_size(total_size)
        );
    }

    Ok(())
}

/// Print a security warning about exporting signing keys.
fn print_key_security_warning() {
    eprintln!();
    eprintln!("================================================================================");
    eprintln!("                         SECURITY WARNING");
    eprintln!("================================================================================");
    eprintln!();
    eprintln!("  You are about to export your SIGNING KEY!");
    eprintln!();
    eprintln!("  The signing key is used to cryptographically sign your software releases.");
    eprintln!("  If this key falls into the wrong hands, an attacker could:");
    eprintln!();
    eprintln!("    - Sign malicious software that appears to come from you");
    eprintln!("    - Push fake updates to your users");
    eprintln!("    - Compromise the security of all users of your software");
    eprintln!();
    eprintln!("  RECOMMENDATIONS:");
    eprintln!("    - Only export keys when absolutely necessary (e.g., migration)");
    eprintln!("    - Transfer the archive over secure channels only");
    eprintln!("    - Delete the archive immediately after use");
    eprintln!("    - Never store the archive in cloud services or version control");
    eprintln!("    - Consider re-generating keys after migration if possible");
    eprintln!();
    eprintln!("================================================================================");
    eprintln!();
}

/// Format file size in human-readable format.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
