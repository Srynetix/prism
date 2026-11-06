//! What a block may use of Prism: a block of another repository (`PRISM_EXTRA_BLOCKS`) is
//! compiled inside Prism's app crate, so it could reach anything; this is what stays stable for it.
//! Besides it, a block uses Prism's crates (serde, tokio, chrono, reqwest…).

// A facade: what a block may use, whether a block uses it yet or not.
#![allow(unused_imports)]

pub use super::{section, Block, BlockCtx, BlockEnv, BlockRepo, RepoUse};

/// A block's settings as a JSON schema (`Block::schema`: `schemars::schema_for!(MyConfig)`), its
/// struct deriving `schemars::JsonSchema` (`#[cfg_attr(feature = "schema", …)]` is not needed: the
/// app is always built with it).
#[cfg(feature = "schema")]
pub use schemars;

/// The class of a block's command called from a phone (`Block::remote`).
pub use prism_core::remote::Class as RemoteClass;

/// The forge: which one is read (`BlockEnv::forge`).
pub use crate::forge::Provider as Forge;

/// The settings the core reads and a block may read too.
pub use crate::config::GitlabConfig;

/// Team conventions: ticket prefix, forge host, main and release branches (`conventions::get()`),
/// the forge's web and registry addresses, the latest release branch of a clone.
pub use prism_core::conventions;

/// What a block's collector spawns (`spawn`: stopped with it when the config changes), and a value
/// set again on each start (`Swap`, rather than a `OnceLock`): a block that `reloads` uses them.
pub use prism_base::rt::{spawn, Swap};

/// Its messages in the user's language: `tr!("… {x}", x = …)`, their translations given by
/// `Block::messages` for each `Lang`.
pub use prism_base::i18n::Lang;
pub use prism_base::tr;

/// A program run with the login shell's environment (PATH…).
pub use crate::platform::command;

/// Git in a clone: `git(dir, args)`, its standard output.
pub use crate::git::run as git;

/// Ticket ids in texts (`[tracker] ticket_prefix`).
pub use prism_core::ticket;

/// GitLab through glab: its REST API (`api`, `glab`, `glab_text`), a project's path encoded, and
/// another project whose pipelines' logs the window may read (`allow_log_project`).
pub mod gitlab {
    pub use crate::collectors::gitlab::{allow_log_project, api, encode, glab, glab_text};
}

/// The stages a block can declare (`Block::stages`): their names and environments, and what they
/// tell of the workloads.
pub use crate::stages::{Hints, Provider as StagesProvider, Stage, Stages};

/// The images a block can name for Vulnerabilities (`Block::images`), and the repositories with a
/// local clone it may read them in.
pub use crate::collectors::vulnerabilities::{ImageProvider, ImageRef, ImageTag, ImageTarget, LocalRepo};

/// What a block adds to the Changes timeline (`Block::changes`).
pub use prism_core::changes::{ChangeEvent, ChangeKind, ChangeLink, ChangeMr, ChangeSeverity};

/// A clone's default branch (`origin/HEAD`, otherwise `main`).
pub use crate::collectors::repos::default_branch;
