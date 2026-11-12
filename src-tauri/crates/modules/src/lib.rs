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

use std::sync::{Arc, OnceLock};

use prism_core::blocks::Block;

mod bindings {
    wasmtime::component::bindgen!({ world: "block", path: "../module/wit" });
}

/// The modules with a back end, as `blocks` found them: `trusted` reaches them.
static MODULES: OnceLock<Vec<Arc<worker::Module>>> = OnceLock::new();

/// The blocks of the active profile's modules that have a back end, those Prism loads (not
/// refused: their contract, the Prism they need, their component inside them). Read once, as the
/// core first reads its blocks (`prism_core::blocks::register_late`). One not trusted yet is
/// registered all the same: its blocks wait, said on them, until I trust it (`trusted`).
pub fn blocks() -> Vec<&'static dyn Block> {
    let root = prism_core::modules::dir();
    let mut out: Vec<&'static dyn Block> = Vec::new();
    let mut modules = Vec::new();
    for info in prism_core::modules::list().into_iter().filter(|m| m.backend && m.refused.is_none()) {
        let folder = root.join(&info.id);
        let spec = match manifest::read(&folder) {
            Ok(spec) => spec,
            Err(e) => {
                tracing::error!("module {}: {e:#}", info.id);
                continue;
            }
        };
        let module = Arc::new(worker::Module::new(info.id.clone(), spec, prism_core::modules::stopped_mark(&root, &info.id), info.trusted));
        out.extend(block::of(&module));
        tracing::info!("module {}: back end {}, blocks {}{}", info.id, module.spec.component.display(), module.spec.blocks.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", "), if info.trusted { "" } else { ", waiting for trust" });
        modules.push(module);
    }
    let _ = MODULES.set(modules);
    out
}

/// Module `id` trusted (Settings › Packs): its back end configured and started now. Whether it
/// has one running in this Prism (installed after launch: once Prism restarts).
pub fn trusted(id: &str) -> bool {
    let Some(m) = MODULES.get().into_iter().flatten().find(|m| m.id == id) else { return false };
    m.trust_now();
    m.warm();
    true
}
