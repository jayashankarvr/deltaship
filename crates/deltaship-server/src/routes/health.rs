//! Health check endpoint.

use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Health check handler
async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Create the health router
pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/health", get(health_check))
}
