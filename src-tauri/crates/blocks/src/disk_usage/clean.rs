//! Freeing space: a file or a folder to the Trash, and the reclaim groups (rebuildable folders
//! removed, caches emptied, the tools' own cleanups). Only paths of the scan, inside its root,
//! never through a symbolic link.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use prism_base::tr;

use super::{kinds::Reclaim, scan};

/// Each path of a reclaim group freed, one by one (one failing leaves the others freed): `None`
/// gone, else what is left in it (size, files).
pub async fn clean(kind: Reclaim, root: &Path, paths: Vec<PathBuf>) -> Result<Vec<Result<Option<(u64, u64)>>>> {
    for p in &paths {
        anyhow::ensure!(p.starts_with(root) && p != root, "{} is outside the scan", p.display());
    }
    match kind {
        Reclaim::Simulators => run("xcrun", &["simctl", "delete", "unavailable"]).await?,
        Reclaim::Docker => run("docker", &["system", "prune", "-a", "-f"]).await?,
        _ => {}
    }
    Ok(tokio::task::spawn_blocking(move || {
        paths
            .iter()
            .map(|p| match kind {
                Reclaim::Simulators | Reclaim::Docker => Ok(Some(scan::measure(p))),
                k if k.removes_folder() => remove(p).map(|()| None),
                _ => empty(p).map(|()| Some(scan::measure(p))),
            })
            .collect()
    })
    .await?)
}

/// A tool's cleanup, its error said.
async fn run(program: &str, args: &[&str]) -> Result<()> {
    let out = tokio::time::timeout(Duration::from_secs(600), prism_base::platform::command(program).args(args).kill_on_drop(true).output())
        .await
        .with_context(|| tr!("{program} did not finish within 10 minutes", program = program))?
        .with_context(|| tr!("{program} not found", program = program))?;
    if !out.status.success() {
        bail!("{program} {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("failed"));
    }
    Ok(())
}

/// A real folder (not a link to one).
fn folder(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).with_context(|| tr!("{path} is no longer there", path = path.display()))?;
    anyhow::ensure!(meta.is_dir(), tr!("{path} is not a folder", path = path.display()));
    Ok(())
}

fn remove(path: &Path) -> Result<()> {
    folder(path)?;
    fs::remove_dir_all(path).with_context(|| tr!("could not remove {path}", path = path.display()))
}

/// Everything in `path` removed, `path` kept; what cannot be (in use, protected) is left.
fn empty(path: &Path) -> Result<()> {
    folder(path)?;
    let mut failed = 0;
    for entry in fs::read_dir(path)?.flatten() {
        let p = entry.path();
        let gone = match entry.file_type() {
            Ok(t) if t.is_dir() => fs::remove_dir_all(&p),
            _ => fs::remove_file(&p),
        };
        if gone.is_err() {
            failed += 1;
        }
    }
    if failed > 0 {
        tracing::info!("disk usage: {failed} entries of {} left in place", path.display());
    }
    Ok(())
}

/// `path` to the Trash: macOS's (`~/.Trash`, on the home's volume), the freedesktop one
/// (`~/.local/share/Trash`, its `.trashinfo` written so that the file manager can put it back), or
/// Windows' Recycle Bin.
pub fn trash(path: &Path) -> Result<()> {
    fs::symlink_metadata(path).with_context(|| tr!("{path} is no longer there", path = path.display()))?;
    // Windows: the shell's Recycle Bin, on the path's own disk; Explorer can put it back.
    #[cfg(windows)]
    return prism_sys::windows::recycle(&std::path::absolute(path)?).map_err(|e| match e.kind() {
        std::io::ErrorKind::Unsupported => anyhow::anyhow!(tr!("{path} is on a disk without a Recycle Bin: delete it from Explorer", path = path.display())),
        _ => anyhow::Error::from(e).context(tr!("could not move {path} to the Trash", path = path.display())),
    });
    #[cfg(not(windows))]
    to_folder(path)
}

/// `path` moved into the home folder's Trash folder (macOS, freedesktop).
#[cfg(not(windows))]
fn to_folder(path: &Path) -> Result<()> {
    let home = dirs::home_dir().context("no home folder")?;
    let name = path.file_name().context("no name")?.to_os_string();
    // The same Trash as the block's line measures and empties.
    let bin = super::trash::home_trash(&home);
    let (files, info) = if bin.freedesktop { (bin.dir.join("files"), Some(bin.dir.join("info"))) } else { (bin.dir.clone(), None::<PathBuf>) };
    fs::create_dir_all(&files)?;
    // A free name in the Trash: `x`, `x 2`, `x 3`…
    let mut target = files.join(&name);
    let mut n = 2;
    while fs::symlink_metadata(&target).is_ok() || info.as_ref().is_some_and(|i| i.join(format!("{}.trashinfo", target.file_name().unwrap_or_default().to_string_lossy())).exists()) {
        target = files.join(format!("{} {n}", name.to_string_lossy()));
        n += 1;
    }
    let mut record = None;
    if let Some(info) = &info {
        fs::create_dir_all(info)?;
        let deleted = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
        let entry = info.join(format!("{}.trashinfo", target.file_name().unwrap_or_default().to_string_lossy()));
        fs::write(&entry, format!("[Trash Info]\nPath={}\nDeletionDate={deleted}\n", percent_encode(&path.to_string_lossy())))?;
        record = Some(entry);
    }
    fs::rename(path, &target)
        .inspect_err(|_| {
            // Not moved: no record of it in the Trash either.
            if let Some(r) = &record {
                let _ = fs::remove_file(r);
            }
        })
        .map_err(|e| match e.raw_os_error() {
            // Another volume than the home's: its own Trash is the file manager's business.
            Some(18) => anyhow::anyhow!(tr!("{path} is on another disk than the Trash: delete it from the file manager", path = path.display())),
            _ => anyhow::Error::from(e).context(tr!("could not move {path} to the Trash", path = path.display())),
        })
}

/// The freedesktop `Path=`: a URI path, `/` kept.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
fn percent_encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freedesktop_paths_are_encoded() {
        assert_eq!(percent_encode("/home/me/a b/é"), "/home/me/a%20b/%C3%A9");
    }

    #[tokio::test]
    async fn cleans_only_inside_the_root() {
        let dir = std::env::temp_dir().join(format!("prism-du-clean-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("p/node_modules/x")).unwrap();
        fs::create_dir_all(dir.join("Caches/app")).unwrap();
        fs::write(dir.join("Caches/app/c"), "x").unwrap();
        assert!(clean(Reclaim::NodeModules, &dir.join("p/node_modules/x"), vec![dir.join("p/node_modules")]).await.is_err());
        let gone = clean(Reclaim::NodeModules, &dir, vec![dir.join("missing/node_modules"), dir.join("p/node_modules")]).await.unwrap();
        // The first one failing leaves the second freed.
        assert!(gone[0].is_err() && matches!(gone[1], Ok(None)));
        assert!(!dir.join("p/node_modules").exists());
        let left = clean(Reclaim::Caches, &dir, vec![dir.join("Caches")]).await.unwrap();
        assert!(dir.join("Caches").exists() && !dir.join("Caches/app").exists());
        assert_eq!(left[0].as_ref().ok().and_then(|l| l.map(|l| l.1)), Some(0));
        fs::remove_dir_all(&dir).unwrap();
    }
}
