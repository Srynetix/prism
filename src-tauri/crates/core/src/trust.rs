//! The trust given to a declaration Prism runs but did not write itself (a routine, a custom
//! block): for each one by name, the hash of the version I accepted and that version as text (the
//! diff shown when it changed), and when a version waiting for trust was first seen. Kept in
//! Prism's data folder (JSON, 0600), never in `config.toml`: writing the config is not enough to
//! be trusted. Each user hashes its declaration and words its reasons (`RoutinePendingReason`,
//! `CustomPendingReason`).

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::textdiff;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Store {
    #[serde(default)]
    pub trusted: BTreeMap<String, Trusted>,
    /// When a version waiting for trust was first seen, and the agents running then.
    #[serde(default)]
    pub seen: BTreeMap<String, Seen>,
}

/// The version trusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(from = "Kept")]
pub struct Trusted {
    pub hash: String,
    /// As text: what a version changed since is compared with.
    pub text: String,
    /// What it is besides its text, for its user (a custom block's declaration, run while a pack's
    /// update waits).
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
    pub at: DateTime<Utc>,
}

/// A trusted version as written, the former files' too: a routine's two hashes (`routine`,
/// `action`, one `routine:action` now), a custom block's declaration (`def`, `data` now).
#[derive(Deserialize)]
struct Kept {
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    routine: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default, alias = "def")]
    data: Value,
    at: DateTime<Utc>,
}

impl From<Kept> for Trusted {
    fn from(k: Kept) -> Self {
        let hash = k.hash.unwrap_or_else(|| format!("{}:{}", k.routine.unwrap_or_default(), k.action.unwrap_or_default()));
        Trusted { hash, text: k.text, data: k.data, at: k.at }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Seen {
    pub hash: String,
    pub at: DateTime<Utc>,
    #[serde(default)]
    pub agents: Vec<String>,
}

/// Why a declaration waits: the version trusted before (none: never trusted), when this one was
/// first seen, and the lines from the one to the other (all added when new).
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub trusted: Option<Trusted>,
    pub seen: Option<Seen>,
    pub diff: Vec<textdiff::Line>,
}

/// The store kept at `path`; empty when there is none, or it cannot be read (said in the log).
pub fn read(path: &Path) -> Store {
    let Ok(raw) = std::fs::read(path) else { return Store::default() };
    serde_json::from_slice(&raw).map_err(|e| tracing::warn!("{}: {e}", path.display())).unwrap_or_default()
}

/// The store at `path` read, changed by `f` and written back, under that file's lock: two
/// changes at once never lose one another. What `f` returns.
pub fn update<T>(path: &Path, f: impl FnOnce(&mut Store) -> T) -> Result<T> {
    let lock = lock_of(path);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read(path);
    let out = f(&mut store);
    store.write(path)?;
    Ok(out)
}

/// The lock of the file at `path`, one per file.
fn lock_of(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: Mutex<Option<HashMap<PathBuf, Arc<Mutex<()>>>>> = Mutex::new(None);
    let mut locks = LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks.get_or_insert_with(HashMap::new).entry(path.to_path_buf()).or_default().clone()
}

impl Store {
    /// Written at `path`, for me only.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        prism_base::files::write_private(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    /// `name`'s version `hash` (as `text`) trusted; `data`: what its user keeps of it besides.
    pub fn trust(&mut self, name: &str, hash: &str, text: &str, data: Value) {
        self.trusted.insert(name.to_string(), Trusted { hash: hash.to_string(), text: text.to_string(), data, at: Utc::now() });
        self.seen.remove(name);
    }

    /// `name` forgotten: trusted no more, never seen.
    pub fn forget(&mut self, name: &str) {
        self.trusted.remove(name);
        self.seen.remove(name);
    }

    /// `name`'s version `hash` (as `text`): None when it is the one trusted, else why it waits.
    pub fn pending(&self, name: &str, hash: &str, text: &str) -> Option<Pending> {
        let trusted = self.trusted.get(name);
        if trusted.is_some_and(|t| t.hash == hash) {
            return None;
        }
        let before = trusted.map(|t| t.text.as_str()).unwrap_or_default();
        Some(Pending { trusted: trusted.cloned(), seen: self.seen.get(name).filter(|s| s.hash == hash).cloned(), diff: textdiff::lines(before, text) })
    }

    /// `name`'s version `hash`, waiting, seen now with these agents near, unless it already was:
    /// whether the store changed.
    pub fn see(&mut self, name: &str, hash: &str, agents: &[String], at: DateTime<Utc>) -> bool {
        if self.seen.get(name).is_some_and(|s| s.hash == hash) {
            return false;
        }
        self.seen.insert(name.to_string(), Seen { hash: hash.to_string(), at, agents: agents.to_vec() });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(d: &[textdiff::Line]) -> Vec<String> {
        d.iter().map(|l| format!("{}{}", l.op, l.text)).collect()
    }

    #[test]
    fn trusted_then_changed_waits_with_its_diff() {
        let mut s = Store::default();
        let p = s.pending("a", "h1", "x = 1\ny = 2").unwrap();
        assert!(p.trusted.is_none() && p.seen.is_none());
        assert_eq!(ops(&p.diff), ["+x = 1", "+y = 2"]);
        assert!(s.see("a", "h1", &["agent".into()], Utc::now()));
        assert!(!s.see("a", "h1", &[], Utc::now()), "seen once per version");
        assert_eq!(s.pending("a", "h1", "").unwrap().seen.unwrap().agents, ["agent"]);
        s.trust("a", "h1", "x = 1\ny = 2", Value::Null);
        assert!(s.seen.is_empty());
        assert!(s.pending("a", "h1", "x = 1\ny = 2").is_none());
        let p = s.pending("a", "h2", "x = 1\ny = 3").unwrap();
        assert_eq!(p.trusted.unwrap().hash, "h1");
        assert_eq!(ops(&p.diff), [" x = 1", "+y = 3", "-y = 2"]);
        s.forget("a");
        assert!(s.trusted.is_empty());
    }

    #[test]
    fn written_and_read_back_private() {
        let dir = std::env::temp_dir().join(format!("prism-trust-{}", std::process::id()));
        let path = dir.join("t.json");
        let mut s = Store::default();
        s.trust("b", "h", "t", serde_json::json!({ "id": "b" }));
        s.write(&path).unwrap();
        assert_eq!(read(&path), s);
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions()) & 0o777, 0o600);
        assert_eq!(read(&dir.join("none.json")), Store::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn updated_under_its_lock() {
        let dir = std::env::temp_dir().join(format!("prism-trust-update-{}", std::process::id()));
        let path = dir.join("t.json");
        let threads: Vec<_> = (0..8)
            .map(|i| {
                let path = path.clone();
                std::thread::spawn(move || update(&path, |s| s.trust(&format!("n{i}"), "h", "t", Value::Null)).unwrap())
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(read(&path).trusted.len(), 8, "no change lost");
        assert!(update(&path, |s| s.trusted.contains_key("n3")).unwrap());
        update(&path, |s| s.forget("n3")).unwrap();
        assert!(!read(&path).trusted.contains_key("n3"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The files written before the store was shared: a routine's two hashes, a block's `def`.
    #[test]
    fn former_files_read() {
        let routines = r#"{"trusted":{"N":{"routine":"r1","action":"a1","text":"t","at":"2026-10-01T09:00:00Z"}},"seen":{"M":{"hash":"x:y","at":"2026-10-01T09:00:00Z","agents":["a"]}}}"#;
        let s: Store = serde_json::from_str(routines).unwrap();
        assert_eq!((s.trusted["N"].hash.as_str(), s.trusted["N"].text.as_str()), ("r1:a1", "t"));
        assert_eq!(s.seen["M"].agents, ["a"]);
        assert!(s.pending("N", "r1:a1", "t").is_none(), "still trusted");
        let blocks = r#"{"trusted":{"a":{"hash":"h","def":{"id":"a","command":["x"]},"at":"2026-10-01T09:00:00Z"}}}"#;
        let s: Store = serde_json::from_str(blocks).unwrap();
        assert_eq!(s.trusted["a"].hash, "h");
        assert_eq!(s.trusted["a"].data["command"][0], "x");
        assert!(s.trusted["a"].text.is_empty());
        // Written again, the new form.
        let again: Store = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(again, s);
    }
}
