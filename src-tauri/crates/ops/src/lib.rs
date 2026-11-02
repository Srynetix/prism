//! What Prism does around its sources, apart from its window so that it is built and tested
//! without Tauri: a project cloned, created or published (`clones`), a setting checked before it
//! is saved (`checks`), Prism's own updates (`updates`), feedback sent to a project (`feedback`).

pub mod checks;
pub mod clones;
pub mod feedback;
pub mod updates;

// The paths these modules were written with in the app (`crate::config`, `crate::collectors`…).
use prism_base::{git, http, platform, store};
use prism_collectors::{self as collectors, forge};
use prism_core::{agents, config, conventions, deps, logs, packs, settings};
