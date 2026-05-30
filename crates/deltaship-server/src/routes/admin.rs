//! Admin endpoints for server operators.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use std::sync::Arc;

use crate::models::{AppDetails, AppListItem, DeleteVersionResponse, ServerStats, VersionListItem};
use crate::state::AppState;
use crate::stats;
use crate::storage;
use crate::validation;

/// List all registered applications
/// GET /api/v1/admin/apps
async fn list_apps(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AppListItem>>, (StatusCode, String)> {
    tracing::info!(action = "list_apps", "Admin listing all applications");
    storage::list_apps(&state.data_dir)
        .map(Json)
        .map_err(|e| {
            tracing::error!(action = "list_apps", error = %e, "Failed to list apps");
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        })
}

/// Get detailed info about an application
/// GET /api/v1/admin/apps/{app_name}
async fn get_app_details(
    State(state): State<Arc<AppState>>,
    Path(app_name): Path<String>,
) -> Result<Json<AppDetails>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(action = "get_app_details", app = %app_name, error = %e, "Invalid app name");
        (StatusCode::BAD_REQUEST, "Invalid application name".to_string())
    })?;

    tracing::info!(action = "get_app_details", app = %app_name, "Admin retrieving app details");
    storage::get_app_info(&state.data_dir, &app_name)
        .map(Json)
        .map_err(|e| match e {
            storage::StorageError::NotFound(_) => {
                tracing::warn!(action = "get_app_details", app = %app_name, "App not found");
                (StatusCode::NOT_FOUND, "Application not found".to_string())
            }
            _ => {
                tracing::error!(action = "get_app_details", app = %app_name, error = %e, "Failed to get app details");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
        })
}

/// List all versions for an application
/// GET /api/v1/admin/apps/{app_name}/versions
async fn list_versions(
    State(state): State<Arc<AppState>>,
    Path(app_name): Path<String>,
) -> Result<Json<Vec<VersionListItem>>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(action = "list_versions", app = %app_name, error = %e, "Invalid app name");
        (StatusCode::BAD_REQUEST, "Invalid application name".to_string())
    })?;

    tracing::info!(action = "list_versions", app = %app_name, "Admin listing versions");
    storage::list_versions(&state.data_dir, &app_name)
        .map(Json)
        .map_err(|e| match e {
            storage::StorageError::NotFound(_) => {
                tracing::warn!(action = "list_versions", app = %app_name, "App not found");
                (StatusCode::NOT_FOUND, "Application not found".to_string())
            }
            _ => {
                tracing::error!(action = "list_versions", app = %app_name, error = %e, "Failed to list versions");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
        })
}

/// Soft-delete a version (mark as unavailable without removing files)
/// DELETE /api/v1/admin/apps/{app_name}/versions/{version}
async fn delete_version(
    State(state): State<Arc<AppState>>,
    Path((app_name, version)): Path<(String, String)>,
    axum::Extension(publisher): axum::Extension<crate::state::Publisher>,
) -> Result<Json<DeleteVersionResponse>, (StatusCode, String)> {
    // Validate app name to prevent path traversal
    validation::validate_name(&app_name).map_err(|e| {
        tracing::warn!(action = "delete_version", app = %app_name, error = %e, "Invalid app name");
        (StatusCode::BAD_REQUEST, "Invalid application name".to_string())
    })?;

    // FIX B: enforce per-publisher authorization (consistent with publish/activate).
    // Admin (all-apps) keys pass; scoped keys may only delete versions of their apps.
    if !publisher.can_publish(&app_name) {
        tracing::warn!(
            action = "delete_version",
            owner = %publisher.owner,
            app = %app_name,
            "Publisher not authorized to delete this app's versions"
        );
        return Err((StatusCode::FORBIDDEN, "Not authorized for this app".to_string()));
    }

    // Validate version to prevent path traversal
    validation::validate_version(&version).map_err(|e| {
        tracing::warn!(action = "delete_version", version = %version, error = %e, "Invalid version");
        (StatusCode::BAD_REQUEST, "Invalid version".to_string())
    })?;

    tracing::info!(
        action = "delete_version",
        app = %app_name,
        version = %version,
        "Admin deleting version"
    );
    storage::delete_version(&state.data_dir, &app_name, &version)
        .await
        .map(|_| {
            tracing::info!(
                action = "delete_version",
                app = %app_name,
                version = %version,
                "Version successfully deleted"
            );
            Json(DeleteVersionResponse {
                success: true,
                message: format!(
                    "Version {} of {} has been marked as deleted",
                    version, app_name
                ),
            })
        })
        .map_err(|e| match e {
            storage::StorageError::NotFound(_) => {
                tracing::warn!(
                    action = "delete_version",
                    app = %app_name,
                    version = %version,
                    "Version not found"
                );
                (StatusCode::NOT_FOUND, "Version not found".to_string())
            }
            _ => {
                tracing::error!(
                    action = "delete_version",
                    app = %app_name,
                    version = %version,
                    error = %e,
                    "Failed to delete version"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
        })
}

/// Get server-wide statistics
/// GET /api/v1/admin/stats
async fn get_stats(State(state): State<Arc<AppState>>) -> Json<ServerStats> {
    tracing::info!(action = "get_stats", "Admin retrieving server statistics");
    let apps_count = stats::count_apps(&state.data_dir);
    let total_versions = stats::count_versions(&state.data_dir);
    let total_storage_bytes = stats::calculate_storage_usage(&state.data_dir);
    let uptime_seconds = state.uptime_seconds();

    Json(ServerStats {
        apps_count,
        total_versions,
        total_storage_bytes,
        uptime_seconds,
    })
}

/// Create the admin router
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/v1/admin/apps", get(list_apps))
        .route("/api/v1/admin/apps/{app_name}", get(get_app_details))
        .route("/api/v1/admin/apps/{app_name}/versions", get(list_versions))
        .route(
            "/api/v1/admin/apps/{app_name}/versions/{version}",
            delete(delete_version),
        )
        .route("/api/v1/admin/stats", get(get_stats))
}
