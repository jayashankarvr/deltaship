//! Publish endpoints for uploading new versions.

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    routing::{post, put},
    Json, Router,
};
use base64::Engine;
use std::sync::Arc;
use deltaship_core::Platform;

use crate::models::{
    ActivateRequest, DiffInfo, PublishResponse, RolloutConfig, RolloutRequest, RolloutResponse,
    VersionInfo,
};
use crate::state::AppState;
use crate::storage;
use crate::validation;
use chrono::Utc;

/// Ed25519 signature size in bytes
const ED25519_SIGNATURE_SIZE: usize = 64;

/// Maximum number of multipart fields to prevent DoS attacks
const MAX_MULTIPART_FIELDS: usize = 10;

/// Per-field size limits for multipart uploads.
///
/// FIX A: lowered from 1GB/500MB. Large binary/diff fields are streamed to a
/// temp file (not buffered in RAM) and hashed incrementally, so these caps bound
/// on-disk and bookkeeping cost rather than memory. 512MB/256MB are generous for
/// real binaries while limiting abuse; raise them deliberately if you ship larger
/// artifacts (and keep the global body limit in main.rs in sync).
/// Binary field: 512MB max.
const MAX_BINARY_FIELD_SIZE: u64 = 512 * 1024 * 1024; // 512MB
/// Diff field: 256MB max (binary diffs are typically much smaller than full binaries).
const MAX_DIFF_FIELD_SIZE: u64 = 256 * 1024 * 1024; // 256MB
/// Streaming chunk size used when hashing large fields (64KB).
const STREAM_CHUNK_SIZE: usize = 64 * 1024;
/// Text fields (platform, checksum, algorithm, release_notes): 64KB max
const MAX_TEXT_FIELD_SIZE: usize = 64 * 1024; // 64KB
/// Signature field: 1KB max (Ed25519 signatures are 64 bytes, base64 ~88 chars)
const MAX_SIGNATURE_FIELD_SIZE: usize = 1024; // 1KB

/// Read bytes from a multipart field with size limit enforcement
async fn read_field_bytes_limited(
    field: axum::extract::multipart::Field<'_>,
    max_size: usize,
    field_name: &str,
) -> Result<Vec<u8>, (StatusCode, Json<PublishResponse>)> {
    let bytes = field.bytes().await.map_err(|e| {
        tracing::warn!(error = %e, field = %field_name, "Failed to read field data");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    if bytes.len() > max_size {
        tracing::warn!(
            field = %field_name,
            size = bytes.len(),
            max_size = max_size,
            "Field size exceeds limit"
        );
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(PublishResponse::error(format!(
                "Field '{}' exceeds size limit",
                field_name
            ))),
        ));
    }

    Ok(bytes.to_vec())
}

/// Read text from a multipart field with size limit enforcement
async fn read_field_text_limited(
    field: axum::extract::multipart::Field<'_>,
    max_size: usize,
    field_name: &str,
) -> Result<String, (StatusCode, Json<PublishResponse>)> {
    let bytes = read_field_bytes_limited(field, max_size, field_name).await?;
    String::from_utf8(bytes).map_err(|_| {
        tracing::warn!(field = %field_name, "Field contains invalid UTF-8");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error(format!(
                "Field '{}' contains invalid UTF-8",
                field_name
            ))),
        )
    })
}

/// Result of streaming a large multipart field to a temp file.
struct StreamedField {
    /// Path to the temp file holding the field bytes.
    temp_path: std::path::PathBuf,
    /// BLAKE3 hash computed incrementally while streaming.
    hash: blake3::Hash,
    /// Total number of bytes written.
    size: u64,
}

/// Stream a large multipart field to a temp file under `dir`, computing its
/// BLAKE3 hash incrementally in `STREAM_CHUNK_SIZE` chunks.
///
/// FIX A: this never holds the whole field in memory. Previously the binary was
/// buffered via `field.bytes()` (up to ~1GB) and then copied again. Here we read
/// the field chunk-by-chunk straight into a temp file while feeding each chunk to
/// a `blake3::Hasher`, so peak memory is one chunk regardless of artifact size.
/// The temp file is created in the destination directory so the later rename is
/// atomic (same filesystem). On any error the partial temp file is removed.
async fn stream_field_to_temp(
    mut field: axum::extract::multipart::Field<'_>,
    dir: &std::path::Path,
    max_size: u64,
    field_name: &str,
) -> Result<StreamedField, (StatusCode, Json<PublishResponse>)> {
    use tokio::io::AsyncWriteExt;

    let bad_request = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error(msg.to_string())),
        )
    };
    let server_error = || {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PublishResponse::error("Internal server error")),
        )
    };

    tokio::fs::create_dir_all(dir).await.map_err(|e| {
        tracing::error!(error = %e, dir = %dir.display(), "Failed to create upload directory");
        server_error()
    })?;

    // Unique temp file name in the destination dir.
    let temp_path = dir.join(format!(
        ".upload-{}-{}.tmp",
        field_name,
        uniq_suffix()
    ));

    let file = tokio::fs::File::create(&temp_path).await.map_err(|e| {
        tracing::error!(error = %e, path = %temp_path.display(), "Failed to create temp file");
        server_error()
    })?;
    let mut writer = tokio::io::BufWriter::with_capacity(STREAM_CHUNK_SIZE, file);
    let mut hasher = blake3::Hasher::new();
    let mut total: u64 = 0;

    // Helper to clean up the partial temp file on any failure path.
    async fn cleanup(path: &std::path::Path) {
        let _ = tokio::fs::remove_file(path).await;
    }

    loop {
        match field.chunk().await {
            Ok(Some(chunk)) => {
                total = total.saturating_add(chunk.len() as u64);
                if total > max_size {
                    cleanup(&temp_path).await;
                    tracing::warn!(
                        field = %field_name,
                        size = total,
                        max_size = max_size,
                        "Field size exceeds limit"
                    );
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(PublishResponse::error(format!(
                            "Field '{}' exceeds size limit",
                            field_name
                        ))),
                    ));
                }
                hasher.update(&chunk);
                if let Err(e) = writer.write_all(&chunk).await {
                    cleanup(&temp_path).await;
                    tracing::error!(error = %e, "Failed writing upload chunk");
                    return Err(server_error());
                }
            }
            Ok(None) => break,
            Err(e) => {
                cleanup(&temp_path).await;
                tracing::warn!(error = %e, field = %field_name, "Failed to read field chunk");
                return Err(bad_request("Invalid request"));
            }
        }
    }

    if let Err(e) = writer.flush().await {
        cleanup(&temp_path).await;
        tracing::error!(error = %e, "Failed to flush upload temp file");
        return Err(server_error());
    }

    Ok(StreamedField {
        temp_path,
        hash: hasher.finalize(),
        size: total,
    })
}

/// Generate a short unique suffix for temp file names (random + timestamp).
fn uniq_suffix() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut buf);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}{}", u64::from_le_bytes(buf), nanos)
}

/// Path parameters for version endpoints
#[derive(Debug, serde::Deserialize)]
pub struct VersionPath {
    pub app_name: String,
    pub version: String,
}

/// Path parameters for diff endpoints
#[derive(Debug, serde::Deserialize)]
pub struct DiffPath {
    pub app_name: String,
    pub from_version: String,
    pub to_version: String,
}

/// Publish a new version
///
/// POST /api/v1/apps/{app_name}/versions/{version}
///
/// Multipart form fields:
/// - `binary` - the binary file (required)
/// - `platform` - target platform string (required)
/// - `signature` - Ed25519 signature, base64 or hex (required)
/// - `checksum` - Blake3 hash, hex (required)
/// - `release_notes` - optional text
async fn publish_version(
    State(state): State<Arc<AppState>>,
    Path(path): Path<VersionPath>,
    axum::Extension(publisher): axum::Extension<crate::state::Publisher>,
    mut multipart: Multipart,
) -> Result<Json<PublishResponse>, (StatusCode, Json<PublishResponse>)> {
    let app_name = &path.app_name;
    let version = &path.version;

    // Validate app name to prevent path traversal
    validation::validate_name(app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in publish request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    // Validate version string
    validation::validate_version(version).map_err(|e| {
        tracing::warn!(version = %version, error = %e, "Invalid version in publish request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    // Validate version format (semver)
    let parsed_version: deltaship_core::Version = version.parse().map_err(|e| {
        tracing::warn!(version = %version, error = ?e, "Invalid version format");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid version format")),
        )
    })?;

    // FIX B: enforce per-publisher authorization. The publisher identity was
    // attached by the auth middleware. Admin (all-apps) keys pass; scoped keys
    // may only publish to their allowed app set.
    if !publisher.can_publish(app_name) {
        tracing::warn!(
            owner = %publisher.owner,
            app = %app_name,
            "Publisher not authorized to publish this app"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(PublishResponse::error("Not authorized for this app")),
        ));
    }

    // Stage the streamed binary temp file in the version directory so the final
    // rename is atomic (same filesystem).
    let version_dir = storage::ensure_version_dir(&state.data_dir, app_name, version)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create version directory");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PublishResponse::error("Internal server error")),
            )
        })?;

    // Collect multipart fields
    let mut binary_stream: Option<StreamedField> = None;
    let mut platform_str: Option<String> = None;
    let mut signature_data: Option<Vec<u8>> = None;
    let mut checksum: Option<String> = None;
    let mut release_notes: Option<String> = None;
    let mut field_count: usize = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::warn!(error = %e, "Failed to read multipart field");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })? {
        field_count += 1;

        // Check field count limit to prevent DoS attacks
        if field_count > MAX_MULTIPART_FIELDS {
            tracing::warn!(
                field_count = field_count,
                max_fields = MAX_MULTIPART_FIELDS,
                "Exceeded maximum multipart field count"
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(PublishResponse::error("Too many multipart fields")),
            ));
        }

        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "binary" => {
                // FIX A: stream the binary to a temp file in the version dir,
                // hashing incrementally, instead of buffering up to ~1GB in RAM.
                let streamed = stream_field_to_temp(
                    field,
                    &version_dir,
                    MAX_BINARY_FIELD_SIZE,
                    "binary",
                )
                .await?;
                binary_stream = Some(streamed);
            }
            "platform" => {
                platform_str =
                    Some(read_field_text_limited(field, MAX_TEXT_FIELD_SIZE, "platform").await?);
            }
            "signature" => {
                let sig_text =
                    read_field_text_limited(field, MAX_SIGNATURE_FIELD_SIZE, "signature").await?;
                // Try to decode as base64, fall back to hex
                signature_data = Some(decode_signature(&sig_text).map_err(|e| {
                    tracing::warn!(error = %e, "Invalid signature encoding");
                    (
                        StatusCode::BAD_REQUEST,
                        Json(PublishResponse::error("Invalid signature")),
                    )
                })?);
            }
            "checksum" => {
                checksum =
                    Some(read_field_text_limited(field, MAX_TEXT_FIELD_SIZE, "checksum").await?);
            }
            "release_notes" => {
                let notes =
                    read_field_text_limited(field, MAX_TEXT_FIELD_SIZE, "release_notes").await?;
                if !notes.is_empty() {
                    release_notes = Some(notes);
                }
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // The streamed binary temp file must be cleaned up on any error after this
    // point. `cleanup` removes it best-effort; `fail` is a small helper that
    // cleans up and returns the error tuple.
    let binary_stream = binary_stream.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Missing required field: binary")),
        )
    })?;
    let temp_path = binary_stream.temp_path.clone();
    async fn cleanup(p: &std::path::Path) {
        let _ = tokio::fs::remove_file(p).await;
    }

    // Helper macro: clean up the staged temp file and return an error response.
    macro_rules! bail {
        ($status:expr, $msg:expr) => {{
            cleanup(&temp_path).await;
            return Err(($status, Json(PublishResponse::error($msg))));
        }};
    }

    let platform_str = match platform_str {
        Some(p) => p,
        None => bail!(StatusCode::BAD_REQUEST, "Missing required field: platform"),
    };

    let platform: Platform = match platform_str.parse() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(platform = %platform_str, "Invalid platform");
            bail!(StatusCode::BAD_REQUEST, "Invalid platform");
        }
    };

    let signature_data = match signature_data {
        Some(s) => s,
        None => bail!(StatusCode::BAD_REQUEST, "Missing required field: signature"),
    };

    // Validate signature size (Ed25519 signature = 64 bytes)
    if signature_data.len() != ED25519_SIGNATURE_SIZE {
        tracing::warn!(
            size = signature_data.len(),
            expected = ED25519_SIGNATURE_SIZE,
            "Invalid signature size"
        );
        bail!(StatusCode::BAD_REQUEST, "Invalid signature");
    }

    let checksum = match checksum {
        Some(c) => c,
        None => bail!(StatusCode::BAD_REQUEST, "Missing required field: checksum"),
    };

    // Validate checksum is valid hex (Blake3 hash = 64 hex chars)
    if !checksum.chars().all(|c| c.is_ascii_hexdigit()) || checksum.len() != 64 {
        tracing::warn!("Invalid checksum format");
        bail!(StatusCode::BAD_REQUEST, "Invalid checksum format");
    }

    // Verify the incrementally-computed BLAKE3 hash matches the provided checksum.
    let binary_hash = binary_stream.hash;
    let computed_checksum = binary_hash.to_hex().to_string();
    if computed_checksum.to_lowercase() != checksum.to_lowercase() {
        tracing::warn!(
            provided = %checksum,
            computed = %computed_checksum,
            "Checksum mismatch - binary does not match provided checksum"
        );
        bail!(
            StatusCode::BAD_REQUEST,
            "Checksum verification failed"
        );
    }

    // FIX C: if the authenticated publisher has a registered Ed25519 public key,
    // verify the uploaded signature over the canonical "DELTASHIP-sig-v1" payload:
    //   b"DELTASHIP-sig-v1\x00" ++ 32 raw BLAKE3 bytes ++ version UTF-8 bytes
    // Reject on failure. If no pubkey is configured, skip with a warning.
    match &publisher.pubkey {
        Some(pubkey_bytes) => {
            let payload = deltaship_crypto::signing_payload(binary_hash.as_bytes(), version);

            let mut sig_arr = [0u8; ED25519_SIGNATURE_SIZE];
            sig_arr.copy_from_slice(&signature_data);

            let verify_result = deltaship_crypto::VerifyingKey::from_bytes(pubkey_bytes)
                .map_err(|_| ())
                .and_then(|vk| {
                    vk.verify(&payload, &deltaship_crypto::Signature::from_bytes(sig_arr))
                        .map_err(|_| ())
                });

            if verify_result.is_err() {
                tracing::warn!(
                    owner = %publisher.owner,
                    app = %app_name,
                    version = %version,
                    "Ed25519 signature verification FAILED for publish"
                );
                bail!(StatusCode::BAD_REQUEST, "Signature verification failed");
            }
            tracing::info!(owner = %publisher.owner, "Signature verified for publish");
        }
        None => {
            tracing::warn!(
                owner = %publisher.owner,
                app = %app_name,
                "No publisher public key configured; skipping signature verification. \
                 Configure 'pubkey=<hex>' on this key to enable verification."
            );
        }
    }

    // Move the verified binary into place (atomic rename within the version dir).
    let binary_size = binary_stream.size;
    if let Err(e) =
        storage::save_binary_from_temp(&state.data_dir, app_name, version, &platform, &temp_path)
            .await
    {
        tracing::error!(
            app = %app_name,
            version = %version,
            platform = %platform.as_str(),
            error = %e,
            "Failed to save binary"
        );
        bail!(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
    }

    // Save signature
    storage::save_signature(&state.data_dir, app_name, version, &signature_data)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to save signature");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PublishResponse::error("Internal server error")),
            )
        })?;

    // Update manifest with platform info
    storage::update_manifest(
        &state.data_dir,
        app_name,
        version,
        &platform,
        &checksum,
        binary_size,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to update manifest");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PublishResponse::error("Internal server error")),
        )
    })?;

    // Update catalog
    let version_info = VersionInfo {
        version: parsed_version,
        platforms: vec![platform],
        release_notes,
        forced: false,
        rollout: None,
    };

    storage::update_catalog_atomic(&state.data_dir, app_name, version_info)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update catalog");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PublishResponse::error("Internal server error")),
            )
        })?;

    tracing::info!(
        "Published {}/{} for platform {}",
        app_name,
        version,
        platform.as_str()
    );

    Ok(Json(PublishResponse::success(
        version.to_string(),
        format!(
            "Successfully published version {} for {}",
            version, app_name
        ),
    )))
}

/// Activate a version (make it the latest)
///
/// PUT /api/v1/apps/{app_name}/versions/{version}/activate
async fn activate_version(
    State(state): State<Arc<AppState>>,
    Path(path): Path<VersionPath>,
    axum::Extension(publisher): axum::Extension<crate::state::Publisher>,
    Json(_request): Json<Option<ActivateRequest>>,
) -> Result<Json<PublishResponse>, (StatusCode, Json<PublishResponse>)> {
    let app_name = &path.app_name;
    let version = &path.version;

    // Validate app name
    validation::validate_name(app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in activate request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    // FIX B: enforce per-publisher authorization for activation.
    if !publisher.can_publish(app_name) {
        tracing::warn!(
            owner = %publisher.owner,
            app = %app_name,
            "Publisher not authorized to activate this app"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(PublishResponse::error("Not authorized for this app")),
        ));
    }

    // Validate version
    validation::validate_version(version).map_err(|e| {
        tracing::warn!(version = %version, error = %e, "Invalid version in activate request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    // Set as latest version
    storage::set_latest_version(&state.data_dir, app_name, version)
        .await
        .map_err(|e| {
            let status = match &e {
                storage::StorageError::NotFound(_) => {
                    tracing::warn!(app = %app_name, version = %version, "Version not found");
                    StatusCode::NOT_FOUND
                }
                _ => {
                    tracing::error!(error = %e, "Failed to activate version");
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            };
            let msg = if status == StatusCode::NOT_FOUND {
                "Version not found"
            } else {
                "Internal server error"
            };
            (status, Json(PublishResponse::error(msg)))
        })?;

    tracing::info!("Activated version {} for {}", version, app_name);

    Ok(Json(PublishResponse::success(
        version.to_string(),
        format!("Version {} is now the latest for {}", version, app_name),
    )))
}

/// Upload a diff between two versions
///
/// POST /api/v1/apps/{app_name}/diffs/{from_version}/to/{to_version}
///
/// Multipart form fields:
/// - `diff` - the diff file (required)
/// - `platform` - target platform string (required)
/// - `checksum` - Blake3 hash, hex (required)
/// - `algorithm` - diff algorithm used (bsdiff, courgette, xdelta3), defaults to bsdiff
async fn upload_diff(
    State(state): State<Arc<AppState>>,
    Path(path): Path<DiffPath>,
    axum::Extension(publisher): axum::Extension<crate::state::Publisher>,
    mut multipart: Multipart,
) -> Result<Json<PublishResponse>, (StatusCode, Json<PublishResponse>)> {
    let app_name = &path.app_name;
    let from_version = &path.from_version;
    let to_version = &path.to_version;

    // Validate app name
    validation::validate_name(app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in diff upload request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    // FIX B: enforce per-publisher authorization for diff upload.
    if !publisher.can_publish(app_name) {
        tracing::warn!(
            owner = %publisher.owner,
            app = %app_name,
            "Publisher not authorized to upload diffs for this app"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(PublishResponse::error("Not authorized for this app")),
        ));
    }

    // Validate version strings
    validation::validate_version(from_version).map_err(|e| {
        tracing::warn!(version = %from_version, error = %e, "Invalid from_version in diff upload request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    validation::validate_version(to_version).map_err(|e| {
        tracing::warn!(version = %to_version, error = %e, "Invalid to_version in diff upload request");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })?;

    // Validate version formats (semver)
    let parsed_from: deltaship_core::Version = from_version.parse().map_err(|e| {
        tracing::warn!(version = %from_version, error = ?e, "Invalid from_version format");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid version format")),
        )
    })?;

    let _parsed_to: deltaship_core::Version = to_version.parse().map_err(|e| {
        tracing::warn!(version = %to_version, error = ?e, "Invalid to_version format");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid version format")),
        )
    })?;

    // Ensure the diffs directory exists so we can stage the streamed temp file
    // there (same filesystem -> atomic rename later). FIX A.
    storage::ensure_diffs_dir(&state.data_dir, app_name)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to create diffs directory");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PublishResponse::error("Internal server error")),
            )
        })?;
    let diffs_dir = state.data_dir.join("apps").join(app_name).join("diffs");

    // Collect multipart fields
    let mut diff_stream: Option<StreamedField> = None;
    let mut platform_str: Option<String> = None;
    let mut checksum: Option<String> = None;
    let mut algorithm: String = "bsdiff".to_string();
    let mut field_count: usize = 0;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::warn!(error = %e, "Failed to read multipart field");
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Invalid request")),
        )
    })? {
        field_count += 1;

        // Check field count limit to prevent DoS attacks
        if field_count > MAX_MULTIPART_FIELDS {
            tracing::warn!(
                field_count = field_count,
                max_fields = MAX_MULTIPART_FIELDS,
                "Exceeded maximum multipart field count"
            );
            return Err((
                StatusCode::BAD_REQUEST,
                Json(PublishResponse::error("Too many multipart fields")),
            ));
        }

        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "diff" => {
                // FIX A: stream the diff to a temp file, hashing incrementally.
                let streamed =
                    stream_field_to_temp(field, &diffs_dir, MAX_DIFF_FIELD_SIZE, "diff").await?;
                diff_stream = Some(streamed);
            }
            "platform" => {
                platform_str =
                    Some(read_field_text_limited(field, MAX_TEXT_FIELD_SIZE, "platform").await?);
            }
            "checksum" => {
                checksum =
                    Some(read_field_text_limited(field, MAX_TEXT_FIELD_SIZE, "checksum").await?);
            }
            "algorithm" => {
                let alg =
                    read_field_text_limited(field, MAX_TEXT_FIELD_SIZE, "algorithm").await?;
                // Validate algorithm
                if !["bsdiff", "courgette", "xdelta3"].contains(&alg.as_str()) {
                    tracing::warn!(algorithm = %alg, "Invalid algorithm");
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(PublishResponse::error("Invalid algorithm")),
                    ));
                }
                algorithm = alg;
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // The streamed diff temp file must be cleaned up on any error after this.
    let diff_stream = diff_stream.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(PublishResponse::error("Missing required field: diff")),
        )
    })?;
    let temp_path = diff_stream.temp_path.clone();
    async fn cleanup(p: &std::path::Path) {
        let _ = tokio::fs::remove_file(p).await;
    }
    macro_rules! bail {
        ($status:expr, $msg:expr) => {{
            cleanup(&temp_path).await;
            return Err(($status, Json(PublishResponse::error($msg))));
        }};
    }

    let platform_str = match platform_str {
        Some(p) => p,
        None => bail!(StatusCode::BAD_REQUEST, "Missing required field: platform"),
    };

    let platform: Platform = match platform_str.parse() {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(platform = %platform_str, "Invalid platform");
            bail!(StatusCode::BAD_REQUEST, "Invalid platform");
        }
    };

    let checksum = match checksum {
        Some(c) => c,
        None => bail!(StatusCode::BAD_REQUEST, "Missing required field: checksum"),
    };

    // Validate checksum is valid hex (Blake3 hash = 64 hex chars)
    if !checksum.chars().all(|c| c.is_ascii_hexdigit()) || checksum.len() != 64 {
        tracing::warn!("Invalid checksum format");
        bail!(StatusCode::BAD_REQUEST, "Invalid checksum format");
    }

    // Verify the incrementally-computed BLAKE3 hash matches the provided checksum.
    let computed_checksum = diff_stream.hash.to_hex().to_string();
    if computed_checksum.to_lowercase() != checksum.to_lowercase() {
        tracing::warn!(
            provided = %checksum,
            computed = %computed_checksum,
            "Checksum mismatch - diff does not match provided checksum"
        );
        bail!(StatusCode::BAD_REQUEST, "Checksum verification failed");
    }

    // Move the verified diff into place (atomic rename within the diffs dir).
    let diff_size = diff_stream.size;
    if let Err(e) = storage::save_diff_from_temp(
        &state.data_dir,
        app_name,
        from_version,
        to_version,
        &platform,
        &temp_path,
    )
    .await
    {
        tracing::error!(error = %e, "Failed to save diff");
        bail!(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error");
    }

    // Create diff info and add to manifest
    let diff_info = DiffInfo {
        from_version: parsed_from,
        checksum,
        size: diff_size,
        algorithm,
    };

    storage::add_diff_to_manifest(&state.data_dir, app_name, to_version, &platform, diff_info)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to update manifest with diff info");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PublishResponse::error("Internal server error")),
            )
        })?;

    tracing::info!(
        "Uploaded diff for {}: {} -> {} ({})",
        app_name,
        from_version,
        to_version,
        platform.as_str()
    );

    Ok(Json(PublishResponse::success(
        to_version.to_string(),
        format!(
            "Successfully uploaded diff from {} to {} for {}",
            from_version, to_version, app_name
        ),
    )))
}

/// Decode signature from base64 or hex
fn decode_signature(input: &str) -> Result<Vec<u8>, String> {
    let input = input.trim();

    // Try hex first (128 chars = 64 bytes for Ed25519 signature)
    if input.chars().all(|c| c.is_ascii_hexdigit()) {
        return hex_decode(input).map_err(|e| format!("Hex decode error: {}", e));
    }

    // Try base64 using the base64 crate (more secure than custom implementation)
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| format!("Base64 decode error: {}", e))
}

/// Robust hex decoder using the hex crate
///
/// Uses the well-tested hex crate for robust decoding, avoiding potential
/// panics or edge cases in manual parsing.
fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    hex::decode(input)
        .map_err(|e| format!("Invalid hex string: {}", e))
}

/// Update the rollout configuration for a version
///
/// PUT /api/v1/apps/{app_name}/versions/{version}/rollout
async fn update_rollout(
    State(state): State<Arc<AppState>>,
    Path(path): Path<VersionPath>,
    axum::Extension(publisher): axum::Extension<crate::state::Publisher>,
    Json(request): Json<RolloutRequest>,
) -> Result<Json<RolloutResponse>, (StatusCode, Json<RolloutResponse>)> {
    let app_name = &path.app_name;
    let version = &path.version;

    // Validate app name
    validation::validate_name(app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in rollout request");
        (
            StatusCode::BAD_REQUEST,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Invalid request".to_string(),
            }),
        )
    })?;

    // FIX B: enforce per-publisher authorization for rollout changes.
    if !publisher.can_publish(app_name) {
        tracing::warn!(
            owner = %publisher.owner,
            app = %app_name,
            "Publisher not authorized to change rollout for this app"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Not authorized for this app".to_string(),
            }),
        ));
    }

    // Validate version
    validation::validate_version(version).map_err(|e| {
        tracing::warn!(version = %version, error = %e, "Invalid version in rollout request");
        (
            StatusCode::BAD_REQUEST,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Invalid request".to_string(),
            }),
        )
    })?;

    // Validate percentage is 0-100
    if request.percentage > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Percentage must be between 0 and 100".to_string(),
            }),
        ));
    }

    // Check if version exists
    let manifest = storage::get_version_manifest(&state.data_dir, app_name, version).await;
    if manifest.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Version not found".to_string(),
            }),
        ));
    }

    // Capture the new percentage for the closure
    let new_percentage = request.percentage;

    // Use atomic update to prevent race conditions in read-modify-write
    // The entire operation (read current config, modify, write) happens under a file lock
    let config = storage::update_rollout_config_atomic(
        &state.data_dir,
        app_name,
        version,
        move |mut config| {
            let old_percentage = config.percentage;
            config.percentage = new_percentage;

            // Track rollout lifecycle
            if old_percentage == 0 && new_percentage > 0 && config.started_at.is_none() {
                config.started_at = Some(Utc::now().to_rfc3339());
            }
            if new_percentage >= 100 && config.completed_at.is_none() {
                config.completed_at = Some(Utc::now().to_rfc3339());
            }

            config
        },
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "Failed to update rollout config");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Internal server error".to_string(),
            }),
        )
    })?;

    tracing::info!(
        "Updated rollout for {}/{} to {}%",
        app_name,
        version,
        config.percentage
    );

    Ok(Json(RolloutResponse {
        success: true,
        rollout: config.clone(),
        message: format!(
            "Rollout updated to {}% for version {} of {}",
            config.percentage, version, app_name
        ),
    }))
}

/// Get the current rollout configuration for a version
///
/// GET /api/v1/apps/{app_name}/versions/{version}/rollout
async fn get_rollout(
    State(state): State<Arc<AppState>>,
    Path(path): Path<VersionPath>,
) -> Result<Json<RolloutResponse>, (StatusCode, Json<RolloutResponse>)> {
    let app_name = &path.app_name;
    let version = &path.version;

    // Validate app name
    validation::validate_name(app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in get_rollout request");
        (
            StatusCode::BAD_REQUEST,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Invalid request".to_string(),
            }),
        )
    })?;

    // Validate version
    validation::validate_version(version).map_err(|e| {
        tracing::warn!(version = %version, error = %e, "Invalid version in get_rollout request");
        (
            StatusCode::BAD_REQUEST,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Invalid request".to_string(),
            }),
        )
    })?;

    // Check if version exists
    let manifest = storage::get_version_manifest(&state.data_dir, app_name, version).await;
    if manifest.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(RolloutResponse {
                success: false,
                rollout: RolloutConfig::default(),
                message: "Version not found".to_string(),
            }),
        ));
    }

    // Get config or return default (100% rollout)
    let config =
        storage::get_rollout_config(&state.data_dir, app_name, version).await.unwrap_or_default();

    Ok(Json(RolloutResponse {
        success: true,
        rollout: config,
        message: format!("Rollout config for version {} of {}", version, app_name),
    }))
}

/// Create the publish router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/v1/apps/{app_name}/versions/{version}",
            post(publish_version),
        )
        .route(
            "/api/v1/apps/{app_name}/versions/{version}/activate",
            put(activate_version),
        )
        .route(
            "/api/v1/apps/{app_name}/versions/{version}/rollout",
            put(update_rollout).get(get_rollout),
        )
        .route(
            "/api/v1/apps/{app_name}/diffs/{from_version}/to/{to_version}",
            post(upload_diff),
        )
}
