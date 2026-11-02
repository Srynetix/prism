//! Files Prism writes whole: replaced at once, the owner's only.

use std::io::Write;
use std::path::Path;

/// Writes `bytes` to `path` through a file next to it, created 0600, then renamed over it: a
/// crash mid-write leaves the former file, never half of one (a config cut short), and nothing
/// is readable by others, even a moment.
pub fn write_private(path: &Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    // A link (a config kept with one's dotfiles) stays one: its target is replaced.
    let resolved = path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()).then(|| path.canonicalize().ok()).flatten();
    let path = resolved.as_deref().unwrap_or(path);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
    let written = open.open(&tmp).and_then(|mut f| {
        f.write_all(bytes.as_ref())?;
        f.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn replaces_whole_and_private() {
        let dir = std::env::temp_dir().join(format!("prism-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "old").unwrap();
        super::write_private(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions()) & 0o777, 0o600);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left");
        #[cfg(unix)]
        {
            let link = dir.join("link.toml");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            super::write_private(&link, "via the link").unwrap();
            assert!(link.symlink_metadata().unwrap().file_type().is_symlink(), "the link stays a link");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "via the link");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
