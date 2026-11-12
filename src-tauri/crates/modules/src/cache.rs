//! The engine, and the components compiled once: Cranelift takes a second or more for a module's
//! component, so its native code is kept by the fingerprint of its `.wasm` and of this engine
//! (another wasmtime, another CPU: another key), in Prism's cache folder.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use wasmtime::component::Component;
use wasmtime::{Config, Engine};

/// Components are interpreted (Pulley), not compiled to native code: see `engine`.
pub const INTERPRETED: bool = cfg!(target_os = "macos");

/// The engine's clock for deadlines (`epoch_interruption`): one tick every 100 ms.
pub const EPOCH: Duration = Duration::from_millis(100);

/// wasmtime's errors are its own type.
pub fn w<T>(r: wasmtime::Result<T>) -> Result<T> {
    r.map_err(|e| anyhow!("{e:?}"))
}

/// One engine for every module; its epoch advances on a thread of its own, so that a call running
/// past its deadline (a loop that never ends) stops with a trap instead of holding its thread.
pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let mut config = Config::new();
        config.epoch_interruption(true);
        // macOS: the app runs under the hardened runtime, which kills a process running code it
        // did not sign (`SIGKILL (Code Signature Invalid)`): components run on Pulley,
        // wasmtime's interpreter, without executable memory. Elsewhere, native code.
        if INTERPRETED {
            config.target("pulley64").expect("pulley64");
        }
        let engine = Engine::new(&config).expect("wasmtime engine");
        let ticking = engine.clone();
        let _ = std::thread::Builder::new().name("modules epoch".into()).spawn(move || {
            loop {
                std::thread::sleep(EPOCH);
                ticking.increment_epoch();
            }
        });
        engine
    })
}

fn folder() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join("prism-modules-cache-test");
    }
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join(prism_core::profile::IDENTIFIER).join("modules")
}

/// The component of `file`, from the cache when this engine compiled it before.
pub fn component(file: &Path) -> Result<Component> {
    component_in(&folder(), file)
}

pub fn component_in(cache: &Path, file: &Path) -> Result<Component> {
    let bytes = std::fs::read(file).with_context(|| file.display().to_string())?;
    let mut hash = Sha256::new();
    hash.update(&bytes);
    std::hash::Hash::hash(&engine().precompile_compatibility_hash(), &mut Hasher(&mut hash));
    let key: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let cached = cache.join(format!("{key}.cwasm"));
    let t0 = Instant::now();
    if cached.is_file() {
        // SAFETY: wasmtime runs what the file holds as native code. This one is Prism's own
        // writing (below), in the user's cache folder, named by the hash of the `.wasm` it was
        // compiled from and of this engine: one another engine wrote has another name.
        match unsafe { Component::deserialize_file(engine(), &cached) } {
            Ok(c) => {
                tracing::info!("module {}: from the cache in {:?}", file.display(), t0.elapsed());
                return Ok(c);
            }
            Err(e) => tracing::warn!("module {}: cache unreadable, compiled again: {e:?}", file.display()),
        }
    }
    let native = w(engine().precompile_component(&bytes)).with_context(|| file.display().to_string())?;
    tracing::info!("module {}: compiled in {:?}", file.display(), t0.elapsed());
    // Written apart, then renamed: a Prism reading it meanwhile never sees half of it.
    if std::fs::create_dir_all(cache).is_ok() {
        let part = cache.join(format!("{key}.{}.part", std::process::id()));
        if std::fs::write(&part, &native).and_then(|()| std::fs::rename(&part, &cached)).is_err() {
            let _ = std::fs::remove_file(&part);
        }
    }
    // SAFETY: `native` is what this engine just compiled, in memory.
    w(unsafe { Component::deserialize(engine(), &native) })
}

/// `Hash` into the digest.
struct Hasher<'a>(&'a mut Sha256);

impl std::hash::Hasher for Hasher<'_> {
    fn finish(&self) -> u64 {
        0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }
}
