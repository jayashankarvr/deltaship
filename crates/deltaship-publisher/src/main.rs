//! Deltaship Publisher Toolkit CLI
//!
//! A command-line tool for software publishers to manage binary releases.

mod api_client;
mod archive;
mod commands;
mod config;
mod diff_manager;
mod utils;

use clap::{CommandFactory, Parser, Subcommand};

use clap_complete::Shell;
use commands::{
    cleanup, completions, config as config_cmd, diff, export, import, info, init, keygen, list,
    publish, register, sign, stats, verify, version,
};

/// Valid status values for diff jobs.
const VALID_DIFF_STATUSES: &[&str] = &["pending", "running", "completed", "failed"];

/// Validate diff status value for clap argument parser.
fn validate_diff_status(s: &str) -> Result<String, String> {
    if VALID_DIFF_STATUSES.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid status '{}'. Valid values are: {}",
            s,
            VALID_DIFF_STATUSES.join(", ")
        ))
    }
}

#[derive(Parser)]
#[command(name = "deltaship-publisher")]
#[command(author, version, about = "Deltaship Publisher Toolkit - Manage binary releases", long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
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
    /// Initialize a new Deltaship project in the current directory
    Init {
        /// Passphrase for encrypting the signing key (prompts interactively if not provided)
        #[arg(short, long)]
        passphrase: Option<String>,
    },

    /// Generate a new Ed25519 signing keypair
    Keygen {
        /// Output directory for key files (defaults to .deltaship/keys/)
        #[arg(short, long)]
        output_dir: Option<String>,

        /// Passphrase for encrypting the signing key (prompts if not provided)
        #[arg(short, long)]
        passphrase: Option<String>,
    },

    /// Register a new binary version
    Register {
        /// Binary name (e.g., "myapp")
        #[arg(short, long)]
        name: String,

        /// Version string (semver, e.g., "1.0.0")
        #[arg(short = 'V', long)]
        version: String,

        /// Path to the binary file
        #[arg(short, long)]
        file: String,

        /// Target platform (e.g., "linux-x86_64", "windows-x86_64")
        #[arg(short, long)]
        platform: String,

        /// Optional description for the binary
        #[arg(short, long)]
        description: Option<String>,

        /// Skip automatic diff generation from previous versions
        #[arg(long)]
        no_diff: bool,
    },

    /// Sign a registered version
    Sign {
        /// Version ID to sign (alternative to --name + --version)
        #[arg(long)]
        version_id: Option<String>,

        /// Binary name (use with --version)
        #[arg(short, long)]
        name: Option<String>,

        /// Version string (use with --name)
        #[arg(short = 'V', long)]
        version: Option<String>,

        /// Path to signing key file (defaults to .deltaship/keys/signing.key)
        #[arg(short, long)]
        key_file: Option<String>,

        /// Passphrase for the signing key (prompts interactively if not provided)
        #[arg(short, long)]
        passphrase: Option<String>,
    },

    /// List registered binaries and versions
    List {
        /// Filter by binary name
        #[arg(short, long)]
        name: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Publish a signed version to the update server
    Publish {
        /// Binary name (e.g., "myapp")
        #[arg(short, long)]
        name: String,

        /// Version string (semver, e.g., "1.0.0")
        #[arg(short = 'V', long)]
        version: String,

        /// Server URL (overrides config)
        #[arg(short, long)]
        server_url: Option<String>,

        /// API key for authentication
        #[arg(short, long)]
        api_key: Option<String>,

        /// Skip confirmation prompt for non-default server URLs
        #[arg(short, long)]
        yes: bool,
    },

    /// Generate a diff between two versions
    Diff {
        /// Binary name (e.g., "myapp")
        #[arg(short, long)]
        name: String,

        /// Source version (e.g., "1.0.0")
        #[arg(long)]
        from_version: String,

        /// Target version (e.g., "1.1.0")
        #[arg(long)]
        to_version: String,

        /// Target platform (e.g., "linux-x86_64")
        #[arg(short, long, default_value = "linux-x86_64")]
        platform: String,

        /// Output path for the diff file (defaults to .deltaship/diffs/)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// List diff jobs
    DiffList {
        /// Filter by binary name
        #[arg(short, long)]
        name: Option<String>,

        /// Filter by status. Valid values: pending, running, completed, failed
        #[arg(short, long, value_parser = validate_diff_status)]
        status: Option<String>,
    },

    /// Manage configuration settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Show project information and status
    Info,

    /// Clean up orphaned files and failed jobs
    Cleanup {
        /// Show what would be removed without removing anything
        #[arg(long)]
        dry_run: bool,

        /// Maximum age in days for diff files (default: 90)
        #[arg(long)]
        max_age_days: Option<u64>,
    },

    /// Keep only the last N versions of a binary
    Prune {
        /// Binary name to prune
        #[arg(short, long)]
        name: String,

        /// Number of versions to keep
        #[arg(short, long)]
        keep: usize,

        /// Delete associated files (binaries and diffs)
        #[arg(long)]
        delete_files: bool,

        /// Show what would be removed without removing anything
        #[arg(long)]
        dry_run: bool,
    },

    /// Verify registered versions have valid signatures and checksums
    Verify {
        /// Binary name to verify
        #[arg(short, long)]
        name: String,

        /// Specific version to verify (omit to verify all versions)
        #[arg(short = 'V', long)]
        version: Option<String>,

        /// Auto-fix checksum mismatches by recalculating from file
        #[arg(long)]
        fix: bool,

        /// Output results as JSON
        #[arg(long)]
        json: bool,
    },

    /// Export project data to an archive for backup or migration
    Export {
        /// Output path for the archive (will add .tar.gz extension if needed)
        #[arg(short, long)]
        output: String,

        /// Include signing keys in the export (SECURITY WARNING: handle with care)
        #[arg(long)]
        include_keys: bool,

        /// Include binary files in the export
        #[arg(long)]
        include_binaries: bool,

        /// Output format (currently only 'tar' for tar.gz is supported)
        #[arg(long, default_value = "tar")]
        format: String,
    },

    /// Import project data from an archive
    Import {
        /// Input archive path
        #[arg(short, long)]
        input: String,

        /// Merge with existing data (add new, keep existing)
        #[arg(long)]
        merge: bool,

        /// Overwrite existing data (DESTRUCTIVE)
        #[arg(long)]
        overwrite: bool,
    },

    /// Show detailed version information
    Version,

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Show bandwidth savings statistics
    Stats {
        /// Filter by binary name (omit for all binaries)
        #[arg(short, long)]
        name: Option<String>,
    },
}

/// Configuration subcommands
#[derive(Subcommand)]
enum ConfigAction {
    /// Get configuration value(s)
    Get {
        /// Configuration key to get (omit for all values)
        key: Option<String>,
    },

    /// Set a configuration value
    Set {
        /// Configuration key to set
        key: String,

        /// Value to set
        value: String,
    },

    /// List all configuration keys with descriptions
    List,

    /// Reset configuration to default value(s)
    Reset {
        /// Configuration key to reset (omit for all)
        key: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_logging(cli.verbose, cli.quiet);

    let result = match cli.command {
        Commands::Init { passphrase } => init::run(passphrase).await,
        Commands::Keygen {
            output_dir,
            passphrase,
        } => keygen::run(output_dir, passphrase).await,
        Commands::Register {
            name,
            version,
            file,
            platform,
            description,
            no_diff,
        } => register::run(name, version, file, platform, description, no_diff).await,
        Commands::Sign {
            version_id,
            name,
            version,
            key_file,
            passphrase,
        } => sign::run(version_id, name, version, key_file, passphrase).await,
        Commands::List { name, json } => list::run(name, json).await,
        Commands::Publish {
            name,
            version,
            server_url,
            api_key,
            yes,
        } => publish::run(name, version, server_url, api_key, yes).await,
        Commands::Diff {
            name,
            from_version,
            to_version,
            platform,
            output,
        } => diff::run(name, platform, from_version, to_version, output).await,
        Commands::DiffList { name, status } => diff::run_list(name, status).await,
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => config_cmd::run_get(key).await,
            ConfigAction::Set { key, value } => config_cmd::run_set(key, value).await,
            ConfigAction::List => config_cmd::run_list().await,
            ConfigAction::Reset { key } => config_cmd::run_reset(key).await,
        },
        Commands::Info => info::run().await,
        Commands::Cleanup {
            dry_run,
            max_age_days,
        } => cleanup::run_cleanup(dry_run, max_age_days).await,
        Commands::Prune {
            name,
            keep,
            delete_files,
            dry_run,
        } => cleanup::run_prune(name, keep, delete_files, dry_run).await,
        Commands::Verify {
            name,
            version,
            fix,
            json,
        } => verify::run(name, version, fix, json).await,
        Commands::Export {
            output,
            include_keys,
            include_binaries,
            format,
        } => export::run(output, include_keys, include_binaries, format).await,
        Commands::Import {
            input,
            merge,
            overwrite,
        } => import::run(input, merge, overwrite).await,
        Commands::Version => {
            version::run();
            Ok(())
        }
        Commands::Completions { shell } => {
            completions::run(shell, &mut Cli::command());
            Ok(())
        }
        Commands::Stats { name } => stats::run(name).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
