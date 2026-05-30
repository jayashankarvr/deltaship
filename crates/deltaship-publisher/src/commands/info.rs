//! Project info command

use std::env;
use std::path::Path;

use deltaship_db::PublisherDb;

use crate::config::{ConfigKey, DB_FILE, DEFAULT_SERVER_URL, PUBLIC_KEY_FILE, DELTASHIP_DIR};

/// Run the info command
pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    let cwd = env::current_dir()?;
    let deltaship_path = cwd.join(DELTASHIP_DIR);
    let public_key_path = cwd.join(PUBLIC_KEY_FILE);

    let db = PublisherDb::open(db_path).await?;

    println!("Deltaship Project Info");
    println!("=================");
    println!();

    // Project directory
    println!("Project directory: {}", cwd.display());
    println!("Deltaship directory:    {}", deltaship_path.display());
    println!("Database path:     {}", cwd.join(DB_FILE).display());
    println!();

    // Server URL
    let server_url = db
        .get_config(ConfigKey::ServerUrl.as_db_key())
        .await?
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_string());
    println!("Server URL: {}", server_url);
    println!();

    // Publisher name
    if let Some(name) = db.get_config(ConfigKey::PublisherName.as_db_key()).await? {
        if !name.is_empty() {
            println!("Publisher name: {}", name);
            println!();
        }
    }

    // Public key
    if let Some(public_key) = db.get_config(ConfigKey::PublicKey.as_db_key()).await? {
        let fingerprint = compute_fingerprint(&public_key);
        println!("Public key:");
        println!("  Path:        {}", public_key_path.display());
        println!("  Fingerprint: {}", fingerprint);
        println!();
    } else {
        println!("Public key: (not configured)");
        println!();
    }

    // Binary and version counts
    let binaries = db.list_binaries().await?;
    let binary_count = binaries.len();

    let mut version_count = 0;
    let mut signed_count = 0;
    let mut published_count = 0;

    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;
        version_count += versions.len();
        signed_count += versions
            .iter()
            .filter(|v| v.signature_ed25519.is_some())
            .count();
        published_count += versions.iter().filter(|v| v.is_published).count();
    }

    println!("Statistics:");
    println!("  Registered binaries: {}", binary_count);
    println!("  Registered versions: {}", version_count);
    println!("  Signed versions:     {}", signed_count);
    println!("  Published versions:  {}", published_count);

    Ok(())
}

/// Compute a short fingerprint from a public key hex string
fn compute_fingerprint(public_key_hex: &str) -> String {
    // Take first 16 characters and format in groups of 4
    let short = if public_key_hex.len() >= 16 {
        &public_key_hex[..16]
    } else {
        public_key_hex
    };

    // Format as XXXX:XXXX:XXXX:XXXX
    short
        .chars()
        .collect::<Vec<_>>()
        .chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(":")
        .to_uppercase()
}
