//! Cleanup and prune commands for Deltaship Publisher.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use deltaship_db::{DiffJobStatus, PublisherDb};

use crate::config::{DB_FILE, DELTASHIP_DIR};

/// Default age for old diff files in days.
const DEFAULT_DIFF_AGE_DAYS: u64 = 90;

/// Format bytes as a human-readable size.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Cleanup result for tracking removed items.
#[derive(Default)]
struct CleanupResult {
    orphaned_diffs: Vec<(String, u64)>,
    orphaned_binaries: Vec<(String, u64)>,
    failed_jobs_cleaned: usize,
    total_space: u64,
}

/// Run the cleanup command.
pub async fn run_cleanup(
    dry_run: bool,
    max_age_days: Option<u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    let _max_age = max_age_days.unwrap_or(DEFAULT_DIFF_AGE_DAYS);

    if dry_run {
        println!("Running cleanup in dry-run mode (no files will be removed)...\n");
    } else {
        println!("Running cleanup...\n");
    }

    let mut result = CleanupResult::default();

    // 1. Find orphaned diff files (diffs not tracked in database)
    find_orphaned_diffs(&db, &mut result, dry_run).await?;

    // 2. Find orphaned binary files (versions not in database)
    find_orphaned_binaries(&db, &mut result, dry_run).await?;

    // 3. Clean up failed diff jobs
    cleanup_failed_jobs(&db, &mut result, dry_run).await?;

    // Print summary
    println!("\nCleanup summary:");

    if !result.orphaned_diffs.is_empty() {
        let total: u64 = result.orphaned_diffs.iter().map(|(_, s)| s).sum();
        println!(
            "  - Removed {} orphaned diff files ({})",
            result.orphaned_diffs.len(),
            format_size(total)
        );
    }

    if !result.orphaned_binaries.is_empty() {
        let total: u64 = result.orphaned_binaries.iter().map(|(_, s)| s).sum();
        println!(
            "  - Removed {} orphaned binary files ({})",
            result.orphaned_binaries.len(),
            format_size(total)
        );
    }

    if result.failed_jobs_cleaned > 0 {
        println!(
            "  - Cleaned {} failed diff jobs",
            result.failed_jobs_cleaned
        );
    }

    if result.total_space > 0 {
        println!(
            "  - Total space recovered: {}",
            format_size(result.total_space)
        );
    } else {
        println!("  - No cleanup needed");
    }

    if dry_run && result.total_space > 0 {
        println!("\n(Dry run - no files were actually removed. Run without --dry-run to remove.)");
    }

    Ok(())
}

/// Find and optionally remove orphaned diff files.
async fn find_orphaned_diffs(
    db: &PublisherDb,
    result: &mut CleanupResult,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let diffs_dir = Path::new(DELTASHIP_DIR).join("diffs");

    if !diffs_dir.exists() {
        return Ok(());
    }

    // Get all diff paths from database
    let completed_jobs = db.list_diff_jobs_by_status(DiffJobStatus::Completed).await?;
    let known_paths: HashSet<String> = completed_jobs
        .iter()
        .filter_map(|j| j.diff_path.clone())
        .collect();

    // Walk the diffs directory
    walk_directory_for_orphans(&diffs_dir, &known_paths, result, dry_run, "diff")?;

    Ok(())
}

/// Find and optionally remove orphaned binary files.
async fn find_orphaned_binaries(
    db: &PublisherDb,
    result: &mut CleanupResult,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let binaries_dir = Path::new(DELTASHIP_DIR).join("binaries");

    if !binaries_dir.exists() {
        return Ok(());
    }

    // Get all binary paths from database
    let binaries = db.list_binaries().await?;
    let mut known_paths: HashSet<String> = HashSet::new();

    for binary in &binaries {
        let versions = db.list_versions(&binary.binary_id).await?;
        for version in versions {
            known_paths.insert(version.file_path.clone());
        }
    }

    // Walk the binaries directory
    walk_directory_for_orphans(&binaries_dir, &known_paths, result, dry_run, "binary")?;

    Ok(())
}

/// Walk a directory looking for orphaned files.
fn walk_directory_for_orphans(
    dir: &Path,
    known_paths: &HashSet<String>,
    result: &mut CleanupResult,
    dry_run: bool,
    file_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        if path.is_file() {
            let path_str = path.to_string_lossy().to_string();

            // Check if this file is known in the database
            if !known_paths.contains(&path_str) {
                let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

                if dry_run {
                    println!(
                        "  Would remove orphaned {}: {} ({})",
                        file_type,
                        path.display(),
                        format_size(size)
                    );
                } else {
                    match fs::remove_file(path) {
                        Ok(_) => {
                            println!(
                                "  Removed orphaned {}: {} ({})",
                                file_type,
                                path.display(),
                                format_size(size)
                            );
                        }
                        Err(e) => {
                            eprintln!("  Failed to remove {}: {}", path.display(), e);
                            continue;
                        }
                    }
                }

                result.total_space += size;
                if file_type == "diff" {
                    result.orphaned_diffs.push((path_str, size));
                } else {
                    result.orphaned_binaries.push((path_str, size));
                }
            }
        }
    }

    Ok(())
}

/// Clean up failed diff jobs.
async fn cleanup_failed_jobs(
    db: &PublisherDb,
    result: &mut CleanupResult,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let failed_jobs = db.list_diff_jobs_by_status(DiffJobStatus::Failed).await?;

    if failed_jobs.is_empty() {
        return Ok(());
    }

    for job in &failed_jobs {
        if dry_run {
            println!(
                "  Would clean failed job: {} ({})",
                job.job_id,
                job.error_message.as_deref().unwrap_or("unknown error")
            );
        } else {
            // Delete the job from database
            match db.delete_diff_job(&job.job_id).await {
                Ok(deleted) => {
                    if deleted {
                        println!(
                            "  Cleaned failed job: {} ({})",
                            job.job_id,
                            job.error_message.as_deref().unwrap_or("unknown error")
                        );
                    } else {
                        println!("  Job {} already removed", job.job_id);
                    }
                }
                Err(e) => {
                    eprintln!("  Failed to delete job {}: {}", job.job_id, e);
                    continue;
                }
            }
        }
        result.failed_jobs_cleaned += 1;
    }

    Ok(())
}

/// Run the prune command - keep only the last N versions.
pub async fn run_prune(
    name: String,
    keep: usize,
    delete_files: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    if keep == 0 {
        return Err("Cannot keep 0 versions. Use at least --keep 1.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    if dry_run {
        println!("Running prune in dry-run mode (no changes will be made)...\n");
    }

    // Find the binary (we need to find it across all platforms)
    let binaries = db.list_binaries().await?;
    let matching_binaries: Vec<_> = binaries
        .into_iter()
        .filter(|b| b.binary_name == name)
        .collect();

    if matching_binaries.is_empty() {
        return Err(format!("No binary found with name '{}'", name).into());
    }

    let mut total_versions_removed = 0;
    let mut total_space_recovered: u64 = 0;
    let mut diffs_removed = 0;

    for binary in &matching_binaries {
        println!("Pruning {} ({})...", binary.binary_name, binary.platform);

        // Get all versions ordered by creation date (newest first)
        let versions = db.list_versions(&binary.binary_id).await?;

        if versions.len() <= keep {
            println!("  Only {} versions exist, keeping all.", versions.len());
            continue;
        }

        let versions_to_remove = &versions[keep..];

        for version in versions_to_remove {
            let file_size = version.file_size_bytes.max(0) as u64;

            if dry_run {
                println!(
                    "  Would remove version {} ({})",
                    version.version_string,
                    format_size(file_size)
                );
            } else {
                println!(
                    "  Removing version {} ({})",
                    version.version_string,
                    format_size(file_size)
                );
            }

            // Delete the file if requested
            if delete_files {
                let file_path = Path::new(&version.file_path);
                if file_path.exists() {
                    if dry_run {
                        println!("    Would delete file: {}", version.file_path);
                    } else {
                        match fs::remove_file(file_path) {
                            Ok(_) => {
                                println!("    Deleted file: {}", version.file_path);
                                total_space_recovered += file_size;
                            }
                            Err(e) => {
                                eprintln!("    Failed to delete file: {}", e);
                            }
                        }
                    }
                }

                // Also remove associated diff files
                let completed_jobs = db.list_diff_jobs_by_status(DiffJobStatus::Completed).await?;
                for job in completed_jobs {
                    if job.from_version_id == version.version_id
                        || job.to_version_id == version.version_id
                    {
                        if let Some(diff_path) = &job.diff_path {
                            let diff_file = Path::new(diff_path);
                            if diff_file.exists() {
                                let diff_size =
                                    fs::metadata(diff_file).map(|m| m.len()).unwrap_or(0);
                                if dry_run {
                                    println!(
                                        "    Would delete diff: {} ({})",
                                        diff_path,
                                        format_size(diff_size)
                                    );
                                } else {
                                    match fs::remove_file(diff_file) {
                                        Ok(_) => {
                                            println!(
                                                "    Deleted diff: {} ({})",
                                                diff_path,
                                                format_size(diff_size)
                                            );
                                            total_space_recovered += diff_size;
                                            diffs_removed += 1;
                                        }
                                        Err(e) => {
                                            eprintln!("    Failed to delete diff: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Delete version record from database
            if !dry_run {
                // First delete associated diff jobs to avoid foreign key constraint violations
                match db.delete_diff_jobs_for_version(&version.version_id).await {
                    Ok(count) => {
                        if count > 0 {
                            println!("    Deleted {} associated diff job(s)", count);
                        }
                    }
                    Err(e) => {
                        eprintln!("    Failed to delete diff jobs: {}", e);
                    }
                }

                // Then delete the version record
                match db.delete_version(&version.version_id).await {
                    Ok(deleted) => {
                        if deleted {
                            println!("    Deleted version record from database");
                        } else {
                            println!("    Version record already removed");
                        }
                    }
                    Err(e) => {
                        eprintln!("    Failed to delete version record: {}", e);
                    }
                }
            }

            total_versions_removed += 1;
        }
    }

    // Print summary
    println!("\nPrune summary:");
    if total_versions_removed > 0 {
        println!("  - Removed {} version records", total_versions_removed);
    }
    if diffs_removed > 0 {
        println!("  - Removed {} diff files", diffs_removed);
    }
    if total_space_recovered > 0 {
        println!(
            "  - Total space recovered: {}",
            format_size(total_space_recovered)
        );
    }
    if total_versions_removed == 0 {
        println!("  - No versions to prune");
    }

    if dry_run && total_versions_removed > 0 {
        println!("\n(Dry run - no changes were actually made. Run without --dry-run to apply.)");
    }

    Ok(())
}
