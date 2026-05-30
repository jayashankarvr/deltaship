//! Update checker for Deltaship Client.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use deltaship_core::{UpdateCheckResponse, Version};
use deltaship_db::DbManagedBinary;

use crate::config::ClientConfig;

/// Maximum allowed size for release notes in bytes (64 KB).
///
/// This limit prevents memory exhaustion from malicious servers sending
/// excessively large release notes. 64 KB is generous for text content
/// while still providing reasonable protection.
const MAX_RELEASE_NOTES_SIZE: usize = 64 * 1024;

/// Determine if an error should be retried.
///
/// # P1-5 Fix: Retry Logic for Transient Errors
///
/// Returns true for transient errors that may succeed on retry:
/// - Network timeouts and connection failures
/// - 429 (Too Many Requests)
/// - 5xx server errors
///
/// Returns false for permanent errors that won't succeed on retry:
/// - 4xx client errors (except 429)
/// - Parse/validation errors
fn should_retry_error(err: &anyhow::Error) -> bool {
    // Try to extract HTTP status code from the error
    if let Some(reqwest_err) = err.downcast_ref::<reqwest::Error>() {
        // Check for network/timeout errors (always retry)
        if reqwest_err.is_timeout() || reqwest_err.is_connect() {
            return true;
        }

        // Check HTTP status codes
        if let Some(status) = reqwest_err.status() {
            let code = status.as_u16();

            // Retry on 429 (rate limiting) and 5xx (server errors)
            if code == 429 || (500..600).contains(&code) {
                return true;
            }

            // Don't retry on other 4xx errors (client errors)
            if (400..500).contains(&code) {
                return false;
            }
        }
    }

    // Fallback: parse error string for status codes
    let error_str = err.to_string().to_lowercase();

    // Check for timeout/connection errors in error message
    if error_str.contains("timeout") || error_str.contains("connection") {
        return true;
    }

    // Check for status codes in error message
    for word in error_str.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| !c.is_ascii_digit());
        if let Ok(code) = trimmed.parse::<u16>() {
            if (100..600).contains(&code) {
                // Found a status code
                if code == 429 || (500..600).contains(&code) {
                    return true;
                }
                if PERMANENT_ERROR_CODES.contains(&code) {
                    return false;
                }
            }
        }
    }

    // For unknown errors, retry (conservative approach)
    true
}

/// URL-encode a string for use in URL path segments.
///
/// This uses percent-encoding to escape characters that have special meaning in URLs
/// (like `/`, `?`, `#`, etc.) to prevent URL structure corruption and potential
/// injection attacks.
fn url_encode_path_segment(s: &str) -> String {
    // Encode all characters except alphanumeric, hyphen, underscore, period, and tilde
    // These are the "unreserved" characters in RFC 3986
    use std::fmt::Write;

    let mut encoded = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
            encoded.push(c);
        } else {
            // Percent-encode the character
            for byte in c.to_string().as_bytes() {
                write!(encoded, "%{:02X}", byte).unwrap();
            }
        }
    }
    encoded
}

/// Maximum allowed length for version strings.
const MAX_VERSION_LENGTH: usize = 64;

/// Validate that a version string has a safe format.
///
/// Validates:
/// - Length is at most MAX_VERSION_LENGTH (64) characters
/// - Contains only alphanumeric characters, dots, hyphens, and underscores
/// - Does not start or end with a dot
/// - Does not contain consecutive dots (prevents path traversal like "..")
///
/// This prevents potential security issues from malformed version strings
/// from the server being used in file paths or other operations.
pub fn validate_version_format(version: &str) -> anyhow::Result<()> {
    if version.is_empty() {
        anyhow::bail!("Version string cannot be empty");
    }

    if version.len() > MAX_VERSION_LENGTH {
        anyhow::bail!(
            "Version string too long: {} characters (max {})",
            version.len(),
            MAX_VERSION_LENGTH
        );
    }

    // Disallow leading or trailing dots
    if version.starts_with('.') || version.ends_with('.') {
        anyhow::bail!(
            "Invalid version format: '{}'. Version strings cannot start or end with a dot.",
            version
        );
    }

    // Disallow consecutive dots (prevents path traversal like "..")
    if version.contains("..") {
        anyhow::bail!(
            "Invalid version format: '{}'. Version strings cannot contain consecutive dots.",
            version
        );
    }

    // Only allow alphanumeric, dots, hyphens, and underscores
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        anyhow::bail!(
            "Invalid version format: '{}'. Only alphanumeric characters, dots, hyphens, and underscores are allowed.",
            version
        );
    }

    Ok(())
}

/// Sanitize release notes from server response.
///
/// Truncates release notes that exceed MAX_RELEASE_NOTES_SIZE to prevent
/// memory issues from malicious or misconfigured servers.
fn sanitize_release_notes(notes: Option<String>) -> Option<String> {
    notes.map(|s| {
        if s.len() > MAX_RELEASE_NOTES_SIZE {
            tracing::warn!(
                "Release notes exceed {} bytes, truncating from {} bytes",
                MAX_RELEASE_NOTES_SIZE,
                s.len()
            );
            // Truncate at a character boundary to avoid invalid UTF-8
            let truncated: String = s.chars().take(MAX_RELEASE_NOTES_SIZE).collect();
            format!("{}... [truncated]", truncated)
        } else {
            s
        }
    })
}

/// Information about an available update.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// Target version.
    pub version: Version,
    /// Version ID from server.
    pub version_id: String,
    /// URL to download the diff patch (if available).
    pub diff_url: Option<String>,
    /// Checksum of the diff file (Blake3 hex).
    pub diff_checksum: Option<String>,
    /// Size of the diff file in bytes.
    pub diff_size: Option<u64>,
    /// URL to download the full binary.
    pub full_binary_url: Option<String>,
    /// Size of the full binary in bytes.
    pub full_binary_size: Option<u64>,
    /// Expected checksum of the final binary (Blake3 hex).
    pub checksum: String,
    /// URL to download the signature.
    pub signature_url: Option<String>,
    /// Whether this update is forced.
    pub forced: bool,
    /// Release notes.
    pub release_notes: Option<String>,
}

/// Server response for update check (extended from core).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerUpdateResponse {
    #[serde(flatten)]
    pub base: UpdateCheckResponse,
    /// Version ID for tracking.
    pub version_id: Option<String>,
}

/// Maximum number of retry attempts for failed update checks.
const MAX_RETRIES: u32 = 3;

/// Initial backoff duration in milliseconds.
const INITIAL_BACKOFF_MS: u64 = 1000;

/// Maximum backoff duration in milliseconds (30 seconds).
const MAX_BACKOFF_MS: u64 = 30_000;

/// HTTP status codes that indicate permanent errors (should not retry).
const PERMANENT_ERROR_CODES: &[u16] = &[
    400, // Bad Request
    401, // Unauthorized
    403, // Forbidden
    404, // Not Found
    405, // Method Not Allowed
    410, // Gone
];

/// Update checker that communicates with the Deltaship server.
pub struct UpdateChecker {
    client: Client,
    config: ClientConfig,
}

impl UpdateChecker {
    /// Create a new update checker.
    ///
    /// # P2 Issue 64 Fix: Rate Limiting on Checks
    ///
    /// The HTTP client is configured with conservative timeouts to prevent
    /// hanging update checks from blocking the system:
    /// - **Overall timeout: 60s** - Maximum time for a complete check request/response cycle.
    ///   This is reduced from the download timeout (300s) since update checks should be quick.
    /// - **Connect timeout: 10s** - Maximum time to establish connection to the server.
    ///
    /// These timeouts apply per check request, providing effective rate limiting by
    /// preventing slow or unresponsive servers from monopolizing resources.
    pub fn new(config: ClientConfig) -> anyhow::Result<Self> {
        use std::time::Duration;

        // FIX-3: Enforce HTTPS for update checks against any non-loopback server.
        // Plain http is only tolerated for loopback hosts (localhost / 127.0.0.0/8
        // / ::1) for local development. Building with https_only(true) makes reqwest
        // refuse to send the check request (and to follow redirects) over cleartext.
        let https_only = match url::Url::parse(&config.server_url) {
            Ok(parsed) => !crate::config::is_loopback_host(parsed.host_str()),
            Err(_) => true,
        };

        // P2 Issue 64 Fix: Use shorter timeout for update checks (60s vs 300s for downloads)
        // Update checks should be fast; this prevents hanging on unresponsive servers
        let client = Client::builder()
            .user_agent("deltaship-client/0.1.0")
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .https_only(https_only)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        Ok(Self { client, config })
    }

    /// Check for updates for a managed binary.
    ///
    /// # P1-5 Fix: Retry Mechanism for Transient Errors
    ///
    /// Implements exponential backoff retry logic:
    /// - **Max attempts**: 3 (1 initial + 2 retries)
    /// - **Initial backoff**: 1 second
    /// - **Max backoff**: 30 seconds
    /// - **Backoff strategy**: Exponential (1s, 2s, 4s...)
    ///
    /// Retries on:
    /// - Network timeouts and connection failures
    /// - 429 (Too Many Requests)
    /// - 5xx server errors
    ///
    /// Does NOT retry on:
    /// - 4xx client errors (except 429)
    /// - Validation/parse errors after successful response
    pub async fn check_for_updates(
        &self,
        binary: &DbManagedBinary,
    ) -> anyhow::Result<Option<UpdateInfo>> {
        let server_url = &self.config.server_url;
        // URL-encode the binary name to prevent URL structure corruption
        // (e.g., if binary_name contains '/', '?', '#', or other special characters)
        let encoded_name = url_encode_path_segment(&binary.binary_name);
        let url = format!(
            "{}/api/v1/apps/{}/check-update",
            server_url.trim_end_matches('/'),
            encoded_name
        );

        // P1-5 Fix: Retry logic with exponential backoff
        let mut all_errors: Vec<String> = Vec::new();

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff_ms = std::cmp::min(
                    INITIAL_BACKOFF_MS * 2u64.pow(attempt - 1),
                    MAX_BACKOFF_MS,
                );
                tracing::warn!(
                    "Retrying update check after {} ms (attempt {}/{})",
                    backoff_ms,
                    attempt + 1,
                    MAX_RETRIES
                );
                tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            }

            match self.check_for_updates_attempt(binary, &url).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let error_msg = format!("Attempt {}: {}", attempt + 1, e);
                    tracing::warn!("Update check attempt {} failed: {}", attempt + 1, e);

                    // Check if this is a permanent error that shouldn't be retried
                    if !should_retry_error(&e) {
                        tracing::error!(
                            "Permanent error detected, not retrying: {}",
                            e
                        );
                        all_errors.push(error_msg);
                        break;
                    }

                    all_errors.push(error_msg);
                }
            }
        }

        // All attempts failed
        tracing::error!(
            "Update check failed after {} attempt(s). Error history:",
            all_errors.len()
        );
        for error in &all_errors {
            tracing::error!("  - {}", error);
        }

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
            "Update check failed after {} attempt(s): {}",
            all_errors.len(),
            error_summary
        ))
    }

    /// Single update check attempt (internal helper).
    async fn check_for_updates_attempt(
        &self,
        binary: &DbManagedBinary,
        url: &str,
    ) -> anyhow::Result<Option<UpdateInfo>> {
        tracing::debug!("Checking for updates at {}", url);

        // Build the request with current version info
        let mut request = self
            .client
            .get(url)
            .query(&[("platform", &binary.platform)]);

        if let Some(ref current_version) = binary.current_version_string {
            request = request.query(&[("current_version", current_version)]);
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            // P1-2 Fix: Limit response body to prevent information leakage
            // Truncate to 200 chars and sanitize control characters
            let sanitized_body: String = body
                .chars()
                .take(200)
                .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
                .collect();
            let truncated = if body.len() > 200 { "... [truncated]" } else { "" };
            anyhow::bail!("Update check failed with status {}: {}{}", status, sanitized_body, truncated);
        }

        let update_response: ServerUpdateResponse = response.json().await?;

        if !update_response.base.update_available {
            tracing::debug!("No update available for {}", binary.binary_name);
            return Ok(None);
        }

        // Extract update info
        let version = update_response
            .base
            .target_version
            .ok_or_else(|| anyhow::anyhow!("Update available but no target version provided"))?;

        // Validate version format from server
        // Only allow alphanumeric characters, dots, hyphens, and underscores
        // Maximum length of 64 characters to prevent abuse
        let version_str = version.to_string();
        validate_version_format(&version_str)?;

        let checksum = update_response
            .base
            .checksum
            .ok_or_else(|| anyhow::anyhow!("Update available but no checksum provided"))?;

        let version_id = update_response
            .version_id
            .unwrap_or_else(|| version_str.clone());

        // Also validate version_id format
        validate_version_format(&version_id)?;

        let base = self.config.server_url.trim_end_matches('/');
        let absolutize = |url: Option<String>| -> Option<String> {
            url.map(|u| if u.starts_with('/') { format!("{}{}", base, u) } else { u })
        };

        let info = UpdateInfo {
            version,
            version_id,
            diff_url: absolutize(update_response.base.diff_url),
            diff_checksum: update_response.base.diff_checksum,
            diff_size: update_response.base.diff_size,
            full_binary_url: absolutize(update_response.base.full_binary_url),
            full_binary_size: update_response.base.full_binary_size,
            checksum,
            signature_url: absolutize(update_response.base.signature_url),
            forced: update_response.base.forced,
            // Sanitize release notes to limit size and prevent memory issues
            release_notes: sanitize_release_notes(update_response.base.release_notes),
        };

        tracing::info!(
            "Update available for {}: {} -> {}",
            binary.binary_name,
            binary.current_version_string.as_deref().unwrap_or("(none)"),
            info.version
        );

        Ok(Some(info))
    }

    /// Check server connectivity.
    pub async fn health_check(&self) -> anyhow::Result<bool> {
        let url = format!(
            "{}/api/v1/health",
            self.config.server_url.trim_end_matches('/')
        );

        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}
