//! Semantic versioning support for VBDP.
//!
//! # P3 Issue #111 Fix: Build Metadata Handling Documentation
//!
//! ## SemVer 2.0 Compliance
//!
//! This implementation follows [SemVer 2.0 specification](https://semver.org/) with one
//! important note about build metadata:
//!
//! **Per SemVer 2.0 Section 10**: Build metadata MUST be ignored when determining version precedence.
//!
//! This means:
//! - `1.0.0+build1` == `1.0.0+build2` (equal for comparison)
//! - `1.0.0` == `1.0.0+build` (equal for comparison)
//! - `1.0.0+abc` == `1.0.0+xyz` (equal for comparison)
//!
//! Build metadata is preserved and accessible via `build_metadata()` but does not affect
//! version ordering or equality checks. See `test_version_build_metadata_comparison()` for
//! comprehensive test coverage of this behavior.
//!
//! **Why this matters for VBDP**: When the server compares client versions for updates,
//! build metadata differences (e.g., different CI build numbers) won't trigger unnecessary
//! updates. Only the core version (major.minor.patch) and prerelease identifiers affect
//! version precedence.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use crate::error::VbdpError;

/// Semantic version representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version(semver::Version);

impl Version {
    /// Create a new version from major, minor, patch components
    #[must_use]
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self(semver::Version::new(major, minor, patch))
    }

    /// Create a version with prerelease identifier
    pub fn with_prerelease(
        major: u64,
        minor: u64,
        patch: u64,
        prerelease: &str,
    ) -> Result<Self, VbdpError> {
        let pre = semver::Prerelease::new(prerelease)
            .map_err(|e| VbdpError::InvalidVersion(e.to_string()))?;
        let mut v = semver::Version::new(major, minor, patch);
        v.pre = pre;
        Ok(Self(v))
    }

    /// Create a version with build metadata
    pub fn with_build_metadata(
        major: u64,
        minor: u64,
        patch: u64,
        build: &str,
    ) -> Result<Self, VbdpError> {
        let build_meta = semver::BuildMetadata::new(build)
            .map_err(|e| VbdpError::InvalidVersion(e.to_string()))?;
        let mut v = semver::Version::new(major, minor, patch);
        v.build = build_meta;
        Ok(Self(v))
    }

    pub fn major(&self) -> u64 {
        self.0.major
    }

    pub fn minor(&self) -> u64 {
        self.0.minor
    }

    pub fn patch(&self) -> u64 {
        self.0.patch
    }

    pub fn prerelease(&self) -> &str {
        self.0.pre.as_str()
    }

    pub fn build_metadata(&self) -> &str {
        self.0.build.as_str()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Version {
    type Err = VbdpError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        semver::Version::parse(s)
            .map(Self)
            .map_err(|e| VbdpError::InvalidVersion(e.to_string()))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        // According to semver spec, build metadata should be ignored for equality
        // Two versions are equal if they have the same major.minor.patch and prerelease
        self.0.major == other.0.major
            && self.0.minor == other.0.minor
            && self.0.patch == other.0.patch
            && self.0.pre == other.0.pre
        // Build metadata is intentionally ignored per semver spec
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare versions according to semver spec: ignore build metadata
        // Build metadata should NOT affect precedence (semver spec section 10)
        // https://semver.org/#spec-item-10

        // Compare major.minor.patch
        match self.0.major.cmp(&other.0.major) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.0.minor.cmp(&other.0.minor) {
            Ordering::Equal => {}
            ord => return ord,
        }
        match self.0.patch.cmp(&other.0.patch) {
            Ordering::Equal => {}
            ord => return ord,
        }

        // Compare prerelease (empty prerelease is greater than any prerelease)
        match (&self.0.pre.is_empty(), &other.0.pre.is_empty()) {
            (true, false) => return Ordering::Greater,
            (false, true) => return Ordering::Less,
            (true, true) => return Ordering::Equal,
            (false, false) => {}
        }

        // Both have prerelease - compare them
        self.0.pre.cmp(&other.0.pre)

        // Build metadata is intentionally ignored per semver spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_new() {
        let v = Version::new(1, 2, 3);
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 2);
        assert_eq!(v.patch(), 3);
    }

    #[test]
    fn test_version_from_str() {
        let v: Version = "1.2.3".parse().unwrap();
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 2);
        assert_eq!(v.patch(), 3);
    }

    #[test]
    fn test_version_from_str_invalid() {
        let result = "not-a-version".parse::<Version>();
        assert!(result.is_err());
    }

    #[test]
    fn test_version_display() {
        let v = Version::new(1, 2, 3);
        assert_eq!(format!("{}", v), "1.2.3");
    }

    #[test]
    fn test_version_comparison() {
        let v1 = Version::new(1, 0, 0);
        let v2 = Version::new(1, 0, 1);
        let v3 = Version::new(1, 1, 0);
        let v4 = Version::new(2, 0, 0);

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert!(v3 < v4);
    }

    #[test]
    fn test_version_equality() {
        let v1 = Version::new(1, 2, 3);
        let v2 = Version::new(1, 2, 3);
        let v3 = Version::new(1, 2, 4);

        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_version_with_prerelease() {
        let v = Version::with_prerelease(1, 0, 0, "alpha.1").unwrap();
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 0);
        assert_eq!(v.patch(), 0);
        assert_eq!(v.prerelease(), "alpha.1");
        assert_eq!(format!("{}", v), "1.0.0-alpha.1");
    }

    #[test]
    fn test_version_with_build_metadata() {
        let v = Version::with_build_metadata(1, 0, 0, "20231201").unwrap();
        assert_eq!(v.major(), 1);
        assert_eq!(v.minor(), 0);
        assert_eq!(v.patch(), 0);
        assert_eq!(v.build_metadata(), "20231201");
        assert_eq!(format!("{}", v), "1.0.0+20231201");
    }

    #[test]
    fn test_version_prerelease_ordering() {
        let v1: Version = "1.0.0-alpha".parse().unwrap();
        let v2: Version = "1.0.0-beta".parse().unwrap();
        let v3: Version = "1.0.0".parse().unwrap();

        assert!(v1 < v2);
        assert!(v2 < v3);
    }

    #[test]
    fn test_version_parse_complex() {
        let v: Version = "2.1.3-beta.2+build.456".parse().unwrap();
        assert_eq!(v.major(), 2);
        assert_eq!(v.minor(), 1);
        assert_eq!(v.patch(), 3);
        assert_eq!(v.prerelease(), "beta.2");
        assert_eq!(v.build_metadata(), "build.456");
    }

    #[test]
    fn test_version_build_metadata_comparison() {
        // According to SemVer 2.0 spec section 10, build metadata MUST be ignored
        // when determining version precedence. We now implement this correctly.

        let v1: Version = "1.0.0+build1".parse().unwrap();
        let v2: Version = "1.0.0+build2".parse().unwrap();

        // Build metadata is ignored for comparison per semver spec
        assert_eq!(
            v1.cmp(&v2),
            Ordering::Equal,
            "Versions with different build metadata should compare as equal"
        );
        assert_eq!(v1, v2, "Versions with different build metadata should be equal");

        // Build metadata accessors should still show the different values
        assert_ne!(
            v1.build_metadata(),
            v2.build_metadata(),
            "Build metadata should be different"
        );

        // Version without build metadata vs with build metadata
        let v3: Version = "1.0.0".parse().unwrap();
        let v4: Version = "1.0.0+build".parse().unwrap();

        // Build metadata is ignored per semver spec
        assert_eq!(
            v3.cmp(&v4),
            Ordering::Equal,
            "Empty vs non-empty build metadata should compare as equal"
        );
        assert_eq!(
            v3, v4,
            "Version without build metadata should equal version with build metadata"
        );
    }
}
