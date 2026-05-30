//! Add command - register a binary to be managed.

use std::path::Path;

use deltaship_crypto::{hash_file, load_verifying_key};
use deltaship_db::{ClientDb, NewManagedBinary};

use crate::audit::{log_audit_event, AuditEvent};

/// Validate that a binary name has a safe format.
///
/// # P2 Issue 65 Fix: Binary Name Path Injection
///
/// Binary names are restricted to alphanumeric characters, hyphens, and underscores
/// to prevent path injection attacks. This prevents:
/// - Path traversal (e.g., "../../../etc/passwd")
/// - Directory separators that could corrupt file paths
/// - Special characters that might be interpreted by shells or filesystems
///
/// Valid examples: "myapp", "my-app", "my_app", "myapp123"
/// Invalid examples: "my/app", "../app", "my app", "my$app"
fn validate_binary_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("Binary name cannot be empty");
    }

    if name.len() > 64 {
        anyhow::bail!(
            "Binary name too long: {} characters (max 64)",
            name.len()
        );
    }

    // Only allow alphanumeric, hyphens, and underscores
    // This prevents path injection and special character issues
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!(
            "Invalid binary name: '{}'. Only alphanumeric characters, hyphens, and underscores are allowed.",
            name
        );
    }

    Ok(())
}

/// Add a binary to be managed by the client.
///
/// # Arguments
/// * `db` - Database connection
/// * `name` - Name of the binary (e.g., "myapp")
/// * `path` - Path to the binary file
/// * `public_key_file` - Path to the publisher's public key file
/// * `server_url` - Server URL for updates (optional, stored for reference)
///
/// # Security
///
/// Binary names are validated to prevent path injection attacks (P2 Issue 65).
/// Only alphanumeric characters, hyphens, and underscores are allowed.
pub async fn add_binary(
    db: &ClientDb,
    name: &str,
    path: &str,
    public_key_file: &str,
) -> anyhow::Result<()> {
    // P2 Issue 65 Fix: Validate binary name format to prevent path injection
    validate_binary_name(name)?;

    let binary_path = Path::new(path);
    let key_path = Path::new(public_key_file);

    // Validate binary path
    if !binary_path.exists() {
        anyhow::bail!("Binary not found: {}", path);
    }

    // Load and validate public key
    let verifying_key = load_verifying_key(key_path)?;
    let public_key_bytes = verifying_key.to_bytes().to_vec();

    // Check if binary is already registered
    if db.get_binary_by_name(name).await?.is_some() {
        anyhow::bail!("Binary '{}' is already registered", name);
    }

    // Compute current binary hash for reference
    let binary_hash = hash_file(binary_path)?;

    // Detect platform
    let platform = detect_platform();

    // Generate a unique ID for this binary
    let binary_id = uuid::Uuid::new_v4().to_string();

    // CRITICAL: Check for symlinks BEFORE canonicalization to prevent symlink attacks
    // Canonicalize() follows symlinks, so we must check first
    //
    // SECURITY NOTE: TOCTOU Race Condition Limitation
    // This check has a Time-of-Check-Time-of-Use (TOCTOU) race condition. Between
    // this check and the canonicalize() call below, a local attacker could replace
    // the file with a symlink. However, this window is extremely small (microseconds).
    //
    // Since this is a one-time registration (not a repeated update operation), the
    // attack surface is limited. The attacker would need:
    // - Local access to the file system
    // - Write permissions in the directory
    // - Precise timing to create symlink in microsecond window
    //
    // Mitigation: Users should run this command in secure environments and verify
    // the registered path in the output immediately after registration.
    if binary_path.is_symlink() {
        anyhow::bail!(
            "Security error: binary path '{}' is a symlink. \
             Symlinks are not allowed to prevent symlink attacks. \
             Please use the actual file path instead.",
            binary_path.display()
        );
    }

    // P1-3 Fix: Walk entire path checking each component for symlinks
    // This prevents attacks where intermediate directories are symlinks
    // (e.g., /home/user/symlink_dir/binary where symlink_dir points elsewhere)
    {
        let mut current = std::path::PathBuf::new();
        for component in binary_path.components() {
            current.push(component);
            // Check each component as we build up the path
            if current.exists() && current.is_symlink() {
                anyhow::bail!(
                    "Security error: path component '{}' is a symlink. \
                     This could be used for a symlink attack. \
                     Please use a path with no symlinks in the directory chain.",
                    current.display()
                );
            }
        }
    }

    // Additional validation: compare user-provided path with canonical path
    // If they differ significantly (beyond just normalization), warn about potential issues
    let canonical_path = binary_path.canonicalize()?;
    if canonical_path != binary_path {
        tracing::debug!(
            "Path '{}' canonicalizes to '{}'. Using canonical path.",
            binary_path.display(),
            canonical_path.display()
        );
    }

    // Register the binary
    let new_binary = NewManagedBinary {
        binary_id: binary_id.clone(),
        binary_name: name.to_string(),
        platform: platform.to_string(),
        install_path: binary_path.canonicalize()?.to_string_lossy().to_string(),
        publisher_public_key: public_key_bytes,
    };

    let registered = db.register_binary(new_binary).await?;

    println!("Registered binary: {}", registered.binary_name);
    println!("  ID: {}", registered.binary_id);
    println!("  Path: {}", registered.install_path);
    println!("  Platform: {}", registered.platform);
    println!("  Current hash: {}", binary_hash);

    // Audit log binary addition
    log_audit_event(&AuditEvent::BinaryAdded {
        binary_name: registered.binary_name.clone(),
        install_path: registered.install_path.clone(),
    });

    Ok(())
}

/// Detect the current platform.
fn detect_platform() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "linux-x86_64";

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "linux-aarch64";

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "windows-x86_64";

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "macos-x86_64";

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "macos-aarch64";

    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    return "unknown";
}
