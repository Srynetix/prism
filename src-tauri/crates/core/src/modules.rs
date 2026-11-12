//! Modules: a team's blocks loaded at run time, without rebuilding Prism. A module is a folder of
//! `modules/` next to `config.toml` (each profile its own), named after its id:
//!
//! ```text
//! modules/<id>/
//!   module.toml      [module] id, version, contract, requires; [web] dir
//!   web/defs.js      its blocks' declarations and words, read first
//!   web/components.js  their components, and the chunks they import
//! ```
//!
//! Prism serves its web files to the window (`prism-module://localhost/<id>/<file>`) and to the
//! phone (`/modules/<id>/<file>`, behind the session): its web folder only, nothing else of the
//! module nor of the disk. What the files declare (the contract, the Svelte they were compiled
//! with, their blocks) is checked by the window as it loads them (`web/src/modules/modules.svelte.ts`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The module contract this Prism gives: a module built for another major one is refused.
pub const CONTRACT: u32 = 1;

/// A module as the window and Settings › Packs see it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModuleInfo {
    /// Its folder's name, the id its files are served under.
    pub id: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// The oldest Prism it works with (`[module] requires`).
    pub requires: Option<String>,
    /// It has a web part (`[web]`, its `defs.js` there).
    pub web: bool,
    /// It has a back end (`[wasm]`, its component there): `prism-modules` runs it.
    pub backend: bool,
    /// Why Prism does not load it (its manifest unreadable, another contract, a newer Prism
    /// needed); absent: the window loads it, and checks its files in turn.
    pub refused: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Manifest {
    module: ModuleTable,
    web: Option<WebTable>,
    wasm: Option<WasmTable>,
}

/// `[wasm]`: what the core checks of it (the rest is `prism-modules`').
#[derive(Deserialize)]
struct WasmTable {
    component: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ModuleTable {
    id: Option<String>,
    version: Option<String>,
    description: Option<String>,
    contract: Option<u32>,
    requires: Option<String>,
}

#[derive(Deserialize)]
struct WebTable {
    #[serde(default = "web_dir")]
    dir: String,
}

fn web_dir() -> String {
    "web".into()
}

/// `modules/`, next to the active profile's `config.toml`.
pub fn dir() -> PathBuf {
    dir_of(&crate::profile::config_path())
}

/// `modules/` of profile `id` of this computer (the active one's: `dir()`).
pub fn dir_for(id: &str) -> PathBuf {
    if id == crate::profile::active() { dir() } else { dir_of(&crate::profile::config_of(id)) }
}

/// `modules/` next to `config`.
pub fn dir_of(config: &Path) -> PathBuf {
    config.parent().unwrap_or(Path::new(".")).join("modules")
}

/// An id Prism serves: a plain folder name.
fn plain(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// The modules of `root`, by id: each folder with a `module.toml`.
pub fn list_in(root: &Path) -> Vec<ModuleInfo> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<ModuleInfo> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path())))
        .filter(|(id, path)| plain(id) && path.join("module.toml").is_file())
        .map(|(id, path)| info(&id, &path))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The active profile's modules.
pub fn list() -> Vec<ModuleInfo> {
    list_in(&dir())
}

fn info(id: &str, folder: &Path) -> ModuleInfo {
    inspect(id, folder).0
}

/// A module as listed, and its web folder when Prism serves it (not refused, a `defs.js` there).
fn inspect(id: &str, folder: &Path) -> (ModuleInfo, Option<PathBuf>) {
    let mut out = ModuleInfo { id: id.to_string(), version: None, description: None, requires: None, web: false, backend: false, refused: None };
    let manifest: Manifest = match std::fs::read_to_string(folder.join("module.toml")).map_err(|e| e.to_string()).and_then(|t| toml::from_str(&t).map_err(|e| e.message().to_string())) {
        Ok(m) => m,
        Err(e) => {
            out.refused = Some(prism_base::tr!("module.toml does not read: {error}", error = e));
            return (out, None);
        }
    };
    let m = manifest.module;
    out.version = m.version;
    out.description = m.description;
    out.requires = m.requires;
    let web = manifest.web.as_ref().map(|w| within(folder, &w.dir));
    let component = manifest.wasm.as_ref().map(|w| within(folder, &w.component));
    out.backend = component.as_ref().is_some_and(|c| c.as_ref().is_some_and(|c| c.is_file()));
    out.web = web.as_ref().is_some_and(|w| w.as_ref().is_some_and(|w| w.join("defs.js").is_file()));
    out.refused = if m.id.as_deref().is_some_and(|declared| declared != id) {
        Some(prism_base::tr!("its folder is {folder} but module.toml names it {id}", folder = id, id = m.id.unwrap_or_default()))
    } else if m.contract.is_some_and(|c| c != CONTRACT) {
        Some(prism_base::tr!("built for module contract {theirs}, this Prism gives {ours}", theirs = m.contract.unwrap_or_default(), ours = CONTRACT))
    } else if let Some(need) = out.requires.as_deref().filter(|need| !crate::packs::satisfies(crate::packs::PRISM, need)) {
        Some(prism_base::tr!("needs Prism {version} or later", version = need))
    } else if web.as_ref().is_some_and(|w| w.is_none()) {
        Some(prism_base::tr!("its web folder is not inside the module"))
    } else if web.is_some() && !out.web {
        Some(prism_base::tr!("no defs.js in its web folder"))
    } else if component.is_some() && !out.backend {
        Some(prism_base::tr!("no component at its [wasm] component, inside the module"))
    } else {
        None
    };
    let served = if out.refused.is_none() && out.web { web.flatten() } else { None };
    (out, served)
}

/// A path of the module's manifest (`[web] dir`, `[wasm] component`) in its folder: a relative
/// path that stays inside it, links resolved.
pub fn within(folder: &Path, dir: &str) -> Option<PathBuf> {
    let rel = Path::new(dir);
    if rel.components().any(|c| !matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir)) {
        return None;
    }
    let (Ok(root), Ok(base)) = (folder.join(rel).canonicalize(), folder.canonicalize()) else { return None };
    root.starts_with(&base).then_some(root)
}

/// A web file of module `id` in `root`, by its path in the module's web folder: its bytes and
/// type. Only a module Prism loads, only inside its web folder.
pub fn web_file_in(root: &Path, id: &str, path: &str) -> Result<(Vec<u8>, &'static str), String> {
    if !plain(id) {
        return Err("no such module".into());
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains('\\') || p.contains(':')) {
        return Err("no such file".into());
    }
    let (_, Some(web)) = inspect(id, &root.join(id)) else { return Err("no such module".into()) };
    // Symbolic links out of the folder are not followed out of it (`web` is resolved already).
    let Ok(real) = web.join(path).canonicalize() else { return Err("no such file".into()) };
    if !real.starts_with(&web) || !real.is_file() {
        return Err("no such file".into());
    }
    let bytes = std::fs::read(&real).map_err(|e| e.to_string())?;
    Ok((bytes, mime(path)))
}

/// A web file of the active profile's module `id`.
pub fn web_file(id: &str, path: &str) -> Result<(Vec<u8>, &'static str), String> {
    web_file_in(&dir(), id, path)
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "md" | "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(root: &Path, id: &str, manifest: &str, files: &[(&str, &str)]) {
        let folder = root.join(id);
        std::fs::create_dir_all(folder.join("web/chunks")).unwrap();
        std::fs::write(folder.join("module.toml"), manifest).unwrap();
        for (path, text) in files {
            std::fs::write(folder.join(path), text).unwrap();
        }
    }

    fn root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prism-modules-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn lists_its_modules_and_why_one_is_refused() {
        let r = root();
        module(&r, "hello", "[module]\nversion = \"1.0.0\"\ncontract = 1\n[web]\n", &[("web/defs.js", "export const prism = 1")]);
        module(&r, "later", "[module]\nrequires = \"999.0.0\"\n[web]\n", &[("web/defs.js", "")]);
        module(&r, "other", "[module]\ncontract = 2\n[web]\n", &[("web/defs.js", "")]);
        module(&r, "named", "[module]\nid = \"else\"\n[web]\n", &[("web/defs.js", "")]);
        module(&r, "empty", "[module]\n[web]\n", &[]);
        module(&r, "broken", "[module\n", &[]);
        std::fs::create_dir_all(r.join("no-manifest")).unwrap();
        let list = list_in(&r);
        let by = |id: &str| list.iter().find(|m| m.id == id).unwrap().clone();
        assert_eq!(list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["broken", "empty", "hello", "later", "named", "other"]);
        assert!(by("hello").refused.is_none() && by("hello").web);
        assert_eq!(by("hello").version.as_deref(), Some("1.0.0"));
        assert!(by("later").refused.unwrap().contains("999.0.0"));
        assert!(by("other").refused.unwrap().contains("contract 2"));
        assert!(by("named").refused.unwrap().contains("else"));
        assert!(by("empty").refused.unwrap().contains("defs.js"));
        assert!(by("broken").refused.is_some());
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn serves_its_web_folder_only() {
        let r = root();
        module(&r, "hello", "[module]\n[web]\n", &[("web/defs.js", "defs"), ("web/chunks/a.js", "chunk"), ("module.toml.bak", "secret")]);
        std::fs::write(r.join("outside.txt"), "outside").unwrap();
        module(&r, "off", "[module]\ncontract = 9\n[web]\n", &[("web/defs.js", "defs")]);
        // A web folder out of the module: refused, nothing served, not even its defs.js.
        std::fs::write(r.join("defs.js"), "outside").unwrap();
        module(&r, "up", "[module]\n[web]\ndir = \"..\"\n", &[]);
        module(&r, "abs", &format!("[module]\n[web]\ndir = '{}'\n", r.display()), &[]);
        assert_eq!(web_file_in(&r, "hello", "defs.js").unwrap(), (b"defs".to_vec(), "text/javascript; charset=utf-8"));
        assert_eq!(web_file_in(&r, "hello", "chunks/a.js").unwrap().0, b"chunk");
        for bad in ["../module.toml", "../../outside.txt", "chunks/../../module.toml", "/etc/hosts", "", "chunks//a.js", "c:\\x.js"] {
            assert!(web_file_in(&r, "hello", bad).is_err(), "{bad}");
        }
        assert!(web_file_in(&r, "../hello", "defs.js").is_err());
        assert!(web_file_in(&r, "missing", "defs.js").is_err());
        // A refused module serves nothing.
        assert!(web_file_in(&r, "off", "defs.js").is_err());
        for id in ["up", "abs"] {
            assert!(web_file_in(&r, id, "defs.js").is_err(), "{id}");
            assert!(web_file_in(&r, id, "outside.txt").is_err(), "{id}");
            assert!(list_in(&r).iter().find(|m| m.id == id).unwrap().refused.as_deref().is_some_and(|why| why.contains("not inside")), "{id}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(r.join("outside.txt"), r.join("hello/web/link.js")).unwrap();
            assert!(web_file_in(&r, "hello", "link.js").is_err(), "a link out of the folder");
        }
        std::fs::remove_dir_all(r).unwrap();
    }
}
