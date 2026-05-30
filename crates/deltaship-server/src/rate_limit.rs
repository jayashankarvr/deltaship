//! Simple in-memory rate limiter middleware with memory exhaustion protection
//!
//! Uses LRU eviction to prevent memory exhaustion under attack conditions.

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{HeaderMap, Request, Response, StatusCode},
    middleware::Next,
    response::IntoResponse,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// Derive the client IP used for rate-limiting and auth backoff buckets.
///
/// # Trust model
///
/// - When `trust_proxy` is `false` (the safe default), the IP is always the
///   direct TCP peer (`addr.ip()`). `X-Forwarded-For` is ignored entirely, so a
///   direct attacker cannot forge it to evade or poison rate limits.
///
/// - When `trust_proxy` is `true`, the server is assumed to sit behind a trusted
///   reverse proxy that appends the real client IP to `X-Forwarded-For`. In that
///   case we take the **right-most** entry of the XFF list. Rationale: the
///   right-most value is the one written by the closest (trusted) hop, so it is
///   the hardest for a client several hops upstream to spoof. Operators MUST only
///   enable this when every request genuinely passes through a proxy they control
///   that sets/overwrites XFF; otherwise clients can forge the header.
///
/// Falls back to the socket peer if the header is absent or unparseable.
pub fn client_ip(addr: &SocketAddr, headers: &HeaderMap, trust_proxy: bool) -> String {
    if trust_proxy {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            // Right-most non-empty hop (closest trusted proxy wrote it).
            if let Some(ip) = xff
                .split(',')
                .map(|s| s.trim())
                .rev()
                .find(|s| !s.is_empty())
            {
                return ip.to_string();
            }
        }
    }
    addr.ip().to_string()
}

/// Maximum number of entries to prevent memory exhaustion
const MAX_ENTRIES: usize = 100_000;

/// Number of oldest entries to evict when at capacity (10% of MAX_ENTRIES)
const LRU_EVICTION_COUNT: usize = MAX_ENTRIES / 10;

/// Maximum auth failures before applying exponential backoff (starts at 5)
const AUTH_FAILURE_THRESHOLD: u32 = 5;

/// Base delay for exponential backoff (in seconds)
const AUTH_BACKOFF_BASE_SECONDS: u64 = 60;

/// Maximum backoff duration (10 minutes)
const MAX_AUTH_BACKOFF_SECONDS: u64 = 600;

#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
    /// Last time this entry was accessed (for LRU eviction)
    last_access: Instant,
}

/// Entry for tracking authentication failures with exponential backoff
#[derive(Debug, Clone)]
struct AuthFailureEntry {
    /// Number of consecutive failures
    failure_count: u32,
    /// When the current backoff period started
    backoff_until: Option<Instant>,
    /// Last time this entry was accessed (for LRU eviction)
    last_access: Instant,
}

#[derive(Debug, Clone)]
pub struct RateLimiter {
    // IP -> (count, window_start, last_access)
    entries: Arc<Mutex<HashMap<String, RateLimitEntry>>>,
    max_requests: u32,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u32, window_duration: Duration) -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window_duration,
        }
    }

    /// Remove expired entries to prevent memory exhaustion
    fn cleanup_expired_entries(&self, entries: &mut HashMap<String, RateLimitEntry>, now: Instant) {
        entries.retain(|_, entry| {
            now.duration_since(entry.window_start) < self.window_duration
        });
    }

    /// Evict entries when at capacity using O(k) sampled eviction instead of an
    /// O(n log n) clone+sort of every key under the lock.
    ///
    /// We first drop any expired entries, then, if still at capacity, evict a
    /// bounded number of the oldest entries found within a small random-ish
    /// sample. This keeps memory bounded under attack without ever sorting 100k
    /// keys while holding the global mutex. Background cleanup (see
    /// [`RateLimiter::spawn_cleanup_task`]) handles the steady-state expiry.
    fn evict_when_full(&self, entries: &mut HashMap<String, RateLimitEntry>, now: Instant) {
        if entries.len() < MAX_ENTRIES {
            return;
        }

        // First reclaim expired entries cheaply.
        self.cleanup_expired_entries(entries, now);
        if entries.len() < MAX_ENTRIES {
            return;
        }

        // Still full: evict the oldest entries seen within a bounded sample.
        // Iterating a HashMap yields entries in a randomized order, so a single
        // linear scan that keeps the oldest `evict` candidates approximates LRU
        // at O(n) without sorting. We cap work at the sample size.
        let evict = LRU_EVICTION_COUNT.min(entries.len());
        let sample = (evict * 4).min(entries.len());

        let mut candidates: Vec<(String, Instant)> = entries
            .iter()
            .take(sample)
            .map(|(ip, e)| (ip.clone(), e.last_access))
            .collect();
        candidates.sort_by_key(|(_, t)| *t);
        for (ip, _) in candidates.into_iter().take(evict) {
            entries.remove(&ip);
        }

        tracing::warn!(
            "Rate limiter at capacity: sampled-evicted entries, {} remaining",
            entries.len()
        );
    }

    /// Spawn a background task that periodically reclaims expired entries so that
    /// cleanup no longer piggybacks on request volume. The task holds the mutex
    /// only for the brief `retain` pass, never across `.await`.
    pub fn spawn_cleanup_task(&self) {
        let entries = self.entries.clone();
        let window = self.window_duration;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let now = Instant::now();
                if let Ok(mut map) = entries.lock() {
                    map.retain(|_, e| now.duration_since(e.window_start) < window);
                }
            }
        });
    }

    pub fn check_rate_limit(&self, ip: &str) -> bool {
        let mut entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();

        // Best-effort capacity guard. Steady-state expiry is handled by the
        // background cleanup task; this only fires when we're actually at the
        // hard cap (e.g. a flood faster than the 30s sweep) and is O(sample).
        let is_new_ip = !entries.contains_key(ip);
        if is_new_ip && entries.len() >= MAX_ENTRIES {
            self.evict_when_full(&mut entries, now);
        }

        // Update or insert entry
        let entry = entries.entry(ip.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
            last_access: now,
        });

        // Update last access time for LRU tracking
        entry.last_access = now;

        // Reset window if expired
        if now.duration_since(entry.window_start) >= self.window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        // Check if under limit
        if entry.count >= self.max_requests {
            return false;
        }

        entry.count += 1;
        true
    }
}

/// Middleware for rate limiting requests by IP.
///
/// `trust_proxy` controls whether `X-Forwarded-For` is honored when deriving the
/// client IP (see [`client_ip`]). Default deployments should pass `false`.
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    limiter: Arc<RateLimiter>,
    trust_proxy: bool,
    req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, impl IntoResponse> {
    let ip = client_ip(&addr, req.headers(), trust_proxy);

    if !limiter.check_rate_limit(&ip) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded. Please try again later.",
        ));
    }

    Ok(next.run(req).await)
}

/// Rate limiter specifically for authentication failures with exponential backoff.
///
/// Uses LRU eviction to prevent memory exhaustion under attack conditions.
#[derive(Debug, Clone)]
pub struct AuthFailureLimiter {
    entries: Arc<Mutex<HashMap<String, AuthFailureEntry>>>,
}

impl AuthFailureLimiter {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Remove expired entries (those past their backoff period with no recent failures)
    fn cleanup_expired_entries(&self, entries: &mut HashMap<String, AuthFailureEntry>, now: Instant) {
        entries.retain(|_, entry| {
            // Keep entries that are still in backoff period
            if let Some(backoff_until) = entry.backoff_until {
                if now < backoff_until {
                    return true;
                }
            }
            // Keep entries accessed within the maximum backoff duration
            // This ensures we track repeat offenders
            now.duration_since(entry.last_access) < Duration::from_secs(MAX_AUTH_BACKOFF_SECONDS)
        });
    }

    /// Evict entries when at capacity using O(sample) sampled eviction (no full
    /// clone+sort under the lock). Entries still inside an active backoff window
    /// are preserved so an attacker cannot flush their own penalty by flooding
    /// new IPs. See [`RateLimiter::evict_when_full`] for the rationale.
    fn evict_when_full(&self, entries: &mut HashMap<String, AuthFailureEntry>, now: Instant) {
        if entries.len() < MAX_ENTRIES {
            return;
        }

        self.cleanup_expired_entries(entries, now);
        if entries.len() < MAX_ENTRIES {
            return;
        }

        let evict = LRU_EVICTION_COUNT.min(entries.len());
        let sample = (evict * 4).min(entries.len());
        let mut candidates: Vec<(String, Instant)> = entries
            .iter()
            // Never evict IPs currently serving a backoff penalty.
            .filter(|(_, e)| e.backoff_until.map(|b| now >= b).unwrap_or(true))
            .take(sample)
            .map(|(ip, e)| (ip.clone(), e.last_access))
            .collect();
        candidates.sort_by_key(|(_, t)| *t);
        for (ip, _) in candidates.into_iter().take(evict) {
            entries.remove(&ip);
        }

        tracing::warn!(
            "AuthFailureLimiter at capacity: sampled-evicted entries, {} remaining",
            entries.len()
        );
    }

    /// Spawn a background task that periodically reclaims expired entries instead
    /// of piggybacking cleanup on request counts. Holds the mutex only for the
    /// brief `retain` pass, never across an `.await`.
    pub fn spawn_cleanup_task(&self) {
        let entries = self.entries.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let now = Instant::now();
                if let Ok(mut map) = entries.lock() {
                    map.retain(|_, entry| {
                        if let Some(backoff_until) = entry.backoff_until {
                            if now < backoff_until {
                                return true;
                            }
                        }
                        now.duration_since(entry.last_access)
                            < Duration::from_secs(MAX_AUTH_BACKOFF_SECONDS)
                    });
                }
            }
        });
    }

    /// Record an authentication failure for an IP address
    /// Returns true if the request should be allowed (not in backoff), false otherwise
    pub fn record_failure(&self, ip: &str) -> bool {
        let mut entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();

        // Capacity guard only (steady-state expiry handled by background task).
        let is_new_ip = !entries.contains_key(ip);
        if is_new_ip && entries.len() >= MAX_ENTRIES {
            self.evict_when_full(&mut entries, now);
        }

        let entry = entries.entry(ip.to_string()).or_insert(AuthFailureEntry {
            failure_count: 0,
            backoff_until: None,
            last_access: now,
        });

        entry.last_access = now;

        // Check if currently in backoff period
        if let Some(backoff_until) = entry.backoff_until {
            if now < backoff_until {
                // Still in backoff period
                tracing::warn!(
                    ip = %ip,
                    backoff_remaining = ?backoff_until.duration_since(now),
                    "Auth attempt during backoff period"
                );
                return false;
            }
            // Backoff period expired, reset
            entry.backoff_until = None;
        }

        // Increment failure count
        entry.failure_count += 1;

        // If exceeded threshold, apply exponential backoff
        if entry.failure_count >= AUTH_FAILURE_THRESHOLD {
            // Calculate backoff duration: base * 2^(failures - threshold)
            // This gives 60s, 120s, 240s, 480s, 600s (capped)
            let exponent = entry.failure_count - AUTH_FAILURE_THRESHOLD;
            let backoff_seconds = (AUTH_BACKOFF_BASE_SECONDS * 2_u64.pow(exponent))
                .min(MAX_AUTH_BACKOFF_SECONDS);

            let backoff_duration = Duration::from_secs(backoff_seconds);
            entry.backoff_until = Some(now + backoff_duration);

            tracing::warn!(
                ip = %ip,
                failure_count = entry.failure_count,
                backoff_seconds = backoff_seconds,
                "Auth failure threshold exceeded, applying exponential backoff"
            );

            return false;
        }

        true
    }

    /// Record a successful authentication.
    ///
    /// A single success does NOT fully reset accumulated failures (that would let
    /// an attacker erase a long brute-force history with one lucky/known-good
    /// auth). Instead the failure count is *decayed* by one and any active
    /// backoff window is cleared. Sustained legitimate use therefore drains the
    /// counter over several successes, while a burst of failures still escalates.
    pub fn record_success(&self, ip: &str) {
        let mut entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();
        if let Some(entry) = entries.get_mut(ip) {
            // Decay failures rather than zeroing them.
            entry.failure_count = entry.failure_count.saturating_sub(1);
            // A genuine success clears any active backoff so the client isn't
            // penalized further, but residual failure_count remains.
            entry.backoff_until = None;
            entry.last_access = now;

            // Once fully decayed and not in backoff, drop the entry to reclaim memory.
            if entry.failure_count == 0 {
                entries.remove(ip);
            }
        }
    }

    /// Check if an IP is currently in backoff period
    pub fn is_blocked(&self, ip: &str) -> bool {
        let entries = self.entries.lock().expect("rate limiter lock poisoned");
        let now = Instant::now();

        if let Some(entry) = entries.get(ip) {
            if let Some(backoff_until) = entry.backoff_until {
                return now < backoff_until;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn addr(ip: [u8; 4]) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3])), 12345)
    }

    #[test]
    fn client_ip_ignores_xff_when_proxy_untrusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "1.2.3.4".parse().unwrap());
        // trust_proxy = false -> always the socket peer
        assert_eq!(client_ip(&addr([10, 0, 0, 1]), &headers, false), "10.0.0.1");
    }

    #[test]
    fn client_ip_uses_rightmost_xff_when_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "9.9.9.9, 8.8.8.8, 7.7.7.7".parse().unwrap(),
        );
        // Right-most hop (written by the closest trusted proxy).
        assert_eq!(client_ip(&addr([10, 0, 0, 1]), &headers, true), "7.7.7.7");
    }

    #[test]
    fn client_ip_falls_back_to_peer_without_xff() {
        let headers = HeaderMap::new();
        assert_eq!(client_ip(&addr([10, 0, 0, 2]), &headers, true), "10.0.0.2");
    }

    #[test]
    fn record_success_decays_rather_than_resets() {
        let limiter = AuthFailureLimiter::new();
        let ip = "5.5.5.5";
        // Three failures (below threshold so not blocked yet).
        for _ in 0..3 {
            assert!(limiter.record_failure(ip));
        }
        // One success decays by one, leaving residual failures (entry retained).
        limiter.record_success(ip);
        {
            let entries = limiter.entries.lock().unwrap();
            let e = entries.get(ip).expect("entry should persist after one success");
            assert_eq!(e.failure_count, 2);
        }
        // Enough successes eventually drain it and drop the entry.
        limiter.record_success(ip);
        limiter.record_success(ip);
        let entries = limiter.entries.lock().unwrap();
        assert!(entries.get(ip).is_none());
    }
}
