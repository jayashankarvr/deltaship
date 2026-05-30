//! Configuration for Deltaship Client Patcher.
//!
//! # Security Note
//!
//! Signature verification is MANDATORY and cannot be disabled. This is a critical
//! security design decision to prevent arbitrary code execution attacks. All updates
//! MUST be cryptographically signed and verified before being applied to the system.
//!
//! There is no `verify_signatures` configuration option - verification is always enabled
//! and compiled into the client. This prevents users from accidentally or maliciously
//! disabling the security mechanism.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Client configuration loaded from TOML file.
///
/// # Security
///
/// Note that signature verification is not configurable - it is always enabled.
/// All updates are verified cryptographically before being applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// Default server URL for update checks.
    #[serde(default = "default_server_url")]
    pub server_url: String,

    /// Interval between update checks in seconds.
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,

    /// Data directory for client state (database, backups, etc.).
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    /// Disk space check mode.
    ///
    /// # P2 Issue 6 Fix: Configurable disk space check behavior
    ///
    /// - `strict`: Disk space check failures abort the update (safest, default)
    /// - `permissive`: Log warning but continue if disk info cannot be determined
    ///
    /// Set to `permissive` on systems where disk info cannot be reliably retrieved
    /// (e.g., unusual filesystems, containers with limited sysfs access).
    #[serde(default = "default_disk_check_mode")]
    pub disk_check_mode: DiskCheckMode,
}

/// Disk space check mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiskCheckMode {
    /// Strict mode: fail updates if disk space cannot be determined.
    Strict,
    /// Permissive mode: log warning but continue if disk info unavailable.
    Permissive,
}

fn default_disk_check_mode() -> DiskCheckMode {
    DiskCheckMode::Strict
}

/// WARNING: Default uses HTTP for local development only.
/// In production, always configure an HTTPS URL in the config file.
fn default_server_url() -> String {
    "http://localhost:3000".to_string()
}

fn default_check_interval() -> u64 {
    3600 // 1 hour
}

fn default_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("deltaship")
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_url: default_server_url(),
            check_interval_secs: default_check_interval(),
            data_dir: default_data_dir(),
            disk_check_mode: default_disk_check_mode(),
        }
    }
}

impl ClientConfig {
    /// Get the default config file path.
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("deltaship")
            .join("client.toml")
    }

    /// Get the database path.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("client.db")
    }

    /// Get the backups directory.
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Get the downloads directory.
    pub fn downloads_dir(&self) -> PathBuf {
        self.data_dir.join("downloads")
    }
}

/// Returns true if the given URL host is a loopback address for which plain
/// `http://` is acceptable (local development only).
///
/// Recognizes `localhost`, IPv4 `127.0.0.0/8`, and IPv6 `::1`.
pub(crate) fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    // Normalize an IPv6 host that url::Url returns bracketed (e.g. "[::1]").
    let host = host.trim_start_matches('[').trim_end_matches(']');

    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }

    false
}

/// Validate that a server URL uses an http or https scheme, enforcing HTTPS for
/// any non-loopback host.
///
/// # P3 Issue 92 Fix + FIX-3: Server URL Validation / HTTPS enforcement
///
/// Only http and https URLs are allowed. This prevents:
/// - File:// URLs that could read local files
/// - Ftp:// or other protocols that might behave unexpectedly
/// - Malformed URLs that could cause panics
///
/// Additionally, **plain `http://` is now an error for any non-loopback host**.
/// Cleartext HTTP to a remote server exposes update traffic to tampering and
/// eavesdropping, so it is rejected early at config load. Plain `http://` is
/// still permitted for loopback hosts (`localhost`, `127.0.0.0/8`, `::1`) to
/// support local development.
fn validate_server_url(url: &str) -> anyhow::Result<()> {
    // Parse the URL
    let parsed = url::Url::parse(url)
        .map_err(|e| anyhow::anyhow!("Invalid server URL '{}': {}", url, e))?;

    // Only allow http and https schemes
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        anyhow::bail!(
            "Invalid server URL scheme: '{}'. Only 'http' and 'https' are allowed.",
            scheme
        );
    }

    // FIX-3: Require https for any non-loopback host; allow http only for loopback.
    if scheme == "http" && !is_loopback_host(parsed.host_str()) {
        anyhow::bail!(
            "Insecure server URL: '{}'. Plain 'http' is only permitted for loopback hosts \
             (localhost, 127.0.0.1, ::1). Use 'https' for any remote server to protect \
             update traffic against tampering and eavesdropping.",
            url
        );
    }

    Ok(())
}

/// Load configuration from file.
///
/// If path is None, uses the default config path.
/// If the file doesn't exist, returns default configuration.
pub fn load_config(path: Option<&Path>) -> anyhow::Result<ClientConfig> {
    let config_path = path
        .map(PathBuf::from)
        .unwrap_or_else(ClientConfig::default_config_path);

    if !config_path.exists() {
        tracing::info!(
            "Config file not found at {}, using defaults",
            config_path.display()
        );
        let config = ClientConfig::default();
        // Validate default server URL
        validate_server_url(&config.server_url)?;
        return Ok(config);
    }

    let content = std::fs::read_to_string(&config_path)?;
    let config: ClientConfig = toml::from_str(&content)?;

    // P3 Issue 92 Fix: Validate server URL scheme
    validate_server_url(&config.server_url)?;

    tracing::info!("Loaded config from {}", config_path.display());
    Ok(config)
}

/// Save configuration to file.
// Used by install/init commands when writing out the initial config.
#[allow(dead_code)]
pub fn save_config(config: &ClientConfig, path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let content = toml::to_string_pretty(config)?;
    std::fs::write(path, content)?;

    Ok(())
}
