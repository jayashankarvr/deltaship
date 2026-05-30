//! Status command - show update status for all binaries.

use deltaship_db::ClientDb;

use crate::checker::UpdateChecker;
use crate::config::ClientConfig;

/// Format bytes in a human-readable format.
fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Show update status for all managed binaries.
pub async fn show_status(config: &ClientConfig, db: &ClientDb) -> anyhow::Result<()> {
    let binaries = db.list_binaries().await?;

    if binaries.is_empty() {
        println!("No managed binaries registered.");
        return Ok(());
    }

    let checker = UpdateChecker::new(config.clone())?;

    // Check server connectivity
    let server_ok = checker.health_check().await.unwrap_or(false);

    println!("Deltaship Client Status");
    println!("==================");
    println!();
    println!(
        "Server: {} ({})",
        config.server_url,
        if server_ok {
            "connected"
        } else {
            "unreachable"
        }
    );
    println!("Check interval: {} seconds", config.check_interval_secs);
    println!("Signature verification: enabled (required)");
    println!();
    println!("Managed binaries ({}):", binaries.len());
    println!();

    for binary in binaries {
        println!("  {}", binary.binary_name);
        println!(
            "    Current version: {}",
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

        // Check for updates if server is reachable
        if server_ok {
            match checker.check_for_updates(&binary).await {
                Ok(Some(update)) => {
                    println!("    Status: UPDATE AVAILABLE ({})", update.version);
                    if update.forced {
                        println!("    Note: This is a forced update");
                    }
                    if let Some(notes) = update.release_notes {
                        println!("    Release notes: {}", notes);
                    }
                }
                Ok(None) => {
                    println!("    Status: Up to date");
                }
                Err(e) => {
                    println!("    Status: Check failed ({})", e);
                }
            }
        } else {
            println!("    Status: Unknown (server unreachable)");
        }

        // Show recent update history with bandwidth stats
        let history = db.get_recent_updates(&binary.binary_id, 3).await?;
        if !history.is_empty() {
            println!("    Recent updates:");
            for update in history {
                let status_str = match (update.success, update.status.as_str()) {
                    (Some(true), _) => "success",
                    (Some(false), _) => "failed",
                    (None, status) => status,
                };

                // Build bandwidth info string
                let bandwidth_info = build_bandwidth_info(&update);

                println!(
                    "      {} -> {} ({}) - {}{}",
                    update.from_version_string.as_deref().unwrap_or("(none)"),
                    update.to_version_string,
                    status_str,
                    update.started_at,
                    bandwidth_info
                );
            }
        }

        println!();
    }

    Ok(())
}

/// Build a bandwidth info string from update history.
fn build_bandwidth_info(update: &deltaship_db::DbUpdateHistory) -> String {
    match (
        update.actual_downloaded_bytes,
        update.full_size_bytes,
        update.diff_size_bytes,
    ) {
        // Differential update with full size info
        (Some(actual), Some(full), Some(_diff)) if actual < full => {
            let saved = full - actual;
            let pct = if full > 0 {
                (saved as f64 / full as f64) * 100.0
            } else {
                0.0
            };
            format!(
                "\n        Downloaded: {} (saved {} - {:.1}%)",
                format_bytes(actual),
                format_bytes(saved),
                pct
            )
        }
        // Full binary download (no savings)
        (Some(actual), _, None) => {
            format!(
                "\n        Downloaded: {} (full binary)",
                format_bytes(actual)
            )
        }
        // Differential update without full size info
        (Some(actual), None, Some(_diff)) => {
            format!(
                "\n        Downloaded: {} (differential)",
                format_bytes(actual)
            )
        }
        // Fallback: just show diff size if available
        (None, _, Some(diff)) => {
            format!(
                "\n        Downloaded: {} (differential)",
                format_bytes(diff)
            )
        }
        _ => String::new(),
    }
}
