use std::path::PathBuf;

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
        std::fs::create_dir_all(&self.dir)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }
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
}
