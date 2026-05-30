//! List command - show all managed binaries.

use deltaship_db::ClientDb;

/// List all managed binaries.
pub async fn list_binaries(db: &ClientDb) -> anyhow::Result<()> {
    let binaries = db.list_binaries().await?;

    if binaries.is_empty() {
        println!("No managed binaries registered.");
        println!();
        println!("Use 'deltaship-client add' to register a binary.");
        return Ok(());
    }

    println!("Managed binaries ({}):", binaries.len());
    println!();

    for binary in binaries {
        println!("  {} ({})", binary.binary_name, binary.binary_id);
        println!("    Path: {}", binary.install_path);
        println!("    Platform: {}", binary.platform);
        println!(
            "    Version: {}",
            binary
                .current_version_string
                .as_deref()
                .unwrap_or("(unknown)")
        );
        println!(
            "    Auto-update: {}",
            if binary.auto_update {
                "enabled"
            } else {
                "disabled"
            }
        );
        println!(
            "    Last check: {}",
            binary.last_check_at.as_deref().unwrap_or("never")
        );
        println!(
            "    Last update: {}",
            binary.last_update_at.as_deref().unwrap_or("never")
        );
        println!();
    }

    Ok(())
}
