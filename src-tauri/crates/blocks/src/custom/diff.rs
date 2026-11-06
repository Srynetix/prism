//! What changed in a custom block's declaration since it was trusted, line by line, shown before
//! trusting it again.

pub use prism_core::textdiff::{lines, Line};

/// A declaration as TOML, as one would read it in config.toml.
pub fn text(def: &super::Def) -> String {
    toml::to_string(def).unwrap_or_default()
}
