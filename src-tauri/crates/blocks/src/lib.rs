//! Prism's own blocks (their contract: `prism_core::blocks`, re-exported here), and what a block
//! of another repository may use of Prism (`sdk`). The app registers them, a team's after them.

pub use prism_core::blocks::*;

mod alerts;
pub mod changes;
pub mod custom;
pub mod data;
pub mod sdk;
pub mod dev_servers;
pub mod disk_usage;
pub mod docker;
pub mod pet;
pub mod sound;
pub mod status;
pub mod system;
pub mod tokens;
pub mod trail;

use prism_base::{git, http, platform};
use prism_core::{config, profile, stages, state, ticket, tracker};
use prism_collectors::{self as collectors, forge};
use prism_agents as agents;

/// Prism's own blocks.
pub fn builtin() -> [&'static dyn Block; 12] {
    [&docker::BLOCK, &changes::BLOCK, &custom::BLOCK, &disk_usage::BLOCK, &dev_servers::BLOCK, &data::BLOCK, &trail::BLOCK, &pet::BLOCK, &status::BLOCK, &tokens::BLOCK, &system::BLOCK, &sound::BLOCK]
}

#[cfg(test)]
mod tests {
    /// Every table of the example and demo configs is read, by the core or one of Prism's own
    /// blocks; another is reported.
    #[test]
    fn tables_nothing_reads() {
        prism_core::blocks::register(super::builtin().to_vec());
        let defaults = prism_core::config::defaults();
        for text in [include_str!("../../../../config.example.toml"), prism_core::profile::DEMO_CONFIG] {
            assert_eq!(prism_core::settings::unread(&text.parse().unwrap(), &defaults), Vec::<String>::new());
        }
        assert_eq!(prism_core::settings::unread(&"[nope]\na = 1\n[gitlab]\n".parse().unwrap(), &defaults), ["nope"]);
    }
}
