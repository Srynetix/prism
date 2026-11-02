//! The blocks compiled into this Prism: Prism's own (`prism-blocks`), then those of another
//! repository (a team's own), compiled in when `PRISM_EXTRA_BLOCKS` names its folder at build time
//! (see `build.rs`): its `src-tauri/mod.rs` becomes the module `blocks::extra` and lists them in
//! `BLOCKS`. Such a block reaches the core through `blocks::sdk` only; its dependencies are the
//! app's crates.

pub use prism_blocks::*;

// `extra`: the blocks of `PRISM_EXTRA_BLOCKS`, or none.
include!(concat!(env!("OUT_DIR"), "/extra_blocks.rs"));

/// Gives the core every block, before anything reads them.
pub fn register() {
    prism_commands::process::register_blocks(extra_blocks());
}
