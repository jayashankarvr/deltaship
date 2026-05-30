//! Authentication middleware for API key validation.
//!
//! # Security Model
//!
//! API keys are stored as Argon2id hashes in the server configuration.
//! When validating a request, the provided key is verified against each
//! stored hash using Argon2's built-in constant-time comparison.
//!
//! ## Why Argon2id?
//!
//! - Memory-hard: Resistant to GPU/ASIC brute-force attacks
//! - Time-hard: Configurable iteration count
//! - Side-channel resistant: Built-in constant-time comparison
//! - Industry standard: Winner of the Password Hashing Competition
//!
//! ## Timing Attack Prevention
//!
//! Argon2's `verify_password` function performs constant-time comparison
//! internally, preventing timing-based attacks that could leak information
//! about partial key matches.

use std::sync::Arc;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

use crate::state::{AppState, Publisher};

/// Verify an API key against stored Argon2id hashes and, if it matches, return
/// the hash string that matched (so the caller can look up its publisher identity).
///
/// Uses Argon2's built-in constant-time verification to prevent timing attacks.
/// The function iterates through ALL stored hashes (no early return) so that the
/// number of Argon2 verifications performed does not depend on which hash matched.
///
/// # Arguments
///
/// * `provided_key` - The plaintext API key from the request header
/// * `stored_hashes` - Set of Argon2id hashes in PHC string format
///
/// # Returns
///
/// `Some(hash)` of the matching stored hash, or `None` if no hash matched.
///
/// # Note on timing
///
/// We still iterate every candidate hash and run a full Argon2 verification
/// against each (no short-circuit). Recording *which* hash matched does not add
/// a timing side channel beyond Argon2's own cost: the set of verifications run
/// is identical regardless of the match position, and assignment of the matched
/// string is constant-time relative to the dominating Argon2 work.
fn verify_api_key(
    provided_key: &str,
    stored_hashes: &std::collections::HashSet<String>,
) -> Option<String> {
    let argon2 = Argon2::default();
    let provided_bytes = provided_key.as_bytes();
    let mut matched: Option<String> = None;

    // Dummy hash for constant-time verification when hash parsing fails
    // This prevents timing attacks based on hash validity detection
    // Uses a valid Argon2id PHC format with minimal parameters
    const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$\
        c29tZXNhbHQxMjM0NTY3ODkwMTIzNDU2Nzg5MDEyMzQ1Njc$\
        YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXoxMjM0NTY3ODk";

    // Iterate through ALL hashes to prevent timing attacks that could
    // reveal how many hashes were checked before finding a match
    for hash_str in stored_hashes {
        // Parse the PHC string format hash
        // If parsing fails, use a dummy hash to maintain constant-time behavior
        let parsed_hash = PasswordHash::new(hash_str).unwrap_or_else(|_| {
            // This should never happen since we validate hashes during load,
            // but we handle it defensively for timing attack prevention
            PasswordHash::new(DUMMY_HASH).expect("Dummy hash should always be valid")
        });

        // Argon2's verify_password uses constant-time comparison internally
        if argon2.verify_password(provided_bytes, &parsed_hash).is_ok() {
            // Record the match but do NOT break early: continue verifying all
            // remaining hashes so the work performed is independent of position.
            matched = Some(hash_str.clone());
        }
    }

    matched
}

/// Middleware that requires a valid API key in the X-API-Key header.
///
/// Returns 401 Unauthorized if the key is missing or invalid.
/// Uses Argon2id password verification with constant-time comparison
/// to prevent timing attacks.
///
/// Implements exponential backoff for authentication failures to prevent
/// brute-force attacks.
#[tracing::instrument(skip(state, request, next), fields(method, path, ip))]
pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract request info for logging before checking auth
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    // Derive the client IP. Behind a trusted reverse proxy (opt-in) this uses
    // X-Forwarded-For; otherwise it is the direct socket peer. See
    // rate_limit::client_ip for the trust model.
    let ip = crate::rate_limit::client_ip(&addr, request.headers(), state.trust_proxy);

    // Record method, path, and IP in span
    tracing::Span::current().record("method", method.as_str());
    tracing::Span::current().record("path", path.as_str());
    tracing::Span::current().record("ip", ip.as_str());

    // Check if IP is blocked due to excessive auth failures
    if state.auth_failure_limiter.is_blocked(&ip) {
        tracing::warn!("Authentication blocked: IP in backoff period");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // Check X-API-Key header
    let api_key = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let matched_hash = api_key
        .as_deref()
        .and_then(|key| verify_api_key(key, &state.api_keys));

    match matched_hash {
        Some(hash) => {
            // Look up the publisher identity for the matched key and attach it
            // to the request so downstream handlers can enforce authorization.
            let publisher: Publisher = state
                .publishers
                .get(&hash)
                .cloned()
                .unwrap_or_else(|| Publisher {
                    // Should not happen (api_keys is derived from publishers),
                    // but fail closed to an empty-scope identity rather than admin.
                    owner: "unknown".to_string(),
                    allowed_apps: Some(std::collections::HashSet::new()),
                    pubkey: None,
                });
            tracing::debug!(owner = %publisher.owner, "Authentication successful");
            request.extensions_mut().insert(publisher);
            // Clear backoff for this IP on successful auth (decays failures).
            state.auth_failure_limiter.record_success(&ip);
            Ok(next.run(request).await)
        }
        None if api_key.is_some() => {
            tracing::warn!("Authentication failed: invalid API key");
            // Record failure and check if should be blocked
            if !state.auth_failure_limiter.record_failure(&ip) {
                // Failure limit exceeded, now in backoff
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            Err(StatusCode::UNAUTHORIZED)
        }
        None => {
            tracing::warn!("Authentication failed: missing API key");
            // Record failure for missing key as well
            if !state.auth_failure_limiter.record_failure(&ip) {
                // Failure limit exceeded, now in backoff
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Hash an API key using Argon2id for storage.
///
/// This function generates a random salt and hashes the key using Argon2id
/// with secure default parameters. The result is returned in PHC string format
/// suitable for storage in the api_keys.txt file.
///
/// # Arguments
///
/// * `api_key` - The plaintext API key to hash
///
/// # Returns
///
/// The Argon2id hash in PHC string format (e.g., `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`)
///
/// # Example
///
/// ```ignore
/// let hash = hash_api_key("my-secret-api-key-12345")?;
/// // Store `hash` in api_keys.txt
/// ```
/// Hash a plaintext API key for storage in api_keys.txt (one Argon2id PHC hash per line).
pub fn hash_api_key(api_key: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let hash = argon2.hash_password(api_key.as_bytes(), &salt)?;
    Ok(hash.to_string())
}
