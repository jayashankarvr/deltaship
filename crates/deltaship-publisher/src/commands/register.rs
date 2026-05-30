//! Register a new binary version

use std::fs;
use std::io::IsTerminal;
use std::path::Path;
use std::str::FromStr;

use tracing::{debug, info};
use deltaship_crypto::{hash_file_with_progress, sha256_file_with_progress};
use deltaship_db::{NewBinary, NewVersion, PublisherDb};
use indicatif::{ProgressBar, ProgressStyle};

use crate::config::DB_FILE;
use crate::diff_manager::DiffManager;

/// Run the register command
pub async fn run(
    name: String,
    version: String,
    file: String,
    platform: String,
    description: Option<String>,
    no_diff: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Registering version {} for binary {}", version, name);
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err(format!(
            "Deltaship project not initialized.\n\nNext steps:\n  - Run 'deltaship-publisher init' in the project root directory\n  - This will create the database at: {}\n  - Then retry this registration command",
            DB_FILE
        ).into());
    }

    // Check binary file exists
    let file_path = Path::new(&file);
    if !file_path.exists() {
        return Err(format!(
            "Binary file not found: {}\n\nNext steps:\n  - Verify the file path is correct\n  - Check that the file exists: ls -l {}\n  - Ensure you're running the command from the correct directory\n  - Use an absolute path if the file is not in the current directory",
            file, file
        ).into());
    }

    // Validate version is semver
    if semver::Version::parse(&version).is_err() {
        return Err(format!(
            "Invalid version '{}': Not valid semantic versioning.\n\nNext steps:\n  - Use semantic versioning format: MAJOR.MINOR.PATCH\n  - Examples: 1.0.0, 2.1.3, 0.1.0-beta\n  - See https://semver.org for full specification\n  - Current input: '{}'",
            version, version
        )
        .into());
    }

    // Validate platform
    if deltaship_core::Platform::from_str(&platform).is_err() {
        return Err(format!(
            "Invalid platform '{}': Not a recognized platform identifier.\n\nNext steps:\n  - Use one of the supported platforms:\n    * linux-x86_64 (64-bit Linux on Intel/AMD)\n    * linux-aarch64 (64-bit Linux on ARM)\n    * windows-x86_64 (64-bit Windows)\n    * macos-x86_64 (macOS on Intel)\n    * macos-aarch64 (macOS on Apple Silicon)\n  - Current input: '{}'",
            platform, platform
        ).into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    // Get or create binary
    let binary = match db.get_binary_by_name(&name, &platform).await? {
        Some(b) => {
            println!("Found existing binary: {} ({})", b.binary_name, b.platform);
            b
        }
        None => {
            println!("Creating new binary: {} ({})", name, platform);
            db.insert_binary(NewBinary {
                binary_name: name.clone(),
                platform: platform.clone(),
                binary_path: file.clone(),
                description,
            })
            .await?
        }
    };

    // Check if version already exists
    if db
        .get_version_by_string(&binary.binary_id, &version)
        .await?
        .is_some()
    {
        return Err(format!(
            "Version {} already registered for {} ({}).\n\nNext steps:\n  - List versions with: deltaship-publisher list --name {}\n  - Use a different version number if this is a new release\n  - To replace the existing version, delete it first with: deltaship-publisher cleanup --version {}\n  - Or use a pre-release identifier: {}-rc.1",
            version, name, platform, name, version, version
        )
        .into());
    }

    // Compute hashes with progress reporting for large files
    debug!("Computing hash for file: {:?}", file_path);
    let file_size = fs::metadata(file_path)?.len();

    // Show progress bar for files larger than 10MB and in TTY environments
    let show_progress = file_size > 10 * 1024 * 1024 && std::io::stderr().is_terminal();

    if show_progress {
        println!("Computing file hashes (this may take a moment for large files)...");
    } else {
        info!("Computing file hashes...");
    }

    let blake3_hash = if show_progress {
        let pb = ProgressBar::new(file_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-")
        );
        pb.set_message("Blake3 hash");
        let hash = match hash_file_with_progress(file_path, |bytes| {
            pb.set_position(bytes);
        }) {
            Ok(h) => h,
            Err(e) => {
                pb.abandon_with_message("Blake3 failed");
                return Err(e.into());
            }
        };
        pb.finish_with_message("Blake3 complete");
        hash
    } else {
        hash_file_with_progress(file_path, |_| {})?
    };

    let sha256_hash = if show_progress {
        let pb = ProgressBar::new(file_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("#>-")
        );
        pb.set_message("SHA256 hash");
        let hash = match sha256_file_with_progress(file_path, |bytes| {
            pb.set_position(bytes);
        }) {
            Ok(h) => h,
            Err(e) => {
                pb.abandon_with_message("SHA256 failed");
                return Err(e.into());
            }
        };
        pb.finish_with_message("SHA256 complete");
        hash
    } else {
        sha256_file_with_progress(file_path, |_| {})?
    };
    let file_size_bytes = file_size as i64;

    // Insert version
    let db_version = db
        .insert_version(NewVersion {
            binary_id: binary.binary_id.clone(),
            version_string: version.clone(),
            file_path: file,
            file_size_bytes,
            file_hash_blake3: blake3_hash.to_bytes().to_vec(),
            file_hash_sha256: sha256_hash.to_vec(),
        })
        .await?;

    println!("\nVersion registered successfully!");
    println!("  Binary:     {} ({})", name, platform);
    println!("  Version:    {}", version);
    println!("  Version ID: {}", db_version.version_id);
    println!("  Size:       {} bytes", file_size_bytes);
    println!("  Blake3:     {}", blake3_hash);
    println!("  SHA256:     {}", hex_encode(&sha256_hash));

    // Auto-generate diffs from previous versions unless --no-diff is specified
    if !no_diff {
        println!("\nGenerating diffs from previous versions...");
        let diff_manager = DiffManager::new();
        let diff_results = diff_manager
            .generate_diff_for_version(&db, &name, &binary.binary_id, &db_version)
            .await?;

        if diff_results.is_empty() {
            println!("  No previous versions to diff against.");
        } else {
            println!("  Generated {} diff(s).", diff_results.len());
        }
    }

    println!(
        "\nNext step: Sign this version with 'deltaship-publisher sign --version-id {}'",
        db_version.version_id
    );

    Ok(())
}

/// Simple hex encoding
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
