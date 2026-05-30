//! Remove command - unregister a managed binary.

use deltaship_db::ClientDb;

use crate::audit::{log_audit_event, AuditEvent};

/// Remove a binary from management.
///
/// This only removes the binary from the client's database.
/// The actual binary file is not deleted.
pub async fn remove_binary(db: &ClientDb, name: &str) -> anyhow::Result<()> {
    // Find the binary
    let binary = db
        .get_binary_by_name(name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Binary '{}' not found", name))?;

    // Delete associated backups
    let backups = db.list_backups(&binary.binary_id).await?;
    for backup in backups {
        // Delete backup file
        let backup_path = std::path::Path::new(&backup.backup_path);
        if backup_path.exists() {
            if let Err(e) = std::fs::remove_file(backup_path) {
                tracing::warn!("Failed to delete backup file {}: {}", backup.backup_path, e);
            }
        }
    }

    // Store info for audit log before deletion
    let binary_name = binary.binary_name.clone();
    let install_path = binary.install_path.clone();

    // Delete the binary from the database (cascades to related records)
    db.delete_managed_binary(&binary.binary_id).await?;

    println!("Removed binary '{}' from management", name);
    println!(
        "Note: The binary file at '{}' was not deleted",
        install_path
    );

    // Audit log binary removal
    log_audit_event(&AuditEvent::BinaryRemoved {
        binary_name,
        install_path,
    });

    Ok(())
}
