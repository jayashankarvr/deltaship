//! Stats command - show bandwidth savings statistics.

use std::path::Path;

use deltaship_db::{DiffJobStatus, PublisherDb};

use crate::config::DB_FILE;

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

/// Run the stats command to show bandwidth savings.
pub async fn run(name: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    let db = PublisherDb::open(db_path).await?;

    let binaries = db.list_binaries().await?;

    if binaries.is_empty() {
        println!("No binaries registered.");
        return Ok(());
    }

    // Filter binaries if --name is provided
    let binaries: Vec<_> = if let Some(ref filter_name) = name {
        binaries
            .into_iter()
            .filter(|b| b.binary_name == *filter_name)
            .collect()
    } else {
        binaries
    };

    if binaries.is_empty() {
        if let Some(ref filter_name) = name {
            return Err(format!("No binary found with name: {}", filter_name).into());
        }
        println!("No binaries registered.");
        return Ok(());
    }

    println!("Bandwidth Savings Report");
    println!("========================");
    println!();

    let mut overall_binary_size: i64 = 0;
    let mut overall_diff_size: i64 = 0;
    let mut overall_potential_savings: i64 = 0;

    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;

        if versions.is_empty() {
            println!("{}:", binary.binary_name);
            println!("  No versions registered");
            println!();
            continue;
        }

        // Calculate total binary sizes
        let total_binary_size: i64 = versions.iter().map(|v| v.file_size_bytes).sum();
        overall_binary_size += total_binary_size;

        // Get all completed diff jobs for this binary's versions
        let version_ids: Vec<_> = versions.iter().map(|v| v.version_id.clone()).collect();
        let completed_diffs = db.list_diff_jobs_by_status(DiffJobStatus::Completed).await?;

        // Filter diffs relevant to this binary
        let binary_diffs: Vec<_> = completed_diffs
            .into_iter()
            .filter(|d| {
                version_ids.contains(&d.from_version_id) && version_ids.contains(&d.to_version_id)
            })
            .collect();

        let total_diff_size: i64 = binary_diffs.iter().filter_map(|d| d.diff_size_bytes).sum();
        overall_diff_size += total_diff_size;

        println!("{}:", binary.binary_name);
        println!("  Versions: {}", versions.len());
        println!(
            "  Full binary sizes: {} total",
            format_bytes(total_binary_size)
        );
        println!(
            "  Diff sizes generated: {} total",
            format_bytes(total_diff_size)
        );
        println!();

        // Show potential savings per update if we have diffs
        if !binary_diffs.is_empty() {
            println!("  Potential savings per update:");

            // Sort versions by created_at to show in order
            let mut sorted_versions = versions.clone();
            sorted_versions.sort_by(|a, b| a.created_at.cmp(&b.created_at));

            // Create a map of version_id -> version_string and size
            let version_map: std::collections::HashMap<_, _> = sorted_versions
                .iter()
                .map(|v| {
                    (
                        v.version_id.clone(),
                        (v.version_string.clone(), v.file_size_bytes),
                    )
                })
                .collect();

            let mut binary_savings: i64 = 0;
            let mut savings_count = 0;
            let mut total_savings_pct: f64 = 0.0;

            for diff in &binary_diffs {
                if let (Some(from_info), Some(to_info)) = (
                    version_map.get(&diff.from_version_id),
                    version_map.get(&diff.to_version_id),
                ) {
                    if let Some(diff_size) = diff.diff_size_bytes {
                        // Full size would be the target version size
                        let full_size = to_info.1;
                        let saved = full_size - diff_size;
                        let savings_pct = if full_size > 0 {
                            (saved as f64 / full_size as f64) * 100.0
                        } else {
                            0.0
                        };

                        println!(
                            "    {} -> {}: {} -> {} ({:.1}% saved)",
                            from_info.0,
                            to_info.0,
                            format_bytes(full_size),
                            format_bytes(diff_size),
                            savings_pct
                        );

                        binary_savings += saved;
                        savings_count += 1;
                        total_savings_pct += savings_pct;
                    }
                }
            }

            if savings_count > 0 {
                let avg_savings = total_savings_pct / savings_count as f64;
                println!();
                println!("  Average savings: {:.1}%", avg_savings);
                overall_potential_savings += binary_savings;
            }
        } else {
            println!("  No diffs generated yet");
        }

        println!();
    }

    // Overall summary if we're showing multiple binaries
    if binaries.len() > 1 || name.is_none() {
        println!("Overall:");
        println!(
            "  Total binary sizes: {}",
            format_bytes(overall_binary_size)
        );
        println!("  Total diff sizes: {}", format_bytes(overall_diff_size));
        if overall_potential_savings > 0 {
            println!(
                "  Total potential bandwidth saved: {} per full update cycle",
                format_bytes(overall_potential_savings)
            );
        }
    }

    Ok(())
}
