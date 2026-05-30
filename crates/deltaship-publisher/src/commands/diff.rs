//! Manual diff generation command

use std::path::Path;

use deltaship_db::{DiffJobStatus, PublisherDb};

use crate::config::DB_FILE;
use crate::diff_manager::DiffManager;

/// Run the diff command - manually generate a diff between two versions.
pub async fn run(
    name: String,
    platform: String,
    from_version: String,
    to_version: String,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    println!(
        "Generating diff for {} ({}) from {} to {}...",
        name, platform, from_version, to_version
    );

    // Create diff manager
    let output_path = output.as_ref().map(|s| Path::new(s.as_str()));
    let manager = DiffManager::new();

    // Generate the diff
    let result = manager
        .generate_diff_between(
            &db,
            &name,
            &platform,
            &from_version,
            &to_version,
            output_path,
        )
        .await?;

    match result {
        None => {
            println!("\nDiff skipped: compressed delta is not smaller than the full binary.");
            println!("Clients upgrading from {} will download the full binary.", from_version);
        }
        Some(result) => {
            println!("\nDiff generated successfully!");
            println!("  From:        {}", result.from_version);
            println!("  To:          {}", result.to_version);
            println!("  Output:      {}", result.diff_path.display());
            println!("  Size:        {} bytes", result.diff_size);
            println!(
                "  Compression: {:.1}% of target",
                result.compression_ratio * 100.0
            );
            println!("  Time:        {} ms", result.computation_time_ms);
            println!("  Job ID:      {}", result.job_id);
        }
    }

    Ok(())
}

/// Run the diff list command - list all diff jobs.
pub async fn run_list(
    name: Option<String>,
    status: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Check database exists
    let db_path = Path::new(DB_FILE);
    if !db_path.exists() {
        return Err("Deltaship project not initialized. Run 'deltaship-publisher init' first.".into());
    }

    // Open database
    let db = PublisherDb::open(db_path).await?;

    // Parse and validate the status filter, defaulting to "completed".
    // Valid values: "pending", "running", "completed", "failed" (see DiffJobStatus enum).
    let status_filter = match status.as_deref() {
        None => DiffJobStatus::Completed,
        Some(s) => s.parse::<DiffJobStatus>().map_err(|_| {
            format!(
                "Invalid status '{}'. Valid values: pending, running, completed, failed",
                s
            )
        })?,
    };

    let jobs = db.list_diff_jobs_by_status(status_filter).await?;

    let status_str = status_filter.as_str();

    if jobs.is_empty() {
        println!("No diff jobs with status '{}'.", status_str);
        return Ok(());
    }

    println!("Diff Jobs ({}):", status_str);
    println!(
        "{:<36} {:<12} {:<12} {:<10} {:<10}",
        "JOB ID", "FROM", "TO", "SIZE", "STATUS"
    );
    println!(
        "{:-<36} {:-<12} {:-<12} {:-<10} {:-<10}",
        "", "", "", "", ""
    );

    for job in jobs {
        // Get version info
        let from_version = db.get_version(&job.from_version_id).await?;
        let from_ver = from_version
            .as_ref()
            .map(|v| v.version_string.clone())
            .unwrap_or_else(|| "?".to_string());
        let to_ver = db
            .get_version(&job.to_version_id)
            .await?
            .map(|v| v.version_string)
            .unwrap_or_else(|| "?".to_string());

        // Filter by name if specified
        if let Some(ref filter_name) = name {
            if let Some(ref ver) = from_version {
                let binary = db.get_binary(&ver.binary_id).await?;
                if let Some(b) = binary {
                    if b.binary_name != *filter_name {
                        continue;
                    }
                }
            }
        }

        let size = job
            .diff_size_bytes
            .map(|s| format!("{}", s))
            .unwrap_or_else(|| "-".to_string());

        println!(
            "{:<36} {:<12} {:<12} {:<10} {:<10}",
            job.job_id, from_ver, to_ver, size, job.status
        );
    }

    Ok(())
}
