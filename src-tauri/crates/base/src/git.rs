//! `git -C <dir> …`: its standard output, or an error with the command and its stderr.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result, bail, ensure};
use crate::{i18n::tr, tr};

/// Before any git Prism runs, often in an agent's folder, which the agent writes: a `.git`
/// there may point to a config of its own, whose `core.fsmonitor` command `git status` would run
/// — Prism's, outside the agent's sandbox, at each refresh. A commit's signature is not checked
/// either (`gpg.program`). Its filters and textconv drivers cannot be turned off this way: `guard`.
pub const SAFE: [&str; 4] = ["-c", "core.fsmonitor=false", "-c", "log.showSignature=false"];

/// The repositories of the config (`[[repos]] path`), canonical: the only `.git` Prism trusts.
static TRUSTED: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// The repositories whose `.git` Prism trusts, from the config, at each load.
pub fn trust(repos: impl IntoIterator<Item = PathBuf>) {
    let roots = repos.into_iter().filter_map(|r| r.canonicalize().ok()).filter(|r| r.join(".git").is_dir()).collect();
    *TRUSTED.write().unwrap() = roots;
}

/// Before git runs in `dir`: its `.git` is one of the trusted repositories', or points into one
/// of their `.git/worktrees/` (a linked worktree). An agent writes its folder: a `.git` of its own
/// there (a file pointing elsewhere, a repository in place of a worktree) would make Prism's git
/// read its config — filters, textconv, hooks, ssh command: its programs, run by Prism.
fn guard(dir: &Path) -> Result<()> {
    let trusted = TRUSTED.read().unwrap().clone();
    // No repository configured (tests, a first launch): nothing to compare with.
    if trusted.is_empty() {
        return Ok(());
    }
    let Ok(dir) = dir.canonicalize() else { return Ok(()) };
    // No `.git` in it: git looks further up, the first folder with one is checked (a subfolder of
    // a repository the agent made inside its own).
    let Some((dir, meta)) = dir.ancestors().find_map(|d| std::fs::symlink_metadata(d.join(".git")).ok().map(|m| (d.to_path_buf(), m))) else { return Ok(()) };
    let dotgit = dir.join(".git");
    let refused = || tr!("{path}: its .git is not one of the repositories Prism knows: git not run there", path = dir.display());
    if meta.is_symlink() {
        bail!(refused());
    }
    if meta.is_file() {
        let text = std::fs::read_to_string(&dotgit).unwrap_or_default();
        let target = text.trim().strip_prefix("gitdir:").map(str::trim).with_context(refused)?;
        let admin = dir.join(target).canonicalize().with_context(refused)?;
        ensure!(trusted.iter().any(|r| admin.parent() == Some(r.join(".git/worktrees").as_path())), refused());
        return Ok(());
    }
    // A repository of its own: one of the config's, or a folder no trusted repository registered
    // as its worktree (a clone the user opened).
    if trusted.contains(&dir) {
        return Ok(());
    }
    let registered = trusted.iter().any(|r| {
        std::fs::read_dir(r.join(".git/worktrees")).into_iter().flatten().flatten().any(|e| {
            std::fs::read_to_string(e.path().join("gitdir")).ok().and_then(|g| Path::new(g.trim()).parent().and_then(|p| p.canonicalize().ok())).as_deref() == Some(dir.as_path())
        })
    });
    ensure!(!registered, refused());
    Ok(())
}

/// The longest a git call may take: one waiting on a credential prompt or a dead remote would
/// otherwise hold its caller (a refresh loop) for good. Generous, for a push.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// The repository has a remote named `origin`: a local project without one is never fetched.
pub async fn has_origin(dir: impl AsRef<Path>) -> bool {
    run(dir, &["remote"]).await.is_ok_and(|out| out.lines().any(|l| l.trim() == "origin"))
}

pub async fn run(dir: impl AsRef<Path>, args: &[&str]) -> Result<String> {
    run_bytes(dir, args).await.map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// `run`, its output as it is: a blob that may not be text (an image of a diff).
pub async fn run_bytes(dir: impl AsRef<Path>, args: &[&str]) -> Result<Vec<u8>> {
    guard(dir.as_ref())?;
    let run = crate::platform::command("git").args(SAFE).arg("-C").arg(dir.as_ref()).args(args).env("GIT_TERMINAL_PROMPT", "0").stdin(std::process::Stdio::null()).kill_on_drop(true).output();
    let out = tokio::time::timeout(TIMEOUT, run).await.with_context(|| tr!("git {args}: no answer within {secs} s", args = args.join(" "), secs = TIMEOUT.as_secs()))?.context(tr("git not found in PATH"))?;
    checked(out, args)
}

/// `run_bytes`, its output's first `lines` lines only: git stopped (killed) once they are read, or
/// as soon as `wanted` says they no longer are (a search typed further). Whether it was cut.
pub async fn run_head(dir: impl AsRef<Path>, args: &[&str], lines: usize, wanted: impl Fn() -> bool) -> Result<(Vec<u8>, bool)> {
    use tokio::io::AsyncReadExt;
    guard(dir.as_ref())?;
    let mut child = crate::platform::command("git")
        .args(SAFE)
        .arg("-C")
        .arg(dir.as_ref())
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context(tr("git not found in PATH"))?;
    let mut stdout = child.stdout.take().context("git: no output")?;
    let read = async {
        let (mut out, mut chunk, mut seen) = (Vec::new(), vec![0u8; 64 << 10], 0);
        loop {
            let n = stdout.read(&mut chunk).await?;
            if n == 0 {
                return Ok::<_, std::io::Error>((out, false));
            }
            seen += chunk[..n].iter().filter(|&&b| b == b'\n').count();
            out.extend_from_slice(&chunk[..n]);
            if seen >= lines || !wanted() {
                return Ok((out, true));
            }
        }
    };
    let (out, cut) = tokio::time::timeout(TIMEOUT, read).await.with_context(|| tr!("git {args}: no answer within {secs} s", args = args.join(" "), secs = TIMEOUT.as_secs()))??;
    if cut {
        let _ = child.kill().await;
        return Ok((out, true));
    }
    let done = child.wait_with_output().await?;
    checked(std::process::Output { status: done.status, stdout: out, stderr: done.stderr }, args).map(|o| (o, false))
}

/// `run`, blocking: for the code off the async runtime.
pub fn run_blocking(dir: impl AsRef<Path>, args: &[&str]) -> Result<String> {
    guard(dir.as_ref())?;
    let out = crate::platform::std_command("git").args(SAFE).arg("-C").arg(dir.as_ref()).args(args).output().context(tr("git not found in PATH"))?;
    output(out, args)
}

fn output(out: std::process::Output, args: &[&str]) -> Result<String> {
    checked(out, args).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn checked(out: std::process::Output, args: &[&str]) -> Result<Vec<u8>> {
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    /// Git runs in a trusted repository and its linked worktrees; not where an agent put a `.git`
    /// of its own: a file pointing elsewhere, or a repository in place of its worktree.
    #[cfg(unix)]
    #[test]
    fn trusts_only_the_config_s_repositories() {
        let dir = std::env::temp_dir().join(format!("prism-git-trust-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (repo, wt, evil) = (dir.join("repo"), dir.join("wt"), dir.join("evil"));
        let git = |d: &std::path::Path, args: &[&str]| assert!(std::process::Command::new("git").arg("-C").arg(d).args(args).env("GIT_AUTHOR_NAME", "A").env("GIT_AUTHOR_EMAIL", "a@x").env("GIT_COMMITTER_NAME", "A").env("GIT_COMMITTER_EMAIL", "a@x").output().unwrap().status.success());
        for d in [&repo, &evil] {
            std::fs::create_dir_all(d).unwrap();
            git(d, &["init", "-q"]);
            git(d, &["commit", "-q", "--allow-empty", "-m", "x"]);
        }
        git(&repo, &["worktree", "add", "-q", "-b", "fix", wt.to_str().unwrap()]);
        super::trust([repo.clone()]);
        assert!(super::run_blocking(&repo, &["status"]).is_ok() && super::run_blocking(&wt, &["status"]).is_ok());
        assert!(super::run_blocking(&evil, &["status"]).is_ok(), "a clone the user opened, not a worktree");
        // The worktree's .git made to point to a config of the agent's.
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", evil.join(".git").display())).unwrap();
        assert!(super::run_blocking(&wt, &["status"]).is_err(), "a .git pointing elsewhere");
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        assert!(super::run_blocking(wt.join("sub"), &["status"]).is_err(), "nor in a folder of it, git looking up");
        // A repository of its own in place of the worktree.
        std::fs::remove_file(wt.join(".git")).unwrap();
        std::process::Command::new("git").arg("-C").arg(&wt).args(["init", "-q"]).output().unwrap();
        assert!(super::run_blocking(&wt, &["status"]).is_err(), "a repository in a registered worktree's place");
        super::trust([]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A folder whose `.git` points to a config of its own: its `core.fsmonitor` never runs.
    #[cfg(unix)]
    #[test]
    fn runs_no_fsmonitor_a_folder_sets() {
        let dir = std::env::temp_dir().join(format!("prism-git-fsmonitor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (work, fake, marker) = (dir.join("work"), dir.join("fake"), dir.join("ran"));
        std::fs::create_dir_all(&work).unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q", "--separate-git-dir"]).arg(&fake).arg(&work).status().unwrap().success());
        std::fs::write(dir.join("hook.sh"), format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(dir.join("hook.sh"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        assert!(std::process::Command::new("git").arg("-C").arg(&work).args(["config", "core.fsmonitor"]).arg(dir.join("hook.sh")).status().unwrap().success());
        std::process::Command::new("git").arg("-C").arg(&work).args(["status", "--porcelain"]).output().unwrap();
        assert!(marker.exists(), "the trap works with a plain git");
        std::fs::remove_file(&marker).unwrap();
        // Refused outright when repositories are trusted (another test may set them): run, or not.
        let _ = super::run_blocking(&work, &["status", "--porcelain"]);
        assert!(!marker.exists(), "Prism's git does not run it");
        let _ = std::fs::remove_dir_all(dir);
    }
}
