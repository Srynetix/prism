//! `prism` on the PATH (Settings › General › Command line), as VS Code puts `code` there: a short
//! script in `~/.local/bin` (or `/usr/local/bin`) that runs this Prism's binary. A script, not a
//! link: the binary then runs from its own path (inside `Prism.app` on macOS), where it finds its
//! resources. Linux packages put `prism` in `/usr/bin` themselves.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

/// The line that marks the script as Prism's.
const MARK: &str = "# The prism command, installed by Prism (Settings › General › Command line).";

/// Where `prism` stands for this Prism.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "CliCommand"))]
pub struct Status {
    /// `installed`: `prism` in the PATH runs this Prism; `elsewhere`: another binary (an older
    /// install, a build); `absent`: none.
    pub state: String,
    /// The `prism` the PATH finds, if any.
    pub found: Option<String>,
    /// What it runs, when it is Prism's script.
    pub runs: Option<String>,
    /// This Prism's binary.
    pub binary: String,
    /// Where Install writes it.
    pub install_at: String,
    /// That folder is in the PATH (the login shell's): otherwise `prism` is not found there.
    pub dir_in_path: bool,
    /// The script Install wrote is there (Remove takes it out).
    pub ours: bool,
}

fn script(binary: &Path) -> String {
    let quoted = binary.display().to_string().replace('\'', r"'\''");
    format!("#!/bin/sh\n{MARK}\nexec '{quoted}' \"$@\"\n")
}

/// The binary a script of ours runs.
fn runs_of(text: &str) -> Option<String> {
    if !text.contains(MARK) {
        return None;
    }
    let line = text.lines().find_map(|l| l.strip_prefix("exec '"))?;
    let end = line.rfind("' \"$@\"")?;
    Some(line[..end].replace(r"'\''", "'"))
}

/// The script of ours at `path`: what it runs. Only its start is read (the PATH's `prism` may be
/// a binary of tens of megabytes).
fn ours_at(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut head = Vec::with_capacity(1024);
    std::fs::File::open(path).ok()?.take(1024).read_to_end(&mut head).ok()?;
    runs_of(&String::from_utf8_lossy(&head))
}

fn in_path(dir: &Path) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir))
}

fn writable(dir: &Path) -> bool {
    std::fs::metadata(dir).is_ok_and(|m| !m.permissions().readonly()) && tempfile_in(dir)
}

/// A file can be made in `dir` (permissions alone say little: the owner's matter).
fn tempfile_in(dir: &Path) -> bool {
    let probe = dir.join(format!(".prism-probe-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// The folders Install may write in, by preference.
fn candidates(home: &Path) -> [PathBuf; 2] {
    [home.join(".local/bin"), PathBuf::from("/usr/local/bin")]
}

/// Where Install writes: where a script of ours already is; otherwise `~/.local/bin` when in the
/// PATH; otherwise `/usr/local/bin` when in the PATH and writable (`probe`: a file tried there,
/// only when installing; otherwise its permissions); otherwise `~/.local/bin` all the same (the
/// PATH to complete).
fn install_dir(home: &Path, probe: bool) -> PathBuf {
    let [local, usr] = candidates(home);
    if let Some(dir) = [&local, &usr].into_iter().find(|d| ours_at(&d.join("prism")).is_some()) {
        return dir.clone();
    }
    if in_path(&local) {
        return local;
    }
    let usable = if probe { writable(&usr) } else { std::fs::metadata(&usr).is_ok_and(|m| !m.permissions().readonly()) };
    if in_path(&usr) && usable {
        return usr;
    }
    local
}

/// Two paths are the same file (links resolved).
fn same(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub fn status() -> Result<Status> {
    let home = dirs::home_dir().context("no home folder")?;
    let binary = crate::profile::executable()?;
    let dir = install_dir(&home, false);
    let at = dir.join("prism");
    let ours = ours_at(&at).is_some();
    let found = crate::detect::which("prism");
    let runs = found.as_deref().and_then(ours_at);
    let state = match (&found, &runs) {
        (None, _) => "absent",
        (Some(_), Some(r)) if same(Path::new(r), &binary) => "installed",
        (Some(f), None) if same(f, &binary) => "installed",
        _ => "elsewhere",
    };
    Ok(Status {
        state: state.into(),
        found: found.map(|f| f.display().to_string()),
        runs,
        binary: binary.display().to_string(),
        install_at: at.display().to_string(),
        dir_in_path: in_path(&dir),
        ours,
    })
}

/// Writes the script (replacing one of ours, never another file).
pub fn install() -> Result<Status> {
    let home = dirs::home_dir().context("no home folder")?;
    let binary = crate::profile::executable()?;
    let dir = install_dir(&home, true);
    let at = dir.join("prism");
    write(&dir, &at, &binary)?;
    status()
}

fn write(dir: &Path, at: &Path, binary: &Path) -> Result<()> {
    if cfg!(windows) {
        bail!("not on Windows yet");
    }
    if let Ok(meta) = std::fs::symlink_metadata(at)
        && (ours_at(at).is_none() || meta.file_type().is_symlink())
    {
        bail!("{} exists and is not Prism's script: remove it first", at.display());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let part = at.with_extension("part");
    std::fs::write(&part, script(binary)).with_context(|| format!("writing {}", at.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&part, at).with_context(|| format!("writing {}", at.display()))?;
    Ok(())
}

/// Removes the scripts of ours, wherever Install may have written them.
pub fn remove() -> Result<Status> {
    let home = dirs::home_dir().context("no home folder")?;
    for dir in candidates(&home) {
        let at = dir.join("prism");
        if ours_at(&at).is_some() && !std::fs::symlink_metadata(&at).is_ok_and(|m| m.file_type().is_symlink()) {
            std::fs::remove_file(&at).with_context(|| format!("removing {}", at.display()))?;
        }
    }
    status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_says_what_it_runs() {
        let bin = Path::new("/Applications/Prism's.app/Contents/MacOS/prism");
        let s = script(bin);
        assert!(s.starts_with("#!/bin/sh\n"));
        assert_eq!(runs_of(&s).as_deref(), Some("/Applications/Prism's.app/Contents/MacOS/prism"));
        assert_eq!(runs_of("#!/bin/sh\nexec '/x' \"$@\"\n"), None, "not ours");
    }

    #[cfg(unix)]
    #[test]
    fn install_replaces_its_own_script_only() {
        let dir = std::env::temp_dir().join(format!("prism-cli-{}", std::process::id()));
        let at = dir.join("prism");
        write(&dir, &at, Path::new("/one/prism")).unwrap();
        write(&dir, &at, Path::new("/two/prism")).unwrap();
        assert_eq!(runs_of(&std::fs::read_to_string(&at).unwrap()).as_deref(), Some("/two/prism"));
        // A script run for real: the arguments reach the binary as given.
        write(&dir, &at, Path::new("/bin/echo")).unwrap();
        let out = std::process::Command::new(&at).args(["a b", "c"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "a b c\n");
        std::fs::write(&at, "#!/bin/sh\necho mine\n").unwrap();
        assert!(write(&dir, &at, Path::new("/one/prism")).is_err(), "someone else's file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
