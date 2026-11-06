//! `prism-server`: Prism without a window, for a server reached from a phone only. `serve` runs it
//! (systemd), `pair` pairs a phone from the terminal; the app's other subcommands (`hook`, `pack`,
//! `restart`, `token`…) are the same, from one table (`prism_commands::cli::args`).

/// The blocks compiled into this Prism: Prism's own (`prism-blocks`), then a team's
/// (`PRISM_EXTRA_BLOCKS`, see `build_shared.rs`), as in the app.
mod blocks {
    pub use prism_blocks::*;

    // `extra`: the blocks of `PRISM_EXTRA_BLOCKS`, or none.
    include!(concat!(env!("OUT_DIR"), "/extra_blocks.rs"));

    /// Gives the core every block, before anything reads them.
    pub fn register() {
        prism_commands::process::register_blocks(extra_blocks());
    }
}

/// The phone's page (`build.rs`).
mod page {
    include!(concat!(env!("OUT_DIR"), "/page.rs"));
}

fn main() {
    blocks::register();
    // `prism-server restart` and `watch` start it running Prism, not printing its help.
    prism_core::profile::set_start_args(&["serve"]);
    // The same subcommands as the app's (`prism_commands::cli`), `serve` its own.
    let info = prism_commands::cli::Info { bin: prism_commands::cli::Bin::Server, version: env!("CARGO_PKG_VERSION"), build: env!("PRISM_BUILD") };
    let code = match prism_commands::cli::run(&info, std::env::args_os().collect()) {
        prism_commands::cli::Run::Exit(code) => code,
        prism_commands::cli::Run::Serve => prism_commands::serve::run(page::asset, env!("PRISM_BUILD").to_string()),
        prism_commands::cli::Run::Window => unreachable!("the server has no window"),
    };
    std::process::exit(code);
}
