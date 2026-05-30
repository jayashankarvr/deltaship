//! Library interface for deltaship-server — exposes internals for integration testing.
//!
//! The binary at `src/main.rs` uses `mod` declarations to pull all modules in.
//! This lib target re-exports those same modules so that `tests/` integration
//! tests can reference `deltaship_server::routes`, `deltaship_server::state`, etc.
//!
//! Nothing here is part of a public API — this exists purely to support testing.

pub mod auth;
pub mod models;
pub mod rate_limit;
pub mod routes;
pub mod state;
pub mod stats;
pub mod storage;
pub mod validation;

// commands is a private detail of the CLI binary; not exposed here.
