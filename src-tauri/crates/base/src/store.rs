use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use anyhow::{bail, Context, Result};
use crate::tr;

/// Small JSON documents of the frontend (notes, layout, retro points, pet, usage counts), stored in the app's
/// data folder rather than in the webview's storage: they survive a cache cleanup.
const ALLOWED: [&str; 7] = ["notes", "layout", "retro", "pet", "feedback", "prefs", "usage"];

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The folder of the documents: the profile's data.
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        if !ALLOWED.contains(&name) {
            bail!("unknown store \"{name}\"");
        }
        Ok(self.dir.join(format!("{name}.json")))
    }

    pub fn load(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let path = self.path(name)?;
        match std::fs::read(&path) {
            Ok(raw) => Ok(Some(serde_json::from_slice(&raw).with_context(|| tr!("{path} is not valid JSON", path = path.display()))?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| tr!("reading {path}", path = path.display())),
        }
    }

    /// Atomic write: temporary file then rename, so as never to leave a truncated JSON.
    pub fn save(&self, name: &str, value: &serde_json::Value) -> Result<()> {
        let path = self.path(name)?;
        let lock = lock(&path);
        let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
        self.write(&path, value)
    }

    /// Load, change, save, the document held meanwhile: two writers of its parts (the retro's
    /// points, its sprints kept) never lose each other's. `f` gets it as stored and returns it to
    /// write, or None to leave it; whether it was written.
    pub fn update(&self, name: &str, f: impl FnOnce(Option<serde_json::Value>) -> Result<Option<serde_json::Value>>) -> Result<bool> {
        let path = self.path(name)?;
        let lock = lock(&path);
        let _held = lock.lock().unwrap_or_else(|e| e.into_inner());
        match f(self.load(name)?)? {
            Some(value) => self.write(&path, &value).map(|()| true),
            None => Ok(false),
        }
    }

    fn write(&self, path: &Path, value: &serde_json::Value) -> Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// The lock of a document, by its path: whichever `Store` reaches it (the commands' and the
/// routines' are two).
fn lock(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);
    LOCKS.lock().unwrap_or_else(|e| e.into_inner()).entry(path.to_path_buf()).or_default().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_unknown_names() {
        let dir = std::env::temp_dir().join(format!("prism-store-{}", std::process::id()));
        let store = Store::new(dir.clone());
        assert!(store.load("notes").unwrap().is_none());
        store.save("notes", &serde_json::json!([{ "text": "hello" }])).unwrap();
        assert_eq!(store.load("notes").unwrap().unwrap()[0]["text"], "hello");
        assert!(store.save("../etc", &serde_json::json!(1)).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn updates_never_lose_one_another() {
        let dir = std::env::temp_dir().join(format!("prism-store-update-{}", std::process::id()));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    // Two stores on the same folder, as the commands' and the routines' are.
                    let store = Store::new(dir);
                    for _ in 0..20 {
                        store.update("retro", |v| Ok(Some(serde_json::json!({ "n": v.and_then(|v| v["n"].as_u64()).unwrap_or(0) + 1 })))).unwrap();
                    }
                })
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        let store = Store::new(dir.clone());
        assert_eq!(store.load("retro").unwrap().unwrap()["n"], 160);
        assert!(!store.update("retro", |_| Ok(None)).unwrap());
        let _ = std::fs::remove_dir_all(dir);
    }
}
