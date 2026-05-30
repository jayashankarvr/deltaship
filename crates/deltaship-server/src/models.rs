//! Server-specific models for catalog and version manifests.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use deltaship_core::{Platform, Version};

/// Application catalog listing available versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCatalog {
    /// Name of the application
    pub app_name: String,
    /// List of available versions (newest first)
    pub versions: Vec<VersionInfo>,
    /// The latest stable version
    pub latest_version: Version,
}

/// Rollout configuration for gradual updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutConfig {
    /// Percentage of users to receive the update (0-100)
    pub percentage: u8,
    /// Whether the rollout is enabled
    pub enabled: bool,
    /// When the rollout started (ISO 8601 timestamp)
    pub started_at: Option<String>,
    /// When the rollout completed (reached 100%)
    pub completed_at: Option<String>,
}

impl Default for RolloutConfig {
    fn default() -> Self {
        Self {
            percentage: 100,
            enabled: true,
            started_at: None,
            completed_at: None,
        }
    }
}

/// Basic version information in catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Version string
    pub version: Version,
    /// Supported platforms for this version
    pub platforms: Vec<Platform>,
    /// Release notes (optional)
    pub release_notes: Option<String>,
    /// Whether this is a forced update
    #[serde(default)]
    pub forced: bool,
    /// Rollout configuration for gradual updates
    #[serde(default)]
    pub rollout: Option<RolloutConfig>,
}

/// Detailed manifest for a specific version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    /// Version string
    pub version: Version,
    /// Platform-specific information
    pub platforms: HashMap<Platform, PlatformArtifact>,
}

/// Platform-specific artifact information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformArtifact {
    /// Blake3 checksum of the binary
    pub checksum: String,
    /// File size in bytes
    pub size: u64,
    /// Available diffs from previous versions
    #[serde(default)]
    pub diffs_from: Vec<DiffInfo>,
}

/// Information about an available diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffInfo {
    /// Source version for the diff
    pub from_version: Version,
    /// Blake3 checksum of the diff file
    pub checksum: String,
    /// File size in bytes
    pub size: u64,
    /// Diff algorithm used (bsdiff, courgette, xdelta3)
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
}

/// Returns the default diff algorithm used when not specified.
///
/// # Default: "bsdiff"
///
/// The `bsdiff` algorithm is used as the default because:
/// - Excellent compression ratios for binary diffs (typically 1-10% of full binary size)
/// - Widely supported and well-tested algorithm
/// - Good balance between compression ratio and patch application speed
///
/// Other supported algorithms may include "courgette" and "xdelta3", which can be
/// specified explicitly in the `DiffInfo.algorithm` field when publishing diffs.
fn default_algorithm() -> String {
    "bsdiff".to_string()
}

/// Response from publish endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
    /// The version ID that was published
    pub version_id: String,
}

impl PublishResponse {
    /// Create a success response
    pub fn success(version_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
            version_id: version_id.into(),
        }
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            version_id: String::new(),
        }
    }
}

/// Request body for activation endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivateRequest {
    /// Optional rollout percentage (0-100)
    #[serde(default)]
    pub rollout_percentage: Option<u8>,
}

/// Request body for rollout update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutRequest {
    /// Percentage of users to receive the update (0-100)
    pub percentage: u8,
}

/// Response for rollout status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolloutResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// The current rollout configuration
    pub rollout: RolloutConfig,
    /// Human-readable message
    pub message: String,
}

// ============== Admin Models ==============

/// List item for apps endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppListItem {
    /// Application name
    pub name: String,
    /// Number of versions available
    pub versions_count: usize,
    /// Latest version string
    pub latest_version: String,
}

/// Detailed application information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDetails {
    /// Application name
    pub name: String,
    /// All versions available
    pub versions: Vec<String>,
    /// Total storage size in bytes
    pub total_size_bytes: u64,
    /// Platforms supported across all versions
    pub platforms: Vec<String>,
}

/// Version list item with details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionListItem {
    /// Version string
    pub version: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// Platforms supported
    pub platforms: Vec<String>,
    /// Rollout percentage (100 = fully rolled out)
    #[serde(default = "default_rollout")]
    pub rollout_percentage: u8,
    /// Whether this version is deleted/unavailable
    #[serde(default)]
    pub deleted: bool,
    /// Publication timestamp (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
}

fn default_rollout() -> u8 {
    100
}

/// Server-wide statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStats {
    /// Total number of applications
    pub apps_count: usize,
    /// Total number of versions across all apps
    pub total_versions: usize,
    /// Total storage used in bytes
    pub total_storage_bytes: u64,
    /// Server uptime in seconds
    pub uptime_seconds: u64,
}

/// Response for delete version endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteVersionResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Human-readable message
    pub message: String,
}
