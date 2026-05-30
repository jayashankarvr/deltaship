//! Input validation utilities for security.

use regex::Regex;
use std::sync::LazyLock;

/// Maximum allowed length for names (app names, version strings)
/// Limited to 255 to ensure Windows MAX_PATH compatibility (260 chars total)
/// with room for directory structure
const MAX_NAME_LENGTH: usize = 255;

/// Maximum path length for Windows compatibility (MAX_PATH)
/// https://docs.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
const MAX_PATH_LENGTH: usize = 256;

/// Regex pattern for valid names: alphanumeric, dots, hyphens, underscores
static VALID_NAME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9._-]+$").expect("VALID_NAME_REGEX is a valid regex"));

/// Regex pattern for valid version strings (semver-like): alphanumeric, dots, hyphens, plus
/// Allows formats like: 1.0.0, 1.0.0-alpha, 1.0.0-beta.1, 1.0.0+build.123
static VALID_VERSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9._+-]+$").expect("VALID_VERSION_REGEX is a valid regex"));

/// Validation error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Name is empty
    Empty,
    /// Name exceeds maximum length
    TooLong,
    /// Name contains invalid characters
    InvalidCharacters,
    /// Name contains path traversal sequences
    PathTraversal,
    /// Name contains null bytes
    NullByte,
    /// Path exceeds maximum length (Windows MAX_PATH)
    PathTooLong,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Empty => write!(f, "Name cannot be empty"),
            ValidationError::TooLong => write!(f, "Name exceeds maximum length"),
            ValidationError::InvalidCharacters => write!(f, "Name contains invalid characters"),
            ValidationError::PathTraversal => write!(f, "Name contains invalid path sequences"),
            ValidationError::NullByte => write!(f, "Name contains invalid characters"),
            ValidationError::PathTooLong => write!(f, "Path exceeds maximum length (Windows MAX_PATH)"),
        }
    }
}

impl std::error::Error for ValidationError {}

/// Validate an application name.
///
/// Valid names:
/// - Are non-empty
/// - Are at most 255 characters
/// - Contain only `[a-zA-Z0-9._-]`
/// - Do not contain `..`, `/`, `\`, or null bytes
///
/// Returns `Ok(())` if valid, `Err(ValidationError)` otherwise.
pub fn validate_name(name: &str) -> Result<(), ValidationError> {
    // Check for empty
    if name.is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check length
    if name.len() > MAX_NAME_LENGTH {
        return Err(ValidationError::TooLong);
    }

    // Check for null bytes
    if name.contains('\0') {
        return Err(ValidationError::NullByte);
    }

    // Check for path traversal sequences
    if name.contains("..") || name.contains('/') || name.contains('\\') {
        return Err(ValidationError::PathTraversal);
    }

    // Check character validity
    if !VALID_NAME_REGEX.is_match(name) {
        return Err(ValidationError::InvalidCharacters);
    }

    Ok(())
}

/// Validate a version string.
///
/// Valid versions:
/// - Are non-empty
/// - Are at most 255 characters
/// - Contain only `[a-zA-Z0-9._+-]` (semver compatible)
/// - Do not contain `..`, `/`, `\`, or null bytes
///
/// Returns `Ok(())` if valid, `Err(ValidationError)` otherwise.
pub fn validate_version(version: &str) -> Result<(), ValidationError> {
    // Check for empty
    if version.is_empty() {
        return Err(ValidationError::Empty);
    }

    // Check length
    if version.len() > MAX_NAME_LENGTH {
        return Err(ValidationError::TooLong);
    }

    // Check for null bytes
    if version.contains('\0') {
        return Err(ValidationError::NullByte);
    }

    // Check for path traversal sequences
    if version.contains("..") || version.contains('/') || version.contains('\\') {
        return Err(ValidationError::PathTraversal);
    }

    // Check character validity (more permissive for versions)
    if !VALID_VERSION_REGEX.is_match(version) {
        return Err(ValidationError::InvalidCharacters);
    }

    Ok(())
}

/// Validate a file path length for Windows MAX_PATH compatibility.
///
/// Windows has a maximum path length of 260 characters (MAX_PATH).
/// We validate paths are under 256 characters to be safe.
///
/// Returns `Ok(())` if valid, `Err(ValidationError::PathTooLong)` otherwise.
pub fn validate_path_length(path: &std::path::Path) -> Result<(), ValidationError> {
    // Convert path to string to check length
    let path_str = path.to_string_lossy();

    if path_str.len() >= MAX_PATH_LENGTH {
        return Err(ValidationError::PathTooLong);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(validate_name("my-app").is_ok());
        assert!(validate_name("my_app").is_ok());
        assert!(validate_name("my.app").is_ok());
        assert!(validate_name("MyApp123").is_ok());
        assert!(validate_name("a").is_ok());
    }

    #[test]
    fn test_invalid_names() {
        assert_eq!(validate_name(""), Err(ValidationError::Empty));
        assert_eq!(validate_name("../etc"), Err(ValidationError::PathTraversal));
        assert_eq!(validate_name("foo/bar"), Err(ValidationError::PathTraversal));
        assert_eq!(validate_name("foo\\bar"), Err(ValidationError::PathTraversal));
        assert_eq!(validate_name("foo\0bar"), Err(ValidationError::NullByte));
        assert_eq!(validate_name("foo bar"), Err(ValidationError::InvalidCharacters));
        assert_eq!(validate_name("foo@bar"), Err(ValidationError::InvalidCharacters));
    }

    #[test]
    fn test_name_too_long() {
        let long_name = "a".repeat(256);
        assert_eq!(validate_name(&long_name), Err(ValidationError::TooLong));

        let max_name = "a".repeat(255);
        assert!(validate_name(&max_name).is_ok());
    }

    #[test]
    fn test_valid_versions() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("1.0.0-alpha").is_ok());
        assert!(validate_version("1.0.0-beta.1").is_ok());
        assert!(validate_version("1.0.0+build.123").is_ok());
        assert!(validate_version("v1.0.0").is_ok());
    }

    #[test]
    fn test_invalid_versions() {
        assert_eq!(validate_version(""), Err(ValidationError::Empty));
        assert_eq!(validate_version("1.0.0/../etc"), Err(ValidationError::PathTraversal));
        assert_eq!(validate_version("1.0.0/evil"), Err(ValidationError::PathTraversal));
    }
}
