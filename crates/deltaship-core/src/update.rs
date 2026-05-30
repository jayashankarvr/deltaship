use serde::{Deserialize, Serialize};

use crate::binary::Platform;
use crate::error::DeltashipError;
use crate::version::Version;

/// A validated rollout percentage (0-100).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RolloutPercentage(u8);

impl RolloutPercentage {
    /// Create a new rollout percentage, validating it's in range 0-100.
    pub fn new(value: u8) -> Result<Self, DeltashipError> {
        if value > 100 {
            return Err(DeltashipError::InvalidData(format!(
                "Rollout percentage must be 0-100, got {}",
                value
            )));
        }
        Ok(Self(value))
    }

    /// Get the raw percentage value.
    pub fn value(&self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for RolloutPercentage {
    type Error = DeltashipError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Request to check for available updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckRequest {
    pub app_name: String,
    pub current_version: Version,
    pub platform: Platform,
    pub device_id_hash: Option<String>,
}

impl UpdateCheckRequest {
    #[must_use]
    pub fn new(app_name: impl Into<String>, current_version: Version, platform: Platform) -> Self {
        Self {
            app_name: app_name.into(),
            current_version,
            platform,
            device_id_hash: None,
        }
    }

    #[must_use]
    pub fn with_device_id_hash(mut self, hash: impl Into<String>) -> Self {
        self.device_id_hash = Some(hash.into());
        self
    }
}

/// Response to an update check request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResponse {
    pub update_available: bool,
    pub target_version: Option<Version>,
    pub diff_url: Option<String>,
    /// Checksum of the diff file (Blake3 hex).
    pub diff_checksum: Option<String>,
    /// Size of the diff file in bytes.
    pub diff_size: Option<u64>,
    pub full_binary_url: Option<String>,
    /// Size of the full binary in bytes.
    pub full_binary_size: Option<u64>,
    pub checksum: Option<String>,
    pub signature_url: Option<String>,
    pub release_notes: Option<String>,
    pub forced: bool,
    /// Current rollout percentage (0-100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollout_percentage: Option<u8>,
    /// Whether this device is included in the rollout
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_rollout: Option<bool>,
}

impl UpdateCheckResponse {
    /// Create a response indicating no update is available
    #[must_use]
    pub fn no_update() -> Self {
        Self {
            update_available: false,
            target_version: None,
            diff_url: None,
            diff_checksum: None,
            diff_size: None,
            full_binary_url: None,
            full_binary_size: None,
            checksum: None,
            signature_url: None,
            release_notes: None,
            forced: false,
            rollout_percentage: None,
            in_rollout: None,
        }
    }

    /// Create a response indicating an update is available
    #[must_use]
    pub fn available(target_version: Version) -> Self {
        Self {
            update_available: true,
            target_version: Some(target_version),
            diff_url: None,
            diff_checksum: None,
            diff_size: None,
            full_binary_url: None,
            full_binary_size: None,
            checksum: None,
            signature_url: None,
            release_notes: None,
            forced: false,
            rollout_percentage: None,
            in_rollout: None,
        }
    }

    /// Create a response indicating the device is not in rollout.
    #[must_use]
    pub fn not_in_rollout(target_version: Version, percentage: RolloutPercentage) -> Self {
        Self {
            update_available: false,
            target_version: Some(target_version),
            diff_url: None,
            diff_checksum: None,
            diff_size: None,
            full_binary_url: None,
            full_binary_size: None,
            checksum: None,
            signature_url: None,
            release_notes: None,
            forced: false,
            rollout_percentage: Some(percentage.value()),
            in_rollout: Some(false),
        }
    }

    /// Set the diff download URL.
    ///
    /// # URL Validation
    ///
    /// This method accepts any string as a URL. The caller is responsible for validating
    /// that the URL is well-formed and safe before calling this method. Recommended validations:
    /// - Use the `url` crate to parse and validate the URL format
    /// - Ensure the scheme is `https://` (or `http://` for development)
    /// - Check that the URL does not contain embedded credentials or dangerous characters
    #[must_use]
    pub fn with_diff_url(mut self, url: impl Into<String>) -> Self {
        self.diff_url = Some(url.into());
        self
    }

    #[must_use]
    pub fn with_diff_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.diff_checksum = Some(checksum.into());
        self
    }

    #[must_use]
    pub fn with_diff_size(mut self, size: u64) -> Self {
        self.diff_size = Some(size);
        self
    }

    /// Set the full binary download URL.
    ///
    /// # URL Validation
    ///
    /// This method accepts any string as a URL. The caller is responsible for validating
    /// that the URL is well-formed and safe before calling this method. Recommended validations:
    /// - Use the `url` crate to parse and validate the URL format
    /// - Ensure the scheme is `https://` (or `http://` for development)
    /// - Check that the URL does not contain embedded credentials or dangerous characters
    #[must_use]
    pub fn with_full_binary_url(mut self, url: impl Into<String>) -> Self {
        self.full_binary_url = Some(url.into());
        self
    }

    #[must_use]
    pub fn with_full_binary_size(mut self, size: u64) -> Self {
        self.full_binary_size = Some(size);
        self
    }

    #[must_use]
    pub fn with_checksum(mut self, checksum: impl Into<String>) -> Self {
        self.checksum = Some(checksum.into());
        self
    }

    /// Set the signature download URL.
    ///
    /// # URL Validation
    ///
    /// This method accepts any string as a URL. The caller is responsible for validating
    /// that the URL is well-formed and safe before calling this method. Recommended validations:
    /// - Use the `url` crate to parse and validate the URL format
    /// - Ensure the scheme is `https://` (or `http://` for development)
    /// - Check that the URL does not contain embedded credentials or dangerous characters
    #[must_use]
    pub fn with_signature_url(mut self, url: impl Into<String>) -> Self {
        self.signature_url = Some(url.into());
        self
    }

    #[must_use]
    pub fn with_release_notes(mut self, notes: impl Into<String>) -> Self {
        self.release_notes = Some(notes.into());
        self
    }

    #[must_use]
    pub fn with_forced(mut self, forced: bool) -> Self {
        self.forced = forced;
        self
    }

    /// Set the rollout percentage.
    #[must_use]
    pub fn with_rollout_percentage(mut self, percentage: RolloutPercentage) -> Self {
        self.rollout_percentage = Some(percentage.value());
        self
    }

    #[must_use]
    pub fn with_in_rollout(mut self, in_rollout: bool) -> Self {
        self.in_rollout = Some(in_rollout);
        self
    }
}

/// Status of update availability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStatus {
    /// An update is available for this client
    Available,
    /// No update is available (already on latest)
    NotAvailable,
    /// Update exists but client is not in rollout group
    NotInRollout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_check_request_new() {
        let version = Version::new(1, 0, 0);
        let request = UpdateCheckRequest::new("myapp", version.clone(), Platform::LinuxX86_64);

        assert_eq!(request.app_name, "myapp");
        assert_eq!(request.current_version, version);
        assert_eq!(request.platform, Platform::LinuxX86_64);
        assert_eq!(request.device_id_hash, None);
    }

    #[test]
    fn test_update_check_request_with_device_id() {
        let version = Version::new(1, 0, 0);
        let request = UpdateCheckRequest::new("myapp", version, Platform::LinuxX86_64)
            .with_device_id_hash("device123");

        assert_eq!(request.device_id_hash, Some("device123".to_string()));
    }

    #[test]
    fn test_update_check_response_no_update() {
        let response = UpdateCheckResponse::no_update();

        assert!(!response.update_available);
        assert_eq!(response.target_version, None);
        assert_eq!(response.diff_url, None);
        assert_eq!(response.full_binary_url, None);
        assert!(!response.forced);
    }

    #[test]
    fn test_update_check_response_available() {
        let version = Version::new(2, 0, 0);
        let response = UpdateCheckResponse::available(version.clone());

        assert!(response.update_available);
        assert_eq!(response.target_version, Some(version));
        assert_eq!(response.diff_url, None);
        assert_eq!(response.full_binary_url, None);
        assert!(!response.forced);
    }

    #[test]
    fn test_update_check_response_not_in_rollout() {
        let version = Version::new(2, 0, 0);
        let percentage = RolloutPercentage::new(50).unwrap();
        let response = UpdateCheckResponse::not_in_rollout(version.clone(), percentage);

        assert!(!response.update_available);
        assert_eq!(response.target_version, Some(version));
        assert_eq!(response.rollout_percentage, Some(50));
        assert_eq!(response.in_rollout, Some(false));
    }

    #[test]
    fn test_update_check_response_builder() {
        let version = Version::new(2, 0, 0);
        let percentage = RolloutPercentage::new(75).unwrap();
        let response = UpdateCheckResponse::available(version)
            .with_diff_url("https://example.com/diff")
            .with_diff_checksum("abc123")
            .with_diff_size(1024)
            .with_full_binary_url("https://example.com/binary")
            .with_full_binary_size(10240)
            .with_checksum("def456")
            .with_signature_url("https://example.com/sig")
            .with_release_notes("Bug fixes")
            .with_forced(true)
            .with_rollout_percentage(percentage)
            .with_in_rollout(true);

        assert_eq!(
            response.diff_url,
            Some("https://example.com/diff".to_string())
        );
        assert_eq!(response.diff_checksum, Some("abc123".to_string()));
        assert_eq!(response.diff_size, Some(1024));
        assert_eq!(
            response.full_binary_url,
            Some("https://example.com/binary".to_string())
        );
        assert_eq!(response.full_binary_size, Some(10240));
        assert_eq!(response.checksum, Some("def456".to_string()));
        assert_eq!(
            response.signature_url,
            Some("https://example.com/sig".to_string())
        );
        assert_eq!(response.release_notes, Some("Bug fixes".to_string()));
        assert!(response.forced);
        assert_eq!(response.rollout_percentage, Some(75));
        assert_eq!(response.in_rollout, Some(true));
    }

    #[test]
    fn test_update_status_variants() {
        assert_eq!(UpdateStatus::Available, UpdateStatus::Available);
        assert_eq!(UpdateStatus::NotAvailable, UpdateStatus::NotAvailable);
        assert_eq!(UpdateStatus::NotInRollout, UpdateStatus::NotInRollout);
        assert_ne!(UpdateStatus::Available, UpdateStatus::NotAvailable);
    }
}
