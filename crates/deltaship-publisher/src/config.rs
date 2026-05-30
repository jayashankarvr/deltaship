//! Configuration constants and helpers for Deltaship Publisher

use std::fmt;
use std::str::FromStr;

/// Deltaship project directory
pub const DELTASHIP_DIR: &str = ".deltaship";

/// Keys directory within Deltaship project
pub const KEYS_DIR: &str = ".deltaship/keys";

/// Database file path
pub const DB_FILE: &str = ".deltaship/publisher.db";

/// Default signing key file path
pub const SIGNING_KEY_FILE: &str = ".deltaship/keys/signing.key";

/// Default public key file path
pub const PUBLIC_KEY_FILE: &str = ".deltaship/keys/public.key";

// Config keys for the database
pub const CONFIG_PUBLIC_KEY: &str = "public_key";
pub const CONFIG_SERVER_URL: &str = "server_url";
pub const CONFIG_PUBLISHER_NAME: &str = "publisher_name";

/// Default server URL
pub const DEFAULT_SERVER_URL: &str = "http://localhost:8080";

/// All valid configuration keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    ServerUrl,
    PublisherName,
    PublicKey,
}

/// All available configuration keys
pub const ALL_CONFIG_KEYS: &[ConfigKey] = &[
    ConfigKey::ServerUrl,
    ConfigKey::PublisherName,
    ConfigKey::PublicKey,
];

impl ConfigKey {
    /// Get the database key string for this config key
    pub fn as_db_key(&self) -> &'static str {
        match self {
            ConfigKey::ServerUrl => CONFIG_SERVER_URL,
            ConfigKey::PublisherName => CONFIG_PUBLISHER_NAME,
            ConfigKey::PublicKey => CONFIG_PUBLIC_KEY,
        }
    }

    /// Check if this key is read-only (cannot be set by user)
    pub fn is_read_only(&self) -> bool {
        matches!(self, ConfigKey::PublicKey)
    }
}

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigKey::ServerUrl => write!(f, "server_url"),
            ConfigKey::PublisherName => write!(f, "publisher_name"),
            ConfigKey::PublicKey => write!(f, "public_key"),
        }
    }
}

impl FromStr for ConfigKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "server_url" => Ok(ConfigKey::ServerUrl),
            "publisher_name" => Ok(ConfigKey::PublisherName),
            "public_key" => Ok(ConfigKey::PublicKey),
            _ => Err(format!(
                "Unknown config key: '{}'. Valid keys are: server_url, publisher_name, public_key",
                s
            )),
        }
    }
}

/// Get the description for a configuration key
pub fn get_config_description(key: &ConfigKey) -> &'static str {
    match key {
        ConfigKey::ServerUrl => "URL of the Deltaship update server",
        ConfigKey::PublisherName => "Name of the publisher (used in metadata)",
        ConfigKey::PublicKey => "Public key for signature verification (read-only)",
    }
}

/// Get the default value for a configuration key
pub fn get_default_value(key: &ConfigKey) -> Option<&'static str> {
    match key {
        ConfigKey::ServerUrl => Some(DEFAULT_SERVER_URL),
        ConfigKey::PublisherName => None,
        ConfigKey::PublicKey => None,
    }
}

/// Validate a configuration value for a given key
pub fn validate_config_value(key: &ConfigKey, value: &str) -> Result<(), String> {
    match key {
        ConfigKey::ServerUrl => {
            // Use proper URL parsing for validation
            let parsed_url = url::Url::parse(value).map_err(|e| {
                format!("Invalid URL format: {}", e)
            })?;

            // Ensure scheme is http or https
            match parsed_url.scheme() {
                "http" | "https" => {}
                scheme => {
                    return Err(format!(
                        "Server URL must use 'http' or 'https' scheme, got '{}'",
                        scheme
                    ));
                }
            }

            // Ensure there's a host
            if parsed_url.host().is_none() {
                return Err("Server URL must have a valid host".to_string());
            }

            // Reject URLs with credentials (security concern)
            if !parsed_url.username().is_empty() || parsed_url.password().is_some() {
                return Err("Server URL should not contain embedded credentials".to_string());
            }

            Ok(())
        }
        ConfigKey::PublisherName => {
            if value.is_empty() {
                return Err("Publisher name cannot be empty".to_string());
            }
            if value.len() > 100 {
                return Err("Publisher name is too long (max 100 characters)".to_string());
            }
            Ok(())
        }
        ConfigKey::PublicKey => {
            Err("Public key is read-only and cannot be set directly".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // ── Constants ────────────────────────────────────────────────────────────

    #[test]
    fn test_constant_values() {
        assert_eq!(DELTASHIP_DIR, ".deltaship");
        assert_eq!(DB_FILE, ".deltaship/publisher.db");
        assert_eq!(SIGNING_KEY_FILE, ".deltaship/keys/signing.key");
        assert_eq!(PUBLIC_KEY_FILE, ".deltaship/keys/public.key");
    }

    #[test]
    fn test_default_server_url_is_localhost() {
        assert!(DEFAULT_SERVER_URL.starts_with("http://localhost"));
    }

    // ── ConfigKey::from_str (parsing) ────────────────────────────────────────

    #[test]
    fn test_parse_server_url() {
        let key = ConfigKey::from_str("server_url").unwrap();
        assert_eq!(key, ConfigKey::ServerUrl);
    }

    #[test]
    fn test_parse_publisher_name() {
        let key = ConfigKey::from_str("publisher_name").unwrap();
        assert_eq!(key, ConfigKey::PublisherName);
    }

    #[test]
    fn test_parse_public_key() {
        let key = ConfigKey::from_str("public_key").unwrap();
        assert_eq!(key, ConfigKey::PublicKey);
    }

    #[test]
    fn test_parse_case_insensitive() {
        assert_eq!(ConfigKey::from_str("SERVER_URL").unwrap(), ConfigKey::ServerUrl);
        assert_eq!(ConfigKey::from_str("Publisher_Name").unwrap(), ConfigKey::PublisherName);
    }

    #[test]
    fn test_parse_unknown_key_returns_error() {
        assert!(ConfigKey::from_str("not_a_key").is_err());
    }

    // ── ConfigKey display ────────────────────────────────────────────────────

    #[test]
    fn test_display_server_url() {
        assert_eq!(ConfigKey::ServerUrl.to_string(), "server_url");
    }

    #[test]
    fn test_display_publisher_name() {
        assert_eq!(ConfigKey::PublisherName.to_string(), "publisher_name");
    }

    #[test]
    fn test_display_public_key() {
        assert_eq!(ConfigKey::PublicKey.to_string(), "public_key");
    }

    // ── as_db_key ────────────────────────────────────────────────────────────

    #[test]
    fn test_as_db_key() {
        assert_eq!(ConfigKey::ServerUrl.as_db_key(), CONFIG_SERVER_URL);
        assert_eq!(ConfigKey::PublisherName.as_db_key(), CONFIG_PUBLISHER_NAME);
        assert_eq!(ConfigKey::PublicKey.as_db_key(), CONFIG_PUBLIC_KEY);
    }

    // ── is_read_only ─────────────────────────────────────────────────────────

    #[test]
    fn test_public_key_is_read_only() {
        assert!(ConfigKey::PublicKey.is_read_only());
    }

    #[test]
    fn test_server_url_not_read_only() {
        assert!(!ConfigKey::ServerUrl.is_read_only());
    }

    #[test]
    fn test_publisher_name_not_read_only() {
        assert!(!ConfigKey::PublisherName.is_read_only());
    }

    // ── ALL_CONFIG_KEYS ──────────────────────────────────────────────────────

    #[test]
    fn test_all_config_keys_count() {
        assert_eq!(ALL_CONFIG_KEYS.len(), 3);
    }

    // ── get_config_description ───────────────────────────────────────────────

    #[test]
    fn test_get_config_description_non_empty() {
        for key in ALL_CONFIG_KEYS {
            let desc = get_config_description(key);
            assert!(!desc.is_empty(), "Description for {:?} should not be empty", key);
        }
    }

    // ── get_default_value ────────────────────────────────────────────────────

    #[test]
    fn test_server_url_has_default() {
        assert!(get_default_value(&ConfigKey::ServerUrl).is_some());
    }

    #[test]
    fn test_publisher_name_no_default() {
        assert!(get_default_value(&ConfigKey::PublisherName).is_none());
    }

    #[test]
    fn test_public_key_no_default() {
        assert!(get_default_value(&ConfigKey::PublicKey).is_none());
    }

    // ── validate_config_value ────────────────────────────────────────────────

    #[test]
    fn test_validate_valid_https_url() {
        assert!(validate_config_value(&ConfigKey::ServerUrl, "https://updates.example.com").is_ok());
    }

    #[test]
    fn test_validate_valid_http_url() {
        assert!(validate_config_value(&ConfigKey::ServerUrl, "http://localhost:8080").is_ok());
    }

    #[test]
    fn test_validate_invalid_url_scheme() {
        assert!(validate_config_value(&ConfigKey::ServerUrl, "ftp://example.com").is_err());
    }

    #[test]
    fn test_validate_url_with_embedded_credentials_rejected() {
        assert!(validate_config_value(
            &ConfigKey::ServerUrl,
            "https://user:pass@example.com"
        )
        .is_err());
    }

    #[test]
    fn test_validate_malformed_url() {
        assert!(validate_config_value(&ConfigKey::ServerUrl, "not a url").is_err());
    }

    #[test]
    fn test_validate_valid_publisher_name() {
        assert!(validate_config_value(&ConfigKey::PublisherName, "acme-corp").is_ok());
    }

    #[test]
    fn test_validate_empty_publisher_name_rejected() {
        assert!(validate_config_value(&ConfigKey::PublisherName, "").is_err());
    }

    #[test]
    fn test_validate_publisher_name_too_long_rejected() {
        let long_name = "a".repeat(101);
        assert!(validate_config_value(&ConfigKey::PublisherName, &long_name).is_err());
    }

    #[test]
    fn test_validate_public_key_always_rejected() {
        assert!(validate_config_value(&ConfigKey::PublicKey, "any-value").is_err());
    }
}
