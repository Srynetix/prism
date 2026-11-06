//! The scan: a folder walked by a pool of threads sharing one queue of folders, each folder read
//! once (`read_dir`, then one `lstat` per entry). Only the folders and the big files become nodes
//! of the tree; the other files are summed into their folder, so that the memory grows with the
//! number of folders, not of files. Sizes are what the files take on the disk (their blocks: a
//! sparse disk image counts what it holds), hard links once. The tree lives here; the window asks
//! it a few levels at a time (`Tree::view`).

use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Condvar, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use prism_base::tr;
use serde_json::Value;

use super::{
    kinds::{self, Kind, Reclaim},
    DiskUsageConfig, DiskUsageState, NodeView, Progress, ReclaimGroup, ScanInfo,
};
use crate::BlockCtx;

/// A file this big or more is a node of its own (else it is summed into its folder)…
const KEEP_FILE: u64 = 1 << 20;
/// …among the biggest of its folder.
const KEEP_PER_DIR: usize = 48;
/// A view leaves out what is smaller than this share of the node it starts from.
const VIEW_SHARE: u64 = 2000;
/// And lists this many children of a folder at most.
const VIEW_CHILDREN: usize = 120;

pub const DIR: u8 = 1;
pub const REPO: u8 = 2;
pub const WORKTREE: u8 = 4;
/// Gone since the scan (Trash, Reclaimable).
pub const REMOVED: u8 = 8;

/// A folder, or a file big enough to be listed.
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub name: Box<OsStr>,
    pub parent: u32,
    /// Its children are the nodes `first..first + count` (pushed together).
    pub first: u32,
    pub count: u32,
    /// What it holds that is not a child node: its small files, its own entry.
    pub own: u64,
    pub own_files: u64,
    /// The kind most of `own` is.
    pub own_kind: Kind,
    /// Totals, itself included.
    pub bytes: u64,
    pub files: u64,
    pub kind: Kind,
    /// Its kind is its own (a `node_modules`), not its biggest content's.
    pub classed: bool,
    pub reclaim: Option<Reclaim>,
    pub flags: u8,
    pub delta: Option<i64>,
}

/// A scan's result: nodes in an arena, the root first, each folder's children together after it.
#[derive(Debug, Default)]
pub struct Tree {
    pub path: PathBuf,
    pub nodes: Vec<Node>,
    pub info: ScanInfo,
}

impl Tree {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_dir(&self, id: u32) -> bool {
        self.nodes[id as usize].flags & DIR != 0
    }

    fn children(&self, id: u32) -> impl Iterator<Item = u32> + '_ {
        let n = &self.nodes[id as usize];
        (n.first..n.first + n.count).filter(|c| self.nodes[*c as usize].flags & REMOVED == 0)
    }

    /// Its folders from the root down, itself included: ids and names, for the window's path.
    pub fn ancestors(&self, id: u32) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        let mut i = id;
        loop {
            let name = if i == 0 { super::shorten(&self.path) } else { self.nodes[i as usize].name.to_string_lossy().into_owned() };
            out.push((i, name));
            if i == 0 {
                break;
            }
            i = self.nodes[i as usize].parent;
        }
        out.reverse();
        out
    }

    /// Its path on the disk.
    pub fn path(&self, id: u32) -> PathBuf {
        let mut names = Vec::new();
        let mut i = id;
        while i != 0 {
            names.push(&self.nodes[i as usize].name);
            i = self.nodes[i as usize].parent;
        }
        let mut p = self.path.clone();
        p.extend(names.iter().rev().map(|n| &***n));
        p
    }

    /// Its path from the root, `/`-separated (empty for the root): the saved scan's key.
    pub fn rel_paths(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::with_capacity(self.nodes.len());
        for (i, n) in self.nodes.iter().enumerate() {
            out.push(match i {
                0 => String::new(),
                _ if n.parent == 0 => n.name.to_string_lossy().into_owned(),
                _ => format!("{}/{}", out[n.parent as usize], n.name.to_string_lossy()),
            });
        }
        out
    }

    /// Sums every folder's children into it, the children's ids being higher than their parent's;
    /// a folder without a kind of its own takes its biggest content's.
    pub fn finish(&mut self) {
        for i in (0..self.nodes.len()).rev() {
            let n = &self.nodes[i];
            let (first, count) = (n.first as usize, n.count as usize);
            let (mut bytes, mut files) = (n.own, n.own_files);
            let (mut top, mut kind) = (n.own, n.own_kind);
            for c in &self.nodes[first..first + count] {
                bytes += c.bytes;
                files += c.files;
                if c.bytes > top {
                    (top, kind) = (c.bytes, c.kind);
                }
            }
            let n = &mut self.nodes[i];
            n.bytes = bytes;
            n.files = files;
            if !n.classed {
                n.kind = kind;
            }
        }
    }

    /// Its nodes of rebuildable data, by kind, biggest first.
    pub fn reclaim_groups(&self) -> Vec<ReclaimGroup> {
        let mut groups: Vec<ReclaimGroup> = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            let Some(kind) = n.reclaim.filter(|_| n.flags & REMOVED == 0 && n.bytes > 0) else { continue };
            match groups.iter_mut().find(|g| g.kind == kind) {
                Some(g) => {
                    if n.bytes > self.nodes[g.first as usize].bytes {
                        g.first = i as u32;
                    }
                    g.bytes += n.bytes;
                    g.count += 1;
                }
                None => groups.push(ReclaimGroup { kind, bytes: n.bytes, count: 1, first: i as u32, available: tool_for(kind) }),
            }
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.bytes));
        groups
    }

    /// The nodes of a reclaim group, still there.
    pub fn of_reclaim(&self, kind: Reclaim) -> Vec<u32> {
        (0..self.nodes.len() as u32).filter(|i| self.nodes[*i as usize].reclaim == Some(kind) && self.nodes[*i as usize].flags & REMOVED == 0).collect()
    }

    /// Node `id` gone, and everything under it: its size leaves its ancestors'. The reclaim
    /// groups are the caller's to count again (`refresh`), once for many removals.
    pub fn remove(&mut self, id: u32) {
        if self.nodes[id as usize].flags & REMOVED != 0 {
            return;
        }
        let (bytes, files) = (self.nodes[id as usize].bytes, self.nodes[id as usize].files);
        self.remove_under(id);
        self.shrink_ancestors(id, bytes, files);
    }

    /// `id` and its descendants marked gone (nothing below it is listed nor reclaimed any more).
    fn remove_under(&mut self, id: u32) {
        let mut stack = vec![id];
        while let Some(i) = stack.pop() {
            let n = &mut self.nodes[i as usize];
            n.flags |= REMOVED;
            stack.extend(n.first..n.first + n.count);
        }
    }

    /// Node `id` now holds `bytes` in `files` (a folder emptied): its children are gone.
    pub fn shrink(&mut self, id: u32, bytes: u64, files: u64) {
        let n = &mut self.nodes[id as usize];
        let (less, fewer) = (n.bytes.saturating_sub(bytes), n.files.saturating_sub(files));
        let (first, count) = (n.first as usize, n.count as usize);
        (n.bytes, n.files, n.own, n.own_files) = (bytes, files, bytes, files);
        for c in first..first + count {
            self.remove_under(c as u32);
        }
        self.shrink_ancestors(id, less, fewer);
    }

    fn shrink_ancestors(&mut self, id: u32, bytes: u64, files: u64) {
        let mut i = id;
        while i != 0 {
            i = self.nodes[i as usize].parent;
            let p = &mut self.nodes[i as usize];
            p.bytes = p.bytes.saturating_sub(bytes);
            p.files = p.files.saturating_sub(files);
        }
    }

    /// Its totals and reclaim groups, after removals.
    pub fn refresh(&mut self) {
        self.info.bytes = self.nodes[0].bytes;
        self.info.files = self.nodes[0].files;
        self.info.reclaim = self.reclaim_groups();
    }

    /// Node `id` and `depth` levels of descendants, those under 1/2000th of it summed.
    pub fn view(&self, id: u32, depth: u32) -> NodeView {
        let min = (self.nodes[id as usize].bytes / VIEW_SHARE).max(1);
        self.view_of(id, depth, min, &self.path(id))
    }

    fn view_of(&self, id: u32, depth: u32, min: u64, path: &Path) -> NodeView {
        let n = &self.nodes[id as usize];
        let dir = n.flags & DIR != 0;
        let mut rest_bytes = 0;
        let mut rest_count = 0;
        let children = (dir && depth > 0).then(|| {
            let mut ids: Vec<u32> = self.children(id).collect();
            ids.sort_by_key(|c| std::cmp::Reverse(self.nodes[*c as usize].bytes));
            let mut out = Vec::new();
            for c in ids {
                let child = &self.nodes[c as usize];
                if child.bytes >= min && out.len() < VIEW_CHILDREN {
                    out.push(self.view_of(c, depth - 1, min, &path.join(&*child.name)));
                } else {
                    rest_bytes += child.bytes;
                    rest_count += child.files.max(1) as u32;
                }
            }
            rest_bytes += n.own;
            rest_count += n.own_files as u32;
            out
        });
        NodeView {
            id,
            name: if id == 0 { super::shorten(&self.path) } else { n.name.to_string_lossy().into_owned() },
            path: path.display().to_string(),
            dir,
            bytes: n.bytes,
            files: n.files,
            kind: n.kind,
            delta: n.delta,
            badge: if n.flags & REPO != 0 { Some("repo") } else if n.flags & WORKTREE != 0 { Some("worktree") } else { None },
            reclaim: n.reclaim,
            children,
            rest_bytes,
            rest_count,
        }
    }
}

/// The tool a reclaim group runs is installed (`xcrun`, `docker`); the others need none.
fn tool_for(kind: Reclaim) -> bool {
    static XCRUN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static DOCKER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    match kind {
        Reclaim::Simulators => *XCRUN.get_or_init(|| cfg!(target_os = "macos") && prism_core::detect::which("xcrun").is_some()),
        Reclaim::Docker => *DOCKER.get_or_init(|| prism_core::detect::which("docker").is_some()),
        _ => true,
    }
}

/// What a walk is told.
pub struct WalkOptions {
    /// Never entered.
    pub skip: Vec<PathBuf>,
    /// The devices it stays on; empty: any.
    pub devices: Vec<u64>,
    pub stop: Arc<AtomicBool>,
    pub counters: Arc<Counters>,
}

/// A walk's progress, read while it runs.
#[derive(Default)]
pub struct Counters {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub denied: AtomicU64,
    pub denied_paths: Mutex<Vec<String>>,
    pub current: Mutex<String>,
}

/// Size on the disk, device, inode, links.
#[cfg(unix)]
fn stat(m: &std::fs::Metadata) -> (u64, u64, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.blocks() * 512, m.dev(), m.ino(), m.nlink())
}

#[cfg(not(unix))]
fn stat(m: &std::fs::Metadata) -> (u64, u64, u64, u64) {
    (m.len(), 0, 0, 1)
}

/// A folder's entry, as the walk needs it.
struct Item {
    name: OsString,
    dir: bool,
    /// A regular file (not a link, a socket…).
    file: bool,
    size: u64,
    dev: u64,
    ino: u64,
    links: u64,
    /// Another volume is mounted there (macOS's bulk read says so; elsewhere `dev` does).
    mount: bool,
    /// A folder whose content is in the cloud only: nothing on the disk, and never downloaded to be measured.
    dataless: bool,
}

/// A folder's entries and their sizes: macOS reads them in bulk (`getattrlistbulk`), many entries
/// a call; elsewhere `readdir`, then one `lstat` per entry (relative to the folder).
#[cfg(target_os = "macos")]
fn list(path: &Path) -> std::io::Result<Vec<Item>> {
    use prism_sys::dirlist::Type;
    Ok(prism_sys::dirlist::read(path)?
        .into_iter()
        .map(|e| Item { dir: e.kind == Type::Dir, file: e.kind == Type::File, size: e.alloc, dev: e.dev, ino: e.ino, links: e.links.into(), mount: e.mount_point, dataless: e.dataless, name: e.name })
        .collect())
}

#[cfg(not(target_os = "macos"))]
fn list(path: &Path) -> std::io::Result<Vec<Item>> {
    Ok(std::fs::read_dir(path)?
        .flatten()
        .filter_map(|entry| {
            let ft = entry.file_type().ok()?;
            let (size, dev, ino, links) = stat(&entry.metadata().ok()?);
            Some(Item { name: entry.file_name(), dir: ft.is_dir(), file: ft.is_file(), size, dev, ino, links, mount: false, dataless: false })
        })
        .collect())
}

pub fn device_of(path: &Path) -> Option<u64> {
    std::fs::symlink_metadata(path).ok().map(|m| stat(&m).1)
}

struct Job {
    id: u32,
    path: PathBuf,
    /// Inside rebuildable data: nothing below is classed as reclaimable again.
    reclaimed: bool,
}

struct Walk<'a> {
    opts: &'a WalkOptions,
    home: PathBuf,
    queue: Mutex<Vec<Job>>,
    ready: Condvar,
    /// Folders queued or being read.
    pending: AtomicUsize,
    nodes: Mutex<Vec<Node>>,
    /// Files with several links, by device and inode: counted once. In shards, by inode: a
    /// pnpm `node_modules` is made of hard links.
    linked: Vec<Mutex<HashSet<(u64, u64)>>>,
}

/// Walks `root` with `threads` threads: its tree, sizes summed. Stops early (a partial tree) when
/// `opts.stop` is set.
pub fn walk(root: &Path, threads: usize, opts: &WalkOptions) -> Tree {
    let own = std::fs::symlink_metadata(root).map(|m| stat(&m).0).unwrap_or(0);
    let w = Walk {
        opts,
        home: dirs::home_dir().unwrap_or_default(),
        queue: Mutex::new(vec![Job { id: 0, path: root.to_path_buf(), reclaimed: false }]),
        ready: Condvar::new(),
        pending: AtomicUsize::new(1),
        nodes: Mutex::new(vec![Node { name: root.as_os_str().into(), own, flags: DIR, ..Default::default() }]),
        linked: (0..64).map(|_| Mutex::new(HashSet::new())).collect(),
    };
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| w.work());
        }
    });
    let mut tree = Tree { path: root.to_path_buf(), nodes: w.nodes.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner), info: ScanInfo::default() };
    tree.finish();
    tree
}

impl Walk<'_> {
    fn work(&self) {
        let mut read = 0u32;
        loop {
            let job = {
                let mut q = self.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                loop {
                    if let Some(j) = q.pop() {
                        break Some(j);
                    }
                    if self.pending.load(Ordering::SeqCst) == 0 {
                        break None;
                    }
                    q = self.ready.wait(q).unwrap_or_else(std::sync::PoisonError::into_inner);
                }
            };
            let Some(job) = job else {
                self.ready.notify_all();
                return;
            };
            if !self.opts.stop.load(Ordering::Relaxed) {
                read += 1;
                if read.is_multiple_of(32)
                    && let Ok(mut c) = self.opts.counters.current.try_lock()
                {
                    *c = job.path.display().to_string();
                }
                let path = job.path.clone();
                // A folder that breaks the walk is skipped, never left pending (the others would wait for it).
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.read(job))).is_err() {
                    tracing::warn!("disk usage: {} skipped", path.display());
                }
            }
            if self.pending.fetch_sub(1, Ordering::SeqCst) == 1 {
                self.ready.notify_all();
            }
        }
    }

    /// Reads one folder: its subfolders queued, its big files as nodes, the rest summed.
    fn read(&self, job: Job) {
        let c = &self.opts.counters;
        let entries = match list(&job.path) {
            Ok(e) => e,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(1) {
                    c.denied.fetch_add(1, Ordering::Relaxed);
                    let mut paths = c.denied_paths.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    if paths.len() < 5 {
                        paths.push(super::shorten(&job.path));
                    }
                }
                return;
            }
        };
        let mut names: Vec<OsString> = Vec::with_capacity(entries.len());
        let mut dirs: Vec<(OsString, u64)> = Vec::new();
        let mut big: Vec<(OsString, u64, Kind)> = Vec::new();
        let (mut own, mut own_files, mut bytes) = (0u64, 0u64, 0u64);
        let mut by_kind = [0u64; Kind::ALL.len()];
        let mut flags = 0;
        for e in entries {
            if e.name == ".git" {
                flags |= if e.dir { REPO } else { WORKTREE };
            }
            if e.dir {
                let other_volume = !self.opts.devices.is_empty() && (e.mount || !self.opts.devices.contains(&e.dev));
                if other_volume || e.dataless || self.opts.skip.iter().any(|s| s.file_name() == Some(&*e.name) && s.parent() == Some(&*job.path)) {
                    continue;
                }
                dirs.push((e.name.clone(), e.size));
            } else {
                if e.links > 1 && e.file && !self.linked[(e.ino % 64) as usize].lock().unwrap_or_else(std::sync::PoisonError::into_inner).insert((e.dev, e.ino)) {
                    continue;
                }
                own_files += 1;
                bytes += e.size;
                let kind = kinds::file_kind(&e.name);
                if e.size >= KEEP_FILE && e.file {
                    big.push((e.name.clone(), e.size, kind));
                } else {
                    own += e.size;
                    by_kind[kind.code() as usize] += e.size;
                }
            }
            names.push(e.name);
        }
        big.sort_by_key(|b| std::cmp::Reverse(b.1));
        for (_, size, kind) in big.drain(KEEP_PER_DIR.min(big.len())..) {
            own += size;
            by_kind[kind.code() as usize] += size;
        }
        let own_files = own_files - big.len() as u64;
        c.files.fetch_add(own_files + big.len() as u64, Ordering::Relaxed);
        c.bytes.fetch_add(bytes, Ordering::Relaxed);
        c.dirs.fetch_add(1, Ordering::Relaxed);

        let parent_name = job.path.file_name().unwrap_or_default();
        let home = job.path == self.home;
        let sibling = |s: &str| names.iter().any(|n| n == s);
        let classes: Vec<kinds::DirClass> = dirs.iter().map(|(n, _)| kinds::dir_class(n, parent_name, home, sibling)).collect();
        let own_kind = Kind::from_code(by_kind.iter().enumerate().max_by_key(|(_, b)| **b).map_or(0, |(i, _)| i as u8));

        let first = {
            let mut nodes = self.nodes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let first = nodes.len() as u32;
            for ((name, size), class) in dirs.iter().zip(&classes) {
                let reclaim = if job.reclaimed { None } else { class.reclaim };
                nodes.push(Node { name: name.as_os_str().into(), parent: job.id, own: *size, kind: class.kind.unwrap_or_default(), classed: class.kind.is_some(), reclaim, flags: DIR, ..Default::default() });
            }
            for (name, size, kind) in &big {
                nodes.push(Node { name: name.as_os_str().into(), parent: job.id, own: *size, own_files: 1, own_kind: *kind, kind: *kind, classed: true, ..Default::default() });
            }
            let n = &mut nodes[job.id as usize];
            n.first = first;
            n.count = (dirs.len() + big.len()) as u32;
            n.own += own;
            n.own_files = own_files;
            n.own_kind = own_kind;
            n.flags |= flags;
            first
        };
        if dirs.is_empty() {
            return;
        }
        self.pending.fetch_add(dirs.len(), Ordering::SeqCst);
        let mut q = self.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (i, ((name, _), class)) in dirs.into_iter().zip(classes).enumerate() {
            q.push(Job { id: first + i as u32, path: job.path.join(name), reclaimed: job.reclaimed || class.reclaim.is_some() });
        }
        drop(q);
        self.ready.notify_all();
    }
}

/// Threads for a walk: reading folders waits on the disk, an SSD answers several at once.
pub fn threads() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(4, 12)
}

/// What a walk of `root` leaves out: `exclude`, and the system's own (macOS's system volumes seen
/// again through the firmlinks from `/`, Linux's `/proc`, `/sys`…); and the devices it stays on.
pub fn options(root: &Path, cfg: &DiskUsageConfig, stop: Arc<AtomicBool>, counters: Arc<Counters>) -> WalkOptions {
    let mut skip: Vec<PathBuf> = cfg.exclude.iter().map(|e| super::expand(e)).collect();
    let mut devices = Vec::new();
    if cfg!(target_os = "macos") {
        skip.extend(["/System/Volumes", "/dev"].map(PathBuf::from));
    } else if cfg!(target_os = "linux") {
        skip.extend(["/proc", "/sys", "/dev", "/run"].map(PathBuf::from));
    }
    if cfg.one_volume {
        devices.extend(device_of(root));
        // macOS: `/` is the read-only system volume, its data (`/Users`, `/Applications`,
        // `/Library`…) on the Data volume behind it.
        if cfg!(target_os = "macos") && root == Path::new("/") {
            devices.extend(device_of(Path::new("/System/Volumes/Data")));
        }
    }
    WalkOptions { skip, devices, stop, counters }
}

/// The block's side that lasts: its settings, the tree shown, the scan running.
pub struct Scanner {
    pub cfg: DiskUsageConfig,
    pub ctx: BlockCtx,
    shown: Mutex<DiskUsageState>,
    tree: RwLock<Option<Tree>>,
    /// The running scan's own stop: a scan ending never stops the next one.
    stop: Mutex<Arc<AtomicBool>>,
    running: AtomicBool,
    /// The Trash being emptied: one at a time.
    emptying: AtomicBool,
    /// Bumped by each emptying: a measure started before it is not published.
    trash_gen: AtomicU64,
}

static SCANNER: prism_base::rt::Swap<Scanner> = prism_base::rt::Swap::new();

pub fn scanner() -> Result<&'static Scanner> {
    SCANNER.get().context(tr!("the Disk usage block is off ([disk_usage] enabled)"))
}

/// Ready for scans; the last scan saved is shown again.
pub fn start(cfg: DiskUsageConfig, ctx: BlockCtx) {
    let state = DiskUsageState { roots: cfg.roots.clone(), cleanup: cfg.cleanup, delta: cfg.delta, ..Default::default() };
    ctx.publish(&state);
    ctx.set_error(None);
    let s = SCANNER.set(Scanner { cfg, ctx, shown: Mutex::new(state), tree: RwLock::new(None), stop: Mutex::new(Arc::new(AtomicBool::new(false))), running: AtomicBool::new(false), emptying: AtomicBool::new(false), trash_gen: AtomicU64::new(0) });
    if s.cfg.cleanup {
        s.measure_trash();
    }
    prism_base::rt::spawn(async move {
        let dir = s.store_dir();
        let Ok(Some(tree)) = tokio::task::spawn_blocking(move || super::store::load_latest(&dir)).await.map_err(anyhow::Error::from).and_then(|r| r) else { return };
        if s.running.load(Ordering::SeqCst) || s.tree.read().expect("tree").is_some() {
            return;
        }
        s.show(tree);
    });
}

/// Stops a walk's thread when the task waiting on it is dropped (the block reloads, Prism quits).
struct StopOnDrop(Arc<AtomicBool>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl Scanner {
    fn store_dir(&self) -> PathBuf {
        self.ctx.data_dir().join("disk-usage")
    }

    fn publish(&self, f: impl FnOnce(&mut DiskUsageState)) {
        let mut shown = self.shown.lock().expect("disk usage state");
        f(&mut shown);
        self.ctx.publish(&*shown);
    }

    fn show(&self, tree: Tree) {
        let info = tree.info.clone();
        let disk = super::disk_of(&tree.path);
        *self.tree.write().expect("tree") = Some(tree);
        self.publish(|s| {
            s.scan = Some(info);
            s.disk = disk;
        });
    }

    /// Scans `root` in the background; one scan at a time.
    pub fn scan(&'static self, root: &str) -> Result<()> {
        let path = super::checked_root(root)?;
        if self.running.swap(true, Ordering::SeqCst) {
            bail!(tr!("a scan is already running"));
        }
        let stop = Arc::new(AtomicBool::new(false));
        *self.stop.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = stop.clone();
        let counters = Arc::new(Counters::default());
        let opts = options(&path, &self.cfg, stop.clone(), counters.clone());
        let started = Utc::now();
        let label = super::shorten(&path);
        self.publish(|s| s.running = Some(Progress { root: label.clone(), started_at: started, ..Default::default() }));
        prism_base::rt::spawn(async move {
            let _guard = StopOnDrop(stop.clone());
            let clock = Instant::now();
            let root = path.clone();
            let mut job = tokio::task::spawn_blocking(move || walk(&root, threads(), &opts));
            let tree = loop {
                tokio::select! {
                    done = &mut job => break done,
                    _ = tokio::time::sleep(Duration::from_millis(400)) => {
                        // Stopped: the block answers at once, even with a thread still waiting on a disk that does not.
                        if stop.load(Ordering::SeqCst) {
                            break Ok(Tree::default());
                        }
                        let p = Progress {
                            root: label.clone(),
                            started_at: started,
                            files: counters.files.load(Ordering::Relaxed),
                            dirs: counters.dirs.load(Ordering::Relaxed),
                            bytes: counters.bytes.load(Ordering::Relaxed),
                            current: counters.current.lock().map(|c| super::shorten(Path::new(&*c))).unwrap_or_default(),
                        };
                        self.publish(|s| s.running = Some(p));
                    }
                }
            };
            let stopped = stop.load(Ordering::SeqCst);
            let result = match tree {
                Ok(_) if stopped => Ok(None),
                Ok(mut tree) => {
                    let denied_paths = counters.denied_paths.lock().map(|p| p.clone()).unwrap_or_default();
                    tree.info = ScanInfo {
                        id: started.timestamp_millis() as u64,
                        root: label.clone(),
                        at: started,
                        duration_ms: clock.elapsed().as_millis() as u64,
                        bytes: tree.nodes[0].bytes,
                        files: tree.nodes[0].files,
                        dirs: counters.dirs.load(Ordering::Relaxed),
                        denied: counters.denied.load(Ordering::Relaxed),
                        denied_paths,
                        ..Default::default()
                    };
                    tree.info.reclaim = tree.reclaim_groups();
                    tracing::info!("disk usage: {} scanned in {} ms, {} files, {} folders", label, tree.info.duration_ms, tree.info.files, tree.info.dirs);
                    let dir = self.store_dir();
                    let delta = self.cfg.delta;
                    // Δ against the saved scan of this root, then this one saved in its place.
                    tokio::task::spawn_blocking(move || {
                        if let Err(e) = super::store::compare_and_save(&dir, &mut tree, delta) {
                            tracing::warn!("disk usage: scan not saved: {e:#}");
                        }
                        tree
                    })
                    .await
                    .map(Some)
                    .map_err(anyhow::Error::from)
                }
                Err(e) => Err(e.into()),
            };
            self.running.store(false, Ordering::SeqCst);
            self.publish(|s| s.running = None);
            match result {
                Ok(Some(tree)) => {
                    self.ctx.set_error(None);
                    self.show(tree);
                }
                Ok(None) => {}
                Err(e) => self.ctx.set_error(Some(format!("{e:#}"))),
            }
        });
        Ok(())
    }

    pub fn stop(&self) {
        self.stop.lock().unwrap_or_else(std::sync::PoisonError::into_inner).store(true, Ordering::SeqCst);
    }

    pub fn cleanup_allowed(&self) -> Result<()> {
        anyhow::ensure!(self.cfg.cleanup, tr!("the Disk usage block only reads ([disk_usage] cleanup)"));
        Ok(())
    }

    /// The path of node `args.id` to move to the Trash: never the scanned folder itself.
    pub fn trashable(&self, args: &Value) -> Result<PathBuf> {
        anyhow::ensure!(args.get("id").and_then(Value::as_u64).is_some_and(|id| id != 0), tr!("the scanned folder itself does not go to the Trash"));
        self.read(args, |tree, id| tree.path(id))
    }

    /// `f` on the tree of the scan `args.scan` and its node `args.id`.
    pub fn read<T>(&self, args: &Value, f: impl FnOnce(&Tree, u32) -> T) -> Result<T> {
        let guard = self.tree.read().expect("tree");
        let tree = guard.as_ref().context(tr!("no scan yet"))?;
        let id = node_arg(tree, args)?;
        Ok(f(tree, id))
    }

    /// Node `id` gone (to the Trash): the tree and the block follow.
    pub fn removed(&self, args: &Value) -> Result<()> {
        let info = {
            let mut guard = self.tree.write().expect("tree");
            let tree = guard.as_mut().context(tr!("no scan yet"))?;
            let id = node_arg(tree, args)?;
            tree.remove(id);
            tree.refresh();
            tree.info.clone()
        };
        self.publish(|s| s.scan = Some(info));
        Ok(())
    }

    /// The Trash measured again, in the background (a walk of the Trash only). Not published when
    /// an emptying started meanwhile: its own figures are the newer ones.
    pub fn measure_trash(&'static self) {
        let generation = self.trash_gen.load(Ordering::SeqCst);
        prism_base::rt::spawn(async move {
            let Ok(m) = tokio::task::spawn_blocking(|| super::trash::measure(&super::trash::bins())).await else { return };
            if self.emptying.load(Ordering::SeqCst) || self.trash_gen.load(Ordering::SeqCst) != generation {
                return;
            }
            self.publish(|s| s.trash = Some(m));
        });
    }

    /// The Trash emptied: every one, or (`only`) those of these folders (a scan's). The scan shown
    /// follows (its Trash folders measured again), and the disk.
    pub async fn empty_trash(&'static self, only: Option<Vec<PathBuf>>) -> Result<super::trash::Emptied> {
        if self.emptying.swap(true, Ordering::SeqCst) {
            bail!(tr!("the Trash is already being emptied"));
        }
        /// Never left set, whatever happens: another Empty must stay possible.
        struct Done(&'static Scanner);
        impl Drop for Done {
            fn drop(&mut self) {
                self.0.emptying.store(false, Ordering::SeqCst);
                self.0.trash_gen.fetch_add(1, Ordering::SeqCst);
                self.0.publish(|s| {
                    if let Some(t) = &mut s.trash {
                        t.emptying = false;
                    }
                });
            }
        }
        let _done = Done(self);
        self.trash_gen.fetch_add(1, Ordering::SeqCst);
        self.publish(|s| {
            if let Some(t) = &mut s.trash {
                t.emptying = true;
            }
        });
        tracing::info!("disk usage: emptying the Trash");
        let done = tokio::task::spawn_blocking(move || {
            let all = super::trash::bins();
            let bins: Vec<_> = match &only {
                Some(dirs) => all.into_iter().filter(|b| dirs.contains(&b.dir)).collect(),
                None => all,
            };
            anyhow::ensure!(!bins.is_empty(), tr!("nothing left to free there"));
            let mut done = super::trash::empty(&bins)?;
            // What the line shows: every Trash, not only those emptied.
            if only.is_some() {
                done.after = super::trash::measure(&super::trash::bins());
            }
            Ok(done)
        })
        .await??;
        let after = done.after.clone();
        self.publish(|s| s.trash = Some(after));
        // The scan's Trash folders, as they are now.
        let (scan, targets): (u64, Vec<(u32, PathBuf)>) = {
            let guard = self.tree.read().expect("tree");
            match guard.as_ref() {
                Some(tree) => (tree.info.id, tree.of_reclaim(Reclaim::Trash).into_iter().map(|id| (id, tree.path(id))).collect()),
                None => (0, Vec::new()),
            }
        };
        let path = self.tree.read().expect("tree").as_ref().map(|t| t.path.clone()).or_else(dirs::home_dir).unwrap_or_default();
        let (sizes, disk): (Vec<(u32, (u64, u64))>, _) = tokio::task::spawn_blocking(move || (targets.into_iter().map(|(id, p)| (id, measure(&p))).collect(), super::disk_of(&path))).await?;
        if !sizes.is_empty() {
            let mut guard = self.tree.write().expect("tree");
            if let Some(tree) = guard.as_mut().filter(|t| t.info.id == scan) {
                for (id, (bytes, files)) in sizes {
                    // Emptied: gone from the tree and the treemap, as a cleanup's folder.
                    if bytes == 0 {
                        tree.remove(id);
                    } else {
                        tree.shrink(id, bytes, files);
                    }
                }
                tree.refresh();
                let info = tree.info.clone();
                self.publish(|s| s.scan = Some(info));
            }
        }
        self.publish(|s| s.disk = disk);
        if done.left > 0 {
            tracing::info!("disk usage: {} items left in the Trash", done.left);
        }
        Ok(done)
    }

    /// Frees a reclaim group: what it freed, in bytes. The Trash: every one emptied, as from its line.
    pub async fn clean(&'static self, kind: Reclaim) -> Result<u64> {
        // The Trash folders of the scan only (what the group shows and its button confirms),
        // through the same path as the Trash's line.
        if kind == Reclaim::Trash {
            let dirs: Vec<PathBuf> = {
                let guard = self.tree.read().expect("tree");
                let tree = guard.as_ref().context(tr!("no scan yet"))?;
                tree.of_reclaim(kind).into_iter().map(|id| tree.path(id)).collect()
            };
            return self.empty_trash(Some(dirs)).await.map(|d| d.freed);
        }
        let (scan, root, targets): (u64, PathBuf, Vec<(u32, PathBuf, u64)>) = {
            let guard = self.tree.read().expect("tree");
            let tree = guard.as_ref().context(tr!("no scan yet"))?;
            (tree.info.id, tree.path.clone(), tree.of_reclaim(kind).into_iter().map(|id| (id, tree.path(id), tree.nodes[id as usize].bytes)).collect())
        };
        anyhow::ensure!(!targets.is_empty(), tr!("nothing left to free there"));
        tracing::info!("disk usage: freeing {kind:?} ({} places)", targets.len());
        let after = super::clean::clean(kind, &root, targets.iter().map(|(_, p, _)| p.clone()).collect()).await?;
        let mut failed = Vec::new();
        let mut freed = 0;
        {
            let mut guard = self.tree.write().expect("tree");
            // Another scan replaced it meanwhile: its ids are not these; it measured the disk after the cleanup anyway.
            if let Some(tree) = guard.as_mut().filter(|t| t.info.id == scan) {
                for ((id, _, before), left) in targets.iter().zip(after) {
                    match left {
                        Ok(None) => tree.remove(*id),
                        Ok(Some((bytes, files))) => tree.shrink(*id, bytes, files),
                        Err(e) => {
                            failed.push(e);
                            continue;
                        }
                    }
                    freed += before.saturating_sub(tree.nodes[*id as usize].bytes * u64::from(tree.nodes[*id as usize].flags & REMOVED == 0));
                }
                tree.refresh();
                let info = tree.info.clone();
                self.publish(|s| s.scan = Some(info));
            }
        }
        let disk = super::disk_of(&root);
        self.publish(|s| s.disk = disk);
        // What was freed stays freed; the first failure is said, with how many there were.
        if let Some(first) = failed.first() {
            bail!("{first:#} ({})", tr!("{n} of {total} not freed", n = failed.len(), total = targets.len()));
        }
        Ok(freed)
    }
}

fn node_arg(tree: &Tree, args: &Value) -> Result<u32> {
    let scan = args.get("scan").and_then(Value::as_u64);
    anyhow::ensure!(scan.is_none_or(|s| s == tree.info.id), tr!("this scan was replaced: look again"));
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0);
    anyhow::ensure!((id as usize) < tree.len() && tree.nodes[id as usize].flags & REMOVED == 0, tr!("no such file or folder in the scan"));
    Ok(id as u32)
}

/// What is left in `path` after it was emptied: its size and files, by a quick walk.
pub fn measure(path: &Path) -> (u64, u64) {
    let opts = WalkOptions { skip: Vec::new(), devices: Vec::new(), stop: Arc::new(AtomicBool::new(false)), counters: Arc::new(Counters::default()) };
    let tree = walk(path, threads(), &opts);
    (tree.nodes[0].bytes, tree.nodes[0].files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> WalkOptions {
        WalkOptions { skip: Vec::new(), devices: Vec::new(), stop: Arc::new(AtomicBool::new(false)), counters: Arc::new(Counters::default()) }
    }

    #[test]
    fn walks_sums_and_classes() {
        let dir = std::env::temp_dir().join(format!("prism-du-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("app/node_modules/a/node_modules/b")).unwrap();
        std::fs::create_dir_all(dir.join("rust/target/debug")).unwrap();
        std::fs::write(dir.join("rust/Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.join("rust/target/debug/big.rlib"), vec![7u8; 3 << 20]).unwrap();
        std::fs::write(dir.join("app/node_modules/a/index.js"), vec![1u8; 10_000]).unwrap();
        std::fs::write(dir.join("small.txt"), "hi").unwrap();
        std::fs::create_dir(dir.join(".git")).unwrap();
        let tree = walk(&dir, 3, &opts());
        let root = &tree.nodes[0];
        assert_eq!(root.files, 4);
        assert!(root.bytes >= (3 << 20) + 10_000, "{}", root.bytes);
        assert_eq!(root.flags & REPO, REPO);
        // Each folder's total is its own plus its children's.
        for (i, n) in tree.nodes.iter().enumerate() {
            let kids: u64 = (n.first..n.first + n.count).map(|c| tree.nodes[c as usize].bytes).sum();
            assert_eq!(n.bytes, n.own + kids, "node {i}");
        }
        let groups = tree.reclaim_groups();
        let kinds: Vec<Reclaim> = groups.iter().map(|g| g.kind).collect();
        assert_eq!(kinds, [Reclaim::RustTarget, Reclaim::NodeModules]);
        // The node_modules inside a node_modules is part of the first one.
        assert_eq!(groups[1].count, 1);
        let target = tree.of_reclaim(Reclaim::RustTarget)[0];
        assert_eq!(tree.path(target), dir.join("rust/target"));
        assert_eq!(tree.nodes[target as usize].kind, Kind::Build);
        // The big file is a node, the small ones are not.
        let v = tree.view(0, 4);
        let rust = v.children.as_ref().unwrap().iter().find(|c| c.name == "rust").unwrap();
        let debug = &rust.children.as_ref().unwrap()[0].children.as_ref().unwrap()[0];
        assert_eq!(debug.children.as_ref().unwrap()[0].name, "big.rlib");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn removal_leaves_the_ancestors() {
        let dir = std::env::temp_dir().join(format!("prism-du-rm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::write(dir.join("a/b/f.bin"), vec![1u8; 2 << 20]).unwrap();
        let mut tree = walk(&dir, 2, &opts());
        let total = tree.nodes[0].bytes;
        let b = (0..tree.len() as u32).find(|i| &*tree.nodes[*i as usize].name == "b").unwrap();
        let size = tree.nodes[b as usize].bytes;
        tree.remove(b);
        assert_eq!(tree.nodes[0].bytes, total - size);
        let a = tree.nodes[b as usize].parent;
        assert_eq!(tree.nodes[a as usize].bytes, tree.nodes[a as usize].own);
        assert!(tree.view(a, 2).children.unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_removed_takes_its_reclaimable_with_it() {
        let dir = std::env::temp_dir().join(format!("prism-du-sub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("proj/node_modules")).unwrap();
        std::fs::write(dir.join("proj/node_modules/x.bin"), vec![1u8; 2 << 20]).unwrap();
        let mut tree = walk(&dir, 2, &opts());
        assert_eq!(tree.of_reclaim(Reclaim::NodeModules).len(), 1);
        let proj = (0..tree.len() as u32).find(|i| &*tree.nodes[*i as usize].name == "proj").unwrap();
        tree.remove(proj);
        tree.refresh();
        assert!(tree.of_reclaim(Reclaim::NodeModules).is_empty() && tree.info.reclaim.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The speed of a real walk: `PRISM_DU=~ cargo test -p prism-blocks --release disk_usage::scan::tests::speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn speed() {
        let root = super::super::expand(&std::env::var("PRISM_DU").unwrap_or_else(|_| "~".into()));
        for threads in [4, 8, 12] {
            let cfg = DiskUsageConfig::default();
            let o = options(&root, &cfg, Arc::new(AtomicBool::new(false)), Arc::new(Counters::default()));
            let t = Instant::now();
            let tree = walk(&root, threads, &o);
            println!("{threads:>2} threads: {:>6} ms, {} files, {} nodes, {} MB", t.elapsed().as_millis(), tree.nodes[0].files, tree.len(), tree.nodes[0].bytes >> 20);
        }
    }
}
