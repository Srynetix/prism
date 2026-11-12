//! The trust given to modules (`crate::trust`, as the routines and the custom blocks): for each one,
//! the fingerprint of its folder as I accepted it, and what it is as text (its version, blocks,
//! config tables and capabilities), the lines a new version changes shown before I trust it.
//! Kept in the profile's data folder, never next to the modules: a pack writes those.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use super::{Manifest, ModuleInfo};

/// One lock for the file: two trusts never cross.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The store of profile `id`'s modules.
pub fn trust_path_for(id: &str) -> PathBuf {
    crate::profile::data_dir_of(&crate::profile::data_base(), id).join("modules-trust.json")
}

/// The fingerprint of a module's folder: every file but the mark of its pack (`.pack`), by its
/// path, and every symbolic link by where it points (not followed: what it points to inside the
/// folder is a file of it, hashed as such). Kept while no file of it changes (sizes, times, links):
/// a window's files are served without hashing the component again each time.
pub fn fingerprint_of(folder: &Path) -> String {
    enum Entry {
        File(PathBuf, u64, u128),
        Link(String),
    }
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Entry)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            let path = e.path();
            let rel = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            if rel == super::install::PACK_MARK {
                continue;
            }
            if kind.is_symlink() {
                let to = std::fs::read_link(&path).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
                out.push((rel, Entry::Link(to)));
            } else if kind.is_dir() {
                walk(&path, base, out);
            } else if kind.is_file() {
                let meta = e.metadata().ok();
                let len = meta.as_ref().map_or(0, |m| m.len());
                let at = meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                out.push((rel, Entry::File(path, len, at)));
            }
        }
    }
    let mut entries = Vec::new();
    walk(folder, folder, &mut entries);
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    // Its files as they stand: the same, the same fingerprint.
    let mut stamp = Sha256::new();
    for (rel, e) in &entries {
        stamp.update(rel.as_bytes());
        match e {
            Entry::File(_, len, at) => stamp.update(format!("\0f{len}:{at}\0")),
            Entry::Link(to) => stamp.update(format!("\0l{to}\0")),
        }
    }
    let stamp: Vec<u8> = stamp.finalize().to_vec();
    /// By folder: the stamp of its files, its fingerprint.
    type Kept = std::collections::HashMap<PathBuf, (Vec<u8>, String)>;
    static CACHE: std::sync::Mutex<Option<Kept>> = std::sync::Mutex::new(None);
    if let Some((_, f)) = CACHE.lock().unwrap().get_or_insert_with(Default::default).get(folder).filter(|(s, _)| *s == stamp) {
        return f.clone();
    }
    let mut hash = Sha256::new();
    for (rel, e) in entries {
        hash.update(rel.as_bytes());
        match e {
            Entry::File(path, _, _) => {
                hash.update([0]);
                hash.update(std::fs::read(&path).unwrap_or_default());
            }
            Entry::Link(to) => {
                hash.update([1]);
                hash.update(to.as_bytes());
            }
        }
        hash.update([0]);
    }
    let f: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    CACHE.lock().unwrap().get_or_insert_with(Default::default).insert(folder.to_path_buf(), (stamp, f.clone()));
    f
}

/// What a module is, as trusting it shows it: one line each, its capabilities among them.
pub(super) fn text(id: &str, m: &Manifest) -> String {
    let mut lines = vec![format!("module {id} {}", m.module.version.as_deref().unwrap_or("?"))];
    if m.web.is_some() {
        lines.push("web".into());
    }
    if let Some(w) = &m.wasm {
        lines.push("back end".into());
        lines.extend(w.blocks.iter().map(|b| format!("block {}", b.id)));
        lines.extend(w.tables.iter().map(|t| format!("config table [{t}]")));
        // Worded by what it reaches: worded anew when its reach grows, every module trusted
        // before is asked again (`apply`).
        lines.extend(super::capabilities(&w.capabilities).into_iter().map(|c| match c.as_str() {
            "git" => "capability git (reading, in your clones)".to_string(),
            _ => format!("capability {c}"),
        }));
    }
    lines.join("\n")
}

/// Whether `info` (its folder `folder`, as `text`) is trusted, and why not: what changed.
pub(super) fn apply(info: &mut ModuleInfo, folder: &Path, store: &Path, text: &str) {
    info.fingerprint = fingerprint_of(folder);
    let store = crate::trust::read(store);
    // Its files the same, what it is worded otherwise (a capability reaching further): asked again.
    let pending = store.pending(&info.id, &info.fingerprint, text).or_else(|| {
        let t = store.trusted.get(&info.id).filter(|t| t.text != text)?;
        Some(crate::trust::Pending { trusted: Some(t.clone()), seen: None, diff: crate::textdiff::lines(&t.text, text) })
    });
    match pending {
        None => info.trusted = true,
        Some(p) => {
            info.trusted_version = p.trusted.as_ref().and_then(|t| t.text.lines().next()).and_then(|l| l.rsplit(' ').next()).map(String::from);
            info.changes = p.diff.iter().filter(|l| l.op == "+" || l.op == "-").map(|l| format!("{} {}", l.op, l.text)).collect();
        }
    }
}

/// Module `id` of `root` trusted as it is, its fingerprint the one I was shown (`fingerprint`):
/// changed since, it is not.
pub fn trust(root: &Path, store: &Path, id: &str, fingerprint: &str) -> Result<()> {
    let folder = root.join(id);
    let (info, _) = super::inspect(id, &folder, store);
    if let Some(why) = info.refused {
        bail!(why);
    }
    if info.fingerprint != fingerprint {
        bail!(prism_base::tr!("module {id} changed since it was shown: look at it again", id = id));
    }
    let manifest: Manifest = toml::from_str(&std::fs::read_to_string(folder.join("module.toml"))?).context("module.toml")?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = crate::trust::read(store);
    s.trust(id, fingerprint, &text(id, &manifest), serde_json::Value::Null);
    s.write(store)
}

/// Module `id` forgotten (its pack removed it).
pub fn forget(store: &Path, id: &str) {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut s = crate::trust::read(store);
    s.forget(id);
    let _ = s.write(store);
}
