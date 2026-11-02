//! `prism-server`: Prism without a window, for a server reached from a phone only. `serve` runs it
//! (systemd), `pair` pairs a phone from the terminal; the app's other subcommands (`hook`, `pack`,
//! `restart`, the tokens) are the same.

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

const USAGE: &str = "usage: prism-server [--profile <id>] serve | pair | restart | pack … | clickup-token | jira-token | google-secret";

fn main() {
    blocks::register();
    // `prism-server restart` and `watch` start it running Prism, not printing its usage.
    prism_core::profile::set_start_args(&["serve"]);
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // `prism-server --profile <id> …`: the profile, before the subcommands.
    prism_core::profile::take_arg(&mut args);
    if let Some(code) = prism_commands::cli::run(&args) {
        std::process::exit(code);
    }
    let code = match args.get(1).and_then(|a| a.to_str()) {
        Some("serve") => prism_commands::serve::run(page::asset, env!("PRISM_BUILD").to_string()),
        Some("help" | "-h" | "--help") => {
            println!("{USAGE}");
            0
        }
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}
