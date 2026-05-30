//! CLI commands for Deltaship Client.

pub mod add;
pub mod cleanup;
pub mod completions;
pub mod list;
pub mod remove;
pub mod rollback;
pub mod service;
pub mod status;
pub mod update;
pub mod version;

pub use add::add_binary;
pub use cleanup::{run_cleanup, run_prune_backups};
pub use list::list_binaries;
pub use remove::remove_binary;
pub use rollback::rollback;
pub use status::show_status;
pub use update::update;
