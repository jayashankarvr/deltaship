//! deltaship-updater — sidecar helper process for self-updating applications.
//!
//! Ship this binary alongside your application. Your app spawns it on startup
//! (or on a schedule) and acts on the exit code:
//!
//!   0  → already up to date, nothing to do
//!   2  → update was applied, app should restart itself
//!   1  → error (details on stderr)
//!
//! # Typical integration
//!
//! At startup, spawn this process synchronously and check the exit code:
//!
//! ```no_run
//! let status = std::process::Command::new("myapp-updater")
//!     .args([
//!         "--name",         "myapp",
//!         "--install-path", "/usr/local/bin/myapp",
//!         "--server-url",   "https://updates.example.com",
//!         "--public-key",   "/etc/myapp/publisher.pub",
//!     ])
//!     .status()
//!     .expect("failed to spawn updater");
//!
//! if status.code() == Some(2) {
//!     // An update was applied — re-launch the app so users get the new version.
//! }
//! ```
//!
//! # State directory
//!
//! On first run the updater creates a small SQLite database at
//! `<data-dir>/client.db` to track the installed version, backups, and history.
//!
//!   Linux/macOS: `~/.local/share/deltaship/<name>/`
//!   Windows:     `%APPDATA%\deltaship\<name>\`
//!
//! Override with `--data-dir` to co-locate state with your app's own data.

use std::path::PathBuf;
use std::process;

use clap::Parser;
use deltaship_client::{checker::UpdateChecker, config::ClientConfig, patcher::apply_update};
use deltaship_crypto::load_verifying_key;
use deltaship_db::{ClientDb, NewManagedBinary};

/// Exit code: binary is already on the latest version.
const EXIT_UP_TO_DATE: i32 = 0;
/// Exit code: an error occurred (details on stderr).
const EXIT_ERROR: i32 = 1;
/// Exit code: update was applied (or is available when using --check-only).
const EXIT_UPDATED: i32 = 2;

/// deltaship-updater — sidecar updater for self-updating applications.
///
/// Spawn this process from your application on startup or on a timer.
/// Check the exit code to decide whether to restart:
///   0 = up to date | 2 = updated (restart) | 1 = error
#[derive(Parser)]
#[command(name = "deltaship-updater", version)]
struct Args {
    /// Name of the binary as published on the update server.
    #[arg(long)]
    name: String,

    /// Absolute path where the binary lives (and will be updated in-place).
    #[arg(long)]
    install_path: PathBuf,

    /// Update server base URL (e.g. https://updates.example.com).
    #[arg(long)]
    server_url: String,

    /// Path to the publisher's Ed25519 public key file (.pub).
    #[arg(long)]
    public_key: PathBuf,

    /// Directory for updater state (SQLite DB, backups, downloads).
    /// Defaults to ~/.local/share/deltaship/<name>/ on Linux/macOS.
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Check for an available update but do not apply it.
    /// Exits 0 if up to date, 2 if an update is available.
    #[arg(long)]
    check_only: bool,

    /// Suppress informational output; only errors go to stderr.
    #[arg(long, short)]
    quiet: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    match run(args).await {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("deltaship-updater error: {e:#}");
            process::exit(EXIT_ERROR);
        }
    }
}

async fn run(args: Args) -> anyhow::Result<i32> {
    validate_name(&args.name)?;

    let data_dir = args.data_dir.unwrap_or_else(|| {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("deltaship")
            .join(&args.name)
    });

    tokio::fs::create_dir_all(&data_dir).await?;

    let config = ClientConfig {
        server_url: args.server_url,
        data_dir,
        ..ClientConfig::default()
    };

    let db = ClientDb::open(&config.db_path()).await?;
    db.init().await?;

    // Load publisher key — fail fast if the key file is missing or malformed.
    let verifying_key = load_verifying_key(&args.public_key)?;
    let public_key_bytes = verifying_key.to_bytes().to_vec();

    // Auto-register on first run; reuse the existing registration on subsequent runs.
    // We intentionally do NOT require install_path to exist yet — the first update
    // will download the full binary and create it.
    let binary = match db.get_binary_by_name(&args.name).await? {
        Some(existing) => existing,
        None => {
            let install_path_str = args
                .install_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("--install-path contains invalid UTF-8"))?
                .to_string();

            db.register_binary(NewManagedBinary {
                binary_id: uuid::Uuid::new_v4().to_string(),
                binary_name: args.name.clone(),
                platform: current_platform().to_string(),
                install_path: install_path_str,
                publisher_public_key: public_key_bytes,
            })
            .await?
        }
    };

    let checker = UpdateChecker::new(config.clone())?;
    let update = checker.check_for_updates(&binary).await?;

    match update {
        None => {
            if !args.quiet {
                println!(
                    "{}: up to date ({})",
                    args.name,
                    binary.current_version_string.as_deref().unwrap_or("unknown")
                );
            }
            Ok(EXIT_UP_TO_DATE)
        }
        Some(update_info) => {
            let new_version = update_info.version.to_string();
            let old_version = binary.current_version_string.as_deref().unwrap_or("none");

            if args.check_only {
                if !args.quiet {
                    println!(
                        "{}: update available ({} -> {})",
                        args.name, old_version, new_version
                    );
                }
                return Ok(EXIT_UPDATED);
            }

            if !args.quiet {
                println!("{}: updating {} -> {}", args.name, old_version, new_version);
            }

            apply_update(&config, &db, &binary, &update_info, None).await?;

            if !args.quiet {
                println!("{}: updated to {}", args.name, new_version);
            }

            Ok(EXIT_UPDATED)
        }
    }
}

/// Restrict binary names to alphanumeric, hyphens, and underscores (max 64 chars).
fn validate_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("--name cannot be empty");
    }
    if name.len() > 64 {
        anyhow::bail!("--name is too long ({} chars, max 64)", name.len());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!(
            "--name '{}' is invalid: only alphanumeric characters, hyphens, and underscores are allowed",
            name
        );
    }
    Ok(())
}

fn current_platform() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "linux-x86_64";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "linux-aarch64";
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "windows-x86_64";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "macos-x86_64";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "macos-aarch64";
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    return "unknown";
}
