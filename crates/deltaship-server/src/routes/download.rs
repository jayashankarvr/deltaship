//! Download endpoints for binaries, diffs, and signatures.

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use deltaship_core::Platform;

use crate::state::AppState;
use crate::storage;
use crate::validation;

/// Query parameters for platform selection
#[derive(Debug, Deserialize)]
pub struct PlatformQuery {
    pub platform: String,
}

/// Download binary for a specific version
async fn download_binary(
    State(state): State<Arc<AppState>>,
    Path((app_name, version)): Path<(String, String)>,
    Query(query): Query<PlatformQuery>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    // Validate version string
    validation::validate_version(&version).map_err(|e| {
        tracing::warn!(version = %version, error = %e, "Invalid version in download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    let platform: Platform = query.platform.parse().map_err(|_| {
        tracing::warn!(platform = %query.platform, "Invalid platform in download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    let binary_path = storage::get_binary_path(&state.data_dir, &app_name, &version, &platform);

    if !binary_path.exists() {
        return Err((StatusCode::NOT_FOUND, "Resource not found".to_string()));
    }

    // Get expected checksum from manifest for verification
    let expected_checksum = storage::get_version_manifest(&state.data_dir, &app_name, &version)
        .await
        .and_then(|manifest| manifest.platforms.get(&platform).map(|a| a.checksum.clone()));

    serve_file(binary_path, "application/octet-stream", expected_checksum).await
}

/// Download diff between two versions
async fn download_diff(
    State(state): State<Arc<AppState>>,
    Path((app_name, diff_spec)): Path<(String, String)>,
    Query(query): Query<PlatformQuery>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in diff download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    let platform: Platform = query.platform.parse().map_err(|_| {
        tracing::warn!(platform = %query.platform, "Invalid platform in diff download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    // Parse diff spec: "from_version-to-to_version"
    let parts: Vec<&str> = diff_spec.split("-to-").collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Invalid request".to_string()));
    }

    let from_version = parts[0];
    let to_version = parts[1];

    // Validate version strings to prevent path traversal
    validation::validate_version(from_version).map_err(|e| {
        tracing::warn!(from_version = %from_version, error = %e, "Invalid from_version in diff request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    validation::validate_version(to_version).map_err(|e| {
        tracing::warn!(to_version = %to_version, error = %e, "Invalid to_version in diff request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    let diff_path = storage::get_diff_path(
        &state.data_dir,
        &app_name,
        from_version,
        to_version,
        &platform,
    );

    if !diff_path.exists() {
        return Err((StatusCode::NOT_FOUND, "Resource not found".to_string()));
    }

    // Get expected checksum from manifest for verification
    let expected_checksum = storage::get_version_manifest(&state.data_dir, &app_name, to_version)
        .await
        .and_then(|manifest| {
            manifest.platforms.get(&platform).and_then(|artifact| {
                artifact
                    .diffs_from
                    .iter()
                    .find(|d| d.from_version.to_string() == from_version)
                    .map(|d| d.checksum.clone())
            })
        });

    serve_file(diff_path, "application/octet-stream", expected_checksum).await
}

/// Download signature for a specific version
async fn download_signature(
    State(state): State<Arc<AppState>>,
    Path((app_name, version)): Path<(String, String)>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in signature download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    // Validate version string
    validation::validate_version(&version).map_err(|e| {
        tracing::warn!(version = %version, error = %e, "Invalid version in signature download request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    let signature_path = storage::get_signature_path(&state.data_dir, &app_name, &version);

    if !signature_path.exists() {
        return Err((StatusCode::NOT_FOUND, "Resource not found".to_string()));
    }

    // Signatures don't have checksums in the manifest, so pass None
    serve_file(signature_path, "application/octet-stream", None).await
}

/// Helper to serve a file as a response.
///
/// # Arguments
///
/// * `path` - The path to the file to serve
/// * `content_type` - The MIME type to set in the Content-Type header
/// * `expected_checksum` - Optional Blake3 checksum (hex) to verify before serving
///
/// # Checksum Verification
///
/// If `expected_checksum` is provided, this function will (FIX A):
/// 1. Stream the file through `blake3::Hasher` in 64KB chunks (NOT loading the
///    whole file into memory)
/// 2. Compare the computed hash with the expected checksum
/// 3. Return error if mismatch (prevents serving corrupted files)
/// 4. Re-open the file and serve it as a stream
///
/// If `expected_checksum` is `None`, the file is served without verification (streaming).
/// Either way the response body is streamed, so memory stays bounded regardless
/// of artifact size.
///
/// # Note on Timeouts
///
/// This function does not wrap file operations (`File::open`, `file.metadata`) in
/// `tokio::time::timeout()`. This is intentional for the following reasons:
///
/// 1. **Local filesystem operations**: File operations on local filesystems are typically
///    fast and do not block indefinitely. Timeouts are more critical for network I/O.
///
/// 2. **Request-level timeouts**: Axum/Hyper provides request-level timeout configuration
///    at the server layer, which covers the entire request lifecycle including file serving.
///
/// 3. **Streaming response**: The actual file content is streamed via `ReaderStream`, which
///    is bounded by client read speed and TCP timeouts rather than server-side timeouts.
///
/// If deploying with network-mounted filesystems (NFS, CIFS, etc.) where operations may
/// hang indefinitely, consider wrapping these calls in `tokio::time::timeout()`:
///
/// ```rust,ignore
/// use tokio::time::{timeout, Duration};
/// let file = timeout(Duration::from_secs(30), File::open(&path)).await??;
/// ```
async fn serve_file(
    path: std::path::PathBuf,
    content_type: &str,
    expected_checksum: Option<String>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // If checksum verification is requested, stream the file through a BLAKE3
    // hasher in 64KB chunks (FIX A) instead of reading the whole artifact into
    // memory. On success we re-open the file and stream it to the client.
    if let Some(expected) = expected_checksum {
        use tokio::io::AsyncReadExt;

        let mut verify_file = File::open(&path).await.map_err(|e| {
            tracing::error!(path = %path.display(), error = %e, "Failed to open file for checksum verification");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;

        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = verify_file.read(&mut buf).await.map_err(|e| {
                tracing::error!(path = %path.display(), error = %e, "Failed to read file during checksum verification");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let computed_checksum = hasher.finalize().to_hex().to_string();

        // Verify checksum matches
        if computed_checksum.to_lowercase() != expected.to_lowercase() {
            tracing::error!(
                path = %path.display(),
                expected = %expected,
                computed = %computed_checksum,
                "Checksum mismatch - file corrupted or tampered"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "File integrity check failed".to_string(),
            ));
        }

        // Checksum verified - re-open and stream the file (do not buffer in RAM).
        let file = File::open(&path).await.map_err(|e| {
            tracing::error!(path = %path.display(), error = %e, "Failed to re-open verified file");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;
        let metadata = file.metadata().await.map_err(|e| {
            tracing::error!(path = %path.display(), error = %e, "Failed to read file metadata");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;
        let stream = ReaderStream::new(file);
        let body = Body::from_stream(stream);
        let response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, metadata.len())
            .header("X-Content-Type-Options", "nosniff")
            .body(body)
            .map_err(|e| {
                tracing::error!(error = %e, "Failed to build response");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            })?;

        return Ok(response);
    }

    // No checksum verification - stream the file
    let file = File::open(&path).await.map_err(|e| {
        tracing::error!(path = %path.display(), error = %e, "Failed to open file");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    let metadata = file.metadata().await.map_err(|e| {
        tracing::error!(path = %path.display(), error = %e, "Failed to read file metadata");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error".to_string(),
        )
    })?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, metadata.len())
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to build response");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
            )
        })?;

    Ok(response)
}

/// Create the download router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/apps/{app_name}/versions/{version}/binary",
            get(download_binary),
        )
        .route(
            "/api/v1/apps/{app_name}/diffs/{diff_spec}",
            get(download_diff),
        )
        .route(
            "/api/v1/apps/{app_name}/versions/{version}/signature",
            get(download_signature),
        )
}
