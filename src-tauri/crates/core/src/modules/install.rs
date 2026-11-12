//! The modules a pack brings (`[[modules]]` of a pack): read from where the pack says, put in
//! `modules/<id>/` as a whole (an update replaces the folder), with the mark of their pack
//! (`.pack`): removing the pack, or a module from it, removes the folder. What is installed is not
//! trusted for that: it waits in Settings › Packs (`trust`).
//!
//! ```toml
//! [[modules]]
//! id = "delivery"
//! source = "glab:group/prism-pack:modules/delivery@main"   # a folder of a repository
//! # or "gh:owner/repo:dist/delivery.zip@v2" (an archive), or a local folder or .zip
//! ```

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};

use crate::forge::cli::Cli;
use crate::packs::Source;

/// The mark of a module's pack, in its folder.
pub const PACK_MARK: &str = ".pack";

/// A module a pack declares.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PackModule {
    pub id: String,
    pub source: String,
}

/// What `.pack` says of an installed module.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Mark {
    pub pack: String,
    pub source: String,
    /// The commit (a repository's source) or fingerprint (a local one) it was read at.
    pub revision: Option<String>,
}

/// The pack that installed the module in `folder`.
pub fn pack_of(folder: &Path) -> Option<String> {
    mark(folder).map(|m| m.pack)
}

pub fn mark(folder: &Path) -> Option<Mark> {
    toml::from_str(&std::fs::read_to_string(folder.join(PACK_MARK)).ok()?).ok()
}

/// A module's files, read from its source: their paths in its folder, their bytes.
pub struct Fetched {
    pub files: BTreeMap<String, Vec<u8>>,
    pub revision: Option<String>,
}

/// `source` of a pack's module; a local one relative to the pack's folder (`base`).
fn local(base: Option<&Path>, file: &str) -> PathBuf {
    let p = crate::config::expand_home(Path::new(file));
    match base {
        Some(b) if p.is_relative() => b.join(p),
        _ => p,
    }
}

/// The module's files at `source`: a folder (of a repository, or local), or a `.zip` archive.
pub async fn fetch(source: &str, base: Option<&Path>) -> Result<Fetched> {
    let enc = crate::http::encode;
    let zip = source.split('@').next().unwrap_or(source).ends_with(".zip");
    match crate::packs::source(source)? {
        Source::Builtin(_) => bail!("a module is not built into Prism"),
        Source::File(file) => {
            let path = local(base, file);
            let revision = Some(fingerprint_path(&path));
            let files = if zip { unzip(&std::fs::read(&path).with_context(|| path.display().to_string())?)? } else { read_folder(&path)? };
            Ok(Fetched { files, revision })
        }
        Source::Gitlab { project, path, reference } => {
            let r = reference.unwrap_or("HEAD");
            let revision = gitlab_commit(project, path, r).await?;
            let raw = |file: String| async move { Cli::Glab.run(&["api".to_string(), format!("projects/{}/repository/files/{}/raw?ref={}", enc(project), enc(&file), enc(r))]).await };
            if zip {
                return Ok(Fetched { files: unzip(&raw(path.to_string()).await?)?, revision });
            }
            #[derive(Deserialize)]
            struct Entry {
                path: String,
                r#type: String,
            }
            let out = Cli::Glab.run(&["api".to_string(), "--paginate".into(), format!("projects/{}/repository/tree?path={}&ref={}&recursive=true&per_page=100", enc(project), enc(path), enc(r))]).await?;
            // `--paginate`: one JSON array per page.
            let entries: Vec<Entry> = serde_json::Deserializer::from_slice(&out).into_iter::<Vec<Entry>>().collect::<Result<Vec<_>, _>>().context("GitLab's tree")?.into_iter().flatten().collect();
            let mut files = BTreeMap::new();
            for e in entries.into_iter().filter(|e| e.r#type == "blob") {
                let rel = e.path.strip_prefix(path).unwrap_or(&e.path).trim_start_matches('/').to_string();
                files.insert(rel, raw(e.path).await?);
            }
            ensure!(!files.is_empty(), "{project}: no folder {path} at {r}");
            Ok(Fetched { files, revision })
        }
        Source::Github { project, path, reference } => {
            let r = reference.unwrap_or("HEAD");
            let revision = github_commit(project, path, r).await?;
            let raw = |file: String| async move {
                let file = file.split('/').map(enc).collect::<Vec<_>>().join("/");
                Cli::Gh.run(&["api".to_string(), "-H".into(), "Accept: application/vnd.github.raw".into(), format!("repos/{project}/contents/{file}?ref={}", enc(r))]).await
            };
            if zip {
                return Ok(Fetched { files: unzip(&raw(path.to_string()).await?)?, revision });
            }
            #[derive(Deserialize)]
            struct Tree {
                tree: Vec<Entry>,
            }
            #[derive(Deserialize)]
            struct Entry {
                path: String,
                r#type: String,
            }
            let out = Cli::Gh.run(&["api".to_string(), format!("repos/{project}/git/trees/{}?recursive=1", enc(r))]).await?;
            let tree: Tree = serde_json::from_slice(&out).context("GitHub's tree")?;
            let prefix = format!("{}/", path.trim_end_matches('/'));
            let mut files = BTreeMap::new();
            for e in tree.tree.into_iter().filter(|e| e.r#type == "blob" && e.path.starts_with(&prefix)) {
                files.insert(e.path[prefix.len()..].to_string(), raw(e.path).await?);
            }
            ensure!(!files.is_empty(), "{project}: no folder {path} at {r}");
            Ok(Fetched { files, revision })
        }
    }
}

/// The last commit that touched `path` at `r`.
async fn gitlab_commit(project: &str, path: &str, r: &str) -> Result<Option<String>> {
    let enc = crate::http::encode;
    #[derive(Deserialize)]
    struct Commit {
        id: String,
    }
    let out = Cli::Glab.run(&["api".to_string(), format!("projects/{}/repository/commits?path={}&ref_name={}&per_page=1", enc(project), enc(path), enc(r))]).await?;
    let commits: Vec<Commit> = serde_json::from_slice(&out).context("GitLab's commits")?;
    Ok(commits.into_iter().next().map(|c| c.id))
}

async fn github_commit(project: &str, path: &str, r: &str) -> Result<Option<String>> {
    let enc = crate::http::encode;
    let out = Cli::Gh.run(&["api".to_string(), format!("repos/{project}/commits?path={}&sha={}&per_page=1", enc(path), enc(r)), "--jq".into(), ".[0].sha".into()]).await?;
    Ok(String::from_utf8(out).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != "null"))
}

/// What tells a local source from another version of it.
fn fingerprint_path(path: &Path) -> String {
    use sha2::Digest;
    if path.is_dir() {
        return super::fingerprint_of(path);
    }
    std::fs::read(path).map(|b| sha2::Sha256::digest(&b).iter().map(|x| format!("{x:02x}")).collect()).unwrap_or_default()
}

fn read_folder(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    ensure!(dir.is_dir(), "no folder {}", dir.display());
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
        for e in std::fs::read_dir(dir)?.flatten() {
            let path = e.path();
            let kind = e.file_type()?;
            if kind.is_dir() {
                walk(&path, base, out)?;
            } else if kind.is_file() {
                let rel = path.strip_prefix(base).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                out.insert(rel, std::fs::read(&path)?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// An archive's files; all in one folder (`delivery/…`), that folder's.
fn unzip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a zip archive")?;
    let mut files = BTreeMap::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        let name = f.enclosed_name().context("a path out of the archive")?.to_string_lossy().replace('\\', "/");
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        files.insert(name, data);
    }
    let top: Vec<&str> = files.keys().map(|k| k.split('/').next().unwrap_or_default()).collect();
    if !files.contains_key("module.toml") && top.windows(2).all(|w| w[0] == w[1]) && files.keys().all(|k| k.contains('/')) {
        let prefix = format!("{}/", top.first().copied().unwrap_or_default());
        files = files.into_iter().map(|(k, v)| (k[prefix.len()..].to_string(), v)).collect();
    }
    Ok(files)
}

/// Module `m` of pack `pack` put in `root`, replacing the former folder whole.
pub fn install(root: &Path, pack: &str, m: &PackModule, fetched: Fetched) -> Result<()> {
    ensure!(super::plain(&m.id), "module id {:?}: letters, digits, - and _", m.id);
    ensure!(fetched.files.contains_key("module.toml"), "module {}: no module.toml at {}", m.id, m.source);
    if let Some(owner) = pack_of(&root.join(&m.id)).filter(|p| p != pack) {
        bail!("module {}: already installed by the pack {owner}", m.id);
    }
    // Every path checked before anything is written.
    for rel in fetched.files.keys() {
        ensure!(!rel.is_empty() && rel.split('/').all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains(['\\', ':'])), "module {}: a path out of it: {rel}", m.id);
        ensure!(rel != PACK_MARK, "module {}: {PACK_MARK} is Prism's", m.id);
    }
    std::fs::create_dir_all(root)?;
    let incoming = root.join(format!(".incoming-{}", m.id));
    let _ = std::fs::remove_dir_all(&incoming);
    for (rel, bytes) in &fetched.files {
        let path = incoming.join(rel);
        std::fs::create_dir_all(path.parent().unwrap_or(&incoming))?;
        std::fs::write(&path, bytes)?;
    }
    let mark = Mark { pack: pack.to_string(), source: m.source.clone(), revision: fetched.revision };
    std::fs::write(incoming.join(PACK_MARK), toml::to_string(&mark)?)?;
    let target = root.join(&m.id);
    let old = root.join(format!(".old-{}", m.id));
    let _ = std::fs::remove_dir_all(&old);
    if target.exists() {
        std::fs::rename(&target, &old)?;
    }
    std::fs::rename(&incoming, &target)?;
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// The modules pack `pack` installed in `root`.
pub fn installed_by(root: &Path, pack: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut ids: Vec<String> = entries.flatten().filter_map(|e| Some(e.file_name().to_str()?.to_string())).filter(|id| super::plain(id) && pack_of(&root.join(id)).as_deref() == Some(pack)).collect();
    ids.sort();
    ids
}

/// Pack `pack`'s modules that are not among `keep` removed, and forgotten (`trust`).
pub fn remove_others(root: &Path, trust: &Path, pack: &str, keep: &[String]) {
    for id in installed_by(root, pack).into_iter().filter(|id| !keep.contains(id)) {
        if let Err(e) = std::fs::remove_dir_all(root.join(&id)) {
            tracing::warn!("module {id}: not removed: {e}");
        }
        super::trust::forget(trust, &id);
    }
}

/// A pack's module read again from its source: whether it changed there.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ModuleCheck {
    pub id: String,
    /// The installed version, and the source's (when it says).
    pub version: Option<String>,
    pub latest: Option<String>,
    /// Not installed, or its source is at another revision.
    pub changed: bool,
    pub error: Option<String>,
}

/// Module `m` of a pack, as installed in `root`, against its source; nothing is written.
pub async fn check(root: &Path, m: &PackModule, base: Option<&Path>) -> ModuleCheck {
    let folder = root.join(&m.id);
    let installed = mark(&folder);
    let version = |toml_text: &str| toml::from_str::<toml::Table>(toml_text).ok().and_then(|t| t.get("module")?.get("version")?.as_str().map(String::from));
    let mut out = ModuleCheck { id: m.id.clone(), version: std::fs::read_to_string(folder.join("module.toml")).ok().and_then(|t| version(&t)), latest: None, changed: false, error: None };
    let latest = async {
        let enc = crate::http::encode;
        let zip = m.source.split('@').next().unwrap_or(&m.source).ends_with(".zip");
        Ok::<_, anyhow::Error>(match crate::packs::source(&m.source)? {
            Source::File(file) => {
                let p = local(base, file);
                let v = (!zip).then(|| std::fs::read_to_string(p.join("module.toml")).ok()).flatten().and_then(|t| version(&t));
                (Some(fingerprint_path(&p)), v)
            }
            Source::Gitlab { project, path, reference } => {
                let r = reference.unwrap_or("HEAD");
                let v = if zip { None } else { Cli::Glab.run(&["api".to_string(), format!("projects/{}/repository/files/{}/raw?ref={}", enc(project), enc(&format!("{path}/module.toml")), enc(r))]).await.ok().and_then(|b| version(&String::from_utf8_lossy(&b))) };
                (gitlab_commit(project, path, r).await?, v)
            }
            Source::Github { project, path, reference } => {
                let r = reference.unwrap_or("HEAD");
                (github_commit(project, path, r).await?, None)
            }
            Source::Builtin(_) => bail!("a module is not built into Prism"),
        })
    };
    match latest.await {
        Ok((revision, v)) => {
            out.latest = v.or_else(|| out.version.clone());
            out.changed = installed.as_ref().is_none_or(|i| i.source != m.source || i.revision != revision);
        }
        Err(e) => out.error = Some(format!("{e:#}")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("prism-install-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[tokio::test]
    async fn a_packs_module_is_installed_replaced_then_removed_with_it() {
        let src = temp("src");
        std::fs::create_dir_all(src.join("hello/web")).unwrap();
        std::fs::write(src.join("hello/module.toml"), "[module]\nversion = \"1.0.0\"\n[web]\n").unwrap();
        std::fs::write(src.join("hello/web/defs.js"), "export const prism = 1").unwrap();
        let (root, trust) = (temp("root"), temp("trust").join("modules-trust.json"));
        let m = PackModule { id: "hello".into(), source: "hello".into() };
        install(&root, "team", &m, fetch(&m.source, Some(&src)).await.unwrap()).unwrap();
        assert_eq!(pack_of(&root.join("hello")).as_deref(), Some("team"));
        assert!(!check(&root, &m, Some(&src)).await.changed);
        // Installed, not trusted: nothing of it is served.
        let info = &super::super::list_in(&root, &trust)[0];
        assert!(!info.trusted && info.refused.is_none());
        assert!(super::super::web_file_in(&root, &trust, "hello", "defs.js").is_err());
        // A new version at the source: changed, replaced whole.
        std::fs::write(src.join("hello/module.toml"), "[module]\nversion = \"1.1.0\"\n[web]\n").unwrap();
        let c = check(&root, &m, Some(&src)).await;
        assert!(c.changed && c.latest.as_deref() == Some("1.1.0"), "{c:?}");
        install(&root, "team", &m, fetch(&m.source, Some(&src)).await.unwrap()).unwrap();
        assert!(std::fs::read_to_string(root.join("hello/module.toml")).unwrap().contains("1.1.0"));
        // Another pack may not take it.
        assert!(install(&root, "other", &m, fetch(&m.source, Some(&src)).await.unwrap()).unwrap_err().to_string().contains("team"));
        // Its pack gone (or the module out of it): its folder too.
        remove_others(&root, &trust, "team", &[]);
        assert!(!root.join("hello").exists());
        for d in [src, root] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }

    #[test]
    fn an_archive_in_one_folder_is_that_folders() {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("hello/module.toml", o).unwrap();
            std::io::Write::write_all(&mut z, b"[module]\n").unwrap();
            z.start_file("hello/web/defs.js", o).unwrap();
            std::io::Write::write_all(&mut z, b"x").unwrap();
            z.finish().unwrap();
        }
        let files = unzip(buf.get_ref()).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["module.toml", "web/defs.js"]);
    }

    #[test]
    fn a_path_out_of_the_module_is_refused() {
        let root = temp("root");
        let files = BTreeMap::from([("module.toml".to_string(), b"[module]\n".to_vec()), ("../evil".to_string(), b"x".to_vec())]);
        let m = PackModule { id: "hello".into(), source: "x".into() };
        assert!(install(&root, "team", &m, Fetched { files, revision: None }).is_err());
        assert!(!root.join("evil").exists() && !root.join("hello").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
