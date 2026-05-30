//! Import project data from an archive.

use std::fs;
use std::path::Path;

use deltaship_db::PublisherDb;

use crate::archive::{extract_archive, find_export_dir, read_extracted_metadata};
use crate::config::{DB_FILE, KEYS_DIR, PUBLIC_KEY_FILE, SIGNING_KEY_FILE, DELTASHIP_DIR};

/// Run the import command.
pub async fn run(
    input: String,
    merge: bool,
    overwrite: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let input_path = Path::new(&input);

    // Check input file exists
    if !input_path.exists() {
        return Err(format!("Archive not found: {}", input).into());
    }

    // Check for conflicting options
    if merge && overwrite {
        return Err("Cannot use both --merge and --overwrite. Choose one.".into());
    }

    // Check if project already exists
    let deltaship_path = Path::new(DELTASHIP_DIR);
    let project_exists = deltaship_path.exists();

    if project_exists && !merge && !overwrite {
        return Err(
            "Deltaship project already exists. Use --merge to add new entries or --overwrite to replace."
                .into(),
        );
    }

    println!("Importing Deltaship project from archive...");
    println!("  Input: {}", input);
    println!(
        "  Mode: {}",
        if merge {
            "merge"
        } else if overwrite {
            "overwrite"
        } else {
            "new"
        }
    );
    println!();

    // Extract archive to temp directory
    println!("  Extracting archive...");
    let temp_dir = extract_archive(input_path)?;
    let export_dir = find_export_dir(temp_dir.path())?;

    // Read and validate metadata
    println!("  Validating metadata...");
    let metadata = read_extracted_metadata(&export_dir)?;

    println!();
    println!("Archive information:");
    println!("  Format version: {}", metadata.format_version);
    println!("  Exported at: {}", metadata.exported_at);
    println!("  Publisher version: {}", metadata.publisher_version);
    if let Some(ref name) = metadata.publisher_name {
        println!("  Publisher name: {}", name);
    }
    println!("  Includes keys: {}", metadata.includes_keys);
    println!("  Includes binaries: {}", metadata.includes_binaries);
    println!("  Binary count: {}", metadata.binary_count);
    println!("  Version count: {}", metadata.version_count);
    println!();

    // If overwriting, remove existing project
    if overwrite && project_exists {
        println!("  Removing existing project...");
        fs::remove_dir_all(deltaship_path)?;
    }

    // Create project structure if needed
    if !deltaship_path.exists() {
        fs::create_dir_all(deltaship_path)?;
    }

    let keys_path = Path::new(KEYS_DIR);
    if !keys_path.exists() {
        fs::create_dir_all(keys_path)?;
    }

    // Import binary/diff payloads FIRST, so the files exist on disk before the
    // database merge rewrites each record's path to its imported location and
    // `insert_binary` validates that the path is a readable file.
    if metadata.includes_binaries {
        import_binaries(&export_dir, merge)?;
        import_diffs(&export_dir, merge)?;
    }

    // Import database
    import_database(&export_dir, merge).await?;

    // Import keys
    import_keys(&export_dir, &metadata, merge)?;

    // Verify imported data
    println!();
    println!("  Verifying imported data...");
    verify_import().await?;

    println!();
    println!("Import complete!");
    println!();

    if metadata.includes_keys {
        println!(
            "NOTE: Signing key was imported. You may need to enter the passphrase when signing."
        );
    }

    Ok(())
}

/// Import the database file.
async fn import_database(export_dir: &Path, merge: bool) -> Result<(), Box<dyn std::error::Error>> {
    let source_db = export_dir.join("publisher.db");
    let target_db = Path::new(DB_FILE);

    if !source_db.exists() {
        return Err("Archive is missing publisher.db".into());
    }

    if merge && target_db.exists() {
        println!("  Merging database entries...");
        merge_databases(&source_db, target_db).await?;
    } else {
        println!("  Copying database...");
        fs::copy(&source_db, target_db)?;
    }

    Ok(())
}

/// Compute the on-disk path of an imported binary file.
///
/// `import_binaries` copies archive entries into `<DELTASHIP_DIR>/binaries/` using the
/// same `{name}/{version}/{platform}` layout that `export` wrote them with, so
/// this reconstructs where a given version's file actually lives on *this* host.
fn imported_binary_file(binary_name: &str, version_string: &str, platform: &str) -> std::path::PathBuf {
    Path::new(DELTASHIP_DIR)
        .join("binaries")
        .join(binary_name)
        .join(version_string)
        .join(platform)
}

/// Merge entries from source database into target database.
///
/// Cross-host fix: the source DB records absolute `binary_path`/`file_path`
/// values from the *exporting* host, which do not exist on this machine. Because
/// the binary files have already been imported under `<DELTASHIP_DIR>/binaries/`
/// (this runs after `import_binaries`), we rewrite each path to its imported
/// location so later `publish`/`sign`/`verify` can find the files. If an expected
/// imported file is absent (e.g. an export without binary payloads), we fall back
/// to the original path to preserve same-host behavior.
async fn merge_databases(
    source_path: &Path,
    target_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_db = PublisherDb::open(source_path).await?;
    let target_db = PublisherDb::open(target_path).await?;

    // Get binaries from source
    let source_binaries = source_db.list_binaries().await?;

    let mut binaries_added = 0;
    let mut versions_added = 0;

    for binary in &source_binaries {
        // Check if binary exists in target
        let existing = target_db
            .get_binary_by_name(&binary.binary_name, &binary.platform)
            .await?;

        if existing.is_some() {
            println!(
                "    Skipping existing binary: {} ({})",
                binary.binary_name, binary.platform
            );
            continue;
        }

        let versions = source_db.list_versions(&binary.binary_id).await?;

        // `insert_binary` requires `binary_path` to point at an existing, readable
        // file. Resolve it to the first version whose imported file is present;
        // fall back to the original path (same-host case) if none was imported.
        let binary_path = versions
            .iter()
            .map(|v| {
                imported_binary_file(&binary.binary_name, &v.version_string, &binary.platform)
            })
            .find(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| binary.binary_path.clone());

        let new_binary = target_db
            .insert_binary(deltaship_db::NewBinary {
                binary_name: binary.binary_name.clone(),
                platform: binary.platform.clone(),
                binary_path,
                description: binary.description.clone(),
            })
            .await?;

        println!(
            "    Added binary: {} ({})",
            binary.binary_name, binary.platform
        );
        binaries_added += 1;

        // Add versions for this binary, rewriting each file_path to its imported
        // location when the file is present on disk.
        for version in &versions {
            let imported = imported_binary_file(
                &binary.binary_name,
                &version.version_string,
                &binary.platform,
            );
            let file_path = if imported.is_file() {
                imported.to_string_lossy().into_owned()
            } else {
                version.file_path.clone()
            };

            let new_version = target_db
                .insert_version(deltaship_db::NewVersion {
                    binary_id: new_binary.binary_id.clone(),
                    version_string: version.version_string.clone(),
                    file_path,
                    file_size_bytes: version.file_size_bytes,
                    file_hash_blake3: version.file_hash_blake3.clone(),
                    file_hash_sha256: version.file_hash_sha256.clone(),
                })
                .await?;

            // Copy signature if present
            if let Some(ref sig) = version.signature_ed25519 {
                if let Some(ref ts) = version.signature_timestamp {
                    target_db
                        .set_version_signature(&new_version.version_id, sig, ts)
                        .await?;
                }
            }

            println!("      Added version: {}", version.version_string);
            versions_added += 1;
        }
    }

    println!(
        "    Merge complete: {} binaries, {} versions added",
        binaries_added, versions_added
    );

    Ok(())
}

/// Import key files.
fn import_keys(
    export_dir: &Path,
    metadata: &crate::archive::ArchiveMetadata,
    merge: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let keys_dir = export_dir.join("keys");

    // Import public key
    let source_public = keys_dir.join("public.key");
    let target_public = Path::new(PUBLIC_KEY_FILE);

    if source_public.exists() {
        if merge && target_public.exists() {
            println!("  Skipping existing public key");
        } else {
            println!("  Copying public key...");
            fs::copy(&source_public, target_public)?;
        }
    }

    // Import signing key if present
    if metadata.includes_keys {
        let source_signing = keys_dir.join("signing.key");
        let target_signing = Path::new(SIGNING_KEY_FILE);

        if source_signing.exists() {
            if merge && target_signing.exists() {
                println!("  Skipping existing signing key");
            } else {
                println!("  Copying signing key...");
                fs::copy(&source_signing, target_signing)?;
                // `fs::copy` preserves the source mode bits. A signing key that
                // arrived inside an archive typically has world-readable tar
                // permissions (0644), so explicitly tighten it to 0600
                // (owner read/write only) to match deltaship-crypto's
                // `save_signing_key`. Sensitive key material must never be
                // left world-readable on disk.
                set_owner_only_permissions(target_signing)?;
            }
        }
    }

    Ok(())
}

/// Restrict a file to owner read/write only (0600) on Unix.
///
/// No-op on non-Unix platforms, which do not use POSIX mode bits.
#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

/// Validate that a path is safe (no symlinks, stays within target directory).
fn validate_safe_path(
    path: &Path,
    base_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Reject symlinks
    if path.is_symlink() {
        return Err(format!(
            "Security error: symlinks not allowed in archive: {}",
            path.display()
        )
        .into());
    }

    // Check for path traversal components
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!(
                "Security error: path traversal detected in archive: {}",
                path.display()
            )
            .into());
        }
    }

    // Canonicalize both paths and verify the file is within the base directory
    // Note: We canonicalize the base_dir first, then check that the path starts with it
    let canonical_base = base_dir.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize base directory {}: {}",
            base_dir.display(),
            e
        )
    })?;

    let canonical_path = path.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize path {}: {}",
            path.display(),
            e
        )
    })?;

    if !canonical_path.starts_with(&canonical_base) {
        return Err(format!(
            "Security error: path escapes target directory: {} is not within {}",
            canonical_path.display(),
            canonical_base.display()
        )
        .into());
    }

    Ok(())
}

/// Validate that a target path will be safe after creation.
fn validate_target_path(
    rel_path: &Path,
    target_base: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check for path traversal components in the relative path
    for component in rel_path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!(
                "Security error: path traversal detected: {}",
                rel_path.display()
            )
            .into());
        }
    }

    // Build the target path and verify it stays within target_base
    // Use lexical path resolution (without following symlinks) to check
    let mut resolved = target_base.to_path_buf();
    for component in rel_path.components() {
        match component {
            std::path::Component::Normal(c) => resolved.push(c),
            std::path::Component::CurDir => {} // Skip "."
            std::path::Component::ParentDir => {
                return Err(format!(
                    "Security error: path traversal detected: {}",
                    rel_path.display()
                )
                .into());
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(format!(
                    "Security error: absolute path in archive: {}",
                    rel_path.display()
                )
                .into());
            }
        }
    }

    Ok(())
}

/// Import binary files.
fn import_binaries(export_dir: &Path, merge: bool) -> Result<(), Box<dyn std::error::Error>> {
    let source_binaries = export_dir.join("binaries");

    if !source_binaries.exists() {
        return Ok(());
    }

    let target_binaries = Path::new(DELTASHIP_DIR).join("binaries");
    if !target_binaries.exists() {
        fs::create_dir_all(&target_binaries)?;
    }

    println!("  Copying binary files...");

    let mut count = 0;
    for entry in walkdir::WalkDir::new(&source_binaries)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Reject symlinks
        if path.is_symlink() {
            return Err(format!(
                "Security error: symlinks not allowed in archive: {}",
                path.display()
            )
            .into());
        }

        if path.is_file() {
            // Validate source path is within source_binaries
            validate_safe_path(path, &source_binaries)?;

            let rel_path = path.strip_prefix(&source_binaries)?;

            // Validate target path won't escape target_binaries
            validate_target_path(rel_path, &target_binaries)?;

            let target_path = target_binaries.join(rel_path);

            if merge && target_path.exists() {
                continue; // Skip existing files in merge mode
            }

            // Create parent directory
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::copy(path, &target_path)?;
            count += 1;
        }
    }

    if count > 0 {
        println!("    Copied {} binary files", count);
    }

    Ok(())
}

/// Import diff files.
fn import_diffs(export_dir: &Path, merge: bool) -> Result<(), Box<dyn std::error::Error>> {
    let source_diffs = export_dir.join("diffs");

    if !source_diffs.exists() {
        return Ok(());
    }

    let target_diffs = Path::new(DELTASHIP_DIR).join("diffs");
    if !target_diffs.exists() {
        fs::create_dir_all(&target_diffs)?;
    }

    println!("  Copying diff files...");

    let mut count = 0;
    for entry in walkdir::WalkDir::new(&source_diffs)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Reject symlinks
        if path.is_symlink() {
            return Err(format!(
                "Security error: symlinks not allowed in archive: {}",
                path.display()
            )
            .into());
        }

        if path.is_file() {
            // Validate source path is within source_diffs
            validate_safe_path(path, &source_diffs)?;

            let rel_path = path.strip_prefix(&source_diffs)?;

            // Validate target path won't escape target_diffs
            validate_target_path(rel_path, &target_diffs)?;

            let target_path = target_diffs.join(rel_path);

            if merge && target_path.exists() {
                continue; // Skip existing files in merge mode
            }

            // Create parent directory
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs::copy(path, &target_path)?;
            count += 1;
        }
    }

    if count > 0 {
        println!("    Copied {} diff files", count);
    }

    Ok(())
}

/// Verify the imported data is valid.
async fn verify_import() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);

    if !db_path.exists() {
        return Err("Database not found after import".into());
    }

    // Try to open and query database
    let db = PublisherDb::open(db_path).await?;
    let binaries = db.list_binaries().await?;

    let mut version_count = 0;
    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;
        version_count += versions.len();
    }

    println!(
        "    Database: OK ({} binaries, {} versions)",
        binaries.len(),
        version_count
    );

    // Check public key
    let public_key = Path::new(PUBLIC_KEY_FILE);
    if public_key.exists() {
        println!("    Public key: OK");
    } else {
        println!("    Public key: MISSING");
    }

    // Check signing key
    let signing_key = Path::new(SIGNING_KEY_FILE);
    if signing_key.exists() {
        println!("    Signing key: OK");
    } else {
        println!("    Signing key: Not present");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_binary_file_matches_export_layout() {
        // Must mirror export.rs's archive layout: binaries/{name}/{version}/{platform}
        let p = imported_binary_file("myapp", "1.2.3", "linux-x86_64");
        let expected = Path::new(DELTASHIP_DIR)
            .join("binaries")
            .join("myapp")
            .join("1.2.3")
            .join("linux-x86_64");
        assert_eq!(p, expected);
    }
}
