//! The trust given to custom blocks: for each one, the hash of what it runs as I last accepted it,
//! and that definition whole (what a pack's block runs until its update is accepted, what
//! "Back to the trusted version" writes back). Kept in Prism's data folder, never in
//! `config.toml`: writing the config is not enough to be trusted.

use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::config::Def;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub trusted: BTreeMap<String, Trusted>,
    /// When a version waiting for trust was first seen, and the agents running then.
    #[serde(default)]
    pub seen: BTreeMap<String, Seen>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trusted {
    pub hash: String,
    pub def: Def,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seen {
    pub hash: String,
    pub at: DateTime<Utc>,
    #[serde(default)]
    pub agents: Vec<String>,
}

/// One lock for the file: the window's trust and the collector's reads never cross.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn path(data: &std::path::Path) -> PathBuf {
    data.join("custom-blocks-trust.json")
}

pub fn load(data: &std::path::Path) -> Store {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    read(data)
}

fn read(data: &std::path::Path) -> Store {
    std::fs::read_to_string(path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Changes the store under its lock, and writes it.
pub fn update<T>(data: &std::path::Path, f: impl FnOnce(&mut Store) -> T) -> Result<T> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read(data);
    let out = f(&mut store);
    std::fs::create_dir_all(data)?;
    prism_base::files::write_private(&path(data), serde_json::to_vec_pretty(&store)?)?;
    Ok(out)
}

impl Store {
    /// `def` trusted as it is.
    pub fn trust(&mut self, def: &Def) {
        self.trusted.insert(def.id.clone(), Trusted { hash: def.hash(), def: def.clone(), at: Utc::now() });
        self.seen.remove(&def.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_is_kept_by_hash() {
        let data = std::env::temp_dir().join(format!("prism-custom-trust-{}", std::process::id()));
        let def: Def = toml::from_str("id = \"a\"\ncommand = [\"x\"]\n").unwrap();
        update(&data, |s| s.trust(&def)).unwrap();
        let store = load(&data);
        assert_eq!(store.trusted["a"].hash, def.hash());
        assert_eq!(store.trusted["a"].def, def);
        let _ = std::fs::remove_dir_all(&data);
    }
}
