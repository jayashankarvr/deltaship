//! File downloader with resume support for Deltaship Client.

use std::path::Path;

use anyhow::Context;
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use deltaship_crypto::hash_file;

/// Maximum number of retry attempts for failed downloads.
const MAX_RETRIES: u32 = 3;

/// Initial backoff duration in milliseconds.
const INITIAL_BACKOFF_MS: u64 = 1000;

/// Maximum backoff duration in milliseconds (30 seconds).
const MAX_BACKOFF_MS: u64 = 30_000;

/// User-Agent string for HTTP requests, dynamically set from Cargo package version.
const USER_AGENT: &str = concat!("deltaship-client/", env!("CARGO_PKG_VERSION"));

/// Decide whether a reqwest client built for `url` must enforce HTTPS.
///
/// FIX-3: We require HTTPS for every download/check, relaxing to allow plain
/// `http://` ONLY when the URL host is loopback (localhost / 127.0.0.0/8 / ::1)
/// for local development. Building the client with `https_only(true)` makes
/// reqwest refuse to follow any non-HTTPS URL (including redirects), so a
/// remote http URL fails fast instead of leaking update traffic in cleartext.
///
/// If the URL cannot be parsed we conservatively require HTTPS.
fn require_https_for(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => !crate::config::is_loopback_host(parsed.host_str()),
        Err(_) => true,
    }
}

/// HTTP status codes that indicate permanent errors (should not retry).
const PERMANENT_ERROR_CODES: &[u16] = &[
    400, // Bad Request
    401, // Unauthorized
    403, // Forbidden
    404, // Not Found
    405, // Method Not Allowed
    410, // Gone
];

/// Download a file from a URL to a destination path.
///
/// Supports resuming downloads via HTTP Range headers.
///
/// # Timeout Configuration
///
/// Default timeout values:
/// - **Overall timeout**: 300 seconds (5 minutes) - Maximum time for the entire request
///   including connection, sending, and receiving. Suitable for most binary downloads.
/// - **Connect timeout**: 10 seconds - Maximum time to establish TCP connection.
///   Helps fail fast on unreachable servers.
///
/// To override these defaults, use the `download_file_with_progress` function with a
/// custom `reqwest::Client`, or modify the `ClientConfig` to expose timeout settings
/// (not currently implemented - file an issue if needed).
///
/// For very large binaries (>1GB), consider increasing the overall timeout or using
/// chunked downloads with resume support.
pub async fn download_file(url: &str, dest: &Path) -> anyhow::Result<()> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        // Overall request timeout: 5 minutes (300s) - covers entire download
        .timeout(std::time::Duration::from_secs(300))
        // Connection timeout: 10 seconds - fail fast on unreachable servers
        .connect_timeout(std::time::Duration::from_secs(10))
        // FIX-3: Reject non-HTTPS for any non-loopback host (localhost dev still allowed).
        .https_only(require_https_for(url))
        .build()
        .context("Failed to build HTTP client")?;

    // Check if partial download exists
    let existing_size = if dest.exists() {
        tokio::fs::metadata(dest)
            .await
            .with_context(|| format!("Failed to read metadata for {:?}", dest))?
            .len()
    } else {
        0
    };

    tracing::info!("Downloading {} to {}", url, dest.display());

    // Build request, potentially with Range header for resume
    let mut request = client.get(url);
    if existing_size > 0 {
        tracing::info!("Resuming download from byte {}", existing_size);
        request = request.header("Range", format!("bytes={}-", existing_size));
    }

    let response = request
        .send()
        .await
        .with_context(|| format!("Failed to send HTTP request to {}", url))?;
    let status = response.status();

    // Handle response status
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // File is already complete
        tracing::info!("File already fully downloaded");
        return Ok(());
    }

    if !status.is_success() {
        anyhow::bail!("Download failed from {}: status {}", url, status);
    }

    // Check if server supports range requests
    let is_partial = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let total_size = if is_partial {
        // Parse Content-Range header to get total size
        response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split('/').next_back())
            .and_then(|s| s.parse::<u64>().ok())
    } else {
        response.content_length()
    };

    // Open file for writing (append if resuming)
    let mut file = if is_partial && existing_size > 0 {
        OpenOptions::new()
            .append(true)
            .open(dest)
            .await
            .with_context(|| format!("Failed to open file for appending: {:?}", dest))?
    } else {
        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
        }
        File::create(dest)
            .await
            .with_context(|| format!("Failed to create file: {:?}", dest))?
    };

    // Download with progress logging
    let mut downloaded = if is_partial { existing_size } else { 0 };
    let mut stream = response.bytes_stream();
    let mut last_progress = 0;

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.with_context(|| format!("Failed to read chunk from {}", url))?;
        file.write_all(&chunk)
            .await
            .with_context(|| format!("Failed to write to file: {:?}", dest))?;
        downloaded += chunk.len() as u64;

        // Log progress every 10%, capped at 100% to handle inaccurate Content-Length
        if let Some(total) = total_size {
            // Cap progress at 100% in case downloaded exceeds Content-Length
            // (can happen with compression or incorrect server headers)
            let progress = std::cmp::min((downloaded * 100 / total) as u8, 100);
            if progress >= last_progress + 10 {
                tracing::info!("Download progress: {}%", progress);
                last_progress = progress;
            }
        }
    }

    file.flush().await?;

    tracing::info!("Download complete: {} bytes", downloaded);

    Ok(())
}

/// Download a file and return its contents as bytes.
pub async fn download_bytes(url: &str) -> anyhow::Result<Vec<u8>> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(10))
        // FIX-3: enforce HTTPS, relaxed only for loopback hosts (dev).
        .https_only(require_https_for(url))
        .build()?;

    let response = client.get(url).send().await?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let bytes = response.bytes().await?;
    Ok(bytes.to_vec())
}

/// Create a progress bar with a standard style.
// Used by download_file_with_visible_progress below.
#[allow(dead_code)]
fn create_progress_bar(total_size: u64) -> ProgressBar {
    let pb = ProgressBar::new(total_size);
    let style = ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("#>-");
    pb.set_style(style);
    pb
}

/// Extract HTTP status code from an anyhow error if it contains one.
fn extract_http_status(err: &anyhow::Error) -> Option<u16> {
    // Try to downcast to reqwest::Error first (proper way)
    if let Some(reqwest_err) = err.downcast_ref::<reqwest::Error>() {
        if let Some(status) = reqwest_err.status() {
            return Some(status.as_u16());
        }
    }

    // Fallback: parse error string (less reliable but works for wrapped errors)
    let error_str = err.to_string();
    // Match patterns like "status: 404" or "status 404" or "HTTP 404"
    if let Some(pos) = error_str.find("status") {
        let rest = &error_str[pos..];
        for word in rest.split_whitespace().skip(1) {
            let trimmed = word.trim_matches(|c: char| !c.is_ascii_digit());
            if let Ok(code) = trimmed.parse::<u16>() {
                if (100..600).contains(&code) {
                    return Some(code);
                }
            }
        }
    }
    None
}

/// Download a file from a URL to a destination path with an optional progress bar.
///
/// # Arguments
/// * `url` - The URL to download from
/// * `dest` - The destination path to save the file
/// * `progress_bar` - Optional progress bar to update during download
///
/// Returns the number of bytes downloaded.
pub async fn download_file_with_progress(
    url: &str,
    dest: &Path,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<u64> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(10))
        // FIX-3: enforce HTTPS, relaxed only for loopback hosts (dev).
        .https_only(require_https_for(url))
        .build()?;

    // Retry logic with exponential backoff
    let mut all_errors: Vec<String> = Vec::new();
    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            let backoff_ms = std::cmp::min(
                INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1),
                MAX_BACKOFF_MS,
            );
            tracing::warn!(
                "Retrying download after {} ms (attempt {}/{})",
                backoff_ms,
                attempt + 1,
                MAX_RETRIES
            );
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
        }

        match download_attempt(&client, url, dest, progress_bar).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                let error_msg = format!("Attempt {}: {}", attempt + 1, e);
                tracing::warn!("Download attempt {} failed: {}", attempt + 1, e);

                // Check if this is a permanent error that shouldn't be retried
                if let Some(status_code) = extract_http_status(&e) {
                    if PERMANENT_ERROR_CODES.contains(&status_code) {
                        tracing::error!(
                            "Permanent error (HTTP {}), not retrying: {}",
                            status_code,
                            e
                        );
                        all_errors.push(error_msg);
                        break;
                    }
                }

                all_errors.push(error_msg);
            }
        }
    }

    // Log all errors for debugging - always log every error for troubleshooting
    tracing::error!(
        "Download failed after {} attempt(s). Error history:",
        all_errors.len()
    );
    for error in &all_errors {
        tracing::error!("  - {}", error);
    }

    // Clean up partial file on final failure to prevent leaving corrupted files
    if dest.exists() {
        tracing::warn!(
            "Cleaning up partial download file after final failure: {}",
            dest.display()
        );
        if let Err(cleanup_err) = tokio::fs::remove_file(dest).await {
            tracing::warn!(
                "Failed to clean up partial file {}: {}",
                dest.display(),
                cleanup_err
            );
        }
    }

    // Include all errors in the returned error message for context
    let error_summary = if all_errors.len() == 1 {
        all_errors[0].clone()
    } else {
        format!(
            "Last error: {}. See logs for all {} errors.",
            all_errors.last().unwrap_or(&"Unknown error".to_string()),
            all_errors.len()
        )
    };

    Err(anyhow::anyhow!(
        "Download failed after {} attempt(s): {}",
        all_errors.len(),
        error_summary
    ))
}

/// Verify partial download integrity before resuming.
///
/// # P3 Issue 93 Fix: Download Resume Trusts Size
///
/// When resuming a download, we cannot fully verify the partial file's integrity
/// without storing checksums of partial data. However, we can perform basic validation:
/// - Check if the file size is reasonable (not larger than expected)
/// - Compute a checksum of the partial file for logging/debugging
///
/// If the partial file appears corrupted or unreasonably large, we recommend
/// starting fresh rather than risking corruption propagation.
async fn verify_partial_download(dest: &Path, expected_total_size: Option<u64>) -> anyhow::Result<bool> {
    let existing_size = tokio::fs::metadata(dest).await?.len();

    // If we know the expected total size, validate the partial file isn't larger
    if let Some(total) = expected_total_size {
        if existing_size > total {
            tracing::warn!(
                "Partial download file is larger than expected total size ({} > {}). \
                 This indicates corruption. Starting download fresh.",
                existing_size,
                total
            );
            return Ok(false);
        }
    }

    // Compute hash of partial file for integrity tracking
    // This allows us to detect if the partial file gets corrupted between attempts
    let partial_hash = hash_file(dest)?;
    tracing::debug!(
        "Partial download: {} bytes, hash: {}",
        existing_size,
        partial_hash
    );

    // For now, we trust the partial file if it's not obviously corrupted
    // A more robust solution would store partial checksums in the database
    Ok(true)
}

/// Single download attempt (internal helper).
///
/// File handles are properly managed using RAII patterns to prevent descriptor leaks.
/// On any error path, the file handle is explicitly dropped before returning.
async fn download_attempt(
    client: &Client,
    url: &str,
    dest: &Path,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<u64> {
    // Check if partial download exists
    let existing_size = if dest.exists() {
        tokio::fs::metadata(dest).await?.len()
    } else {
        0
    };

    tracing::info!("Downloading {} to {}", url, dest.display());

    // P3 Issue 93 Fix: Verify partial download before resuming
    // First, make a HEAD request to get the expected total size
    let expected_total_size = if existing_size > 0 {
        match client.head(url).send().await {
            Ok(head_response) => head_response.content_length(),
            Err(e) => {
                tracing::debug!("HEAD request failed, continuing with download: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Verify partial download integrity if resuming
    if existing_size > 0 {
        match verify_partial_download(dest, expected_total_size).await {
            Ok(true) => {
                tracing::info!("Resuming download from byte {}", existing_size);
            }
            Ok(false) => {
                // Partial file is corrupted, start fresh
                tracing::warn!("Removing corrupted partial download and starting fresh");
                tokio::fs::remove_file(dest).await?;
                // Fall through to download from beginning
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to verify partial download, starting fresh: {}",
                    e
                );
                tokio::fs::remove_file(dest).await?;
            }
        }
    }

    // Re-check size after potential removal
    let existing_size = if dest.exists() {
        tokio::fs::metadata(dest).await?.len()
    } else {
        0
    };

    // Build request, potentially with Range header for resume
    let mut request = client.get(url);
    if existing_size > 0 {
        request = request.header("Range", format!("bytes={}-", existing_size));
    }

    let response = request.send().await?;
    let status = response.status();

    // Handle response status
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        // File is already complete
        tracing::info!("File already fully downloaded");
        if let Some(pb) = progress_bar {
            pb.finish_with_message("Already downloaded");
        }
        return Ok(existing_size);
    }

    if !status.is_success() {
        anyhow::bail!("Download failed with status: {}", status);
    }

    // P3 Issue 95 Fix: Check Content-Type header for binary downloads
    // Warn if the Content-Type is not application/octet-stream
    if let Some(content_type) = response.headers().get("content-type") {
        if let Ok(ct_str) = content_type.to_str() {
            let ct_lower = ct_str.to_lowercase();
            // Accept application/octet-stream or binary content types
            if !ct_lower.contains("application/octet-stream")
                && !ct_lower.contains("application/binary")
                && !ct_lower.contains("binary/octet-stream")
            {
                tracing::warn!(
                    "Downloaded file has unexpected Content-Type: '{}'. \
                     Expected 'application/octet-stream' for binary files. \
                     This may indicate the server is misconfigured or serving incorrect content.",
                    ct_str
                );
            }
        }
    } else {
        tracing::debug!("No Content-Type header in response");
    }

    // Check if server supports range requests
    let is_partial = status == reqwest::StatusCode::PARTIAL_CONTENT;
    let total_size = if is_partial {
        // Parse Content-Range header to get total size
        response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split('/').next_back())
            .and_then(|s| s.parse::<u64>().ok())
    } else {
        response.content_length()
    };

    // Set up progress bar with total size if available
    if let Some(pb) = progress_bar {
        if let Some(total) = total_size {
            pb.set_length(total);
        }
        if is_partial && existing_size > 0 {
            pb.set_position(existing_size);
        }
    }

    // Open file for writing (append if resuming)
    let mut file = if is_partial && existing_size > 0 {
        OpenOptions::new()
            .append(true)
            .open(dest)
            .await
            .with_context(|| format!("Failed to open file for appending: {:?}", dest))?
    } else {
        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create parent directory: {:?}", parent))?;
        }
        File::create(dest)
            .await
            .with_context(|| format!("Failed to create file: {:?}", dest))?
    };

    // Download with progress - use a helper function to ensure file is dropped on error
    let result = download_stream_to_file(&mut file, response, existing_size, is_partial, progress_bar).await;

    // Explicitly drop the file handle before handling errors
    // This ensures the file descriptor is released even on error paths
    drop(file);

    let downloaded = result?;

    if let Some(pb) = progress_bar {
        pb.finish_with_message("Downloaded");
    }

    tracing::info!("Download complete: {} bytes", downloaded);

    Ok(downloaded)
}

/// Helper function to download stream to file, ensuring proper cleanup.
///
/// This function is separated to enable RAII-based file handle management
/// in the calling function, preventing file descriptor leaks on error.
async fn download_stream_to_file(
    file: &mut File,
    response: reqwest::Response,
    existing_size: u64,
    is_partial: bool,
    progress_bar: Option<&ProgressBar>,
) -> anyhow::Result<u64> {
    let mut downloaded = if is_partial { existing_size } else { 0 };
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("Failed to read chunk from stream")?;
        file.write_all(&chunk)
            .await
            .context("Failed to write chunk to file")?;
        downloaded += chunk.len() as u64;

        if let Some(pb) = progress_bar {
            pb.set_position(downloaded);
        }
    }

    file.flush().await.context("Failed to flush file")?;

    Ok(downloaded)
}

/// Download a file with a visible progress bar (for interactive use).
///
/// Creates and displays a progress bar during the download.
// Public API for interactive CLI commands; not called from the daemon path.
#[allow(dead_code)]
pub async fn download_file_with_visible_progress(
    url: &str,
    dest: &Path,
    show_progress: bool,
) -> anyhow::Result<u64> {
    if show_progress {
        // First, make a HEAD request to get the content length for the progress bar
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(10))
            // FIX-3: enforce HTTPS, relaxed only for loopback hosts (dev).
            .https_only(require_https_for(url))
            .build()?;

        let head_response = client.head(url).send().await;
        let total_size = head_response
            .ok()
            .and_then(|r| r.content_length())
            .unwrap_or(0);

        let pb = if total_size > 0 {
            create_progress_bar(total_size)
        } else {
            // Create a spinner for unknown size
            let pb = ProgressBar::new_spinner();
            let style = ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {bytes} ({bytes_per_sec})")
                .unwrap_or_else(|_| ProgressStyle::default_spinner());
            pb.set_style(style);
            pb
        };

        download_file_with_progress(url, dest, Some(&pb)).await
    } else {
        download_file_with_progress(url, dest, None).await
    }
}

/// Create a multi-progress container for downloading multiple files.
// Public API for batch download commands.
#[allow(dead_code)]
pub fn create_multi_progress() -> MultiProgress {
    MultiProgress::new()
}

/// Add a progress bar to a multi-progress container.
// Public API for batch download commands.
#[allow(dead_code)]
pub fn add_progress_bar(multi: &MultiProgress, total_size: u64, name: &str) -> ProgressBar {
    let pb = multi.add(ProgressBar::new(total_size));
    let style = ProgressStyle::default_bar()
        .template(&format!(
            "{{spinner:.green}} {} [{{elapsed_precise}}] [{{bar:30.cyan/blue}}] {{bytes}}/{{total_bytes}} ({{bytes_per_sec}}, {{eta}})",
            name
        ))
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("#>-");
    pb.set_style(style);
    pb
}
