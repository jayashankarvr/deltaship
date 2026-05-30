//! Deltaship Update Server
//!
//! A REST API server for serving software updates using the Deltaship protocol.
//!
//! # TLS/HTTPS Requirements
//!
//! **IMPORTANT:** This server does NOT handle TLS/HTTPS encryption directly. It binds to
//! a plain HTTP socket and expects to be deployed behind a reverse proxy that handles TLS
//! termination (such as nginx, Caddy, Traefik, or a cloud load balancer).
//!
//! ## Production Deployment
//!
//! In production, you MUST use HTTPS to protect API keys and prevent man-in-the-middle
//! attacks. Configure a reverse proxy to:
//!
//! - Handle TLS certificate management (Let's Encrypt, commercial CA, etc.)
//! - Terminate TLS connections and forward plain HTTP to this server
//! - Add security headers (HSTS, X-Content-Type-Options, etc.)
//! - Enforce HTTPS redirects (HTTP -> HTTPS)
//!
//! ## Example Reverse Proxy Setup
//!
//! **Nginx:**
//! ```nginx
//! server {
//!     listen 443 ssl http2;
//!     server_name updates.example.com;
//!
//!     ssl_certificate /etc/letsencrypt/live/updates.example.com/fullchain.pem;
//!     ssl_certificate_key /etc/letsencrypt/live/updates.example.com/privkey.pem;
//!
//!     location / {
//!         proxy_pass http://127.0.0.1:8080;
//!         proxy_set_header Host $host;
//!         proxy_set_header X-Real-IP $remote_addr;
//!         proxy_set_header X-Forwarded-Proto $scheme;
//!     }
//! }
//! ```
//!
//! **Caddy:**
//! ```
//! updates.example.com {
//!     reverse_proxy localhost:8080
//! }
//! ```
//!
//! ## Security Warning
//!
//! Running this server without TLS in production exposes:
//! - API keys transmitted in plain text
//! - Update content vulnerable to tampering
//! - Client privacy (version info, device IDs)
//!
//! While update content is cryptographically signed (preventing tampering), metadata
//! and authentication credentials require HTTPS protection.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{extract::DefaultBodyLimit, middleware, Router};
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use tokio::net::TcpListener;
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    trace::TraceLayer,
};

mod auth;
mod commands;
mod models;
mod rate_limit;
mod routes;
mod state;
mod stats;
mod storage;
mod validation;

use state::AppState;

/// Deltaship Update Server - serves software updates via REST API
#[derive(Parser, Debug)]
#[command(name = "deltaship-server")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Host address to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Directory for storing update data (binaries, diffs, catalogs)
    #[arg(long, default_value = "./data")]
    data_dir: PathBuf,

    /// Generate a new API key and exit
    #[arg(long)]
    generate_api_key: bool,

    /// Trust `X-Forwarded-For` for client IP (rate limiting / auth backoff).
    ///
    /// OFF by default. Enable ONLY when this server sits behind a reverse proxy
    /// you control that sets/overwrites `X-Forwarded-For`; otherwise clients can
    /// forge the header to evade rate limits. The right-most XFF hop is used.
    /// May also be enabled via `DELTASHIP_TRUST_PROXY=1`.
    #[arg(long)]
    trust_proxy: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show detailed version information
    Version,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Hash a plaintext API key for storage in api_keys.txt
    HashKey {
        /// The plaintext API key to hash (reads from stdin if omitted)
        key: Option<String>,
    },
}

fn init_logging(verbose: u8, quiet: bool) {
    use tracing_subscriber::{fmt, EnvFilter};

    let level = if quiet {
        "error"
    } else {
        match verbose {
            0 => "warn",
            1 => "info,tower_http=info",
            2 => "debug,tower_http=debug",
            _ => "trace,tower_http=trace",
        }
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt().with_env_filter(filter).with_target(false).init();
}

/// Generate a cryptographically secure random 32-byte (256-bit) API key as a hex string.
///
/// # API Key Format
///
/// The generated key is a **64-character lowercase hexadecimal string** representing
/// 32 bytes (256 bits) of cryptographic randomness. Example:
///
/// ```text
/// a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef12345678
/// ```
///
/// This format is expected by the authentication middleware when validating the
/// `X-API-Key` header on protected endpoints.
///
/// # Usage
///
/// Set the generated key as the `DELTASHIP_API_KEY` environment variable:
///
/// ```bash
/// export DELTASHIP_API_KEY=$(deltaship-server --generate-api-key)
/// ```
fn generate_api_key() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 32]; // 256 bits of entropy
    rand::thread_rng().fill_bytes(&mut bytes);

    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Handle subcommands that don't need the server
    if let Some(command) = &cli.command {
        match command {
            Commands::Version => {
                commands::version::run();
                return Ok(());
            }
            Commands::Completions { shell } => {
                commands::completions::run(*shell, &mut Cli::command());
                return Ok(());
            }
            Commands::HashKey { key } => {
                let plaintext = match key {
                    Some(k) => k.clone(),
                    None => {
                        use std::io::BufRead;
                        let stdin = std::io::stdin();
                        stdin.lock().lines().next()
                            .ok_or("No input provided")?
                            .map_err(|e| e.to_string())?
                            .trim().to_string()
                    }
                };
                let hash = auth::hash_api_key(&plaintext)
                    .map_err(|e| format!("Hashing failed: {}", e))?;
                println!("{}", hash);
                return Ok(());
            }
        }
    }

    // Handle API key generation
    if cli.generate_api_key {
        let key = generate_api_key();
        println!("{}", key);
        return Ok(());
    }

    // Start the server
    init_logging(cli.verbose, cli.quiet);

    // Initialize storage directory
    if !cli.data_dir.exists() {
        std::fs::create_dir_all(&cli.data_dir)?;
        tracing::info!("Created data directory: {}", cli.data_dir.display());
    }

    // Create apps subdirectory
    let apps_dir = cli.data_dir.join("apps");
    if !apps_dir.exists() {
        std::fs::create_dir_all(&apps_dir)?;
        tracing::info!("Created apps directory: {}", apps_dir.display());
    }

    // Determine trusted-proxy mode (CLI flag OR env var; default OFF).
    let trust_proxy = cli.trust_proxy
        || matches!(
            std::env::var("DELTASHIP_TRUST_PROXY").ok().as_deref(),
            Some("1") | Some("true") | Some("yes")
        );
    if trust_proxy {
        tracing::warn!(
            "Trusted-proxy mode ENABLED: client IP derived from X-Forwarded-For (right-most hop). \
             Ensure ALL traffic passes through a proxy you control that overwrites this header, \
             otherwise clients can forge it to evade rate limits."
        );
    }

    // Create shared state
    let state = Arc::new(AppState::with_options(cli.data_dir, trust_proxy));

    // Rate limiting configuration
    // 100 requests per minute for check-update endpoint
    let check_update_limiter = Arc::new(rate_limit::RateLimiter::new(100, Duration::from_secs(60)));
    // 10 requests per minute for publish endpoints
    let publish_limiter = Arc::new(rate_limit::RateLimiter::new(10, Duration::from_secs(60)));
    // 30 requests per minute for admin endpoints
    let admin_limiter = Arc::new(rate_limit::RateLimiter::new(30, Duration::from_secs(60)));
    // 60 requests per minute for health endpoint (conservative limit for monitoring)
    let health_limiter = Arc::new(rate_limit::RateLimiter::new(60, Duration::from_secs(60)));
    // 200 requests per minute for download endpoints (allow reasonable download traffic)
    let download_limiter = Arc::new(rate_limit::RateLimiter::new(200, Duration::from_secs(60)));

    // Spawn background cleanup tasks so entry expiry runs on a timer instead of
    // piggybacking on request volume and sorting 100k keys under the lock (FIX E).
    check_update_limiter.spawn_cleanup_task();
    publish_limiter.spawn_cleanup_task();
    admin_limiter.spawn_cleanup_task();
    health_limiter.spawn_cleanup_task();
    download_limiter.spawn_cleanup_task();
    state.auth_failure_limiter.spawn_cleanup_task();

    // CORS configuration
    // SECURITY: By default, allows all origins for development convenience.
    // In production, set DELTASHIP_CORS_ORIGINS to a comma-separated list of allowed origins
    // (e.g., "https://example.com,https://api.example.com")
    let cors = match std::env::var("DELTASHIP_CORS_ORIGINS") {
        Ok(origins) if !origins.is_empty() => {
            let allowed: Vec<_> = origins
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if allowed.is_empty() {
                tracing::warn!(
                    "DELTASHIP_CORS_ORIGINS is set but contains no valid origins, allowing all origins"
                );
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any)
            } else {
                tracing::info!(
                    "CORS configured with allowed origins: {}",
                    origins
                );
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list(allowed))
                    .allow_methods(Any)
                    .allow_headers(Any)
            }
        }
        _ => {
            tracing::warn!(
                "DELTASHIP_CORS_ORIGINS not set, allowing all origins. \
                 Set DELTASHIP_CORS_ORIGINS to restrict allowed origins in production."
            );
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        }
    };

    // Public routes with rate limiting
    let public_routes = Router::new()
        .merge(
            routes::health::router().layer(middleware::from_fn(move |addr, req, next| {
                rate_limit::rate_limit_middleware(addr, health_limiter.clone(), trust_proxy, req, next)
            })),
        )
        .merge(
            routes::update::router().layer(middleware::from_fn(move |addr, req, next| {
                rate_limit::rate_limit_middleware(addr, check_update_limiter.clone(), trust_proxy, req, next)
            })),
        )
        .merge(
            routes::download::router().layer(middleware::from_fn(move |addr, req, next| {
                rate_limit::rate_limit_middleware(addr, download_limiter.clone(), trust_proxy, req, next)
            })),
        );

    // Protected routes (require API key) with rate limiting
    let admin_limiter_clone = admin_limiter.clone();
    let protected_routes = Router::new()
        .merge(
            routes::publish::router().layer(middleware::from_fn(move |addr, req, next| {
                rate_limit::rate_limit_middleware(addr, publish_limiter.clone(), trust_proxy, req, next)
            })),
        )
        .merge(
            routes::admin::router().layer(middleware::from_fn(move |addr, req, next| {
                rate_limit::rate_limit_middleware(addr, admin_limiter_clone.clone(), trust_proxy, req, next)
            })),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_key,
        ));

    // Build combined router with global middleware.
    // Default body limit lowered from 2GB to 600MB (FIX A): the previous 2GB cap
    // let a single request pin ~1GB of binary plus a second copy in RAM. 600MB
    // covers the 512MB max binary field plus multipart/encoding overhead while
    // bounding per-request memory pressure.
    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .layer(DefaultBodyLimit::max(600 * 1024 * 1024)) // 600MB
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("Deltaship Update Server listening on {}", addr);

    // Warn if not using HTTPS (detecting based on common HTTPS ports and localhost)
    let is_localhost = cli.host == "127.0.0.1" || cli.host == "localhost" || cli.host == "::1";
    let is_https_port = cli.port == 443;

    if !is_https_port && !is_localhost {
        tracing::warn!(
            "⚠️  SECURITY WARNING: Server is running in HTTP-only mode without TLS encryption!"
        );
        tracing::warn!(
            "   This server does NOT handle TLS/HTTPS. In production, you MUST use a reverse proxy."
        );
        tracing::warn!(
            "   Recommended: nginx, Caddy, Traefik, or cloud load balancer with TLS termination."
        );
        tracing::warn!(
            "   See documentation: https://docs.rs/deltaship-server for deployment examples."
        );
    } else if is_localhost {
        tracing::info!(
            "Running on localhost - TLS not required for local development/testing"
        );
    }

    // Graceful shutdown handling
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown complete");

    Ok(())
}

/// Wait for shutdown signal (SIGTERM or SIGINT)
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM, initiating graceful shutdown...");
        },
    }
}
