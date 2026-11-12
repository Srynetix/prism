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
//!
//! Nothing of a module runs before I trust it, by the fingerprint of its folder (`trust`), its
//! capabilities shown: a new version waits again, what it may reach that changed said. A pack
//! brings its modules (`install`).

pub mod install;
mod trust;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use trust::{fingerprint_of, trust, trust_path_for};

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
    /// needed); absent: the window loads it once trusted, and checks its files in turn.
    pub refused: Option<String>,
    /// What it may reach, as its manifest declares (`[wasm] capabilities`): `git`, `gitlab`,
    /// `http:<host>`, `exec:<program>`.
    pub capabilities: Vec<String>,
    /// I trust this version of it (its folder's fingerprint): nothing of it runs otherwise.
    pub trusted: bool,
    /// Waiting for trust: what changed since the version trusted before (`+ …`, `- …`), all of
    /// it when none was.
    pub changes: Vec<String>,
    /// The version trusted before, when one was.
    pub trusted_version: Option<String>,
    /// Its folder's fingerprint: what trusting it trusts.
    pub fingerprint: String,
    /// The pack that installed it (`[[modules]]` of a pack); none: put there by hand.
    pub pack: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Manifest {
    module: ModuleTable,
    web: Option<WebTable>,
    wasm: Option<WasmTable>,
}

/// `[wasm]`: what the core checks of it, and what trusting it shows (the rest is `prism-modules`').
#[derive(Deserialize)]
struct WasmTable {
    component: String,
    #[serde(default)]
    tables: Vec<String>,
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default)]
    blocks: Vec<WasmBlock>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Capabilities {
    git: bool,
    gitlab: bool,
    http: Vec<String>,
    exec: Vec<String>,
}

#[derive(Deserialize)]
struct WasmBlock {
    id: String,
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
pub(crate) fn plain(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// The modules of `root`, by id: each folder with a `module.toml`; trusted as the store at
/// `trust` says.
pub fn list_in(root: &Path, trust: &Path) -> Vec<ModuleInfo> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<ModuleInfo> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| Some((e.file_name().to_str()?.to_string(), e.path())))
        .filter(|(id, path)| plain(id) && path.join("module.toml").is_file())
        .map(|(id, path)| info(&id, &path, trust))
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// The mark a module's back end leaves while Prism runs it (`prism-modules`): still there at
/// launch, Prism stopped dead in it (a crash, a kill), and the same component is not run again.
pub fn stopped_mark(root: &Path, id: &str) -> PathBuf {
    root.join(".stopped").join(id)
}

/// What tells a component file from another: its size and when it was written.
pub fn fingerprint(file: &Path) -> Option<String> {
    let meta = std::fs::metadata(file).ok()?;
    let at = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some(format!("{}-{at}", meta.len()))
}

/// The active profile's modules.
pub fn list() -> Vec<ModuleInfo> {
    list_in(&dir(), &trust_path_for(crate::profile::active()))
}

/// The modules of profile `id` of this computer.
pub fn list_for(id: &str) -> Vec<ModuleInfo> {
    list_in(&dir_for(id), &trust_path_for(id))
}

fn info(id: &str, folder: &Path, trust: &Path) -> ModuleInfo {
    inspect(id, folder, trust).0
}

/// A module as listed, and its web folder when Prism serves it (not refused, trusted, a `defs.js`
/// there).
fn inspect(id: &str, folder: &Path, trust: &Path) -> (ModuleInfo, Option<PathBuf>) {
    let mut out = ModuleInfo {
        id: id.to_string(),
        version: None,
        description: None,
        requires: None,
        web: false,
        backend: false,
        refused: None,
        capabilities: Vec::new(),
        trusted: false,
        changes: Vec::new(),
        trusted_version: None,
        fingerprint: String::new(),
        pack: install::pack_of(folder),
    };
    let manifest: Manifest = match std::fs::read_to_string(folder.join("module.toml")).map_err(|e| e.to_string()).and_then(|t| toml::from_str(&t).map_err(|e| e.message().to_string())) {
        Ok(m) => m,
        Err(e) => {
            out.refused = Some(prism_base::tr!("module.toml does not read: {error}", error = e));
            return (out, None);
        }
    };
    out.capabilities = manifest.wasm.as_ref().map(|w| capabilities(&w.capabilities)).unwrap_or_default();
    let text = trust::text(id, &manifest);
    let m = manifest.module;
    out.version = m.version;
    out.description = m.description;
    out.requires = m.requires;
    let web = manifest.web.as_ref().map(|w| within(folder, &w.dir));
    let component = manifest.wasm.as_ref().map(|w| within(folder, &w.component));
    out.backend = component.as_ref().is_some_and(|c| c.as_ref().is_some_and(|c| c.is_file()));
    // Its back end stopped Prism the last time it ran, and it is the same one.
    let stopped = component.as_ref().and_then(|c| c.as_deref()).and_then(fingerprint).is_some_and(|f| folder.parent().is_some_and(|root| std::fs::read_to_string(stopped_mark(root, id)).is_ok_and(|m| m.trim() == f)));
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
    } else if stopped {
        Some(prism_base::tr!("its back end stopped Prism the last time it ran: replace it, or delete modules/.stopped/{id} to try it again", id = id))
    } else {
        None
    };
    if out.refused.is_none() {
        trust::apply(&mut out, folder, trust, &text);
    }
    let served = if out.refused.is_none() && out.trusted && out.web { web.flatten() } else { None };
    (out, served)
}

/// What a back end declares it may reach, as `ModuleInfo::capabilities` says it.
fn capabilities(c: &Capabilities) -> Vec<String> {
    let mut out = Vec::new();
    if c.gitlab {
        out.push("gitlab".to_string());
    }
    if c.git {
        out.push("git".to_string());
    }
    out.extend(c.http.iter().map(|h| format!("http:{h}")));
    out.extend(c.exec.iter().map(|p| format!("exec:{p}")));
    out
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
/// type. Only a module Prism loads (trusted as the store at `trust` says), only inside its web
/// folder.
pub fn web_file_in(root: &Path, trust: &Path, id: &str, path: &str) -> Result<(Vec<u8>, &'static str), String> {
    if !plain(id) {
        return Err("no such module".into());
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains('\\') || p.contains(':')) {
        return Err("no such file".into());
    }
    let (_, Some(web)) = inspect(id, &root.join(id), trust) else { return Err("no such module".into()) };
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
    web_file_in(&dir(), &trust_path_for(crate::profile::active()), id, path)
}

/// A web file of profile `profile`'s module `id` (a window of that profile).
pub fn web_file_for(profile: &str, id: &str, path: &str) -> Result<(Vec<u8>, &'static str), String> {
    web_file_in(&dir_for(profile), &trust_path_for(profile), id, path)
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

    /// A folder of its own: the tests run side by side, at the same instant on a coarse clock.
    fn root() -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("prism-modules-{}-{}-{n}", std::process::id(), rand_suffix()));
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
        // Its back end stopped Prism: refused while it is the same component.
        module(&r, "crashed", "[module]\n[wasm]\ncomponent = \"b.wasm\"\n", &[("b.wasm", "wasm")]);
        std::fs::create_dir_all(r.join(".stopped")).unwrap();
        std::fs::write(stopped_mark(&r, "crashed"), fingerprint(&r.join("crashed/b.wasm")).unwrap()).unwrap();
        std::fs::create_dir_all(r.join("no-manifest")).unwrap();
        let list = list_in(&r, &r.join(".trust.json"));
        let by = |id: &str| list.iter().find(|m| m.id == id).unwrap().clone();
        assert_eq!(list.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["broken", "crashed", "empty", "hello", "later", "named", "other"]);
        assert!(by("crashed").refused.unwrap().contains("stopped Prism"));
        std::fs::write(r.join("crashed/b.wasm"), "another wasm").unwrap();
        assert!(list_in(&r, &r.join(".trust.json")).iter().find(|m| m.id == "crashed").unwrap().refused.is_none(), "replaced: tried again");
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
        // Not trusted yet: nothing served.
        assert!(web_file_in(&r, &r.join(".trust.json"), "hello", "defs.js").is_err());
        trust(&r, &r.join(".trust.json"), "hello", &fingerprint_of(&r.join("hello"))).unwrap();
        assert_eq!(web_file_in(&r, &r.join(".trust.json"), "hello", "defs.js").unwrap(), (b"defs".to_vec(), "text/javascript; charset=utf-8"));
        assert_eq!(web_file_in(&r, &r.join(".trust.json"), "hello", "chunks/a.js").unwrap().0, b"chunk");
        for bad in ["../module.toml", "../../outside.txt", "chunks/../../module.toml", "/etc/hosts", "", "chunks//a.js", "c:\\x.js"] {
            assert!(web_file_in(&r, &r.join(".trust.json"), "hello", bad).is_err(), "{bad}");
        }
        assert!(web_file_in(&r, &r.join(".trust.json"), "../hello", "defs.js").is_err());
        assert!(web_file_in(&r, &r.join(".trust.json"), "missing", "defs.js").is_err());
        // A refused module serves nothing.
        assert!(web_file_in(&r, &r.join(".trust.json"), "off", "defs.js").is_err());
        for id in ["up", "abs"] {
            assert!(web_file_in(&r, &r.join(".trust.json"), id, "defs.js").is_err(), "{id}");
            assert!(web_file_in(&r, &r.join(".trust.json"), id, "outside.txt").is_err(), "{id}");
            assert!(list_in(&r, &r.join(".trust.json")).iter().find(|m| m.id == id).unwrap().refused.as_deref().is_some_and(|why| why.contains("not inside")), "{id}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(r.join("outside.txt"), r.join("hello/web/link.js")).unwrap();
            assert!(web_file_in(&r, &r.join(".trust.json"), "hello", "link.js").is_err(), "a link out of the folder");
        }
        std::fs::remove_dir_all(r).unwrap();
    }

    #[test]
    fn a_new_version_waits_for_trust_its_capabilities_said() {
        let r = root();
        let store = r.join(".trust.json");
        module(&r, "m", "[module]\nversion = \"1.0.0\"\n[wasm]\ncomponent = \"b.wasm\"\ncapabilities = { git = true }\n[[wasm.blocks]]\nid = \"m\"\n", &[("b.wasm", "v1")]);
        let info = list_in(&r, &store).remove(0);
        assert!(!info.trusted && info.trusted_version.is_none());
        assert_eq!(info.capabilities, ["git"]);
        assert!(info.changes.contains(&"+ capability git".to_string()), "{:?}", info.changes);
        // Trusted as shown; changed since, refused.
        assert!(trust(&r, &store, "m", "not what it is").is_err());
        trust(&r, &store, "m", &info.fingerprint).unwrap();
        assert!(list_in(&r, &store)[0].trusted);
        // A new version, reaching more: waits again, saying what it adds.
        module(&r, "m", "[module]\nversion = \"1.1.0\"\n[wasm]\ncomponent = \"b.wasm\"\ncapabilities = { git = true, http = [\"example.org\"] }\n[[wasm.blocks]]\nid = \"m\"\n", &[("b.wasm", "v2")]);
        let info = list_in(&r, &store).remove(0);
        assert!(!info.trusted);
        assert_eq!(info.trusted_version.as_deref(), Some("1.0.0"));
        assert!(info.changes.contains(&"+ capability http:example.org".to_string()) && info.changes.contains(&"+ module m 1.1.0".to_string()), "{:?}", info.changes);
        assert!(!info.changes.contains(&"+ capability git".to_string()), "unchanged lines are not said");
        std::fs::remove_dir_all(r).unwrap();
    }
}
