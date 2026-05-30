//! Concrete integration tests for version validation security
//!
//! These tests verify that the version validation fixes for Issue #29 work correctly.
//! They exercise the real `validate_version_format` function from `deltaship_client::checker`
//! (re-exported as `pub`) rather than a duplicate implementation, so the tests will
//! catch any drift in the production validator.

#[cfg(test)]
mod version_validation {
    use deltaship_client::checker::validate_version_format;

    /// Convenience adapter so the existing assertion style still reads naturally.
    fn is_valid_version(version: &str) -> bool {
        validate_version_format(version).is_ok()
    }

    #[test]
    fn test_valid_versions_accepted() {
        let valid_versions = vec![
            "1.0.0",
            "2.3.4",
            "1.2.3-alpha",
            "1.2.3_beta1",
            "v1.0.0",
            "1.0.0-rc.1",
            "0.1.0",
            "10.20.30",
        ];

        for version in valid_versions {
            assert!(
                is_valid_version(version),
                "Version '{}' should be accepted",
                version
            );
        }
    }

    #[test]
    fn test_path_traversal_rejected() {
        let malicious_versions = vec![
            "..",
            "../etc/passwd",
            "1.0../evil",
            "../../etc",
        ];

        for version in malicious_versions {
            assert!(
                !is_valid_version(version),
                "Version '{}' should be rejected (path traversal)",
                version
            );
        }
    }

    #[test]
    fn test_consecutive_dots_rejected() {
        let invalid_versions = vec![
            "1..0",
            "1...0",
            "1.0..1",
            "a..b..c",
        ];

        for version in invalid_versions {
            assert!(
                !is_valid_version(version),
                "Version '{}' should be rejected (consecutive dots)",
                version
            );
        }
    }

    #[test]
    fn test_leading_dots_rejected() {
        let invalid_versions = vec![
            ".hidden",
            "..secret",
            ".1.0.0",
        ];

        for version in invalid_versions {
            assert!(
                !is_valid_version(version),
                "Version '{}' should be rejected (leading dot)",
                version
            );
        }
    }

    #[test]
    fn test_trailing_dots_rejected() {
        let invalid_versions = vec![
            "1.0.",
            "version.",
            "1.0.0.",
        ];

        for version in invalid_versions {
            assert!(
                !is_valid_version(version),
                "Version '{}' should be rejected (trailing dot)",
                version
            );
        }
    }

    #[test]
    fn test_empty_version_rejected() {
        assert!(
            !is_valid_version(""),
            "Empty version should be rejected"
        );
    }

    #[test]
    fn test_too_long_version_rejected() {
        let too_long = "a".repeat(65);
        assert!(
            !is_valid_version(&too_long),
            "Version longer than 64 chars should be rejected"
        );
    }

    #[test]
    fn test_special_characters_rejected() {
        let invalid_versions = vec![
            "1.0.0/",
            "1.0.0\\",
            "1.0.0?",
            "1.0.0#",
            "1.0.0\n",
            "1.0.0 ",
            "1 0 0",
            "1.0.0\x00",
        ];

        for version in invalid_versions {
            assert!(
                !is_valid_version(version),
                "Version '{}' should be rejected (special characters)",
                version.escape_default()
            );
        }
    }

    #[test]
    fn test_valid_separators_accepted() {
        let valid_versions = vec![
            "1-0-0",
            "1_0_0",
            "v1.2.3-beta_1",
            "1.0.0-rc-1",
        ];

        for version in valid_versions {
            assert!(
                is_valid_version(version),
                "Version '{}' should be accepted",
                version
            );
        }
    }

    /// Boundary case: exactly 64 chars must be accepted.
    #[test]
    fn test_max_length_boundary_accepted() {
        let exactly_64 = "a".repeat(64);
        assert!(
            is_valid_version(&exactly_64),
            "Version of exactly 64 chars should be accepted"
        );
    }
}
