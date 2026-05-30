//! Shell completion generation command

use clap::Command;
use clap_complete::{generate, Shell};

/// Generate shell completions for the specified shell
pub fn run(shell: Shell, cmd: &mut Command) {
    generate(shell, cmd, "deltaship-client", &mut std::io::stdout());
}
