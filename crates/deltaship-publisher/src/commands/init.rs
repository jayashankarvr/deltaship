//! Initialize a new Deltaship project

use std::fs;
use std::path::Path;

use deltaship_crypto::{save_signing_key, save_verifying_key, SecurityLevel, SigningKey};
use deltaship_db::PublisherDb;
use zeroize::Zeroizing;

use crate::config::{self, DB_FILE, KEYS_DIR, DELTASHIP_DIR};

/// Run the init command
pub async fn run(passphrase_arg: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let deltaship_path = Path::new(DELTASHIP_DIR);
    let keys_path = Path::new(KEYS_DIR);

    // Check if already initialized
    if deltaship_path.exists() {
        return Err(
            "Deltaship project already initialized in this directory. Remove .deltaship/ to reinitialize."
                .into(),
        );
    }

    println!("Initializing Deltaship project...");

    // Create .deltaship directory
    fs::create_dir_all(deltaship_path)?;
    println!("  Created {}/", DELTASHIP_DIR);

    // Create keys directory
    fs::create_dir_all(keys_path)?;
    println!("  Created {}/", KEYS_DIR);

    // Get passphrase — from flag or interactive prompt
    let passphrase = match passphrase_arg {
        Some(p) => Zeroizing::new(if p == "UNENCRYPTED" { String::new() } else { p }),
        None => prompt_passphrase()?,
    };

    // Generate signing keypair
    println!("  Generating Ed25519 keypair...");
    let signing_key = SigningKey::generate();
    let verifying_key = signing_key.verifying_key();

    // Save keys
    let signing_key_path = keys_path.join("signing.key");
    let public_key_path = keys_path.join("public.key");

    let passphrase_opt = if passphrase.is_empty() {
        None
    } else {
        Some(&passphrase)
    };

    save_signing_key(&signing_key, &signing_key_path, passphrase_opt, SecurityLevel::Standard)?;
    println!("  Saved signing key to {}", signing_key_path.display());

    save_verifying_key(&verifying_key, &public_key_path)?;
    println!("  Saved public key to {}", public_key_path.display());

    // Initialize database
    let db_path = Path::new(DB_FILE);
    let db = PublisherDb::open(db_path).await?;
    db.init().await?;
    println!("  Created database at {}", DB_FILE);

    // Store public key in config
    let public_key_hex = hex::encode(verifying_key.to_bytes());
    db.set_config(config::CONFIG_PUBLIC_KEY, &public_key_hex)
        .await?;

    println!("\nDeltaship project initialized successfully!");
    println!("\nPublic key (share this with clients):");
    println!("  {}", public_key_hex);

    if passphrase.is_empty() {
        println!(
            "\nWarning: Signing key is NOT encrypted. Consider using a passphrase for production."
        );
    }

    Ok(())
}

/// Prompt user for passphrase with confirmation
fn prompt_passphrase() -> Result<Zeroizing<String>, Box<dyn std::error::Error>> {
    println!("\nEnter passphrase to encrypt signing key:");
    println!("  (Type 'UNENCRYPTED' to save without encryption - NOT RECOMMENDED)");
    let passphrase = Zeroizing::new(rpassword::prompt_password("  Passphrase: ")?);

    if passphrase.is_empty() {
        return Err("Empty passphrase not allowed. Type 'UNENCRYPTED' to save without encryption.".into());
    }

    // Check for explicit unencrypted confirmation
    if *passphrase == "UNENCRYPTED" {
        println!("\n⚠️  WARNING: You are saving the signing key WITHOUT encryption!");
        println!("  This is a SECURITY RISK. Anyone with access to the file can use your signing key.");
        println!("  Type 'UNENCRYPTED' again to confirm:");
        let confirm = Zeroizing::new(rpassword::prompt_password("  Confirm: ")?);
        if *confirm != "UNENCRYPTED" {
            return Err("Unencrypted confirmation failed. Key generation cancelled.".into());
        }
        // Return empty string to signal no encryption
        return Ok(Zeroizing::new(String::new()));
    }

    // Validate passphrase strength
    if passphrase.len() < 12 {
        return Err("Passphrase too short. Minimum 12 characters required for security.".into());
    }

    // Check against common weak passwords
    const COMMON_WEAK_PASSWORDS: &[&str] = &[
        "password", "Password1", "qwertyuiop", "123456789012",
        "passwordpassword", "letmein12345", "admin1234567",
        "welcome12345", "monkey123456", "dragon123456",
    ];

    let passphrase_lower = passphrase.to_lowercase();
    for weak in COMMON_WEAK_PASSWORDS {
        if passphrase_lower.contains(&weak.to_lowercase()) {
            return Err(format!(
                "Passphrase contains common weak pattern '{}'. Please choose a more secure passphrase.",
                weak
            ).into());
        }
    }

    let confirm = Zeroizing::new(rpassword::prompt_password("  Confirm passphrase: ")?);

    if *passphrase != *confirm {
        return Err("Passphrases do not match.".into());
    }

    Ok(passphrase)
}

/// Helper to encode bytes to hex (simple implementation)
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}
