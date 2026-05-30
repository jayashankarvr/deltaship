//! Application state shared across handlers.
//!
//! # API Key Security
//!
//! API keys are stored as Argon2id hashes in `{data_dir}/api_keys.txt`.
//! The file format is one hash per line (PHC string format), with comments starting with `#`.
//!
//! ## File Format
//!
//! ```text
//! # API keys file - store Argon2id hashes, NOT plaintext keys
//! # Generate a hash using: deltaship-server hash-api-key <your-key>
//! $argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>
//! $argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>
//! ```
//!
//! ## File Permissions
//!
//! The api_keys.txt file should have permissions 0600 (owner read/write only).
//! The server will warn if permissions are too permissive.
//!
//! ## Hash Verification
//!
//! When a request comes in with an API key, the key is hashed using Argon2id
//! with the salt from each stored hash, then compared in constant-time.
//!
//! ## Per-publisher identity (scoped keys)
//!
//! Each entry in `api_keys.txt` is an Argon2id PHC hash, optionally followed by
//! whitespace-separated metadata that scopes the key to a publisher identity:
//!
//! ```text
//! # bare key  -> admin / all-apps (backward compatible, logs a warning)
//! $argon2id$v=19$...$...
//!
//! # scoped key -> owner "acme", may only publish apps "foo" and "bar",
//! #               with an optional Ed25519 public key (hex, 32 bytes) used to
//! #               verify uploaded signatures at publish time.
//! $argon2id$v=19$...$...  owner=acme  apps=foo,bar  pubkey=<64-hex-chars>
//!
//! # apps=*  explicitly grants all-apps to a named owner
//! $argon2id$v=19$...$...  owner=ops  apps=*
//! ```
//!
//! Metadata keys are case-insensitive and order-independent. Unknown keys are
//! ignored with a warning. A key with no `apps=` (or `apps=*`) is treated as
//! all-apps (admin) for that owner.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::rate_limit::AuthFailureLimiter;

/// Identity associated with an authenticated API key.
///
/// Created from `api_keys.txt` metadata. Controls which apps the holder may
/// publish/activate/modify and optionally carries the publisher's Ed25519
/// public key for signature verification at publish time (FIX C).
#[derive(Debug, Clone)]
pub struct Publisher {
    /// Human-readable owner identifier (for logging / auditing).
    pub owner: String,
    /// Set of app names this publisher may modify. `None` means all apps (admin).
    pub allowed_apps: Option<HashSet<String>>,
    /// Optional Ed25519 public key (32 raw bytes) used to verify uploaded
    /// signatures at publish time. `None` means signature verification is skipped.
    pub pubkey: Option<[u8; 32]>,
}

impl Publisher {
    /// Returns true if this publisher is allowed to act on `app_name`.
    pub fn can_publish(&self, app_name: &str) -> bool {
        match &self.allowed_apps {
            None => true, // admin / all-apps
            Some(apps) => apps.contains(app_name),
        }
    }
}

/// Shared application state
#[derive(Debug)]
pub struct AppState {
    /// Directory containing update data
    pub data_dir: PathBuf,
    /// When the server started
    started_at: Instant,
    /// Valid API key hashes (Argon2id PHC format) for authentication
    pub api_keys: HashSet<String>,
    /// Map from API key hash (PHC string) to the publisher identity it grants.
    pub publishers: HashMap<String, Publisher>,
    /// Rate limiter for authentication failures
    pub auth_failure_limiter: AuthFailureLimiter,
    /// When true, derive the client IP from `X-Forwarded-For` (trusted proxy
    /// mode). Default OFF: only the direct socket peer is trusted. See
    /// `rate_limit::client_ip`.
    pub trust_proxy: bool,
}

impl AppState {
    /// Create new application state (trusted-proxy mode OFF).
    ///
    /// Retained for the library API / tests; the binary uses
    /// [`AppState::with_options`].
    #[allow(dead_code)]
    pub fn new(data_dir: PathBuf) -> Self {
        Self::with_options(data_dir, false)
    }

    /// Create new application state with explicit trusted-proxy configuration.
    pub fn with_options(data_dir: PathBuf, trust_proxy: bool) -> Self {
        let publishers = Self::load_api_keys(&data_dir);
        let api_keys = publishers.keys().cloned().collect();
        Self {
            data_dir,
            started_at: Instant::now(),
            api_keys,
            publishers,
            auth_failure_limiter: AuthFailureLimiter::new(),
            trust_proxy,
        }
    }

    /// Load API key hashes from {data_dir}/api_keys.txt (one Argon2id hash per line).
    ///
    /// # File Format
    ///
    /// Each line should contain an Argon2id hash in PHC string format:
    /// `$argon2id$v=19$m=19456,t=2,p=1$<base64-salt>$<base64-hash>`
    ///
    /// Lines starting with `#` are treated as comments.
    /// Empty lines are ignored.
    ///
    /// # Security
    ///
    /// - Only Argon2id hashes are accepted (plaintext keys are rejected)
    /// - File permissions are checked and warned if too permissive
    /// - Each hash is validated before being added to the set
    ///
    /// Returns a map from the Argon2id PHC hash string to the [`Publisher`]
    /// identity it grants. Bare keys (no metadata) yield an all-apps admin
    /// identity for backward compatibility (with a warning).
    fn load_api_keys(data_dir: &Path) -> HashMap<String, Publisher> {
        let api_keys_path = data_dir.join("api_keys.txt");
        if !api_keys_path.exists() {
            tracing::info!("No API keys file found at {}", api_keys_path.display());
            return HashMap::new();
        }

        // Check file permissions on Unix systems
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&api_keys_path) {
                let mode = metadata.permissions().mode();
                // Check if group or others have any permissions
                if mode & 0o077 != 0 {
                    tracing::warn!(
                        "API keys file {} has insecure permissions {:o}. \
                         Recommended: chmod 600 {}",
                        api_keys_path.display(),
                        mode & 0o777,
                        api_keys_path.display()
                    );
                }
            }
        }

        let contents = match fs::read_to_string(&api_keys_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to read API keys file: {}", e);
                return HashMap::new();
            }
        };

        let mut publishers: HashMap<String, Publisher> = HashMap::new();
        let mut valid_count = 0;
        let mut invalid_count = 0;
        let mut bare_count = 0;

        for raw_line in contents.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // The hash is the first whitespace-delimited token; anything after
            // it is scoping metadata (owner=, apps=, pubkey=).
            let mut parts = line.splitn(2, char::is_whitespace);
            let hash = parts.next().unwrap_or("").trim();
            let meta = parts.next().unwrap_or("").trim();

            use argon2::PasswordHash;
            if !hash.starts_with("$argon2id$") || PasswordHash::new(hash).is_err() {
                invalid_count += 1;
                tracing::warn!(
                    "Ignoring invalid API key entry (not a valid Argon2id PHC hash). \
                     Use 'deltaship-server hash-api-key <key>' to generate hashes."
                );
                continue;
            }

            let publisher = if meta.is_empty() {
                bare_count += 1;
                Publisher {
                    owner: "admin".to_string(),
                    allowed_apps: None,
                    pubkey: None,
                }
            } else {
                Self::parse_publisher_meta(meta)
            };

            valid_count += 1;
            publishers.insert(hash.to_string(), publisher);
        }

        if invalid_count > 0 {
            tracing::warn!(
                "Skipped {} invalid entries in API keys file (plaintext keys are not allowed)",
                invalid_count
            );
        }
        if bare_count > 0 {
            tracing::warn!(
                "{} API key(s) have no scope metadata and were granted ALL-APPS ADMIN access. \
                 Recommended: scope keys with 'owner=<name> apps=<app1,app2>' (and optionally \
                 'pubkey=<hex>') in {} to limit blast radius.",
                bare_count,
                api_keys_path.display()
            );
        }

        tracing::info!(
            "Loaded {} valid API key hash(es) from {}",
            valid_count,
            api_keys_path.display()
        );
        publishers
    }

    /// Parse scoping metadata (the part of an api_keys.txt line after the hash)
    /// into a [`Publisher`]. Tolerant: unknown keys are warned and ignored.
    fn parse_publisher_meta(meta: &str) -> Publisher {
        let mut owner = "unknown".to_string();
        let mut allowed_apps: Option<HashSet<String>> = Some(HashSet::new());
        let mut pubkey: Option<[u8; 32]> = None;

        for token in meta.split_whitespace() {
            let (key, value) = match token.split_once('=') {
                Some(kv) => kv,
                None => {
                    tracing::warn!("Ignoring malformed key metadata token (expected key=value): {}", token);
                    continue;
                }
            };
            match key.to_ascii_lowercase().as_str() {
                "owner" => owner = value.to_string(),
                "apps" => {
                    if value == "*" {
                        allowed_apps = None; // all apps
                    } else {
                        let set: HashSet<String> = value
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect();
                        allowed_apps = Some(set);
                    }
                }
                "pubkey" => match hex::decode(value) {
                    Ok(bytes) if bytes.len() == 32 => {
                        let mut arr = [0u8; 32];
                        arr.copy_from_slice(&bytes);
                        pubkey = Some(arr);
                    }
                    _ => {
                        tracing::warn!(
                            owner = %owner,
                            "Ignoring invalid pubkey for publisher (expected 64 hex chars = 32 bytes)"
                        );
                    }
                },
                other => {
                    tracing::warn!("Ignoring unknown key metadata field '{}'", other);
                }
            }
        }

        // If apps= was never specified, default to all-apps for the named owner
        // (an owner with apps= and an empty list is intentionally locked out).
        if let Some(set) = &allowed_apps {
            if set.is_empty() && !meta.to_ascii_lowercase().contains("apps=") {
                allowed_apps = None;
            }
        }

        Publisher {
            owner,
            allowed_apps,
            pubkey,
        }
    }

    /// Get server uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Get the apps directory path
    // Public convenience for route handlers and admin tooling.
    #[allow(dead_code)]
    pub fn apps_dir(&self) -> PathBuf {
        self.data_dir.join("apps")
    }

    /// Get the directory for a specific app
    // Public convenience for route handlers and admin tooling.
    #[allow(dead_code)]
    pub fn app_dir(&self, app_name: &str) -> PathBuf {
        self.apps_dir().join(app_name)
    }
}

#[cfg(test)]
mod publisher_tests {
    use super::*;

    #[test]
    fn scoped_key_limits_apps() {
        let p = AppState::parse_publisher_meta("owner=acme apps=foo,bar");
        assert_eq!(p.owner, "acme");
        assert!(p.can_publish("foo"));
        assert!(p.can_publish("bar"));
        assert!(!p.can_publish("baz"));
        assert!(p.pubkey.is_none());
    }

    #[test]
    fn apps_star_is_all_apps() {
        let p = AppState::parse_publisher_meta("owner=ops apps=*");
        assert!(p.allowed_apps.is_none());
        assert!(p.can_publish("anything"));
    }

    #[test]
    fn owner_without_apps_defaults_all_apps() {
        let p = AppState::parse_publisher_meta("owner=solo");
        assert!(p.allowed_apps.is_none());
        assert!(p.can_publish("anything"));
    }

    #[test]
    fn empty_apps_list_locks_out() {
        // apps= present but empty -> no apps allowed.
        let p = AppState::parse_publisher_meta("owner=locked apps=");
        assert_eq!(p.allowed_apps, Some(HashSet::new()));
        assert!(!p.can_publish("foo"));
    }

    #[test]
    fn valid_pubkey_is_parsed() {
        let hexkey = "ab".repeat(32); // 64 hex chars = 32 bytes
        let p = AppState::parse_publisher_meta(&format!("owner=x apps=app1 pubkey={}", hexkey));
        assert_eq!(p.pubkey, Some([0xabu8; 32]));
    }

    #[test]
    fn invalid_pubkey_is_ignored() {
        let p = AppState::parse_publisher_meta("owner=x apps=app1 pubkey=zzzz");
        assert!(p.pubkey.is_none());
    }
}
