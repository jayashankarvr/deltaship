//! HTTP client for server communication.
//!
//! # Timeout Behavior
//!
//! The API client uses the following timeout configuration:
//!
//! - **Request timeout**: None - No timeout for uploads to accommodate large binaries.
//!   Upload operations can take as long as needed for large files over slow connections.
//!
//! - **Connection timeout**: 10 seconds - Maximum time to establish a TCP connection.
//!   If the server is unreachable, this prevents indefinite waiting.
//!
//! # Retry Configuration
//!
//! The client implements automatic retries with exponential backoff for transient failures:
//!
//! - **Max retries**: 3 attempts (initial request + 2 retries)
//! - **Initial backoff**: 1 second
//! - **Backoff multiplier**: 2x (exponential)
//! - **Retryable errors**: Network errors, timeouts, 5xx server errors, 429 rate limit
//!
//! # Rate Limiting
//!
//! The client automatically handles HTTP 429 (Too Many Requests) responses with exponential
//! backoff, respecting the server's rate limits.

use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Request payload for publishing a version.
#[derive(Debug, Clone, Serialize)]
pub struct PublishRequest {
    /// Application name.
    pub app_name: String,
    /// Version string (semver).
    pub version: String,
    /// Target platform.
    pub platform: String,
    /// Binary file data.
    pub binary_data: Vec<u8>,
    /// Ed25519 signature (64 bytes).
    pub signature: Vec<u8>,
    /// SHA-256 checksum of the binary.
    pub checksum: Vec<u8>,
}

/// Response from the publish endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct PublishResponse {
    /// Whether the publish was successful.
    pub success: bool,
    /// Version ID assigned by the server.
    pub version_id: Option<String>,
    /// Human-readable message.
    pub message: String,
}

/// HTTP client for communicating with the Deltaship server.
pub struct ApiClient {
    base_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    max_retries: u32,
    initial_backoff_ms: u64,
}

impl ApiClient {
    /// Create a new API client.
    ///
    /// # Timeout Configuration
    ///
    /// The client is configured with:
    /// - Request timeout: None - No limit on upload duration for large binaries
    /// - Connection timeout: 10 seconds for TCP connection establishment
    ///
    /// Upload operations can take as long as needed for large files over slow connections.
    /// The connection timeout ensures that unreachable servers fail fast.
    ///
    /// # Retry Configuration
    ///
    /// The client is configured with:
    /// - Max retries: 3 attempts (initial + 2 retries)
    /// - Initial backoff: 1 second
    /// - Exponential backoff multiplier: 2x
    pub fn new(base_url: String, api_key: Option<String>) -> Self {
        let https_only = !base_url.starts_with("http://");
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            // Reject non-HTTPS connections unless the caller explicitly chose http:// (e.g. localhost dev).
            .https_only(https_only)
            .build()
            .expect("Failed to build HTTP client: invalid TLS configuration");

        Self {
            base_url,
            api_key,
            client,
            max_retries: 3,
            initial_backoff_ms: 1000,
        }
    }

    /// Helper function to determine if an error is retryable.
    fn is_retryable_error(&self, status: reqwest::StatusCode) -> bool {
        // Retry on:
        // - 5xx server errors (temporary server issues)
        // - 429 rate limit (too many requests)
        // - 408 request timeout
        status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS || status == reqwest::StatusCode::REQUEST_TIMEOUT
    }

    /// Execute a request with exponential backoff retry logic.
    async fn execute_with_retry<F, Fut>(
        &self,
        operation_name: &str,
        mut request_builder: F,
    ) -> Result<reqwest::Response, Box<dyn std::error::Error>>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::Response, reqwest::Error>>,
    {
        let mut last_error = None;

        for attempt in 0..self.max_retries {
            match request_builder().await {
                Ok(response) => {
                    let status = response.status();

                    // If successful or non-retryable error, return immediately
                    if status.is_success() || !self.is_retryable_error(status) {
                        return Ok(response);
                    }

                    // Retryable error - log and retry
                    if attempt < self.max_retries - 1 {
                        let backoff_ms = self.initial_backoff_ms * 2u64.pow(attempt);
                        tracing::warn!(
                            operation = %operation_name,
                            status = %status,
                            attempt = attempt + 1,
                            max = self.max_retries,
                            backoff_ms,
                            "Request failed with status, retrying"
                        );
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    } else {
                        // Last attempt failed
                        return Ok(response);
                    }
                }
                Err(e) => {
                    // Network error or timeout
                    if attempt < self.max_retries - 1 {
                        let backoff_ms = self.initial_backoff_ms * 2u64.pow(attempt);
                        tracing::warn!(
                            operation = %operation_name,
                            error = %e,
                            attempt = attempt + 1,
                            max = self.max_retries,
                            backoff_ms,
                            "Request failed with error, retrying"
                        );
                        last_error = Some(e);
                        tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    } else {
                        // Last attempt failed with network error
                        return Err(Box::new(e));
                    }
                }
            }
        }

        // Should not reach here, but handle just in case
        Err(last_error.map(|e| Box::new(e) as Box<dyn std::error::Error>).unwrap_or_else(|| "Max retries exceeded".into()))
    }

    /// Publish a version to the server.
    ///
    /// Uploads the binary file via multipart POST to:
    /// `POST /api/v1/apps/{app_name}/versions/{version}`
    ///
    /// This method implements automatic retry with exponential backoff for transient failures.
    pub async fn publish_version(
        &self,
        request: PublishRequest,
    ) -> Result<PublishResponse, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/api/v1/apps/{}/versions/{}",
            self.base_url.trim_end_matches('/'),
            request.app_name,
            request.version
        );

        // Store request data for retry attempts
        let app_name = request.app_name.clone();
        let version = request.version.clone();
        let platform = request.platform.clone();
        let binary_data = request.binary_data.clone();
        let signature = request.signature.clone();
        let checksum = request.checksum.clone();

        // Execute request with retry logic
        let response = self
            .execute_with_retry("publish_version", || {
                let url = url.clone();
                let app_name = app_name.clone();
                let version = version.clone();
                let platform = platform.clone();
                let binary_data = binary_data.clone();
                let signature = signature.clone();
                let checksum = checksum.clone();
                let api_key = self.api_key.clone();

                async move {
                    // Build multipart form
                    let binary_part = Part::bytes(binary_data)
                        .file_name(format!("{}-{}", app_name, version))
                        .mime_str("application/octet-stream")?;

                    let form = Form::new()
                        .text("platform", platform)
                        .text("signature", hex_encode(&signature))
                        .text("checksum", hex_encode(&checksum))
                        .part("binary", binary_part);

                    // Build request
                    let mut req = self.client.post(&url).multipart(form);

                    // Add API key header if provided
                    if let Some(ref api_key) = api_key {
                        req = req.header("X-API-Key", api_key);
                    }

                    // Send request
                    req.send().await
                }
            })
            .await?;

        // Check status
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Server returned {}: {}", status, body).into());
        }

        // Parse response
        let publish_response: PublishResponse = response.json().await?;
        Ok(publish_response)
    }
}

/// Request payload for publishing a diff.
#[derive(Debug, Clone)]
pub struct PublishDiffRequest {
    /// Application name.
    pub app_name: String,
    /// From version string.
    pub from_version: String,
    /// To version string.
    pub to_version: String,
    /// Target platform.
    pub platform: String,
    /// Diff file data.
    pub diff_data: Vec<u8>,
    /// Blake3 checksum of the diff.
    pub checksum: Vec<u8>,
}

/// Response from the publish diff endpoint.
// Deserialization target for server responses; fields read by callers of publish_diff().
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct PublishDiffResponse {
    /// Whether the publish was successful.
    pub success: bool,
    /// Diff ID assigned by the server.
    pub diff_id: Option<String>,
    /// Human-readable message.
    pub message: String,
}

impl ApiClient {
    /// Publish a diff to the server.
    ///
    /// Uploads the diff file via multipart POST to:
    /// `POST /api/v1/apps/{app_name}/diffs/{from_version}/to/{to_version}`
    ///
    /// This method implements automatic retry with exponential backoff for transient failures.
    pub async fn publish_diff(
        &self,
        request: PublishDiffRequest,
    ) -> Result<PublishDiffResponse, Box<dyn std::error::Error>> {
        let url = format!(
            "{}/api/v1/apps/{}/diffs/{}/to/{}",
            self.base_url.trim_end_matches('/'),
            request.app_name,
            request.from_version,
            request.to_version
        );

        // Store request data for retry attempts
        let app_name = request.app_name.clone();
        let from_version = request.from_version.clone();
        let to_version = request.to_version.clone();
        let platform = request.platform.clone();
        let diff_data = request.diff_data.clone();
        let checksum = request.checksum.clone();

        // Execute request with retry logic
        let response = self
            .execute_with_retry("publish_diff", || {
                let url = url.clone();
                let app_name = app_name.clone();
                let from_version = from_version.clone();
                let to_version = to_version.clone();
                let platform = platform.clone();
                let diff_data = diff_data.clone();
                let checksum = checksum.clone();
                let api_key = self.api_key.clone();

                async move {
                    // Build multipart form
                    let diff_part = Part::bytes(diff_data)
                        .file_name(format!(
                            "{}-{}-to-{}.patch",
                            app_name, from_version, to_version
                        ))
                        .mime_str("application/octet-stream")?;

                    let form = Form::new()
                        .text("platform", platform)
                        .text("checksum", hex_encode(&checksum))
                        .part("diff", diff_part);

                    // Build request
                    let mut req = self.client.post(&url).multipart(form);

                    // Add API key header if provided
                    if let Some(ref api_key) = api_key {
                        req = req.header("X-API-Key", api_key);
                    }

                    // Send request
                    req.send().await
                }
            })
            .await?;

        // Check status
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Server returned {}: {}", status, body).into());
        }

        // Parse response
        let diff_response: PublishDiffResponse = response.json().await?;
        Ok(diff_response)
    }
}

/// Simple hex encoding helper.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── hex_encode ───────────────────────────────────────────────────────────

    #[test]
    fn test_hex_encode_empty() {
        assert_eq!(hex_encode(&[]), "");
    }

    #[test]
    fn test_hex_encode_known_values() {
        assert_eq!(hex_encode(&[0x00, 0xFF, 0xAB]), "00ffab");
    }

    #[test]
    fn test_hex_encode_all_zeros() {
        assert_eq!(hex_encode(&[0u8; 4]), "00000000");
    }

    #[test]
    fn test_hex_encode_single_byte() {
        assert_eq!(hex_encode(&[0x0F]), "0f");
        assert_eq!(hex_encode(&[0xF0]), "f0");
    }

    // ── is_retryable_error ───────────────────────────────────────────────────

    fn client() -> ApiClient {
        // Construct using the public API; we only test pure methods that
        // don't send network requests.
        ApiClient::new("https://example.com".to_string(), None)
    }

    #[test]
    fn test_is_retryable_server_errors() {
        let c = client();
        for code in [500u16, 502, 503, 504] {
            let status = reqwest::StatusCode::from_u16(code).unwrap();
            assert!(
                c.is_retryable_error(status),
                "Expected {} to be retryable",
                code
            );
        }
    }

    #[test]
    fn test_is_retryable_rate_limit() {
        let c = client();
        assert!(c.is_retryable_error(reqwest::StatusCode::TOO_MANY_REQUESTS));
    }

    #[test]
    fn test_is_retryable_request_timeout() {
        let c = client();
        assert!(c.is_retryable_error(reqwest::StatusCode::REQUEST_TIMEOUT));
    }

    #[test]
    fn test_is_not_retryable_success() {
        let c = client();
        for code in [200u16, 201, 204] {
            let status = reqwest::StatusCode::from_u16(code).unwrap();
            assert!(
                !c.is_retryable_error(status),
                "Expected {} to NOT be retryable",
                code
            );
        }
    }

    #[test]
    fn test_is_not_retryable_client_errors() {
        let c = client();
        for code in [400u16, 401, 403, 404, 409, 422] {
            let status = reqwest::StatusCode::from_u16(code).unwrap();
            assert!(
                !c.is_retryable_error(status),
                "Expected {} to NOT be retryable",
                code
            );
        }
    }

    // ── HTTPS-only enforcement ────────────────────────────────────────────────

    #[test]
    fn test_https_only_rejects_http_url() {
        let https_only_client = reqwest::Client::builder()
            .https_only(true)
            .build()
            .unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(async { https_only_client.get("http://example.com").send().await });

        assert!(
            result.is_err(),
            "Expected HTTP request to fail with https_only(true), but got Ok"
        );
    }

    // ── URL construction ─────────────────────────────────────────────────────

    #[test]
    fn test_url_format_publish_version() {
        let base = "https://example.com/";
        let url = format!(
            "{}/api/v1/apps/{}/versions/{}",
            base.trim_end_matches('/'),
            "myapp",
            "1.2.3"
        );
        assert_eq!(url, "https://example.com/api/v1/apps/myapp/versions/1.2.3");
    }

    #[test]
    fn test_url_format_publish_diff() {
        let base = "https://example.com";
        let url = format!(
            "{}/api/v1/apps/{}/diffs/{}/to/{}",
            base.trim_end_matches('/'),
            "myapp",
            "1.0.0",
            "1.1.0"
        );
        assert_eq!(
            url,
            "https://example.com/api/v1/apps/myapp/diffs/1.0.0/to/1.1.0"
        );
    }

    #[test]
    fn test_url_format_trailing_slash_stripped() {
        assert_eq!("https://example.com///".trim_end_matches('/'), "https://example.com");
    }

    // ── PublishRequest fields ─────────────────────────────────────────────────

    #[test]
    fn test_publish_request_clone() {
        let req = PublishRequest {
            app_name: "app".to_string(),
            version: "1.0.0".to_string(),
            platform: "linux-x86_64".to_string(),
            binary_data: vec![1, 2, 3],
            signature: vec![0xAA; 64],
            checksum: vec![0xBB; 32],
        };
        let cloned = req.clone();
        assert_eq!(cloned.app_name, req.app_name);
        assert_eq!(cloned.binary_data, req.binary_data);
        assert_eq!(cloned.signature.len(), 64);
    }
}
