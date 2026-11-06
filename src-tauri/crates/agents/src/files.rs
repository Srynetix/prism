//! The folder an agent works in, seen from the Agents view: its files, its diff, and its
//! HTML pages served by the `prismfile:` scheme (`prismfile://localhost/<agent>/<path>`).
//! Its artifacts too: the pages Claude Code writes in the session's scratchpad
//! (`prismfile://localhost/<agent>/@scratchpad/<path>`), before publishing them on claude.ai.
//! Folders always come from the agent, never from the window: a requested path that
//! leaves them is refused.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use prism_base::tr;
use serde::{Deserialize, Serialize};

use crate::{forge::FileDiff, git::run as git};

/// Beyond this, the file list is cut.
const MAX_FILES: usize = 20_000;
/// Total size of the rendered diff, and of an untracked file shown in full.
const MAX_DIFF: usize = 4 << 20;
const MAX_UNTRACKED: usize = 256 << 10;
/// A file read whole for its preview in the diff.
const MAX_TEXT: u64 = 2 << 20;
/// A page served by `prismfile:`.
const MAX_SERVED: u64 = 32 << 20;

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentFileListing"))]
pub struct Listing {
    pub root: String,
    pub files: Vec<FileEntry>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentFile"))]
pub struct FileEntry {
    pub path: String,
    /// `git status` code: `M`, `A`, `D`, `R`, `?` (untracked); absent, unchanged.
    pub status: Option<String>,
    /// Left out by `.gitignore` (listed on demand).
    pub ignored: bool,
}

/// Tracked and untracked files (outside `.gitignore`), with their state; with `ignored`, those
/// `.gitignore` leaves out too, their folders walked but the heavy ones (dependencies, caches).
pub async fn list(dir: &Path, ignored: bool) -> Result<Listing> {
    // Both at once: one after the other was 300 ms on 20,000 files, at each file the agent changed.
    let (names, status) = tokio::try_join!(git(dir, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"]), git(dir, &["status", "--porcelain=v1", "-z", "--untracked-files=all"]))?;
    let status = parse_status(&status);
    let mut seen = std::collections::HashSet::new();
    let mut files: Vec<FileEntry> = names
        .split('\0')
        .filter(|p| !p.is_empty() && seen.insert(*p))
        .map(|p| FileEntry { path: p.to_string(), status: status.get(p).cloned(), ignored: false })
        .collect();
    if ignored {
        let raw = git(dir, &["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"]).await?;
        // The folders walked off the async workers.
        let root = dir.to_path_buf();
        let found = tokio::task::spawn_blocking(move || {
            let (mut found, mut visits) = (Vec::new(), MAX_VISITED);
            for entry in raw.split('\0').filter(|e| !e.is_empty() && !heavy(e.trim_end_matches('/'))) {
                if entry.ends_with('/') {
                    walk(&root, entry.trim_end_matches('/'), 0, &|_| true, &mut found, MAX_FILES + 1, &mut visits);
                } else {
                    found.push(entry.to_string());
                }
                if found.len() > MAX_FILES || visits == 0 {
                    break;
                }
            }
            found
        })
        .await?;
        files.extend(found.into_iter().map(|path| FileEntry { path, status: None, ignored: true }));
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    let truncated = files.len() > MAX_FILES;
    files.truncate(MAX_FILES);
    Ok(Listing { root: dir.display().to_string(), files, truncated })
}

/// A file the Web view can show, for its picker; `ignored`: left out by `.gitignore`.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentPage"))]
pub struct Page {
    pub path: String,
    pub ignored: bool,
}

/// What the Web view shows: pages, documents and images.
const VIEWABLE: [&str; 12] = ["html", "htm", "svg", "pdf", "md", "markdown", "txt", "png", "jpg", "jpeg", "gif", "webp"];
/// Folders never walked for ignored files: dependencies, builds of tools, caches.
const HEAVY: [&str; 10] = ["node_modules", "target", ".git", ".venv", "venv", "__pycache__", ".cache", ".next", ".svelte-kit", ".turbo"];
const MAX_PAGES: usize = 5_000;

pub fn is_viewable(path: &str) -> bool {
    Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| VIEWABLE.iter().any(|v| v.eq_ignore_ascii_case(e)))
}
fn heavy(path: &str) -> bool {
    path.split('/').any(|part| HEAVY.contains(&part))
}

/// The files of `dir` the Web view can show, in path order: tracked and untracked ones, and
/// with `ignored` those `.gitignore` leaves out too. Git names the ignored folders whole
/// (`--directory`); they are walked here, the heavy ones (dependencies, caches) skipped.
pub async fn pages(dir: &Path, ignored: bool) -> Result<Vec<Page>> {
    let names = git(dir, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"]).await?;
    let mut out: Vec<Page> = names.split('\0').filter(|p| is_viewable(p)).map(|p| Page { path: p.to_string(), ignored: false }).collect();
    if ignored {
        let raw = git(dir, &["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"]).await?;
        // The folders walked off the async workers.
        let root = dir.to_path_buf();
        out = tokio::task::spawn_blocking(move || {
            let mut visits = MAX_VISITED;
            for entry in raw.split('\0').filter(|e| !e.is_empty() && !heavy(e.trim_end_matches('/'))) {
                if entry.ends_with('/') {
                    let mut found = Vec::new();
                    walk(&root, entry.trim_end_matches('/'), 0, &is_viewable, &mut found, MAX_PAGES, &mut visits);
                    out.extend(found.into_iter().map(|path| Page { path, ignored: true }));
                } else if is_viewable(entry) {
                    out.push(Page { path: entry.to_string(), ignored: true });
                }
                if out.len() >= MAX_PAGES || visits == 0 {
                    break;
                }
            }
            out
        })
        .await?;
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    out.truncate(MAX_PAGES);
    Ok(out)
}

/// Entries a walk looks at, at most, whatever it keeps: a big ignored build folder holding
/// nothing to show was walked whole (120k entries: 26–46 s).
const MAX_VISITED: usize = 50_000;

/// A folder's files that `keep` takes (paths relative to `root`), down to a few levels, the heavy
/// folders skipped, without following links; `out` stops at `max`, the walk after `visits` entries.
fn walk(root: &Path, rel: &str, depth: usize, keep: &dyn Fn(&str) -> bool, out: &mut Vec<String>, max: usize, visits: &mut usize) {
    let Ok(entries) = std::fs::read_dir(root.join(rel)) else { return };
    for e in entries.flatten() {
        if out.len() >= max || *visits == 0 {
            return;
        }
        *visits -= 1;
        let name = e.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() && depth < 8 && !HEAVY.contains(&name.as_str()) {
            walk(root, &path, depth + 1, keep, out, max, visits);
        } else if kind.is_file() && keep(&name) {
            out.push(path);
        }
    }
}

/// The folders of a project's installed dependencies (git leaves them out): what its tasks run
/// too (`node_modules/.bin/vitest`, `.venv/bin/pytest`).
const DEPS: [&str; 4] = ["node_modules", ".venv", "venv", "vendor"];

/// Files of `dir`'s dependencies written since `since` (at most three): an agent's, or the MR's it
/// installed, which a task run outside its sandbox would run. Too many to look through, the folder
/// is named all the same.
pub fn deps_changed(dir: &Path, since: std::time::SystemTime) -> Vec<String> {
    fn walk(path: &Path, rel: &str, since: std::time::SystemTime, out: &mut Vec<String>, visits: &mut usize) -> bool {
        let Ok(entries) = std::fs::read_dir(path) else { return true };
        for e in entries.flatten() {
            if out.len() >= 3 {
                return true;
            }
            if *visits == 0 {
                return false;
            }
            *visits -= 1;
            let rel = format!("{rel}/{}", e.file_name().to_string_lossy());
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                if !walk(&e.path(), &rel, since, out, visits) {
                    return false;
                }
            } else if meta.modified().is_ok_and(|t| t > since) {
                out.push(rel);
            }
        }
        true
    }
    let mut out = Vec::new();
    let mut visits = 400_000;
    for deps in DEPS {
        let path = dir.join(deps);
        if path.symlink_metadata().is_ok_and(|m| m.is_dir()) && !walk(&path, deps, since, &mut out, &mut visits) {
            out.push(format!("{deps}/ (too many files to check)"));
            break;
        }
    }
    out
}

/// Beyond this, a download is refused.
pub const MAX_DOWNLOAD: u64 = 256 << 20;

/// A download made ready for a phone (`prepare`): its name, and the file of the computer's it was
/// written to, which the remote serves once, then removes.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Prepared {
    pub name: String,
    pub path: String,
}

/// What a phone gets for it: the name to save it under, and the address to fetch it from, once.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentDownload"))]
pub struct Sent {
    pub name: String,
    pub url: String,
}

/// The files `paths` of `dir` (relative to it, as the Files list gives them; a folder ends with
/// `/` and brings its files, the heavy folders and `.git` left out) written to `dest`: a single
/// file as it is, anything else as a zip named `name`, written as it goes rather than held whole
/// in memory. Each one inside `dir`, links resolved; at most `max` bytes. `hidden`: a folder brings
/// its hidden files too (`.env`…), else only those named. `create`: opens `dest`. The name to save
/// it under.
pub fn export(dir: &Path, paths: &[String], name: &str, max: u64, hidden: bool, dest: &Path, create: impl Fn(&Path) -> std::io::Result<std::fs::File>) -> Result<String> {
    let write = || -> Result<String> {
        match planned(dir, paths, name, max, hidden)? {
            Planned::One(path, name) => {
                std::io::copy(&mut std::fs::File::open(&path)?, &mut create(dest)?)?;
                Ok(name)
            }
            Planned::Zip(root, files, name) => {
                zip_into(&root, &files, create(dest)?)?;
                Ok(name)
            }
        }
    };
    write().with_context(|| format!("writing {}", dest.display()))
}

/// What a download gives: one file (its path, its name), or the files of a zip (relative to the
/// folder) and its name.
enum Planned {
    One(PathBuf, String),
    Zip(PathBuf, Vec<String>, String),
}

/// The files a download takes, refused before anything is read when they weigh over `max`.
fn planned(dir: &Path, paths: &[String], name: &str, max: u64, hidden: bool) -> Result<Planned> {
    let root = dir.canonicalize().context("agent folder")?;
    let inside = |rel: &str| -> Result<PathBuf> {
        let path = root.join(rel.trim_start_matches('/')).canonicalize().with_context(|| tr!("{path}: not found", path = rel))?;
        if !path.starts_with(&root) {
            bail!(tr!("{path} is outside the agent folder", path = rel));
        }
        Ok(path)
    };
    if let [one] = paths
        && !one.ends_with('/')
    {
        let path = inside(one)?;
        anyhow::ensure!(path.is_file(), "{one}: not a file");
        let size = path.metadata().map(|m| m.len()).unwrap_or(0);
        anyhow::ensure!(size <= max, "{one}: too large to download ({} MB, {} at most)", size >> 20, max >> 20);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string());
        return Ok(Planned::One(path, name));
    }
    let (mut files, mut visits) = (Vec::new(), MAX_VISITED);
    for rel in paths {
        let path = inside(rel)?;
        let rel = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy().into_owned();
        if path.is_dir() {
            let from = files.len();
            walk(&root, &rel, 0, &|_| true, &mut files, MAX_FILES, &mut visits);
            if !hidden {
                let mut found = files.split_off(from);
                found.retain(|f| !f[rel.len()..].split('/').any(|part| part.starts_with('.')));
                files.append(&mut found);
            }
        } else if path.is_file() {
            files.push(rel);
        }
    }
    files.retain(|p| !p.split('/').any(|part| part == ".git"));
    files.sort();
    files.dedup();
    anyhow::ensure!(!files.is_empty(), "no file to download");
    let total: u64 = files.iter().map(|rel| root.join(rel).metadata().map(|m| m.len()).unwrap_or(0)).sum();
    anyhow::ensure!(total <= max, "too large to download ({} files, {} MB, {} at most): narrow it", files.len(), total >> 20, max >> 20);
    let name = name.trim().trim_end_matches(".zip");
    Ok(Planned::Zip(root, files, format!("{}.zip", if name.is_empty() { "files" } else { name })))
}

/// Already compressed: stored as they are, deflating them again costs time for nothing.
const PACKED: [&str; 16] = ["png", "jpg", "jpeg", "gif", "webp", "avif", "zip", "gz", "tgz", "bz2", "xz", "zst", "br", "woff2", "mp4", "pdf"];

/// `files` (relative to `root`) zipped into `out`: deflated fast (level 1, about six times
/// quicker than the default for a few percent more), the already compressed ones stored.
fn zip_into<W: std::io::Write + std::io::Seek>(root: &Path, files: &[String], out: W) -> Result<W> {
    let mut zip = zip::ZipWriter::new(out);
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).compression_level(Some(1)).unix_permissions(0o644);
    let stored = deflated.compression_method(zip::CompressionMethod::Stored).compression_level(None);
    for rel in files {
        let packed = Path::new(rel).extension().and_then(|e| e.to_str()).is_some_and(|e| PACKED.iter().any(|p| p.eq_ignore_ascii_case(e)));
        zip.start_file(rel.as_str(), if packed { stored } else { deflated })?;
        let mut file = std::fs::File::open(root.join(rel)).with_context(|| format!("reading {rel}"))?;
        std::io::copy(&mut file, &mut zip).with_context(|| format!("reading {rel}"))?;
    }
    Ok(zip.finish()?)
}

/// `git status --porcelain=v1 -z`: path → code (the working tree side first).
fn parse_status(raw: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut parts = raw.split('\0');
    while let Some(entry) = parts.next() {
        if entry.len() < 4 {
            continue;
        }
        let (xy, path) = entry.split_at(3);
        let (x, y) = (xy.as_bytes()[0] as char, xy.as_bytes()[1] as char);
        if x == 'R' || x == 'C' {
            parts.next(); // the old path of a rename
        }
        let code = match (x, y) {
            ('?', _) => '?',
            (_, 'D') | ('D', _) => 'D',
            ('R', _) => 'R',
            ('A', _) => 'A',
            _ => 'M',
        };
        out.insert(path.to_string(), code.to_string());
    }
    out
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentDiff"))]
pub struct Diff {
    pub files: Vec<FileDiff>,
    pub truncated: bool,
    /// What the working tree is compared to: `HEAD`, or `origin/<base>` (merge-base).
    pub against: String,
    /// The commit `HEAD` points to (empty before the first one): the review notes of the Diff
    /// tab that were resolved are forgotten once it moves.
    pub head: String,
}

/// The working tree compared to `HEAD` (uncommitted changes), or to the merge-base of
/// `origin/<base>` (everything the branch will bring), untracked files included.
pub async fn diff(dir: &Path, base: Option<&str>) -> Result<Diff> {
    let (rev, against) = compared_to(dir, base).await?;
    let raw = git(dir, &["diff", "--no-color", "--no-ext-diff", "--no-textconv", "-M", &rev]).await?;
    let mut truncated = raw.len() > MAX_DIFF;
    let mut files = parse_diff(if truncated { cut(&raw, MAX_DIFF) } else { &raw });
    let mut size = raw.len().min(MAX_DIFF);
    let untracked = git(dir, &["ls-files", "-z", "--others", "--exclude-standard"]).await?;
    for path in untracked.split('\0').filter(|p| !p.is_empty()) {
        if size >= MAX_DIFF {
            truncated = true;
            break;
        }
        let file = untracked_diff(dir, path);
        size += file.diff.len().max(1);
        files.push(file);
    }
    let head = git(dir, &["rev-parse", "HEAD"]).await.map(|h| h.trim().to_string()).unwrap_or_default();
    Ok(Diff { files, truncated, against, head })
}

/// The two sides of an overlap (`state::Overlap`), each one's diff against its base cut down to
/// the files they share.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OverlapDiff {
    pub a: Option<Diff>,
    pub b: Option<Diff>,
    /// Why a side present has no diff: git failed (its base not fetched, say), not its worktree gone.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a_error: Option<String>,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b_error: Option<String>,
}

/// `o`'s two sides, as the Diff tab shows a branch: the files they share only. A side gone (its
/// worktree removed once merged) has none.
pub async fn overlap_diff(o: &prism_core::state::Overlap) -> OverlapDiff {
    let shared = |d: Diff| Diff { files: d.files.into_iter().filter(|f| o.files.iter().any(|s| s.path == f.old_path || s.path == f.new_path)).collect(), ..d };
    let side = |path: &str| {
        let dir = PathBuf::from(path);
        async move {
            if !dir.is_dir() {
                return (None, None);
            }
            match diff(&dir, Some(&o.base)).await {
                Ok(d) => (Some(shared(d)), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            }
        }
    };
    let ((a, a_error), (b, b_error)) = tokio::join!(side(&o.a), side(&o.b));
    OverlapDiff { a, b, a_error, b_error }
}

/// Every path `diff` would show (old and new names, untracked files), however large the diff:
/// what a task guard reads, which must not miss one past `diff`'s size cap.
pub async fn changed_paths(dir: &Path, base: Option<&str>) -> Result<Vec<String>> {
    let (rev, _) = compared_to(dir, base).await?;
    changed_since(dir, &rev).await
}

/// The files changed since revision `rev` (committed since, changed, or new), relative to the
/// repository's top, sorted.
pub async fn changed_since(dir: &Path, rev: &str) -> Result<Vec<String>> {
    let tracked = git(dir, &["diff", "--name-only", "-z", "--no-renames", "--no-ext-diff", rev]).await?;
    let untracked = git(dir, &["ls-files", "-z", "--others", "--exclude-standard"]).await?;
    let mut paths: Vec<String> = tracked.split('\0').chain(untracked.split('\0')).filter(|p| !p.is_empty()).map(String::from).collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// What `paths` (relative to the repository's top, as `changed_paths` gives them) hold now: a
/// SHA-256 of their names and contents, a missing file counted as such.
pub async fn fingerprint(dir: &Path, paths: &[String]) -> Result<String> {
    use sha2::Digest;
    let top = git(dir, &["rev-parse", "--show-toplevel"]).await?;
    let top = Path::new(top.trim());
    let mut hash = sha2::Sha256::new();
    for p in paths {
        hash.update(p.as_bytes());
        hash.update([0]);
        match std::fs::read(top.join(p)) {
            Ok(bytes) => hash.update(&bytes),
            Err(_) => hash.update(b"\0gone"),
        }
        hash.update([0]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// What `diff` compares the working tree to: the revision, and its name for the window.
async fn compared_to(dir: &Path, base: Option<&str>) -> Result<(String, String)> {
    Ok(match base {
        Some(b) => {
            let upstream = format!("origin/{b}");
            (git(dir, &["merge-base", "HEAD", &upstream]).await?.trim().to_string(), upstream)
        }
        None => ("HEAD".to_string(), "HEAD".to_string()),
    })
}

/// An image of the diff (`rel`, relative to the repository's top, as `diff` gives it) as a
/// `data:` URL: as it is in the working tree, or (`old`) as it is where `diff` compares it to.
/// PNG, JPEG, GIF and WebP only (`image_data_url`).
pub async fn diff_image(dir: &Path, rel: &str, base: Option<&str>, old: bool) -> Result<String> {
    let top = git(dir, &["rev-parse", "--show-toplevel"]).await?;
    let top = Path::new(top.trim());
    let bytes = if old {
        let (rev, _) = compared_to(dir, base).await?;
        let spec = format!("{rev}:{rel}");
        // Its size first: a huge blob is not read whole to be refused.
        if git(top, &["cat-file", "-s", &spec]).await?.trim().parse::<u64>().unwrap_or(u64::MAX) > MAX_IMAGE {
            bail!(tr!("{path}: too large", path = rel));
        }
        // `cat-file`: the blob as stored, no filter or textconv of the repository's run.
        crate::git::run_bytes(top, &["cat-file", "blob", &spec]).await?
    } else {
        let path = resolve(top, rel)?;
        if path.metadata().map(|m| m.len()).unwrap_or(0) > MAX_IMAGE {
            bail!(tr!("{path}: too large", path = rel));
        }
        std::fs::read(&path).with_context(|| format!("reading {rel}"))?
    };
    crate::forge::image_data_url(&bytes)
}

fn cut(s: &str, max: usize) -> &str {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// An untracked file, like a new file; binary or too large, without content.
fn untracked_diff(dir: &Path, path: &str) -> FileDiff {
    let content = std::fs::read(dir.join(path)).ok().filter(|b| b.len() <= MAX_UNTRACKED && !b.contains(&0)).and_then(|b| String::from_utf8(b).ok());
    let diff = content
        .map(|c| {
            let lines: Vec<&str> = c.lines().collect();
            let mut d = format!("@@ -0,0 +1,{} @@\n", lines.len());
            for l in lines {
                d.push('+');
                d.push_str(l);
                d.push('\n');
            }
            d
        })
        .unwrap_or_default();
    FileDiff { old_path: path.into(), new_path: path.into(), new_file: true, deleted_file: false, renamed_file: false, collapsed: diff.is_empty(), diff }
}

/// Splits `git diff` output per file, in GitLab's diff format: the hunks only.
fn parse_diff(raw: &str) -> Vec<FileDiff> {
    let mut out = Vec::new();
    for chunk in raw.split("\ndiff --git ").map(|c| c.strip_prefix("diff --git ").unwrap_or(c)) {
        if chunk.trim().is_empty() {
            continue;
        }
        let (head, body) = match chunk.find("\n@@") {
            Some(i) => (&chunk[..i], &chunk[i + 1..]),
            None => (chunk, ""),
        };
        let first = head.lines().next().unwrap_or("");
        // `a/<old> b/<new>`: with no space in the paths, the halves; otherwise the ---/+++ lines.
        let (mut old, mut new) = first.split_once(" b/").map(|(a, b)| (a.trim_start_matches("a/").to_string(), b.to_string())).unwrap_or_default();
        let mut f = FileDiff { old_path: String::new(), new_path: String::new(), new_file: false, deleted_file: false, renamed_file: false, diff: String::new(), collapsed: false };
        for l in head.lines().skip(1) {
            if l.starts_with("new file mode") {
                f.new_file = true;
            } else if l.starts_with("deleted file mode") {
                f.deleted_file = true;
            } else if let Some(p) = l.strip_prefix("rename from ") {
                f.renamed_file = true;
                old = p.to_string();
            } else if let Some(p) = l.strip_prefix("rename to ") {
                new = p.to_string();
            } else if let Some(p) = l.strip_prefix("--- a/") {
                old = p.to_string();
            } else if let Some(p) = l.strip_prefix("+++ b/") {
                new = p.to_string();
            } else if l.starts_with("Binary files") {
                f.collapsed = true;
            }
        }
        f.old_path = if old.is_empty() { new.clone() } else { old };
        f.new_path = if new.is_empty() { f.old_path.clone() } else { new };
        f.diff = body.to_string();
        if !f.diff.is_empty() && !f.diff.ends_with('\n') {
            f.diff.push('\n');
        }
        out.push(f);
    }
    out
}

/// URL prefix of the scratchpad files, served alongside those of the agent's folder.
pub const SCRATCHPAD: &str = "@scratchpad";

/// The scratchpad of a CLI session `session`: `<tmp>/<project>/<session>/scratchpad`, `tmp` being
/// the CLI's temporary folder (`/tmp/claude-<uid>`, `/tmp/commandcode-<uid>`) and the project the
/// launch folder (or the worktree) turned into a folder name.
pub fn scratchpad(tmp: &Path, session: &str) -> Option<PathBuf> {
    std::fs::read_dir(tmp)
        .ok()?
        .flatten()
        .map(|project| project.path().join(session).join("scratchpad"))
        .find(|p| p.is_dir())
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AgentArtifact"))]
pub struct Artifact {
    /// Relative to the scratchpad.
    pub path: String,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
}

/// The scratchpad's pages (HTML, SVG, Markdown, images), most recent first.
pub fn artifacts(root: &Path) -> Vec<Artifact> {
    fn walk(root: &Path, dir: &Path, depth: u8, out: &mut Vec<Artifact>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                if depth < 3 && !e.file_name().to_string_lossy().starts_with('.') && e.file_name() != "node_modules" {
                    walk(root, &path, depth + 1, out);
                }
            } else if PAGE.iter().any(|ext| path.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext))) {
                let rel = path.strip_prefix(root).unwrap_or(&path).display().to_string();
                out.push(Artifact { path: rel, modified: meta.modified().ok().map(Into::into) });
            }
        }
    }
    const PAGE: [&str; 9] = ["html", "htm", "svg", "md", "png", "jpg", "jpeg", "gif", "pdf"];
    let mut out = Vec::new();
    walk(root, root, 0, &mut out);
    out.sort_by_key(|a| std::cmp::Reverse(a.modified));
    out.truncate(500);
    out
}

/// The file `rel` points to in `root`, if it really is there (links and `..` resolved); a
/// folder serves its `index.html`.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let root = root.canonicalize().context("agent folder")?;
    let mut path = root.join(rel.trim_start_matches('/')).canonicalize().with_context(|| tr!("{path}: not found", path = rel))?;
    if !path.starts_with(&root) {
        bail!(tr!("{path} is outside the agent folder", path = rel));
    }
    if path.is_dir() {
        // Resolved again: the folder's index.html may itself be a link out of the folder.
        path = path.join("index.html").canonicalize().with_context(|| tr!("{path}: not found", path = rel))?;
        if !path.starts_with(&root) {
            bail!(tr!("{path} is outside the agent folder", path = rel));
        }
    }
    if !path.is_file() {
        bail!(tr!("{path}: not a file", path = rel));
    }
    Ok(path)
}

/// A text file of the diff (path relative to the repository root, as git gives it), for its
/// Markdown preview: read from the working tree.
pub async fn text(dir: &Path, rel: &str) -> Result<String> {
    let top = git(dir, &["rev-parse", "--show-toplevel"]).await?;
    page_text(Path::new(top.trim()), rel)
}

/// A text page of the Web view (Markdown, plain text), read from `root` as `serve` would serve it.
pub fn page_text(root: &Path, rel: &str) -> Result<String> {
    let path = resolve(root, rel)?;
    if path.metadata().map(|m| m.len()).unwrap_or(0) > MAX_TEXT {
        bail!(tr!("{path}: too large", path = rel));
    }
    String::from_utf8(std::fs::read(&path)?).with_context(|| tr!("{path}: not text", path = rel))
}

/// A text file of the agent's folder written back (its Files preview, edited): only one that is
/// there already, outside `.git`, as text, and still as it was read (`original`): one the agent
/// changed meanwhile is not overwritten.
pub fn save(dir: &Path, rel: &str, original: &str, text: &str) -> Result<()> {
    let path = resolve(dir, rel)?;
    let root = dir.canonicalize().context("agent folder")?;
    if path.strip_prefix(&root).is_ok_and(|p| p.components().any(|c| c.as_os_str() == ".git")) {
        bail!(tr!("{path}: git's own files are not edited here", path = rel));
    }
    if text.len() as u64 > MAX_TEXT {
        bail!(tr!("{path}: too large", path = rel));
    }
    let now = page_text(dir, rel)?;
    if now != original {
        bail!(tr!("{path} changed since it was opened: open it again", path = rel));
    }
    // Through a file beside it, renamed over it: a link the agent's processes put there since the
    // checks above is replaced, not followed out of the folder; a folder swapped for one is seen.
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else { bail!(tr!("{path}: not a file", path = rel)) };
    let tmp = parent.join(format!(".{}.prism-{}.tmp", name.to_string_lossy(), std::process::id()));
    let written = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        std::io::Write::write_all(&mut f, text.as_bytes())?;
        f.set_permissions(path.metadata()?.permissions())?;
        f.sync_all()?;
        if parent.canonicalize()? != parent {
            return Err(std::io::Error::other(tr!("{path} is outside the agent folder", path = rel)));
        }
        std::fs::rename(&tmp, &path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.with_context(|| format!("writing {rel}"))
}

/// An image shown in the Files preview, as a `data:` URL.
const MAX_IMAGE: u64 = 8 << 20;

/// A file of the agent's folder, for the Files preview.
#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "FilePreview"))]
pub enum Preview {
    /// Text, with what its editor's gutter compares it to: `head`, the file as committed when it
    /// differs (absent: as it is now); `new`, not committed yet at all. `stamp`: its version on
    /// disk, to tell when it changes under the editor (`stamps`).
    Text { text: String, head: Option<String>, new: bool, size: u64, stamp: String },
    Image { url: String, size: u64 },
    Binary { size: u64 },
    TooLarge { size: u64 },
}

/// File `rel` of the agent's folder (relative to it, as the Files list gives it) for its preview.
pub async fn preview(dir: &Path, rel: &str) -> Result<Preview> {
    let path = resolve(dir, rel)?;
    let meta = path.metadata()?;
    let size = meta.len();
    let kind = mime(&path);
    if kind.starts_with("image/") {
        if size > MAX_IMAGE {
            return Ok(Preview::TooLarge { size });
        }
        use base64::Engine;
        let url = format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(&path)?));
        return Ok(Preview::Image { url, size });
    }
    if size > MAX_TEXT {
        return Ok(Preview::TooLarge { size });
    }
    let Ok(text) = String::from_utf8(std::fs::read(&path)?) else { return Ok(Preview::Binary { size }) };
    let (head, new) = match committed(dir, rel).await {
        Some(head) if head == text => (None, false),
        Some(head) => (Some(head), false),
        None => (None, true),
    };
    Ok(Preview::Text { text, head, new, size, stamp: stamp(&meta) })
}

/// `rel` as committed (`HEAD`), if it is and reads as text.
async fn committed(dir: &Path, rel: &str) -> Option<String> {
    let spec = format!("HEAD:./{rel}");
    String::from_utf8(crate::git::run_bytes(dir, &["show", "--no-textconv", &spec]).await.ok()?).ok()
}

/// A file's version on disk: its modification time and size.
fn stamp(meta: &std::fs::Metadata) -> String {
    let at = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
    format!("{at}-{}", meta.len())
}

/// The version on disk of each of `paths` (`stamp`), `None` for one gone: the editor reads again a
/// file the agent changed under it.
pub fn stamps(dir: &Path, paths: &[String]) -> Vec<Option<String>> {
    paths.iter().map(|p| resolve(dir, p).ok().and_then(|f| f.metadata().ok()).map(|m| stamp(&m))).collect()
}

/// Lines found by the Files tab's search: `path` relative to the agent's folder, `line` from 1.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "FileMatch"))]
pub struct Match {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "FileSearch"))]
pub struct Found {
    pub matches: Vec<Match>,
    /// More lines matched than `max`.
    pub truncated: bool,
}

/// What the Files tab's search asks for.
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "FileQuery"))]
pub struct Query {
    pub text: String,
    pub regex: bool,
    pub case: bool,
    pub word: bool,
    /// Globs, comma-separated (`src/**`, `!*.test.ts`): a name alone matches in any folder.
    pub include: String,
    /// Only these files (the changed ones), when given.
    pub paths: Option<Vec<String>>,
    pub max: u32,
}

/// A matched line cut to this many bytes: a minified file's line is not sent whole.
const MAX_LINE: usize = 400;

/// The lines of the agent's folder that match (`git grep`: tracked and untracked files,
/// `.gitignore` honoured, binaries skipped), at most `max`.
pub async fn search(dir: &Path, q: &Query) -> Result<Found> {
    if q.text.is_empty() {
        return Ok(Found { matches: Vec::new(), truncated: false });
    }
    let mut args: Vec<String> = ["grep", "-I", "-n", "-z", "--no-color", "--untracked"].map(String::from).to_vec();
    if !q.case {
        args.push("-i".into());
    }
    if q.word {
        args.push("-w".into());
    }
    args.push(if q.regex { "-P" } else { "-F" }.into());
    // No more lines of one file than are shown in all: a minified bundle's are not all read.
    args.push(format!("--max-count={}", q.max.max(1)));
    args.extend(["-e".into(), q.text.clone(), "--".into()]);
    if let Some(paths) = &q.paths {
        let inside: Vec<String> = paths.iter().filter(|p| !climbs(p)).map(|p| format!(":(literal){p}")).collect();
        if inside.is_empty() {
            return Ok(Found { matches: Vec::new(), truncated: false });
        }
        args.extend(inside);
    }
    args.extend(pathspecs(&q.include));
    // The folder's latest search only: one typed further stops the one before (its git killed).
    static LATEST: std::sync::LazyLock<std::sync::Mutex<HashMap<PathBuf, u64>>> = std::sync::LazyLock::new(Default::default);
    let mine = {
        let mut latest = LATEST.lock().unwrap();
        let n = latest.entry(dir.to_path_buf()).or_default();
        *n += 1;
        *n
    };
    let wanted = || LATEST.lock().unwrap().get(dir) == Some(&mine);
    let run = |args: Vec<String>| async move {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        // Its first `max` lines and one more (to tell there were more): a short query on a large
        // folder was 60 MB of output read whole, for 2000 lines shown.
        match crate::git::run_head(dir, &refs, q.max as usize + 1, wanted).await {
            Ok((out, _)) => Ok(out),
            // Nothing found: git grep says so by failing, silently.
            Err(e) if e.to_string().ends_with(": ") => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    };
    let out = match run(args.clone()).await {
        // A git built without Perl's regular expressions: its extended ones.
        Err(e) if q.regex && e.to_string().contains("PCRE") => run(args.into_iter().map(|a| if a == "-P" { "-E".into() } else { a }).collect()).await?,
        out => out?,
    };
    let (matches, truncated) = parse_grep(&String::from_utf8_lossy(&out), q.max as usize);
    Ok(Found { matches, truncated })
}

/// `include`'s globs as git pathspecs: `!x` excluded, `x` without a `/` in any folder; one that
/// climbs out (`../x/**`) dropped, the search staying in the agent's folder.
fn pathspecs(include: &str) -> Vec<String> {
    let specs: Vec<String> = include
        .split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty() && *g != "!" && !climbs(g))
        .map(|g| {
            let (exclude, g) = g.strip_prefix('!').map_or((false, g), |g| (true, g));
            let g = if g.contains('/') { g.trim_start_matches('/').to_string() } else { format!("**/{g}") };
            format!(":({}glob){g}", if exclude { "exclude," } else { "" })
        })
        .collect();
    // Only exclusions: from everything else.
    if !specs.is_empty() && specs.iter().all(|s| s.starts_with(":(exclude")) {
        return [vec![".".to_string()], specs].concat();
    }
    specs
}

/// A path or a glob going up out of the folder it is read in.
fn climbs(path: &str) -> bool {
    path.split(['/', '\\']).any(|part| part == "..")
}

/// `git grep -n -z` lines (`path\0line\0text`), the first `max`; true when there were more.
fn parse_grep(raw: &str, max: usize) -> (Vec<Match>, bool) {
    let mut out = Vec::new();
    for l in raw.lines() {
        let mut it = l.splitn(3, '\0');
        let (Some(path), Some(line), Some(text)) = (it.next(), it.next(), it.next()) else { continue };
        let Ok(line) = line.parse() else { continue };
        if out.len() == max {
            return (out, true);
        }
        out.push(Match { path: path.to_string(), line, text: cut(text, MAX_LINE).to_string() });
    }
    (out, false)
}

/// A `prismfile:` scheme response: the content and its type, or the error status.
pub fn serve(root: &Path, rel: &str) -> std::result::Result<(Vec<u8>, &'static str), (u16, String)> {
    let path = resolve(root, rel).map_err(|e| (404, format!("{e:#}")))?;
    // Never a hidden file or folder (`.env`, `.git/config`…), whatever links lead there: a page
    // has no business loading them, and a hostile one would want them.
    let inside = root.canonicalize().ok().and_then(|r| path.strip_prefix(r).ok().map(Path::to_path_buf)).unwrap_or_default();
    if inside.components().any(|c| c.as_os_str().to_string_lossy().starts_with('.')) {
        return Err((404, format!("{rel}: a hidden file, not served")));
    }
    let len = path.metadata().map(|m| m.len()).unwrap_or(0);
    if len > MAX_SERVED {
        return Err((413, format!("{rel}: too large")));
    }
    let bytes = std::fs::read(&path).map_err(|e| (500, e.to_string()))?;
    Ok((bytes, mime(&path)))
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("ttf") => "font/ttf",
        Some("pdf") => "application/pdf",
        Some("txt" | "md" | "log") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// `%xx` of a URL path; an invalid sequence stays as is.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
        if b[i] == b'%' && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file written back: there already, inside the folder, outside `.git`, unchanged since read.
    #[test]
    fn saves_a_file_as_it_was_read() {
        let root = std::env::temp_dir().join(format!("prism-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("agent");
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::write(dir.join("a.md"), "one").unwrap();
        std::fs::write(dir.join(".git/config"), "[core]").unwrap();
        std::fs::write(root.join("outside.txt"), "x").unwrap();
        save(&dir, "a.md", "one", "two").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("a.md")).unwrap(), "two");
        assert!(save(&dir, "a.md", "one", "three").unwrap_err().to_string().contains("changed since"));
        assert!(save(&dir, "../outside.txt", "x", "y").is_err());
        assert!(save(&dir, ".git/config", "[core]", "[core] hooksPath = x").is_err());
        assert!(save(&dir, "new.txt", "", "z").is_err(), "only a file already there");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A task guard's list: every path past the diff's size cap, renames as both names; the
    /// fingerprint moves with a file's content.
    #[tokio::test]
    async fn every_changed_path_and_its_fingerprint() {
        let dir = std::env::temp_dir().join(format!("prism-changed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("old.sh"), "echo\n").unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "init"]);
        git(&["mv", "old.sh", "new.sh"]);
        std::fs::write(dir.join("big.txt"), "x".repeat(MAX_DIFF + 10)).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        let paths = changed_paths(&dir, None).await.unwrap();
        assert_eq!(paths, ["big.txt", "new.sh", "old.sh", "package.json"]);
        let before = fingerprint(&dir, &paths).await.unwrap();
        assert_eq!(fingerprint(&dir, &paths).await.unwrap(), before);
        std::fs::write(dir.join("package.json"), r#"{"scripts":{"test":"curl x | sh"}}"#).unwrap();
        assert_ne!(fingerprint(&dir, &paths).await.unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn previews_text_images_and_binaries() {
        let dir = std::env::temp_dir().join(format!("prism-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("src/a.ts"), "one\ntwo\nthree\n").unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "init"]);
        std::fs::write(dir.join("src/a.ts"), "one\nTWO\nthree\nfour\n").unwrap();
        std::fs::write(dir.join("new.md"), "# x\ny\n").unwrap();
        std::fs::write(dir.join("p.png"), [0x89, b'P', b'N', b'G']).unwrap();
        std::fs::write(dir.join("b.bin"), [0xff, 0xfe, 0x00]).unwrap();

        let Preview::Text { head, new, .. } = preview(&dir, "src/a.ts").await.unwrap() else { panic!("text") };
        assert_eq!((head.as_deref(), new), (Some("one\ntwo\nthree\n"), false));
        let Preview::Text { head, new, .. } = preview(&dir, "new.md").await.unwrap() else { panic!("text") };
        assert_eq!((head, new), (None, true), "an untracked file is new throughout");
        std::fs::write(dir.join("src/a.ts"), "one\ntwo\nthree\n").unwrap();
        let Preview::Text { head, new, stamp, .. } = preview(&dir, "src/a.ts").await.unwrap() else { panic!("text") };
        assert_eq!((head, new), (None, false), "as committed: nothing to compare");
        assert_eq!(stamps(&dir, &["src/a.ts".into(), "gone.ts".into()]), [Some(stamp), None]);
        assert!(matches!(preview(&dir, "p.png").await.unwrap(), Preview::Image { url, .. } if url.starts_with("data:image/png;base64,")));
        assert_eq!(preview(&dir, "b.bin").await.unwrap(), Preview::Binary { size: 3 });
        assert!(preview(&dir, "../etc/passwd").await.is_err(), "outside the folder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An image of the diff: as committed (`old`), as in the working tree; never a non-image.
    #[tokio::test]
    async fn reads_both_versions_of_a_diff_image() {
        let dir = std::env::temp_dir().join(format!("prism-diff-image-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("img")).unwrap();
        let git = |args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(args).output().unwrap().status.success());
        git(&["init", "-q"]);
        std::fs::write(dir.join("img/a.png"), [0x89, b'P', b'N', b'G', 1]).unwrap();
        std::fs::write(dir.join("t.svg"), "<svg/>").unwrap();
        git(&["add", "."]);
        git(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "init"]);
        std::fs::write(dir.join("img/a.png"), [0x89, b'P', b'N', b'G', 2]).unwrap();
        let old = diff_image(&dir, "img/a.png", None, true).await.unwrap();
        let new = diff_image(&dir, "img/a.png", None, false).await.unwrap();
        assert!(old.starts_with("data:image/png;base64,") && new.starts_with("data:image/png;base64,") && old != new);
        assert!(diff_image(&dir.join("img"), "img/a.png", None, true).await.is_ok(), "from the repository's top, wherever the agent is");
        assert!(diff_image(&dir, "t.svg", None, false).await.is_err(), "an SVG is not shown as an image");
        assert!(diff_image(&dir, "../x.png", None, false).await.is_err(), "outside the folder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Files search: tracked and untracked files, `.gitignore` honoured, globs, case, words, a regex.
    #[tokio::test]
    async fn searches_the_folder() {
        let dir = std::env::temp_dir().join(format!("prism-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        for (f, text) in [(".gitignore", "out.txt\n"), ("src/a.ts", "const total = 1\nTotal()\n"), ("src/a.test.ts", "total\n"), ("notes.md", "subtotal\n"), ("out.txt", "total\n")] {
            std::fs::write(dir.join(f), text).unwrap();
        }
        assert!(std::process::Command::new("git").arg("-C").arg(&dir).args(["init", "-q"]).output().unwrap().status.success());
        let q = |text: &str, include: &str| Query { text: text.into(), regex: false, case: false, word: false, include: include.into(), paths: None, max: 100 };
        let at = &dir;
        let found = |q: Query| async move { search(at, &q).await.unwrap().matches.into_iter().map(|m| format!("{}:{}", m.path, m.line)).collect::<Vec<_>>() };
        assert_eq!(found(q("total", "")).await, ["notes.md:1", "src/a.test.ts:1", "src/a.ts:1", "src/a.ts:2"], "untracked files, not the ignored one");
        assert_eq!(found(Query { case: true, ..q("Total", "") }).await, ["src/a.ts:2"]);
        assert_eq!(found(Query { word: true, ..q("total", "") }).await, ["src/a.test.ts:1", "src/a.ts:1", "src/a.ts:2"]);
        assert_eq!(found(q("total", "*.ts, !*.test.ts")).await, ["src/a.ts:1", "src/a.ts:2"]);
        assert_eq!(found(q("total", "!src/**")).await, ["notes.md:1"]);
        assert_eq!(found(Query { regex: true, ..q("^const \\w+", "") }).await, ["src/a.ts:1"]);
        assert_eq!(found(Query { paths: Some(vec!["notes.md".into()]), ..q("total", "") }).await, ["notes.md:1"]);
        assert!(found(q("nowhere", "")).await.is_empty());
        assert_eq!(pathspecs("../x/**, *.ts"), [":(glob)**/*.ts"], "nothing out of the folder");
        assert!(found(Query { paths: Some(vec!["../notes.md".into()]), ..q("total", "") }).await.is_empty());
        let cut = search(&dir, &Query { max: 2, ..q("total", "") }).await.unwrap();
        assert_eq!((cut.matches.len(), cut.truncated), (2, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn splits_a_git_diff_by_file() {
        let raw = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,2 +1,2 @@\n-x\n+y\n ctx\ndiff --git a/old.txt b/new.txt\nsimilarity index 90%\nrename from old.txt\nrename to new.txt\ndiff --git a/n.png b/n.png\nnew file mode 100644\nBinary files /dev/null and b/n.png differ\n";
        let files = parse_diff(raw);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].new_path, "src/a.rs");
        assert_eq!(files[0].diff, "@@ -1,2 +1,2 @@\n-x\n+y\n ctx\n");
        assert!(files[1].renamed_file);
        assert_eq!((files[1].old_path.as_str(), files[1].new_path.as_str()), ("old.txt", "new.txt"));
        assert!(files[2].new_file && files[2].collapsed);
    }

    #[test]
    fn reads_porcelain_status() {
        let s = parse_status(" M a.rs\0?? b.rs\0R  new.rs\0old.rs\0D  gone.rs\0");
        assert_eq!(s["a.rs"], "M");
        assert_eq!(s["b.rs"], "?");
        assert_eq!(s["new.rs"], "R");
        assert_eq!(s["gone.rs"], "D");
        assert!(!s.contains_key("old.rs"));
    }

    #[test]
    fn serves_only_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("prism-files-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("site")).unwrap();
        std::fs::write(dir.join("site/index.html"), "<p>hi</p>").unwrap();
        std::fs::write(dir.join("secret.txt"), "no").unwrap();
        assert_eq!(serve(&dir, "site/").unwrap().1, "text/html; charset=utf-8");
        assert_eq!(serve(&dir, "/site/index.html").unwrap().0, b"<p>hi</p>");
        assert_eq!(serve(&dir.join("site"), "../secret.txt").unwrap_err().0, 404);
        std::fs::write(dir.join("site/.env"), "TOKEN=x").unwrap();
        std::fs::create_dir_all(dir.join("site/.git")).unwrap();
        std::fs::write(dir.join("site/.git/config"), "x").unwrap();
        assert_eq!(serve(&dir.join("site"), ".env").unwrap_err().0, 404, "a hidden file");
        assert_eq!(serve(&dir.join("site"), ".git/config").unwrap_err().0, 404, "in a hidden folder");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("site/.env"), dir.join("site/env.txt")).unwrap();
            assert_eq!(serve(&dir.join("site"), "env.txt").unwrap_err().0, 404, "through a link");
        }
        assert_eq!(percent_decode("a%20b%2Fc%zz"), "a b/c%zz");
        let found = artifacts(&dir);
        assert_eq!(found.iter().map(|a| a.path.as_str()).collect::<Vec<_>>(), ["site/index.html"]);
        #[cfg(unix)]
        {
            std::fs::create_dir_all(dir.join("site/docs")).unwrap();
            std::os::unix::fs::symlink(dir.join("secret.txt"), dir.join("site/docs/index.html")).unwrap();
            assert_eq!(serve(&dir.join("site"), "docs/").unwrap_err().0, 404, "a folder's index.html linked out");
            std::fs::create_dir_all(dir.join("site/hid")).unwrap();
            std::os::unix::fs::symlink(dir.join("site/.env"), dir.join("site/hid/index.html")).unwrap();
            assert_eq!(serve(&dir.join("site"), "hid/").unwrap_err().0, 404, "a folder's index.html linked to a hidden file");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The picker's files: viewable ones only; the ignored ones on demand, heavy folders skipped.
    #[tokio::test]
    async fn lists_pages_and_ignored_ones_on_demand() {
        let dir = std::env::temp_dir().join(format!("prism-pages-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["docs/mockups/deep", "node_modules/pkg", "src"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        for (f, text) in [(".gitignore", "docs/mockups/\nnode_modules/\n"), ("README.md", "# r"), ("src/main.rs", "fn main() {}"), ("docs/mockups/a.html", "<p>a</p>"), ("docs/mockups/deep/b.svg", "<svg/>"), ("node_modules/pkg/readme.md", "no")] {
            std::fs::write(dir.join(f), text).unwrap();
        }
        let run = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(&dir).output().unwrap().status.success());
        run(&["init", "-q"]);
        run(&["add", "README.md", ".gitignore", "src/main.rs"]);
        let paths = |v: Vec<Page>| v.into_iter().map(|p| (p.path, p.ignored)).collect::<Vec<_>>();
        assert_eq!(paths(pages(&dir, false).await.unwrap()), [("README.md".to_string(), false)]);
        assert_eq!(
            paths(pages(&dir, true).await.unwrap()),
            [("README.md".to_string(), false), ("docs/mockups/a.html".to_string(), true), ("docs/mockups/deep/b.svg".to_string(), true)],
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Files list with the ignored files on demand; a download: one file as it is, else a zip.
    #[tokio::test]
    async fn lists_ignored_files_and_downloads() {
        let dir = std::env::temp_dir().join(format!("prism-download-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for d in ["dist/deep", "node_modules/pkg", "src"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        for (f, text) in [(".gitignore", "dist/\nnode_modules/\n.env\n"), (".env", "TOKEN=x"), ("src/a.rs", "a"), ("src/b.rs", "b"), ("dist/deep/app.js", "js"), ("node_modules/pkg/i.js", "no")] {
            std::fs::write(dir.join(f), text).unwrap();
        }
        let run = |args: &[&str]| assert!(std::process::Command::new("git").args(args).current_dir(&dir).output().unwrap().status.success());
        run(&["init", "-q"]);
        run(&["add", "."]);
        let paths = |l: Listing| l.files.into_iter().map(|f| (f.path, f.ignored)).collect::<Vec<_>>();
        let s = |p: &str, i: bool| (p.to_string(), i);
        assert_eq!(paths(list(&dir, false).await.unwrap()), [s(".gitignore", false), s("src/a.rs", false), s("src/b.rs", false)]);
        assert_eq!(paths(list(&dir, true).await.unwrap()), [s(".env", true), s(".gitignore", false), s("dist/deep/app.js", true), s("src/a.rs", false), s("src/b.rs", false)]);

        // One file as it is; folders as a zip, written to the file.
        let dest = dir.with_extension("zip");
        let create = |p: &Path| std::fs::File::create(p);
        let export = |paths: &[&str], name: &str, max: u64, hidden: bool| export(&dir, &paths.iter().map(|p| p.to_string()).collect::<Vec<_>>(), name, max, hidden, &dest, create);
        let names = || {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
            let mut names: Vec<_> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().to_string()).collect();
            names.sort();
            names
        };
        assert_eq!(export(&["src/a.rs"], "x", MAX_DOWNLOAD, true).unwrap(), "a.rs");
        assert_eq!(std::fs::read(&dest).unwrap(), b"a");
        assert_eq!(export(&["src/", "dist/"], "work", MAX_DOWNLOAD, true).unwrap(), "work.zip");
        assert_eq!(names(), ["dist/deep/app.js", "src/a.rs", "src/b.rs"]);
        let mut zip = zip::ZipArchive::new(std::fs::File::open(&dest).unwrap()).unwrap();
        assert_eq!(std::io::read_to_string(zip.by_name("src/b.rs").unwrap()).unwrap(), "b");
        export(&["/"], "all", MAX_DOWNLOAD, true).unwrap();
        assert!(names().contains(&".env".to_string()));
        export(&["/"], "all", MAX_DOWNLOAD, false).unwrap();
        assert!(!names().iter().any(|n| n.starts_with('.')), "a phone's folder: no hidden file");
        export(&["/", ".env"], "all", MAX_DOWNLOAD, false).unwrap();
        assert!(names().contains(&".env".to_string()), "unless named");
        assert!(export(&["../x"], "x", MAX_DOWNLOAD, true).is_err(), "outside the folder");
        assert!(format!("{:#}", export(&["src/"], "x", 1, true).unwrap_err()).contains("too large"));
        let _ = std::fs::remove_file(dest);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A dependency written since a launch is told, git ignoring it or not.
    #[test]
    fn tells_the_dependencies_written_since() {
        let dir = std::env::temp_dir().join(format!("prism-deps-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("node_modules/.bin")).unwrap();
        std::fs::write(dir.join("node_modules/.bin/vitest"), "old").unwrap();
        let since = std::time::SystemTime::now() + std::time::Duration::from_secs(1);
        assert!(deps_changed(&dir, since).is_empty());
        assert_eq!(deps_changed(&dir, since - std::time::Duration::from_secs(60)), ["node_modules/.bin/vitest"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A Markdown page of the Web view is read like a served file: inside the folder only.
    #[test]
    fn reads_a_text_page_inside_the_folder() {
        let dir = std::env::temp_dir().join(format!("prism-pages-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("docs/notes.md"), "# Notes").unwrap();
        std::fs::write(dir.join("secret.txt"), "no").unwrap();
        assert_eq!(page_text(&dir, "docs/notes.md").unwrap(), "# Notes");
        assert!(page_text(&dir.join("docs"), "../secret.txt").is_err());
        assert!(page_text(&dir, "docs/missing.md").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
