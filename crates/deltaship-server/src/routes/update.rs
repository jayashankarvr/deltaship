//! Update check endpoint.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use deltaship_core::{Platform, RolloutPercentage, UpdateCheckResponse, Version};

use crate::state::AppState;
use crate::storage;
use crate::validation;

/// Query parameters for update check
#[derive(Debug, Deserialize)]
pub struct UpdateCheckQuery {
    /// Client's current version. If omitted, the server returns the latest version as an update.
    pub current_version: Option<String>,
    pub platform: String,
    /// Optional device ID hash for rollout determination
    pub device_id_hash: Option<String>,
}

/// Check if a device is included in a rollout based on consistent hashing.
///
/// Uses BLAKE3 to hash the device ID and determines rollout inclusion by
/// checking if the hash value (mod 100) is less than the rollout percentage.
/// This provides consistent, deterministic rollout behavior per device.
fn device_in_rollout(device_id: &str, percentage: u8) -> bool {
    // Short-circuit for 0% (no one gets it) and 100% (everyone gets it)
    if percentage == 0 {
        return false;
    }
    if percentage >= 100 {
        return true;
    }

    let hash = blake3::hash(device_id.as_bytes());
    // SAFETY: blake3 always returns a 32-byte hash, so taking the first 8 bytes
    // will always succeed. We use expect here as documentation of this invariant.
    let hash_bytes: [u8; 8] = hash.as_bytes()[..8]
        .try_into()
        .expect("blake3 always returns at least 8 bytes");
    let hash_u64 = u64::from_le_bytes(hash_bytes);
    (hash_u64 % 100) < percentage as u64
}

/// Check for updates for an application
async fn check_update(
    State(state): State<Arc<AppState>>,
    Path(app_name): Path<String>,
    Query(query): Query<UpdateCheckQuery>,
) -> Result<Json<UpdateCheckResponse>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(app = %app_name, error = %e, "Invalid app name in update check request");
        (StatusCode::BAD_REQUEST, "Invalid request".to_string())
    })?;

    // Parse platform
    let platform: Platform = query.platform.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            format!("Invalid platform: {}", query.platform),
        )
    })?;

    // Parse current version — treat absent/unknown as 0.0.0 so the server always offers the latest
    let current_version: Version = match &query.current_version {
        Some(v) => v.parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                format!("Invalid version: {}", v),
            )
        })?,
        None => Version::new(0, 0, 0),
    };

    // Load app catalog
    let catalog = storage::get_app_catalog(&state.data_dir, &app_name)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Application not found: {}", app_name),
            )
        })?;

    // Check if current version is latest
    if current_version >= catalog.latest_version {
        return Ok(Json(UpdateCheckResponse::no_update()));
    }

    // Find the latest version info that supports this platform
    let version_info = catalog
        .versions
        .iter()
        .find(|v| v.version == catalog.latest_version && v.platforms.contains(&platform));

    let version_info = match version_info {
        Some(v) => v,
        None => {
            // Latest version doesn't support this platform
            return Ok(Json(UpdateCheckResponse::no_update()));
        }
    };

    // Check rollout configuration
    let version_str = catalog.latest_version.to_string();
    let rollout_config = storage::get_rollout_config(&state.data_dir, &app_name, &version_str).await;

    // If rollout is configured and < 100%, check if device is in rollout
    if let Some(ref config) = rollout_config {
        if config.enabled && config.percentage < 100 {
            // If device_id_hash is provided, use consistent hashing to determine rollout
            if let Some(ref device_id) = query.device_id_hash {
                if !device_in_rollout(device_id, config.percentage) {
                    // Device not in rollout - return not_in_rollout response
                    // Convert percentage to RolloutPercentage (safe since we already validated 0-100)
                    let rollout_pct = RolloutPercentage::new(config.percentage)
                        .unwrap_or_else(|_| RolloutPercentage::new(0).expect("0 is always a valid rollout percentage"));
                    return Ok(Json(UpdateCheckResponse::not_in_rollout(
                        catalog.latest_version.clone(),
                        rollout_pct,
                    )));
                }
            }
            // If no device_id_hash provided, we still return the update but mark rollout info
        }
    }

    // Build URLs
    let base_url = format!("/api/v1/apps/{}", app_name);

    // Check if diff is available
    let manifest = storage::get_version_manifest(&state.data_dir, &app_name, &version_str).await;

    let (diff_url, diff_checksum, diff_size, checksum) = if let Some(ref manifest) = manifest {
        if let Some(artifact) = manifest.platforms.get(&platform) {
            // Find diff info for current version
            let diff_info = artifact
                .diffs_from
                .iter()
                .find(|d| d.from_version == current_version);

            let (diff_url, diff_checksum, diff_size) = if let Some(info) = diff_info {
                (
                    Some(format!(
                        "{}/diffs/{}-to-{}?platform={}",
                        base_url,
                        current_version,
                        version_str,
                        platform.as_str()
                    )),
                    Some(info.checksum.clone()),
                    Some(info.size),
                )
            } else {
                (None, None, None)
            };

            (
                diff_url,
                diff_checksum,
                diff_size,
                Some(artifact.checksum.clone()),
            )
        } else {
            (None, None, None, None)
        }
    } else {
        (None, None, None, None)
    };

    let mut response = UpdateCheckResponse::available(catalog.latest_version.clone())
        .with_full_binary_url(format!(
            "{}/versions/{}/binary?platform={}",
            base_url,
            version_str,
            platform.as_str()
        ))
        .with_signature_url(format!("{}/versions/{}/signature", base_url, version_str))
        .with_forced(version_info.forced);

    if let Some(url) = diff_url {
        response = response.with_diff_url(url);
    }

    if let Some(cs) = diff_checksum {
        response = response.with_diff_checksum(cs);
    }

    if let Some(size) = diff_size {
        response = response.with_diff_size(size);
    }

    if let Some(cs) = checksum {
        response = response.with_checksum(cs);
    }

    if let Some(ref notes) = version_info.release_notes {
        response = response.with_release_notes(notes.clone());
    }

    // Add rollout information to response
    if let Some(ref config) = rollout_config {
        // Convert percentage to RolloutPercentage (safe since we already validated 0-100)
        if let Ok(rollout_pct) = RolloutPercentage::new(config.percentage) {
            response = response.with_rollout_percentage(rollout_pct);
        }
        // If device_id was provided and we got here, device is in rollout
        if query.device_id_hash.is_some() {
            response = response.with_in_rollout(true);
        }
    }

    Ok(Json(response))
}

/// Create the update router
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/v1/apps/{app_name}/check-update", get(check_update))
}
