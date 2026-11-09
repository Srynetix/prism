//! The trust given to custom blocks (`prism_core::trust`): for each one, the hash of what it runs
//! as I last accepted it, that declaration as TOML (the diff of a version changed since) and whole
//! (what a pack's block runs until its update is accepted, what "Back to the trusted version"
//! writes back). Kept in Prism's data folder, never in `config.toml`: writing the config is not
//! enough to be trusted.

use std::path::{Path, PathBuf};

use anyhow::Result;

pub use prism_core::trust::{Store, Trusted};

use super::{config::Def, diff};

/// One lock for the file: the window's trust and the collector's reads never cross.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn path(data: &Path) -> PathBuf {
    data.join("custom-blocks-trust.json")
}

pub fn load(data: &Path) -> Store {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    read(data)
}

fn read(data: &Path) -> Store {
    let mut store = prism_core::trust::read(&path(data));
    // Trusted before the store kept its text: its declaration's.
    for t in store.trusted.values_mut().filter(|t| t.text.is_empty()) {
        if let Some(d) = def(t) {
            t.text = diff::text(&d);
        }
    }
    store
}

/// Changes the store under its lock, and writes it.
pub fn update<T>(data: &Path, f: impl FnOnce(&mut Store) -> T) -> Result<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read(data);
    let out = f(&mut store);
    store.write(&path(data))?;
    Ok(out)
}

/// `def` trusted as it is.
pub fn trust(store: &mut Store, def: &Def) {
    store.trust(&def.id, &def.hash(), &diff::text(def), serde_json::to_value(def).unwrap_or_default());
}

/// The declaration trusted.
pub fn def(t: &Trusted) -> Option<Def> {
    serde_json::from_value(t.data.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_is_kept_by_hash() {
        let data = std::env::temp_dir().join(format!("prism-custom-trust-{}", std::process::id()));
        let def: Def = toml::from_str("id = \"a\"\ncommand = [\"x\"]\n").unwrap();
        update(&data, |s| trust(s, &def)).unwrap();
        let store = load(&data);
        assert_eq!(store.trusted["a"].hash, def.hash());
        assert_eq!(super::def(&store.trusted["a"]), Some(def.clone()));
        assert!(store.pending("a", &def.hash(), &diff::text(&def)).is_none());
        let _ = std::fs::remove_dir_all(&data);
    }

    /// The file as written before the store was shared: still trusted, its text made from `def`.
    #[test]
    fn former_file_still_trusted() {
        let data = std::env::temp_dir().join(format!("prism-custom-trust-old-{}", std::process::id()));
        let def: Def = toml::from_str("id = \"a\"\ncommand = [\"x\"]\n").unwrap();
        let old = serde_json::json!({ "trusted": { "a": { "hash": def.hash(), "def": def, "at": "2026-10-01T09:00:00Z" } }, "seen": {} });
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(path(&data), old.to_string()).unwrap();
        let store = load(&data);
        assert!(store.pending("a", &def.hash(), &diff::text(&def)).is_none());
        assert_eq!(store.trusted["a"].text, diff::text(&def));
        assert_eq!(super::def(&store.trusted["a"]), Some(def));
        let _ = std::fs::remove_dir_all(&data);
    }
}
