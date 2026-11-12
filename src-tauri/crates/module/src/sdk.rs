//! What a block may use of Prism: `blocks::sdk`'s names, for a module's back end. A block's files
//! written against Prism's `crate::blocks::sdk` compile against this one (`pub mod blocks { pub use
//! prism_module::sdk; }`), within what a module is given (see the crate's docs).

// A facade: what a block may use, whether a block uses it yet or not.
#![allow(unused_imports)]

use std::future::Future;

pub use crate::block::{section, Block, BlockCtx, BlockEnv, BlockRepo, RepoUse};

/// The class of a block's command called from a phone (`Block::remote`).
pub use crate::block::RemoteClass;

/// The forge: which one is read (`BlockEnv::forge`).
pub use crate::types::Provider as Forge;

/// The settings the core reads and a block may read too.
pub use crate::block::GitlabConfig;

/// Team conventions: ticket prefix, forge host, main and release branches (`conventions::get()`).
pub use crate::host::conventions;

/// Its messages in the user's language: `tr!("… {x}", x = …)`, their translations given by
/// `Block::messages` for each `Lang`.
pub use crate::i18n::Lang;
pub use crate::tr;

/// A program Prism runs for it (`[wasm] capabilities.exec`), as `tokio::process::Command`.
pub use crate::host::{command, Command, ExitStatus, Output};

/// Git in a clone: `git(dir, args)`, its standard output.
pub use crate::host::git;

/// Ticket ids in texts.
pub use crate::host::ticket;

/// GitLab through Prism: its REST API (`api`, `glab`, `glab_text`), a project's path encoded, and
/// another project whose pipelines' logs the window may read (`allow_log_project`).
pub use crate::host::gitlab;

/// The stages a block can declare (`Block::stages`).
pub use crate::block::{Hints, Stages, StagesProvider};
pub use crate::types::Stage;

/// The images a block can name for Vulnerabilities (`Block::images`).
pub use crate::block::{ImageProvider, LocalRepo};
pub use crate::types::{ImageRef, ImageTag, ImageTarget};

/// A block's lines in Alerts (`Block::alerts`).
pub use crate::types::{Alert, AlertRule, Fix, OpenTarget, Severity};

/// What a block adds to the Changes timeline (`Block::changes`).
pub use crate::types::{ChangeEvent, ChangeKind, ChangeLink, ChangeMr, ChangeSeverity};

/// A clone's default branch (`origin/HEAD`, otherwise `main`).
pub use crate::host::default_branch;

/// A value set again on each start (`Swap`), and what a block's collector spawns (`spawn`): its
/// loop, resumed by Prism at each `ctx.pause`.
pub use crate::runtime::{JoinHandle, Swap};

pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let flags = crate::runtime::spawn(Box::pin(async move {
        let _ = future.await;
    }));
    JoinHandle { flags, _output: std::marker::PhantomData }
}
