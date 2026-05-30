//! Deltaship Client Patcher Library
//!
//! This crate provides the core functionality for the Deltaship Client Patcher,
//! a background daemon that checks for updates and applies them to managed binaries.
//!
//! # Components
//!
//! - **config**: Configuration loading and management
//! - **checker**: Update checking against Deltaship servers
//! - **downloader**: File downloading with resume support
//! - **patcher**: Update application with backup/rollback
//! - **daemon**: Background daemon loop
//! - **commands**: CLI command implementations
//! - **audit**: Audit logging for sensitive operations

pub mod audit;
pub mod checker;
pub mod commands;
pub mod config;
pub mod daemon;
pub mod downloader;
pub mod patcher;

// Re-export primary types
pub use checker::{UpdateChecker, UpdateInfo};
pub use config::{load_config, ClientConfig};
pub use daemon::{run_check_once, run_daemon};
pub use patcher::{apply_update, rollback_to_backup};
