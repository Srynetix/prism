//! The back ends of a team's modules (`modules/<id>/`, see `prism_core::modules`): WebAssembly
//! components, run by wasmtime in a sandbox, as the core's ordinary blocks.
//!
//! A module declares its back end in `module.toml`:
//!
//! ```toml
//! [wasm]
//! component = "backend.wasm"
//! tables = ["shipping", "gitlab"]          # the only config tables it is given
//! capabilities = { git = true, gitlab = true, http = ["registry.example.org"], exec = ["gcloud"] }
//!
//! [[wasm.blocks]]
//! id = "shipping"
//! parts = ["deploys", "rollouts"]          # its sources read apart (their errors, their pace)
//! repos = "one"                            # none | one | many (Block::repos)
//! reloads = true                           # a change of the config configures it anew
//! provides = ["stages", "images"]
//! remote = { tags = "read", redeploy = "critical" }   # its commands from a phone (never otherwise)
//! ```
//!
//! The component (the `prism:module` world, `../module/wit/prism-module.wit`) is configured with those
//! tables, started with what the core resolved for each of its blocks, then ticked part by part,
//! each tick saying when the next is due. It runs on a thread of its own (`worker`); the window's
//! calls, the stages' hints and the images go to that thread. It reaches nothing but `host`, each
//! function a capability its manifest declares (`host`), and WASI's clocks and randomness: no file,
//! no network, no environment. Compiled once, it is kept in the cache (`cache`).

mod block;
// Its only `unsafe`: a compiled component read back (`Component::deserialize`).
#[allow(unsafe_code)]
mod cache;
mod host;
mod manifest;
mod worker;

#[cfg(test)]
mod tests;

/// The example module's block (`examples/modules/hello/backend/src/hello.rs`), written against
/// `crate::blocks::sdk`: here Prism's own, as a block compiled into Prism reads it.
#[cfg(test)]
mod blocks {
    pub use prism_blocks::sdk;
}
#[cfg(test)]
#[path = "../../../../examples/modules/hello/backend/src/hello.rs"]
mod hello_inside_prism;

use std::sync::Arc;

use prism_core::blocks::Block;

mod bindings {
    wasmtime::component::bindgen!({ world: "block", path: "../module/wit" });
}

/// The blocks of the active profile's modules that have a back end, those Prism loads (not
/// refused: their contract, the Prism they need, their component inside them). Read once, as the
/// core first reads its blocks (`prism_core::blocks::register_late`).
pub fn blocks() -> Vec<&'static dyn Block> {
    let root = prism_core::modules::dir();
    let mut out: Vec<&'static dyn Block> = Vec::new();
    for info in prism_core::modules::list_in(&root).into_iter().filter(|m| m.backend && m.refused.is_none()) {
        let folder = root.join(&info.id);
        let spec = match manifest::read(&folder) {
            Ok(spec) => spec,
            Err(e) => {
                tracing::error!("module {}: {e:#}", info.id);
                continue;
            }
        };
        let module = Arc::new(worker::Module::new(info.id.clone(), spec, prism_core::modules::stopped_mark(&root, &info.id)));
        out.extend(block::of(&module));
        tracing::info!("module {}: back end {}, blocks {}", info.id, module.spec.component.display(), module.spec.blocks.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", "));
    }
    out
}
