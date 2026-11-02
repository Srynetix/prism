//! Prism's model, without Tauri: the config and what it resolves, the snapshot the window shows
//! and the hub that publishes it, and the contracts the upper crates fill (blocks, tracker and
//! forge providers).

pub mod agents;
pub mod blocks;
pub mod config;
pub mod conventions;
pub mod deps;
pub mod detect;
pub mod forge;
pub mod images;
pub mod logs;
pub mod packs;
pub mod problems;
pub mod profile;
pub mod remote;
pub mod settings;
pub mod stages;
pub mod state;
pub mod ticket;
pub mod tracker;

use prism_base::{git, http, platform};
