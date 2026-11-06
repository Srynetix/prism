//! The Trash, apart from any scan: every one of this account's (the home folder's, and each other
//! disk's), measured and emptied directly, without the Finder nor `gio`. macOS: `~/.Trash` and
//! `/Volumes/<disk>/.Trashes/<uid>`. Linux (freedesktop): `~/.local/share/Trash` and each mount's
//! `.Trash-<uid>` or `.Trash/<uid>`, their `files/` with their `info/` records.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::Serialize;

/// The Trash as the block shows it: all the disks' together, and each one.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskTrash"))]
pub struct TrashState {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
    /// The items in it (what the file manager lists: a folder is one).
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub items: u64,
    pub bins: Vec<BinInfo>,
    /// The home folder's Trash could not be read: macOS wants Full Disk Access for it.
    pub denied: bool,
    /// Being emptied now.
    pub emptying: bool,
}

/// One disk's Trash.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DiskTrashBin"))]
pub struct BinInfo {
    /// The disk's name (`Macintosh HD`, `Backup`, `/mnt/data`).
    pub disk: String,
    /// The Trash's folder, `~` for the home folder.
    pub path: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub bytes: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub items: u64,
}

/// A Trash folder: its items in `files` (macOS: the folder itself), their records in `info`
/// (freedesktop).
#[derive(Debug, Clone, PartialEq)]
pub struct Bin {
    pub disk: String,
    pub dir: PathBuf,
    pub freedesktop: bool,
}

impl Bin {
    fn files(&self) -> PathBuf {
        if self.freedesktop { self.dir.join("files") } else { self.dir.clone() }
    }
}

/// What emptying did.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Emptied {
    /// Bytes freed (measured before and after).
    pub freed: u64,
    /// Items left in place: locked, in use, not ours to remove.
    pub left: u64,
}

/// The account's Trash folders that exist: the home folder's first.
pub fn bins() -> Vec<Bin> {
    let Some(home) = dirs::home_dir() else { return Vec::new() };
    let uid = owner(&home);
    let mut out = Vec::new();
    #[cfg(target_os = "macos")]
    {
        let disk = super::disk_of(&home).map(|d| d.name).filter(|n| !n.is_empty()).unwrap_or_else(|| "~".into());
        out.push(Bin { disk, dir: home.join(".Trash"), freedesktop: false });
        if let (Some(uid), Ok(volumes)) = (uid, fs::read_dir("/Volumes")) {
            for v in volumes.flatten() {
                // The startup disk is there too, as a link to `/`.
                if v.file_type().is_ok_and(|t| t.is_symlink()) {
                    continue;
                }
                let dir = v.path().join(".Trashes").join(uid.to_string());
                if dir.is_dir() {
                    out.push(Bin { disk: v.file_name().to_string_lossy().into_owned(), dir, freedesktop: false });
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let base = dirs::data_dir().unwrap_or_else(|| home.join(".local/share")).join("Trash");
        out.push(Bin { disk: "~".into(), dir: base, freedesktop: true });
        if let Some(uid) = uid {
            let disks = sysinfo::Disks::new_with_refreshed_list();
            for d in disks.list() {
                let mount = d.mount_point();
                for dir in [mount.join(format!(".Trash-{uid}")), mount.join(".Trash").join(uid.to_string())] {
                    if dir.join("files").is_dir() && !out.iter().any(|b: &Bin| b.dir == dir) {
                        out.push(Bin { disk: mount.display().to_string(), dir, freedesktop: true });
                    }
                }
            }
        }
    }
    out
}

/// The account's uid, as its home folder's owner.
fn owner(home: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(home).ok().map(|m| m.uid())
    }
    #[cfg(not(unix))]
    {
        let _ = home;
        None
    }
}

/// The items of a Trash folder (`.DS_Store` aside); `Err` when it cannot be read.
fn items(dir: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(entries) => Ok(entries.flatten().filter(|e| e.file_name() != ".DS_Store").collect()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Every Trash measured (a walk of the Trash only). Blocks: call it off the runtime.
pub fn measure(bins: &[Bin]) -> TrashState {
    let mut state = TrashState::default();
    for (i, b) in bins.iter().enumerate() {
        let found = match items(&b.files()) {
            Ok(found) => found,
            Err(e) => {
                if i == 0 {
                    state.denied = true;
                }
                tracing::info!("disk usage: {} not read: {e}", b.dir.display());
                continue;
            }
        };
        let (bytes, _) = if b.dir.exists() { super::scan::measure(&b.dir) } else { (0, 0) };
        let info = BinInfo { disk: b.disk.clone(), path: super::shorten(&b.dir), bytes, items: found.len() as u64 };
        state.bytes += info.bytes;
        state.items += info.items;
        state.bins.push(info);
    }
    state
}

/// Every Trash emptied: each item removed (a freedesktop item with its record), what cannot be
/// left in place and counted. Blocks: call it off the runtime.
pub fn empty(bins: &[Bin]) -> Result<Emptied> {
    let before = measure(bins);
    anyhow::ensure!(!(before.denied && before.bins.is_empty()), prism_base::i18n::tr("The Trash cannot be read: give Prism Full Disk Access (System Settings › Privacy & Security)"));
    let mut left = 0;
    for b in bins {
        let Ok(found) = items(&b.files()) else { continue };
        for entry in found {
            let path = entry.path();
            let gone = match entry.file_type() {
                Ok(t) if t.is_dir() => fs::remove_dir_all(&path),
                _ => fs::remove_file(&path),
            };
            match gone {
                Ok(()) if b.freedesktop => {
                    let _ = fs::remove_file(b.dir.join("info").join(format!("{}.trashinfo", entry.file_name().to_string_lossy())));
                }
                Ok(()) => {}
                Err(e) if e.kind() == ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::info!("disk usage: {} left in the Trash: {e}", path.display());
                    left += 1;
                }
            }
        }
        if b.freedesktop {
            // What the spec adds: an expunge in progress, the folders' sizes cache, records whose file is gone.
            let _ = fs::remove_dir_all(b.dir.join("expunged"));
            let _ = fs::remove_file(b.dir.join("directorysizes"));
            if let Ok(records) = fs::read_dir(b.dir.join("info")) {
                for r in records.flatten() {
                    let name = r.file_name().to_string_lossy().into_owned();
                    if name.strip_suffix(".trashinfo").is_some_and(|f| fs::symlink_metadata(b.files().join(f)).is_err()) {
                        let _ = fs::remove_file(r.path());
                    }
                }
            }
        }
    }
    let after = measure(bins);
    Ok(Emptied { freed: before.bytes.saturating_sub(after.bytes), left })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prism-trash-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_freedesktop_trash_is_measured_then_emptied() {
        let dir = temp("fd");
        fs::create_dir_all(dir.join("files/folder/sub")).unwrap();
        fs::create_dir_all(dir.join("info")).unwrap();
        fs::create_dir_all(dir.join("expunged/x")).unwrap();
        fs::write(dir.join("files/folder/sub/a"), vec![1u8; 200_000]).unwrap();
        fs::write(dir.join("files/b.txt"), vec![1u8; 100_000]).unwrap();
        fs::write(dir.join("info/folder.trashinfo"), "[Trash Info]\n").unwrap();
        fs::write(dir.join("info/b.txt.trashinfo"), "[Trash Info]\n").unwrap();
        fs::write(dir.join("info/gone.trashinfo"), "[Trash Info]\n").unwrap();
        fs::write(dir.join("directorysizes"), "").unwrap();
        let bins = [Bin { disk: "test".into(), dir: dir.clone(), freedesktop: true }];
        let m = measure(&bins);
        assert_eq!(m.items, 2);
        assert!(m.bytes >= 300_000, "{}", m.bytes);
        let done = empty(&bins).unwrap();
        assert_eq!(done.left, 0);
        assert!(done.freed >= 300_000);
        assert_eq!(fs::read_dir(dir.join("files")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(dir.join("info")).unwrap().count(), 0, "every record gone");
        assert!(!dir.join("expunged").exists() && !dir.join("directorysizes").exists());
        assert_eq!(measure(&bins).items, 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn what_cannot_be_removed_is_left_and_counted() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("mac");
        fs::create_dir_all(dir.join("locked")).unwrap();
        fs::write(dir.join("locked/f"), "x").unwrap();
        fs::write(dir.join("free"), "x").unwrap();
        fs::write(dir.join(".DS_Store"), "x").unwrap();
        // A folder whose content cannot be removed (no write on it).
        fs::set_permissions(dir.join("locked"), fs::Permissions::from_mode(0o500)).unwrap();
        let bins = [Bin { disk: "test".into(), dir: dir.clone(), freedesktop: false }];
        assert_eq!(measure(&bins).items, 2, ".DS_Store not counted");
        let done = empty(&bins).unwrap();
        fs::set_permissions(dir.join("locked"), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(done.left, 1);
        assert!(!dir.join("free").exists() && dir.join("locked/f").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
