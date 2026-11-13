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
/// registered all the same: its blocks wait, said on them, until I trust it (`trusted`); one that
/// stopped Prism the last time is held until I start it again (`reset`).
pub fn blocks() -> Vec<&'static dyn Block> {
    let root = prism_core::modules::dir();
    let mut out: Vec<&'static dyn Block> = Vec::new();
    let mut modules = Vec::new();
    for info in prism_core::modules::list().into_iter().filter(|m| m.backend && m.refused.is_none()) {
        let folder = root.join(&info.id);
        // Trusted: its manifest and component read now, as trusted; it runs those, whatever the
        // disk holds later.
        let read = if info.trusted { worker::load_trusted(&folder, &info.fingerprint).map(|(s, b)| (s, Some(b))) } else { manifest::read(&folder).map(|s| (s, None)) };
        let (spec, bytes) = match read {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("module {}: {e:#}", info.id);
                continue;
            }
        };
        let held = info.stopped.then(|| prism_base::tr!("the module {id} is held: its back end stopped Prism the last time it ran. Start it again from Settings › Packs › Modules", id = info.id));
        let module = Arc::new(worker::Module::new(info.id.clone(), spec, prism_core::modules::stopped_mark(&root, &info.id), bytes, held));
        if info.trusted {
            module.set_version(&info.fingerprint);
        }
        out.extend(block::of(&module));
        tracing::info!("module {}: back end {}, blocks {}{}", info.id, module.spec.component.display(), module.spec.blocks.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", "), if info.trusted { "" } else { ", waiting for trust" });
        modules.push(module);
    }
    let _ = MODULES.set(modules);
    out
}

/// Module `id` trusted as its folder's `fingerprint` (Settings › Packs): its back end, read again
/// as that version (its manifest, its component's bytes), configured and started now; running
/// another version, started again on this one, unless it is held (only `reset` lifts that).
/// Whether it runs in this Prism: installed after launch, or its blocks changed since, once Prism
/// restarts.
pub fn trusted(id: &str, fingerprint: &str) -> bool {
    let Some(m) = module(id) else { return false };
    if !load(m, fingerprint) {
        return false;
    }
    m.restart_version();
    m.warm();
    true
}

fn module(id: &str) -> Option<&'static Arc<worker::Module>> {
    MODULES.get().into_iter().flatten().find(|m| m.id == id)
}

/// `m` given the version `fingerprint` of its folder, read again (its manifest, its bytes).
fn load(m: &worker::Module, fingerprint: &str) -> bool {
    let (spec, bytes) = match worker::load_trusted(&prism_core::modules::dir().join(&m.id), fingerprint) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("module {}: {e:#}", m.id);
            return false;
        }
    };
    if !m.trust_as(spec, bytes) {
        return false;
    }
    m.set_version(fingerprint);
    true
}

/// Module `id` started again from nothing (Settings › Packs › Modules): its crashes forgotten, the
/// mark it left stopping Prism lifted, the hold too; it runs the version trusted on the disk, and
/// nothing while none is (its trust forgotten: removed, put back). Whether it runs in this Prism
/// (one installed after launch, or refused at launch, once Prism restarts).
pub fn reset(id: &str) -> bool {
    prism_core::modules::release(&prism_core::modules::dir(), id);
    let Some(m) = module(id) else { return false };
    let info = prism_core::modules::list().into_iter().find(|i| i.id == id && i.trusted && i.refused.is_none());
    match info {
        Some(info) if m.version().as_deref() == Some(info.fingerprint.as_str()) || load(m, &info.fingerprint) => {
            m.reset();
            m.warm();
            true
        }
        _ => {
            m.untrust();
            m.reset();
            false
        }
    }
}

/// Module `id` removed: held, its blocks saying so, until Prism restarts without it; its trust
/// gone with it.
pub fn removed(id: &str) {
    if let Some(m) = module(id) {
        m.hold(prism_base::tr!("the module {id} was removed: its blocks go once Prism restarts", id = id));
        m.untrust();
    }
}

/// The modules trusted on the disk in another version than the one they run (a pack's update
/// trusted for me): run in that one now. Those whose blocks changed wait for a restart.
pub fn refresh() {
    for info in prism_core::modules::list().into_iter().filter(|i| i.trusted && i.refused.is_none()) {
        let Some(m) = module(&info.id) else { continue };
        if m.version().as_deref() != Some(info.fingerprint.as_str()) && trusted(&info.id, &info.fingerprint) {
            tracing::info!("module {}: runs its version trusted now", info.id);
        }
    }
}

/// How the modules' back ends run in this Prism: held or not, their crashes.
pub fn runs() -> Vec<prism_core::modules::ModuleRun> {
    MODULES.get().into_iter().flatten().map(|m| m.run()).collect()
}
