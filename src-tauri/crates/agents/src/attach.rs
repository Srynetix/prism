//! Files attached to an agent's prompt (dropped or pasted images, PDFs, any file): copied where the
//! agent can read them, their path typed in its prompt. A running agent's go to its folder,
//! `.prism/attach/`, which its git ignores (`info/exclude`, never the repository's `.gitignore`);
//! those of an agent still to launch (its worktree does not exist yet) to Prism's working folder,
//! `attach/<day>/`, kept a month.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use prism_base::tr;

/// Larger files are refused: a prompt is no place for them.
const MAX_BYTES: usize = 25 * 1024 * 1024;
/// What one folder of attachments may hold in all: a phone sending photos in a loop fills no disk.
const MAX_FOLDER_BYTES: u64 = 500 * 1024 * 1024;
/// Days the attachments of launches are kept.
const KEEP_DAYS: u64 = 30;

/// Saves `data` as `name` in `dir`, under a name of its own if taken; returns its path.
pub fn save(dir: &Path, name: &str, data: &[u8]) -> Result<PathBuf> {
    if data.len() > MAX_BYTES {
        bail!(tr!("{name} is larger than {mb} MB", name = name, mb = MAX_BYTES / 1024 / 1024));
    }
    // The owner's only: screenshots, PDFs, whatever was dropped.
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir).with_context(|| format!("creating {}", dir.display()))?;
    let held: u64 = std::fs::read_dir(dir)?.flatten().filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum();
    if held + data.len() as u64 > MAX_FOLDER_BYTES {
        bail!(tr!("{dir} holds {mb} MB of attachments already: remove some first", dir = dir.display(), mb = held / 1024 / 1024));
    }
    let name = safe_name(name);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    // Created, never opened: a name taken, even by a link (dangling or not), gives way to the next.
    let mut path = dir.join(&name);
    let mut n = 2;
    let mut file = loop {
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        match open.open(&path) {
            Ok(f) => break f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && n < 1000 => {
                path = dir.join(format!("{stem}-{n}{ext}"));
                n += 1;
            }
            Err(e) => return Err(e).with_context(|| format!("writing {}", path.display())),
        }
    };
    std::io::Write::write_all(&mut file, data).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// A file name without a path, safe in a prompt: letters, digits, `.`, `-`, `_`.
fn safe_name(name: &str) -> String {
    let base = Path::new(name).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let clean: String = base.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '-' }).collect();
    let clean = clean.trim_matches(|c| c == '-' || c == '.').to_string();
    if clean.is_empty() { "attachment".into() } else { clean }
}

/// Into a running agent's folder, `.prism/attach/`, ignored by its git; the path relative to it.
pub fn into_folder(folder: &Path, name: &str, data: &[u8]) -> Result<String> {
    // The folder is the agent's (a branch checked out, a repository's links): `.prism` or
    // `attach` committed as a link would send the file elsewhere.
    for dir in [folder.join(".prism"), folder.join(".prism/attach")] {
        if dir.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("{} is a link: no attachment there", dir.display());
        }
    }
    let path = save(&folder.join(".prism/attach"), name, data)?;
    exclude(folder);
    Ok(path.strip_prefix(folder).unwrap_or(&path).display().to_string())
}

/// For an agent to launch: Prism's `attach/<day>/`; the absolute path. Older days are removed.
pub fn for_launch(work: &Path, name: &str, data: &[u8]) -> Result<String> {
    let root = work.join("attach");
    prune(&root);
    let path = save(&root.join(chrono::Local::now().format("%Y-%m-%d").to_string()), name, data)?;
    Ok(path.display().to_string())
}

/// Larger images are not shown in its Chat: a picture is no reason to send megabytes to a phone.
const MAX_SHOWN: u64 = 8 << 20;

/// An image attached to a prompt, as a `data:` URL for its Chat: `path` as typed in the prompt
/// (relative to the agent's `folder`, or absolute), only from where attachments go (its
/// `.prism/attach/`, Prism's `attach/`), whatever links lead elsewhere.
pub fn image(folder: &Path, work: &Path, path: &str) -> Result<String> {
    let kind = match Path::new(path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => bail!(tr!("{path}: not an image", path = path)),
    };
    let file = folder.join(path).canonicalize().with_context(|| tr!("{path}: not found", path = path))?;
    // `.prism` or `attach` committed as a link would make anywhere it leads an attachment.
    let linked = |d: &Path| d.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink());
    let agent = (!linked(&folder.join(".prism")) && !linked(&folder.join(".prism/attach"))).then(|| folder.join(".prism/attach"));
    let allowed = agent.into_iter().chain([work.join("attach")]).filter(|d| !linked(d)).filter_map(|d| d.canonicalize().ok()).any(|d| file.starts_with(d));
    if !allowed || !file.is_file() {
        bail!(tr!("{path}: not an attachment", path = path));
    }
    if file.metadata().map(|m| m.len()).unwrap_or(0) > MAX_SHOWN {
        bail!(tr!("{path}: too large to show", path = path));
    }
    use base64::Engine;
    Ok(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(&file)?)))
}

/// Removes the launches' attachments older than a month (also at Prism's start: none may come).
pub fn prune(root: &Path) {
    let Ok(days) = std::fs::read_dir(root) else { return };
    let limit = std::time::Duration::from_secs(KEEP_DAYS * 86_400);
    for d in days.flatten() {
        let old = d.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > limit);
        if old {
            let _ = std::fs::remove_dir_all(d.path());
        }
    }
}

/// `.prism/` in the git's own exclude file (shared by its worktrees), once (a git run each time
/// was 11 ms an attachment).
fn exclude(folder: &Path) {
    static DONE: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    if DONE.lock().unwrap().iter().any(|d| d == folder) {
        return;
    }
    DONE.lock().unwrap().push(folder.to_path_buf());
    let Ok(out) = crate::git::run_blocking(folder, &["rev-parse", "--git-common-dir"]) else { return };
    let common = PathBuf::from(out.trim());
    let file = if common.is_absolute() { common } else { folder.join(common) }.join("info/exclude");
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    if text.lines().any(|l| l.trim() == ".prism/") {
        return;
    }
    let _ = std::fs::create_dir_all(file.parent().unwrap_or(folder));
    let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
    let _ = std::fs::write(&file, format!("{text}{sep}# Prism: files attached to agents' prompts\n.prism/\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A folder whose `.prism` is a link, or a name taken by a link, sends nothing elsewhere.
    #[cfg(unix)]
    #[test]
    fn follows_no_link() {
        let dir = std::env::temp_dir().join(format!("prism-attach-links-{}", std::process::id()));
        let (repo, away) = (dir.join("repo"), dir.join("away"));
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&away).unwrap();
        std::os::unix::fs::symlink(&away, repo.join(".prism")).unwrap();
        assert!(into_folder(&repo, "a.png", b"x").is_err(), "a linked .prism");
        let inbox = dir.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::os::unix::fs::symlink(away.join("target.png"), inbox.join("a.png")).unwrap();
        let saved = save(&inbox, "a.png", b"x").unwrap();
        assert_eq!(saved, inbox.join("a-2.png"), "a dangling link's name is taken");
        assert!(!away.join("target.png").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn shows_only_attached_images() {
        let dir = std::env::temp_dir().join(format!("prism-attach-image-{}", std::process::id()));
        let (repo, work) = (dir.join("repo"), dir.join("work"));
        std::fs::create_dir_all(repo.join(".prism/attach")).unwrap();
        std::fs::create_dir_all(work.join("attach/2026-10-20")).unwrap();
        std::fs::write(repo.join(".prism/attach/a.png"), b"x").unwrap();
        std::fs::write(repo.join("b.png"), b"x").unwrap();
        std::fs::write(repo.join(".prism/attach/notes.txt"), b"x").unwrap();
        let launched = work.join("attach/2026-10-20/c.jpg");
        std::fs::write(&launched, b"x").unwrap();
        assert!(image(&repo, &work, ".prism/attach/a.png").unwrap().starts_with("data:image/png;base64,"));
        assert!(image(&repo, &work, launched.to_str().unwrap()).unwrap().starts_with("data:image/jpeg;base64,"), "a launch's, absolute");
        assert!(image(&repo, &work, "b.png").is_err(), "elsewhere in the folder");
        assert!(image(&repo, &work, ".prism/attach/../../b.png").is_err(), "climbing out");
        assert!(image(&repo, &work, ".prism/attach/notes.txt").is_err(), "not an image");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn names_are_safe_and_unique() {
        assert_eq!(safe_name("../../etc/pass wd.png"), "pass-wd.png");
        assert_eq!(safe_name("Capture d’écran 2026.png"), "Capture-d--cran-2026.png");
        let dir = std::env::temp_dir().join(format!("prism-attach-{}", std::process::id()));
        let a = save(&dir, "shot.png", b"1").unwrap();
        let b = save(&dir, "shot.png", b"2").unwrap();
        assert_eq!(b.file_name().unwrap(), "shot-2.png");
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_folder_s_attachments_are_ignored_by_git() {
        let dir = std::env::temp_dir().join(format!("prism-attach-git-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        if crate::platform::std_command("git").arg("-C").arg(&dir).arg("init").arg("-q").status().is_err() {
            return;
        }
        let rel = into_folder(&dir, "cart.png", b"png").unwrap();
        assert_eq!(rel, ".prism/attach/cart.png");
        into_folder(&dir, "cart.png", b"png").unwrap();
        let exclude = std::fs::read_to_string(dir.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches(".prism/").count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
