//! What a Prism process does before Prism itself starts, with its window or without: the same
//! steps, in the same order, for both binaries.

use crate::{config_path, logs, profile};

/// Gives the core every block, Prism's own then a team's (`extra`, compiled into the binary from
/// `PRISM_EXTRA_BLOCKS`), before anything reads them.
pub fn register_blocks(extra: &[&'static dyn prism_blocks::Block]) {
    prism_core::blocks::register(prism_blocks::builtin().into_iter().chain(extra.iter().copied()).collect());
    // The profile's modules (`modules/`, WebAssembly back ends), once the profile is known.
    prism_core::blocks::register_late(prism_modules::blocks);
}

/// To be called before any thread starts (the runtime's workers among them): the login shell's
/// PATH and SSH agent (git, glab, kubectl…), and the profile for the processes Prism starts.
pub fn before_threads() {
    prism_base::shell_env::adopt_login_env();
    profile::export();
}

/// The runtime given to the library crates, Prism's folders made private, the log opened, the
/// profile claimed: `Some(pid)` when another Prism already has it open (this one then goes no
/// further). Also the single crypto provider kube and reqwest share.
pub fn start(runtime: tokio::runtime::Handle) -> Option<u32> {
    prism_base::rt::init(runtime);
    // Prism's folders, the owner's only, before anything is written in them.
    profile::private_dirs();
    // Standard error and the profile's log file (Settings › General › Logs).
    logs::init(&config_path());
    match profile::claim() {
        Ok(Some(pid)) => return Some(pid),
        Ok(None) => {}
        Err(e) => tracing::warn!("profile lock: {e:#}"),
    }
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    None
}
