//! Deltaship Publisher Toolkit
//!
//! A CLI tool for software publishers to manage binary releases.
//!
//! # Commands
//!
//! - `init` - Initialize a new Deltaship project
//! - `keygen` - Generate Ed25519 signing keypair
//! - `register` - Register a new binary version
//! - `sign` - Sign a registered version
//! - `list` - List registered binaries and versions
//! - `publish` - Publish a signed version to the update server
//! - `diff` - Generate a diff between two versions
//! - `diff-list` - List diff jobs

pub mod api_client;
pub mod archive;
pub mod commands;
pub mod config;
pub mod diff_manager;
pub mod utils;
