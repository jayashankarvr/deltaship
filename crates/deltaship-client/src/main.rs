//! Deltaship Client Patcher - Background daemon for automatic binary updates.

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use deltaship_db::ClientDb;

mod audit;
mod checker;
mod commands;
mod config;
mod daemon;
mod downloader;
mod patcher;

use config::load_config;

/// Deltaship Client Patcher - Automatic binary update daemon
#[derive(Parser)]
#[command(name = "deltaship-client")]
#[command(about = "Deltaship Client Patcher - Automatic binary update daemon")]
#[command(version)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Path to configuration file
    #[arg(short = 'C', long, global = true)]
    config: Option<PathBuf>,

    /// Run as daemon (background service)
    #[arg(short, long)]
    daemon: bool,

    /// Run a one-time update check
    #[arg(long)]
    check_now: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

fn init_logging(verbose: u8, quiet: bool) {
    use tracing_subscriber::{fmt, EnvFilter};

    let level = if quiet {
        "error"
    } else {
        match verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    fmt().with_env_filter(filter).with_target(false).init();
}

#[derive(Subcommand)]
enum Commands {
    /// Add a binary to manage
    Add {
        /// Name of the binary
        #[arg(short, long)]
        name: String,

        /// Path to the binary file
        #[arg(short, long)]
        path: String,

        /// Path to the publisher's public key file
        #[arg(short = 'k', long)]
        public_key_file: String,
    },

    /// Remove a managed binary
    Remove {
        /// Name of the binary to remove
        #[arg(short, long)]
        name: String,
    },

    /// List managed binaries
    List,

    /// Show update status for all binaries
    Status,

    /// Rollback to a previous version
    Rollback {
        /// Name of the binary to rollback
        #[arg(long)]
        name: String,

        /// Specific version to rollback to (defaults to previous version)
        #[arg(long)]
        to_version: Option<String>,

        /// List available backups instead of rolling back
        #[arg(long)]
        list: bool,
    },

    /// Manually update binaries
    Update {
        /// Name of a specific binary to update
        #[arg(long)]
        name: Option<String>,

        /// Update all managed binaries (default if no --name specified)
        #[arg(long)]
        all: bool,

        /// Force re-download even if already on latest version
        #[arg(long)]
        force: bool,

        /// Check for updates but don't apply them
        #[arg(long)]
        dry_run: bool,

        /// Download updates but don't apply them
        #[arg(long)]
        download_only: bool,

        /// Allow installing a version that is not newer than the installed one (downgrade)
        #[arg(long)]
        allow_downgrade: bool,
    },

    /// Clean up expired backups and incomplete downloads
    Cleanup {
        /// Show what would be removed without removing anything
        #[arg(long)]
        dry_run: bool,

        /// Maximum age in days for history entries (default: 90)
        #[arg(long)]
        max_age_days: Option<u64>,
    },

    /// Keep only the last N backups for a binary
    PruneBackups {
        /// Binary name to prune backups for
        #[arg(short, long)]
        name: String,

        /// Number of backups to keep
        #[arg(short, long)]
        keep: usize,

        /// Show what would be removed without removing anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Show detailed version information
    Version,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Manage systemd service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install systemd service
    Install {
        /// Install as user service (~/.config/systemd/user/) instead of system service
        #[arg(long)]
        user: bool,
    },

    /// Uninstall systemd service
    Uninstall {
        /// Uninstall user service instead of system service
        #[arg(long)]
        user: bool,
    },

    /// Show service status
    Status {
        /// Show user service status instead of system service
        #[arg(long)]
        user: bool,
    },

    /// Show service logs
    Logs {
        /// Show user service logs instead of system service
        #[arg(long)]
        user: bool,

        /// Follow/tail the logs
        #[arg(short, long)]
        follow: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Handle commands that don't need config/db
    if let Some(ref command) = cli.command {
        match command {
            Commands::Version => {
                commands::version::run();
                return Ok(());
            }
            Commands::Completions { shell } => {
                commands::completions::run(*shell, &mut Cli::command());
                return Ok(());
            }
            Commands::Service { action } => {
                match action {
                    ServiceAction::Install { user } => {
                        commands::service::install_service(*user)?;
                    }
                    ServiceAction::Uninstall { user } => {
                        commands::service::uninstall_service(*user)?;
                    }
                    ServiceAction::Status { user } => {
                        commands::service::show_service_status(*user)?;
                    }
                    ServiceAction::Logs { user, follow } => {
                        commands::service::show_service_logs(*user, *follow)?;
                    }
                }
                return Ok(());
            }
            _ => {}
        }
    }

    init_logging(cli.verbose, cli.quiet);

    // Load configuration
    let config = load_config(cli.config.as_deref())?;

    // Ensure data directory exists
    tokio::fs::create_dir_all(&config.data_dir).await?;

    // Initialize database
    let db_path = config.db_path();
    tracing::info!("Using database at {}", db_path.display());
    let db = ClientDb::open(&db_path).await?;
    db.init().await?;

    // Handle commands
    if let Some(command) = cli.command {
        // Commands that modify state should acquire update lock to avoid conflicts with daemon
        let needs_lock = matches!(
            command,
            Commands::Update { .. }
                | Commands::Rollback { .. }
                | Commands::Cleanup { .. }
                | Commands::PruneBackups { .. }
        );

        // Try to acquire update lock if needed
        let _update_lock = if needs_lock {
            match daemon::try_acquire_update_lock(&config) {
                Ok(Some(lock)) => Some(lock),
                Ok(None) => {
                    anyhow::bail!(
                        "Another update operation is in progress (daemon or manual update). \
                         Please wait for it to complete or stop the daemon service first."
                    );
                }
                Err(e) => {
                    return Err(e.context("Failed to acquire update lock"));
                }
            }
        } else {
            None
        };

        match command {
            Commands::Add {
                name,
                path,
                public_key_file,
            } => {
                commands::add_binary(&db, &name, &path, &public_key_file).await?;
            }
            Commands::Remove { name } => {
                commands::remove_binary(&db, &name).await?;
            }
            Commands::List => {
                commands::list_binaries(&db).await?;
            }
            Commands::Status => {
                commands::show_status(&config, &db).await?;
            }
            Commands::Rollback {
                name,
                to_version,
                list,
            } => {
                commands::rollback(&config, &db, &name, to_version.as_deref(), list).await?;
            }
            Commands::Update {
                name,
                all: _,
                force,
                dry_run,
                download_only,
                allow_downgrade,
            } => {
                commands::update(
                    &config,
                    &db,
                    name.as_deref(),
                    force,
                    dry_run,
                    download_only,
                    cli.quiet,
                    allow_downgrade,
                )
                .await?;
            }
            Commands::Cleanup {
                dry_run,
                max_age_days,
            } => {
                commands::run_cleanup(&config, &db, dry_run, max_age_days).await?;
            }
            Commands::PruneBackups {
                name,
                keep,
                dry_run,
            } => {
                commands::run_prune_backups(&config, &db, name, keep, dry_run).await?;
            }
            // Already handled above
            Commands::Version | Commands::Completions { .. } | Commands::Service { .. } => {
                unreachable!()
            }
        }
        return Ok(());
    }

    // Handle daemon/check-now modes
    if cli.check_now {
        tracing::info!("Running one-time update check");
        daemon::run_check_once(&config, &db).await?;
    } else if cli.daemon {
        tracing::info!("Starting daemon mode");
        daemon::run_daemon(config, db).await?;
    } else {
        // Default: show status
        commands::show_status(&config, &db).await?;
    }

    Ok(())
}
