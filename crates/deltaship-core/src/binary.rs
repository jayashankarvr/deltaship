use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::DeltashipError;

/// Unique identifier for a binary
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BinaryId(Uuid);

impl BinaryId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for BinaryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for BinaryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for BinaryId {
    type Err = DeltashipError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| DeltashipError::InvalidBinaryId(e.to_string()))
    }
}

/// Validated binary name (alphanumeric + hyphens only)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BinaryName(String);

impl BinaryName {
    pub fn new(name: &str) -> Result<Self, DeltashipError> {
        if name.is_empty() {
            return Err(DeltashipError::InvalidBinaryName(
                "name cannot be empty".to_string(),
            ));
        }

        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(DeltashipError::InvalidBinaryName(
                "name must contain only alphanumeric characters and hyphens".to_string(),
            ));
        }

        if name.starts_with('-') || name.ends_with('-') {
            return Err(DeltashipError::InvalidBinaryName(
                "name cannot start or end with a hyphen".to_string(),
            ));
        }

        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BinaryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for BinaryName {
    type Err = DeltashipError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Target platform for binaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    LinuxX86_64,
    LinuxAarch64,
    WindowsX86_64,
    MacosX86_64,
    MacosAarch64,
}

impl Platform {
    /// Returns the platform string identifier
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::LinuxX86_64 => "linux-x86_64",
            Platform::LinuxAarch64 => "linux-aarch64",
            Platform::WindowsX86_64 => "windows-x86_64",
            Platform::MacosX86_64 => "macos-x86_64",
            Platform::MacosAarch64 => "macos-aarch64",
        }
    }

    /// Returns all supported platform identifiers (canonical forms with hyphens).
    ///
    /// # P3 Issue #105 Fix: Platform List Centralized
    ///
    /// This is the single source of truth for supported platforms in the Deltaship system.
    /// All crates should use this function instead of maintaining their own lists.
    pub const fn all_platforms() -> &'static [&'static str] {
        &[
            "linux-x86_64",
            "linux-aarch64",
            "windows-x86_64",
            "macos-x86_64",
            "macos-aarch64",
        ]
    }

    /// Returns all recognized platform identifiers including alternative forms.
    ///
    /// This includes both hyphen and underscore variants for flexible parsing.
    pub const fn all_platform_variants() -> &'static [&'static str] {
        &[
            "linux-x86_64",
            "linux_x86_64",
            "linux-aarch64",
            "linux_aarch64",
            "windows-x86_64",
            "windows_x86_64",
            "macos-x86_64",
            "macos_x86_64",
            "macos-aarch64",
            "macos_aarch64",
        ]
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Platform {
    type Err = DeltashipError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "linux-x86_64" | "linux_x86_64" => Ok(Platform::LinuxX86_64),
            "linux-aarch64" | "linux_aarch64" => Ok(Platform::LinuxAarch64),
            "windows-x86_64" | "windows_x86_64" => Ok(Platform::WindowsX86_64),
            "macos-x86_64" | "macos_x86_64" => Ok(Platform::MacosX86_64),
            "macos-aarch64" | "macos_aarch64" => Ok(Platform::MacosAarch64),
            _ => Err(DeltashipError::InvalidPlatform(s.to_string())),
        }
    }
}

/// Metadata for a binary artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryMetadata {
    pub id: BinaryId,
    pub name: BinaryName,
    pub platform: Platform,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl BinaryMetadata {
    #[must_use]
    pub fn new(name: BinaryName, platform: Platform) -> Self {
        Self {
            id: BinaryId::new(),
            name,
            platform,
            description: None,
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_id_new() {
        let id1 = BinaryId::new();
        let id2 = BinaryId::new();
        // Each new ID should be unique
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_binary_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let id = BinaryId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    #[test]
    fn test_binary_id_display() {
        let uuid = Uuid::new_v4();
        let id = BinaryId::from_uuid(uuid);
        assert_eq!(format!("{}", id), uuid.to_string());
    }

    #[test]
    fn test_binary_id_from_str() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id: BinaryId = uuid_str.parse().unwrap();
        assert_eq!(format!("{}", id), uuid_str);
    }

    #[test]
    fn test_binary_id_from_str_invalid() {
        let result = "not-a-uuid".parse::<BinaryId>();
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_name_valid() {
        let name = BinaryName::new("myapp").unwrap();
        assert_eq!(name.as_str(), "myapp");
    }

    #[test]
    fn test_binary_name_with_hyphens() {
        let name = BinaryName::new("my-cool-app").unwrap();
        assert_eq!(name.as_str(), "my-cool-app");
    }

    #[test]
    fn test_binary_name_alphanumeric() {
        let name = BinaryName::new("myapp123").unwrap();
        assert_eq!(name.as_str(), "myapp123");
    }

    #[test]
    fn test_binary_name_empty() {
        let result = BinaryName::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_name_invalid_chars() {
        let result = BinaryName::new("my_app");
        assert!(result.is_err());

        let result = BinaryName::new("my.app");
        assert!(result.is_err());

        let result = BinaryName::new("my app");
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_name_leading_hyphen() {
        let result = BinaryName::new("-myapp");
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_name_trailing_hyphen() {
        let result = BinaryName::new("myapp-");
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_name_display() {
        let name = BinaryName::new("myapp").unwrap();
        assert_eq!(format!("{}", name), "myapp");
    }

    #[test]
    fn test_binary_name_from_str() {
        let name: BinaryName = "myapp".parse().unwrap();
        assert_eq!(name.as_str(), "myapp");
    }

    #[test]
    fn test_platform_as_str() {
        assert_eq!(Platform::LinuxX86_64.as_str(), "linux-x86_64");
        assert_eq!(Platform::LinuxAarch64.as_str(), "linux-aarch64");
        assert_eq!(Platform::WindowsX86_64.as_str(), "windows-x86_64");
        assert_eq!(Platform::MacosX86_64.as_str(), "macos-x86_64");
        assert_eq!(Platform::MacosAarch64.as_str(), "macos-aarch64");
    }

    #[test]
    fn test_platform_from_str() {
        assert_eq!(
            "linux-x86_64".parse::<Platform>().unwrap(),
            Platform::LinuxX86_64
        );
        assert_eq!(
            "linux_x86_64".parse::<Platform>().unwrap(),
            Platform::LinuxX86_64
        );
        assert_eq!(
            "linux-aarch64".parse::<Platform>().unwrap(),
            Platform::LinuxAarch64
        );
        assert_eq!(
            "windows-x86_64".parse::<Platform>().unwrap(),
            Platform::WindowsX86_64
        );
        assert_eq!(
            "macos-x86_64".parse::<Platform>().unwrap(),
            Platform::MacosX86_64
        );
        assert_eq!(
            "macos-aarch64".parse::<Platform>().unwrap(),
            Platform::MacosAarch64
        );
    }

    #[test]
    fn test_platform_from_str_case_insensitive() {
        assert_eq!(
            "LINUX-X86_64".parse::<Platform>().unwrap(),
            Platform::LinuxX86_64
        );
        assert_eq!(
            "Windows-X86_64".parse::<Platform>().unwrap(),
            Platform::WindowsX86_64
        );
    }

    #[test]
    fn test_platform_from_str_invalid() {
        let result = "invalid-platform".parse::<Platform>();
        assert!(result.is_err());
    }

    #[test]
    fn test_platform_display() {
        assert_eq!(format!("{}", Platform::LinuxX86_64), "linux-x86_64");
    }

    #[test]
    fn test_binary_metadata_new() {
        let name = BinaryName::new("myapp").unwrap();
        let platform = Platform::LinuxX86_64;
        let metadata = BinaryMetadata::new(name.clone(), platform);

        assert_eq!(metadata.name, name);
        assert_eq!(metadata.platform, platform);
        assert_eq!(metadata.description, None);
    }

    #[test]
    fn test_binary_metadata_with_description() {
        let name = BinaryName::new("myapp").unwrap();
        let platform = Platform::LinuxX86_64;
        let metadata = BinaryMetadata::new(name, platform).with_description("My application");

        assert_eq!(metadata.description, Some("My application".to_string()));
    }
}
