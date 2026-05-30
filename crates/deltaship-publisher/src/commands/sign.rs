//! Sign a registered version

use std::path::Path;

use chrono::Utc;
use deltaship_crypto::load_signing_key;
use deltaship_db::PublisherDb;
use zeroize::Zeroizing;

use crate::config::{DB_FILE, SIGNING_KEY_FILE};
use crate::utils::signing_payload;

/// Run the sign command
pub async fn run(
    version_id: Option<String>,
    name: Option<String>,
    version: Option<String>,
    key_file: Option<String>,
    passphrase_arg: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    // Find the version to sign
    let db_version = match (version_id, name, version) {
        (Some(vid), _, _) => db
            .get_version(&vid)
            .await?
            .ok_or_else(|| format!("Version not found: {}", vid))?,
        (None, Some(n), Some(v)) => {
            // Find binary first
            let binaries = db.list_binaries().await?;
            let binary = binaries
                .iter()
                .find(|b| b.binary_name == n)
                .ok_or_else(|| format!("Binary not found: {}", n))?;

            db.get_version_by_string(&binary.binary_id, &v)
                .await?
                .ok_or_else(|| format!("Version {} not found for binary {}", v, n))?
        }
        _ => {
            return Err("Must provide either --version-id or both --name and --version".into());
        }
    };

    // Check if already signed
    if db_version.signature_ed25519.is_some() {
        println!("Warning: Version is already signed. Re-signing...");
    }

    // Load signing key
    let key_path = key_file.as_deref().unwrap_or(SIGNING_KEY_FILE);
    let key_path = Path::new(key_path);

    if !key_path.exists() {
        return Err(format!(
            "Signing key not found at {}. Generate one with 'deltaship-publisher keygen'.",
            key_path.display()
        )
        .into());
    }

    // Prompt for passphrase.
    //
    // Empty passphrase handling:
    // - If the user presses Enter without typing anything, we treat this as "no passphrase"
    //   and attempt to load the key as an unencrypted key file.
    // - This is safe because:
    //   1. Unencrypted key files have a different header ("-----BEGIN DELTASHIP SIGNING KEY-----")
    //      than encrypted ones ("-----BEGIN DELTASHIP ENCRYPTED SIGNING KEY-----").
    //   2. If the key file is encrypted but the user provides an empty passphrase,
    //      the load_signing_key function will return a PassphraseRequired error.
    // - Security note: Unencrypted keys are not recommended for production use.
    //   Consider generating encrypted keys with strong passphrases.
    let passphrase = match passphrase_arg {
        Some(p) => Zeroizing::new(p),
        None => Zeroizing::new(
            rpassword::prompt_password("Enter passphrase (or press Enter if unencrypted): ")?,
        ),
    };
    let passphrase_opt = if passphrase.is_empty() {
        None
    } else {
        Some(&passphrase)
    };

    let signing_key = load_signing_key(key_path, passphrase_opt).map_err(|e| {
        // Provide a helpful error message with context about common causes
        format!(
            "Failed to load signing key: {}. If the key is encrypted, verify your passphrase is correct.",
            e
        )
    })?;

    let timestamp = Utc::now().to_rfc3339();

    // Sign the canonical "DELTASHIP-sig-v1" payload: domain tag || raw BLAKE3 hash ||
    // version string. This binds the binary's content hash to its version so the
    // client can verify with just the downloaded binary — no separate manifest
    // download required — while preventing signature-substitution/downgrade attacks.
    // `db_version.file_hash_blake3` is the raw 32 bytes (BLOB), not hex.
    println!("Signing version...");
    let message = signing_payload(&db_version.file_hash_blake3, &db_version.version_string);
    let signature = signing_key.sign(&message);

    // Update database
    db.set_version_signature(&db_version.version_id, &signature.to_bytes(), &timestamp)
        .await?;

    // Get binary info for display
    let binary = db
        .get_binary(&db_version.binary_id)
        .await?
        .ok_or("Binary not found")?;

    println!("\nVersion signed successfully!");
    println!("  Binary:    {} ({})", binary.binary_name, binary.platform);
    println!("  Version:   {}", db_version.version_string);
    println!("  Timestamp: {}", timestamp);
    println!("  Signature: {}", hex_encode(&signature.to_bytes()[..32]));
    println!(
        "             {}...",
        hex_encode(&signature.to_bytes()[32..])
    );

    Ok(())
}

/// Simple hex encoding
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
