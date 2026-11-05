//! The Disk usage block (`[disk_usage]`): where the space of a folder or a disk goes, scanned on
//! demand only — nothing is watched between two scans. The window asks the tree three levels at a
//! time (`tree`), never the whole of it; the scan keeps the folders and the big files, the small
//! files summed per folder. What can be rebuilt (`node_modules`, `target/`, caches, the Trash…)
//! is gathered in Reclaimable, freed in two clicks. The last scan of each root is saved, for the
//! growth since the one before (Δ) and to show it again after a restart. macOS and Linux. Its
//! window side: `web/src/blocks/disk_usage/`.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use prism_base::tr;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{sdk::RemoteClass, Block, BlockCtx};

mod clean;
mod demo;
mod kinds;
mod scan;
mod store;

pub use kinds::{Kind, Reclaim};

pub struct DiskUsage;

pub static BLOCK: DiskUsage = DiskUsage;

impl Block for DiskUsage {
    fn id(&self) -> &'static str {
        "disk_usage"
    }

    fn remote(&self, command: &str, _args: &Value) -> RemoteClass {
        match command {
            "tree" | "ancestors" => RemoteClass::Read,
            "scan" | "stop" => RemoteClass::Act,
            "trash" | "clean" => RemoteClass::Critical,
            _ => RemoteClass::Never,
        }
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg: DiskUsageConfig = super::section(config, "disk_usage")?;
        if cfg.enabled {
            scan::start(cfg, ctx);
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        super::section::<DiskUsageConfig>(config, "disk_usage").ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("disk_usage", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            let s = scan::scanner()?;
            match command.as_str() {
                // A folder (`root`, `~` expanded), else the first of `[disk_usage] roots`.
                "scan" => {
                    let root = args.get("root").and_then(Value::as_str).map(str::to_string).or_else(|| s.cfg.roots.first().cloned()).unwrap_or_else(|| "~".into());
                    s.scan(&root)?;
                    Ok(Value::Null)
                }
                "stop" => {
                    s.stop();
                    Ok(Value::Null)
                }
                // The node `id` of scan `scan` and its descendants `depth` levels down, small ones summed.
                "tree" => Ok(serde_json::to_value(s.read(&args, |tree, id| tree.view(id, depth(&args)))?)?),
                // Its folders from the root down, itself included: `[[id, name]…]`.
                "ancestors" => Ok(serde_json::to_value(s.read(&args, |tree, id| tree.ancestors(id))?)?),
                // The file manager on it.
                "reveal" => {
                    prism_base::platform::reveal(&s.read(&args, |tree, id| tree.path(id))?)?;
                    Ok(Value::Null)
                }
                // A terminal in it (a file: in its folder).
                "terminal" => {
                    let dir = s.read(&args, |tree, id| {
                        let path = tree.path(id);
                        if tree.is_dir(id) { path } else { path.parent().map(Path::to_path_buf).unwrap_or(path) }
                    })?;
                    let app = s.ctx.snapshot(|snap| snap.open_with.terminal.clone()).unwrap_or_else(|| prism_base::platform::DEFAULT_TERMINAL.into());
                    prism_base::platform::open_terminal_in(&dir, &app).await?;
                    Ok(Value::Null)
                }
                // To the Trash (the space is freed once it is emptied).
                "trash" => {
                    s.cleanup_allowed()?;
                    let path = s.read(&args, |tree, id| tree.path(id))?;
                    tracing::info!("disk usage: {} to the Trash", path.display());
                    tokio::task::spawn_blocking(move || clean::trash(&path)).await??;
                    s.removed(&args)?;
                    Ok(Value::Null)
                }
                // Frees a group of Reclaimable (`reclaim`): what it freed, in bytes.
                "clean" => {
                    s.cleanup_allowed()?;
                    let reclaim: Reclaim = serde_json::from_value(args.get("reclaim").cloned().unwrap_or_default()).context("clean: a reclaim group")?;
                    let freed = s.clean(reclaim).await?;
                    Ok(json!(freed))
                }
                other => bail!("the Disk usage block has no command {other}"),
            }
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }

    fn demo_call(&self, command: &str, args: &Value) -> Option<Result<Value>> {
        match command {
            "tree" => {
                let tree = demo::tree();
                Some(match args.get("id").and_then(Value::as_u64) {
                    Some(id) if (id as usize) < tree.len() => serde_json::to_value(tree.view(id as u32, depth(args))).map_err(Into::into),
                    _ => Err(anyhow::anyhow!("no such node")),
                })
            }
            "ancestors" => {
                let tree = demo::tree();
                Some(match args.get("id").and_then(Value::as_u64) {
                    Some(id) if (id as usize) < tree.len() => serde_json::to_value(tree.ancestors(id as u32)).map_err(Into::into),
                    _ => Err(anyhow::anyhow!("no such node")),
                })
            }
            _ => None,
        }
    }
}

fn depth(args: &Value) -> u32 {
    args.get("depth").and_then(Value::as_u64).unwrap_or(3).clamp(1, 4) as u32
}

/// `[disk_usage]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DiskUsageConfig {
    /// On by default: nothing is read until a scan is asked for.
    pub enabled: bool,
    /// The folders offered to scan (`~` for the home folder, `/` for the whole disk); any other
    /// can be chosen from the block.
    pub roots: Vec<String>,
    /// Folders never entered (`~/Library/Mobile Documents`); `~` expanded.
    pub exclude: Vec<String>,
    /// Stays on the scanned folder's volume: other disks mounted inside it are left out.
    pub one_volume: bool,
    /// The growth of each folder since the scan before (Δ): the last scan of each root is saved.
    pub delta: bool,
    /// Reclaimable and its buttons, and the Trash from the block; off: the block only reads.
    pub cleanup: bool,
}

impl Default for DiskUsageConfig {
    fn default() -> Self {
        Self { enabled: true, roots: vec!["~".into(), "/".into()], exclude: Vec::new(), one_volume: true, delta: true, cleanup: true }
    }
}

/// `~` and `~/…` as the home folder.
pub fn expand(path: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    match path.trim() {
        "~" => home,
        p => match p.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => PathBuf::from(p),
        },
    }
}

/// The home folder as `~`, for display.
pub fn shorten(path: &Path) -> String {
    let home = dirs::home_dir().unwrap_or_default();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// What the block shows: the last scan, the one running, the disk it is on.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiskUsageState {
    /// `[disk_usage] roots`, as written (`~`).
    pub roots: Vec<String>,
    pub cleanup: bool,
    pub delta: bool,
    /// The scan shown; absent before the first one.
    pub scan: Option<ScanInfo>,
    /// The scan running; the one before stays shown meanwhile.
    pub running: Option<Progress>,
    /// The volume of the scan shown.
    pub disk: Option<DiskSpace>,
}

/// A scan done: its root, its totals, and what can be freed.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskScan"))]
pub struct ScanInfo {
    /// Changes with each scan: the window's calls name it, a node id is only valid in its scan.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: u64,
    /// `~/Development`, `/`.
    pub root: String,
    pub at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub duration_ms: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub files: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub dirs: u64,
    /// Folders that could not be read (permissions), a few of them named.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub denied: u64,
    pub denied_paths: Vec<String>,
    /// When the scan Δ compares with was made; absent: no Δ.
    pub previous_at: Option<DateTime<Utc>>,
    /// Read back from the saved scan (after a restart): small folders and files are summed in
    /// their parent's, and no file counts below that.
    pub saved: bool,
    pub reclaim: Vec<ReclaimGroup>,
}

/// A kind of rebuildable data found in the scan: how much, and where.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskReclaimGroup"))]
pub struct ReclaimGroup {
    pub kind: Reclaim,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
    /// How many folders (or files) of that kind.
    pub count: u32,
    /// The biggest one's node, to show it.
    pub first: u32,
    /// The tool it needs is there (`xcrun`, `docker`).
    pub available: bool,
}

/// A scan running.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskScanProgress"))]
pub struct Progress {
    pub root: String,
    pub started_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub files: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub dirs: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
    /// A folder being read.
    pub current: String,
}

/// The volume a scan is on.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiskSpace {
    pub name: String,
    pub mount: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub total: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub available: u64,
}

/// A node as the window gets it (`tree`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskNode"))]
pub struct NodeView {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub dir: bool,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub files: u64,
    pub kind: Kind,
    /// Growth since the scan before, in bytes; absent: no scan before, or not known.
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub delta: Option<i64>,
    /// A git repository (`repo`) or a worktree.
    #[cfg_attr(feature = "ts", ts(type = "'repo' | 'worktree' | null"))]
    pub badge: Option<&'static str>,
    pub reclaim: Option<Reclaim>,
    /// Absent: not given at this depth (asked again with this node as `id`).
    pub children: Option<Vec<NodeView>>,
    /// What its children leave out: its files too small to list and its smallest children.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub rest_bytes: u64,
    pub rest_count: u32,
}

/// The volume `path` is on: the disk mounted at the longest prefix of it.
fn disk_of(path: &Path) -> Option<DiskSpace> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks.list().iter().filter(|d| path.starts_with(d.mount_point())).max_by_key(|d| d.mount_point().as_os_str().len()).map(|d| DiskSpace {
        name: d.name().to_string_lossy().into_owned(),
        mount: d.mount_point().display().to_string(),
        total: d.total_space(),
        available: d.available_space(),
    })
}

/// A folder to scan: it exists and is a folder.
fn checked_root(root: &str) -> Result<PathBuf> {
    let path = expand(root);
    anyhow::ensure!(path.is_absolute(), tr!("{path}: a full path, or one starting with ~", path = root));
    let meta = std::fs::metadata(&path).with_context(|| tr!("{path} cannot be read", path = root))?;
    anyhow::ensure!(meta.is_dir(), tr!("{path} is not a folder", path = root));
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let cfg: DiskUsageConfig = toml::from_str("").unwrap();
        assert!(cfg.enabled && cfg.one_volume && cfg.delta && cfg.cleanup);
        assert_eq!(cfg.roots, ["~", "/"]);
    }

    #[test]
    fn home_is_shortened_and_expanded() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand("~"), home);
        assert_eq!(expand("~/a/b"), home.join("a/b"));
        assert_eq!(expand("/tmp"), PathBuf::from("/tmp"));
        assert_eq!(shorten(&home), "~");
        assert_eq!(shorten(&home.join("x")), "~/x");
        assert_eq!(shorten(Path::new("/opt")), "/opt");
    }
}
