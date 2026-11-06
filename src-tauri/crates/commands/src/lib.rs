//! Prism running, without its window: the state the commands share (`app`), the collectors
//! started and restarted with the config (`bootstrap`), the commands and their table (`commands`),
//! and what a phone reaches of them (`remote`). The window's app registers the commands with Tauri
//! (feature `tauri`) and gives what only it has through `Shell`; a server without a window
//! (`serve`) calls them as a phone does (`commands::phone_call`). Both run the same subcommands
//! (`cli`).

pub mod app;
pub mod bootstrap;
pub mod cli;
pub mod commands;
pub mod mcp;
mod presence;
pub mod process;
pub mod remote;
pub mod serve;

use prism_agents as agents;
use prism_base::{fonts, git, platform, store};
use prism_collectors::{self as collectors, forge, tracker};
use prism_core::{config, conventions, deps, detect, logs, packs, profile, settings, stages, state};
use prism_demo as demo;
use prism_ops::{checks, clones, feedback, updates};

pub use app::{AppState, Shell};

/// The blocks as the app registered them (`prism_core::blocks`): Prism's own and a team's.
mod blocks {
    pub use prism_blocks::*;

    /// Prism's own, for the tests (the app registers a team's too).
    #[cfg(test)]
    pub fn register() {
        prism_core::blocks::register(builtin().to_vec());
    }
}

/// The active profile's `config.toml` (`~/.config/prism/config.toml` for the default one), or
/// `PRISM_CONFIG`.
pub fn config_path() -> std::path::PathBuf {
    std::env::var("PRISM_CONFIG").map(std::path::PathBuf::from).unwrap_or_else(|_| profile::config_of(profile::active()))
}
