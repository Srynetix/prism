//! Write a Prism module's back end: its blocks as Prism's own, `impl Block` against `sdk`, built
//! into a WebAssembly component Prism runs in a sandbox.
//!
//! ```ignore
//! // Cargo.toml: crate-type = ["cdylib"]; build: cargo build --release --target wasm32-wasip2
//! pub mod blocks { pub use prism_module::sdk; }   // the block's files read `crate::blocks::sdk`
//! mod hello;                                       // impl Block for Hello { … }
//!
//! static BLOCKS: &[&dyn prism_module::Block] = &[&hello::HELLO];
//! prism_module::module!(BLOCKS);
//! ```
//!
//! What runs differently from inside Prism:
//!
//! - a collector's loop (`loop { read; ctx.pause(every).await }`, spawned from `start`) is
//!   resumed by Prism when its part is due: each `pause` hands control back, its duration saying
//!   when Prism resumes it. Nothing else may wait (no timer, no thread): what Prism gives answers
//!   at once;
//! - git, GitLab, HTTP and programs go through Prism (`host`), each a capability the module's
//!   manifest declares;
//! - a block's settings are the config tables its manifest names (`[wasm] tables`);
//! - `BlockCtx` has no snapshot (`subscribe`, `snapshot`) nor data folder;
//! - its `tracing` lines (the module depends on `tracing` too) go to Prism's log, `module <id>: …`.

mod block;
mod host;
mod log;
#[doc(hidden)]
pub mod i18n;
#[doc(hidden)]
pub mod runtime;
pub mod sdk;
mod types;

pub use block::{section, Block, BlockCtx, BlockEnv, BlockRepo, GitlabConfig, Hints, ImageProvider, LocalRepo, Pause, RemoteClass, RepoUse, Stages, StagesProvider};
pub use i18n::Lang;
#[doc(hidden)]
pub use i18n as __i18n;

#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        world: "block",
        path: "wit",
        pub_export_macro: true,
        export_macro_name: "__export_world",
        default_bindings_module: "prism_module::bindings",
    });
}

#[doc(hidden)]
pub use runtime as __runtime;

/// The component's exports, its blocks answering: `module!(BLOCKS)`, `BLOCKS` a
/// `&'static [&'static dyn Block]`.
#[macro_export]
macro_rules! module {
    ($blocks:expr) => {
        #[doc(hidden)]
        pub struct __PrismModule;

        impl $crate::bindings::Guest for __PrismModule {
            fn configure(config: String, lang: String, demo: bool) -> Result<$crate::bindings::prism::module::types::Configured, String> {
                $crate::__runtime::configure($blocks, config, lang, demo)
            }
            fn start(env: String) -> Result<(), String> {
                $crate::__runtime::start($blocks, env)
            }
            fn tick(part: String) -> $crate::bindings::prism::module::types::Tick {
                $crate::__runtime::tick($blocks, part)
            }
            fn call(block: String, command: String, args: String, demo: bool) -> Result<String, String> {
                $crate::__runtime::call($blocks, block, command, args, demo)
            }
            fn hints(repo: String) -> Result<String, String> {
                $crate::__runtime::hints($blocks, repo)
            }
            fn images(repo: String, project: String, local: String) -> Result<String, String> {
                $crate::__runtime::images($blocks, repo, project, local)
            }
        }

        $crate::bindings::__export_world!(__PrismModule with_types_in $crate::bindings);
    };
}
