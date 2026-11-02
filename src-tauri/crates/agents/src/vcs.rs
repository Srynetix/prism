//! An agent's folder under git, without the agent (its Git bar, on a phone above all): where it
//! stands, its changes committed, its branch pushed, brought up to date with its base. Never a
//! commit or a push on a protected branch; never a hook (a program the branch or the agent may
//! have written, `chain::open_mr` says why).

use std::path::Path;

use anyhow::{bail, Result};
use prism_base::{i18n::tr, tr};
use serde::Serialize;

use prism_core::conventions::Conventions;

use crate::state::Snapshot;

/// Where the folder stands.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GitState {
    /// Its branch; absent on a detached HEAD.
    pub branch: Option<String>,
    /// `main`, a release branch: nothing committed nor pushed from here.
    pub protected: bool,
    /// Files changed, staged or not, untracked included.
    pub changes: u32,
    /// What it is brought up to date with (`origin/develop`), and how far from it.
    pub base: String,
    pub ahead: u32,
    pub behind: u32,
    /// Its branch on origin, if pushed; the commits not pushed yet, and those of origin not here.
    pub upstream: Option<String>,
    pub unpushed: u32,
    pub unpulled: u32,
}

/// `git` without hooks, its output trimmed.
async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec!["-c", "core.hooksPath=/dev/null"];
    all.extend_from_slice(args);
    Ok(crate::git::run(dir, &all).await?.trim().to_string())
}

pub fn protected(branch: &str, c: &Conventions) -> bool {
    branch == c.main_branch || (!c.release_prefix.is_empty() && branch.starts_with(&c.release_prefix))
}

/// The branch it is brought up to date with: its worktree's base, otherwise the main branch.
fn base_of(dir: &Path, snap: &Snapshot) -> String {
    let path = dir.display().to_string();
    let base = snap.worktrees.iter().find(|w| w.path == path && !w.is_main).map(|w| w.base.clone()).filter(|b| !b.is_empty());
    let base = base.unwrap_or_else(|| snap.conventions.main_branch.clone());
    if base.starts_with("origin/") { base } else { format!("origin/{base}") }
}

fn counts(text: &str) -> (u32, u32) {
    let mut n = text.split_whitespace().map(|x| x.parse().unwrap_or(0));
    (n.next().unwrap_or(0), n.next().unwrap_or(0))
}

pub async fn state(dir: &Path, snap: &Snapshot) -> Result<GitState> {
    let branch = Some(git(dir, &["branch", "--show-current"]).await?).filter(|b| !b.is_empty());
    let changes = git(dir, &["status", "--porcelain"]).await?.lines().count() as u32;
    let base = base_of(dir, snap);
    let (behind, ahead) = git(dir, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")]).await.map(|t| counts(&t)).unwrap_or_default();
    let upstream = git(dir, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).await.ok().filter(|u| !u.is_empty());
    let (unpulled, unpushed) = match &upstream {
        Some(_) => git(dir, &["rev-list", "--left-right", "--count", "@{u}...HEAD"]).await.map(|t| counts(&t)).unwrap_or_default(),
        None => (0, ahead),
    };
    let protected = branch.as_deref().is_some_and(|b| protected(b, &snap.conventions));
    Ok(GitState { branch, protected, changes, base, ahead, behind, upstream, unpushed, unpulled })
}

/// A branch something is committed or pushed on: its own, not a protected one.
async fn own_branch(dir: &Path, c: &Conventions, what: &str) -> Result<String> {
    let branch = git(dir, &["branch", "--show-current"]).await?;
    if branch.is_empty() {
        bail!(tr!("the folder is on no branch: nothing is {what} from a detached HEAD", what = what));
    }
    if protected(&branch, c) {
        bail!(tr!("{branch} is a protected branch: nothing is {what} on it from here", branch = branch, what = what));
    }
    Ok(branch)
}

/// Every change committed (`git add -A`), as `message`; the commit's short hash.
pub async fn commit(dir: &Path, message: &str, snap: &Snapshot) -> Result<String> {
    let message = message.trim();
    if message.is_empty() {
        bail!(tr("a commit needs a message"));
    }
    own_branch(dir, &snap.conventions, tr("committed")).await?;
    if git(dir, &["status", "--porcelain"]).await?.is_empty() {
        bail!(tr("nothing to commit"));
    }
    git(dir, &["add", "-A"]).await?;
    git(dir, &["commit", "--no-verify", "-q", "-m", message]).await?;
    git(dir, &["rev-parse", "--short", "HEAD"]).await
}

/// Its branch pushed to origin (tracked from then on); `force`: over a branch rewritten here (a
/// rebase), as long as origin has nothing this folder has not seen.
pub async fn push(dir: &Path, force: bool, snap: &Snapshot) -> Result<()> {
    let branch = own_branch(dir, &snap.conventions, tr("pushed")).await?;
    let mut args = vec!["push", "--no-verify", "-u", "origin", branch.as_str()];
    if force {
        args.insert(1, "--force-with-lease");
    }
    match git(dir, &args).await {
        Ok(_) => Ok(()),
        Err(e) if !force && format!("{e:#}").contains("rejected") => bail!("origin's {branch} has commits this folder lacks (after a rebase: Force push)"),
        Err(e) => Err(e),
    }
}

/// Brought up to date with its base: rebased onto it (its own branch), or fast-forwarded (a
/// protected branch: the main checkout). Only with nothing uncommitted; a conflict leaves it as it
/// was, for the agent to resolve.
pub async fn sync(dir: &Path, snap: &Snapshot) -> Result<String> {
    if !git(dir, &["status", "--porcelain"]).await?.is_empty() {
        bail!(tr("uncommitted changes: commit them first"));
    }
    let branch = git(dir, &["branch", "--show-current"]).await?;
    if branch.is_empty() {
        bail!(tr("the folder is on no branch"));
    }
    if protected(&branch, &snap.conventions) {
        git(dir, &["pull", "--ff-only", "-q"]).await?;
        return Ok(tr!("{branch} up to date with origin", branch = branch));
    }
    let base = base_of(dir, snap);
    let remote_branch = base.trim_start_matches("origin/");
    git(dir, &["fetch", "-q", "origin", remote_branch]).await?;
    if let Err(e) = git(dir, &["rebase", "-q", &base]).await {
        let _ = git(dir, &["rebase", "--abort"]).await;
        bail!(tr!("conflicts with {base}, left as it was: ask the agent to rebase ({err})", base = base, err = format!("{e:#}")));
    }
    Ok(tr!("{branch} rebased onto {base}", branch = branch, base = base))
}

/// A branch a folder can be switched to (Worktrees).
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BranchChoice {
    pub name: String,
    /// Only on origin: switching to it makes the local branch, tracking it.
    pub remote: bool,
    /// The folder is on it now.
    pub current: bool,
    /// Another worktree is on it (git switches no two folders to one branch): its path.
    pub elsewhere: Option<String>,
}

/// The branches `dir` can be switched to, origin read again first (a failed fetch lists what is
/// known): the local ones, most recent first, then origin's that have no local branch.
pub async fn branches(dir: &Path) -> Result<Vec<BranchChoice>> {
    if let Err(e) = git(dir, &["fetch", "-q", "--prune", "origin"]).await {
        tracing::debug!("vcs: fetch before listing the branches: {e:#}");
    }
    let here = git(dir, &["rev-parse", "--show-toplevel"]).await?;
    let mut out: Vec<BranchChoice> = git(dir, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)%09%(worktreepath)", "refs/heads"])
        .await?
        .lines()
        .filter_map(|l| {
            let (name, path) = l.split_once('\t').unwrap_or((l, ""));
            (!name.is_empty()).then(|| BranchChoice { name: name.into(), remote: false, current: path == here, elsewhere: Some(path.to_string()).filter(|p| !p.is_empty() && *p != here) })
        })
        .collect();
    let remote: Vec<String> = git(dir, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/remotes/origin"])
        .await?
        .lines()
        .filter_map(|l| l.strip_prefix("origin/"))
        .filter(|n| *n != "HEAD" && !n.is_empty() && !out.iter().any(|b| b.name == *n))
        .map(String::from)
        .collect();
    out.extend(remote.into_iter().map(|name| BranchChoice { name, remote: true, current: false, elsewhere: None }));
    Ok(out)
}

/// Its branch brought up to date with its upstream, fast-forward only (nothing rebased nor merged).
pub async fn pull(dir: &Path) -> Result<String> {
    let branch = git(dir, &["branch", "--show-current"]).await?;
    if branch.is_empty() {
        bail!(tr("the folder is on no branch"));
    }
    if git(dir, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]).await.is_err() {
        bail!(tr!("{branch} tracks no branch of origin: nothing to pull", branch = branch));
    }
    let before = git(dir, &["rev-parse", "HEAD"]).await?;
    if let Err(e) = git(dir, &["pull", "--ff-only", "--no-rebase", "-q"]).await {
        let e = format!("{e:#}");
        if e.contains("Not possible to fast-forward") || e.contains("diverging") {
            bail!(tr!("{branch} and its upstream have diverged: nothing pulled (rebase it, or ask an agent)", branch = branch));
        }
        bail!(e);
    }
    let pulled: u32 = git(dir, &["rev-list", "--count", &format!("{before}..HEAD")]).await?.parse().unwrap_or(0);
    Ok(match pulled {
        0 => tr!("{branch} already up to date", branch = branch),
        1 => tr!("{branch}: 1 commit pulled", branch = branch),
        n => tr!("{branch}: {n} commits pulled", branch = branch, n = n),
    })
}

/// The folder switched to `branch`, a local one or origin's (then made local, tracking it). Only
/// with nothing uncommitted: changes carried to another branch are a surprise.
pub async fn switch(dir: &Path, branch: &str) -> Result<String> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') || branch.contains(char::is_whitespace) {
        bail!(tr!("not a branch: {branch}", branch = branch));
    }
    if !git(dir, &["status", "--porcelain"]).await?.is_empty() {
        bail!(tr("uncommitted changes: commit or stash them first"));
    }
    git(dir, &["switch", "-q", branch]).await?;
    Ok(tr!("on {branch}", branch = branch))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn sh(dir: &Path, args: &[&str]) {
        crate::git::run(dir, args).await.unwrap();
    }

    /// A clone of a fresh origin, on `feature` from `main`.
    async fn repo(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!("prism-vcs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (origin, work) = (root.join("origin.git"), root.join("work"));
        std::fs::create_dir_all(&origin).unwrap();
        sh(&origin, &["init", "-q", "--bare", "-b", "main"]).await;
        sh(&root, &["clone", "-q", origin.to_str().unwrap(), "work"]).await;
        for (k, v) in [("user.email", "t@example.com"), ("user.name", "T")] {
            sh(&work, &["config", k, v]).await;
        }
        std::fs::write(work.join("a.txt"), "one\n").unwrap();
        sh(&work, &["checkout", "-q", "-b", "main"]).await;
        sh(&work, &["add", "-A"]).await;
        sh(&work, &["commit", "-q", "-m", "first"]).await;
        sh(&work, &["push", "-q", "origin", "main"]).await;
        sh(&work, &["checkout", "-q", "-b", "feature"]).await;
        (root, work)
    }

    #[tokio::test]
    async fn commits_pushes_and_rebases_its_own_branch_only() {
        let (root, work) = repo("flow").await;
        let mut snap = Snapshot::default();
        snap.conventions.main_branch = "main".into();
        // A hook the branch put there never runs.
        std::fs::write(work.join(".git/hooks/pre-commit"), "#!/bin/sh\ntouch hooked\nexit 1\n").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(work.join(".git/hooks/pre-commit"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

        assert!(commit(&work, " ", &snap).await.is_err(), "a message");
        assert!(commit(&work, "nothing", &snap).await.unwrap_err().to_string().contains("nothing to commit"));
        std::fs::write(work.join("b.txt"), "two\n").unwrap();
        let s = state(&work, &snap).await.unwrap();
        assert_eq!((s.branch.as_deref(), s.changes, s.protected, s.upstream.is_none()), (Some("feature"), 1, false, true));
        commit(&work, "Add b", &snap).await.unwrap();
        assert!(!work.join("hooked").exists());
        push(&work, false, &snap).await.unwrap();
        let s = state(&work, &snap).await.unwrap();
        assert_eq!((s.changes, s.ahead, s.unpushed, s.upstream.as_deref()), (0, 1, 0, Some("origin/feature")));

        // main moves on: the branch is behind, then rebased onto it; its push needs the force.
        sh(&work, &["checkout", "-q", "main"]).await;
        assert!(commit(&work, "on main", &snap).await.unwrap_err().to_string().contains("protected"));
        std::fs::write(work.join("c.txt"), "three\n").unwrap();
        sh(&work, &["add", "-A"]).await;
        sh(&work, &["commit", "-q", "--no-verify", "-m", "on main"]).await;
        assert!(push(&work, false, &snap).await.unwrap_err().to_string().contains("protected"));
        sh(&work, &["push", "-q", "origin", "main"]).await;
        sh(&work, &["checkout", "-q", "feature"]).await;
        sh(&work, &["fetch", "-q"]).await;
        assert_eq!(state(&work, &snap).await.unwrap().behind, 1);
        std::fs::write(work.join("d.txt"), "dirty\n").unwrap();
        assert!(sync(&work, &snap).await.unwrap_err().to_string().contains("uncommitted"));
        std::fs::remove_file(work.join("d.txt")).unwrap();
        sync(&work, &snap).await.unwrap();
        let s = state(&work, &snap).await.unwrap();
        assert_eq!((s.behind, s.ahead), (0, 1));
        assert!(push(&work, false, &snap).await.unwrap_err().to_string().contains("Force push"));
        push(&work, true, &snap).await.unwrap();

        // A conflict leaves it as it was.
        sh(&work, &["checkout", "-q", "main"]).await;
        std::fs::write(work.join("b.txt"), "main's\n").unwrap();
        sh(&work, &["add", "-A"]).await;
        sh(&work, &["commit", "-q", "--no-verify", "-m", "b on main"]).await;
        sh(&work, &["push", "-q", "origin", "main"]).await;
        sh(&work, &["checkout", "-q", "feature"]).await;
        let head = crate::git::run(&work, &["rev-parse", "HEAD"]).await.unwrap();
        assert!(sync(&work, &snap).await.unwrap_err().to_string().contains("conflicts"));
        assert_eq!(crate::git::run(&work, &["rev-parse", "HEAD"]).await.unwrap(), head);
        assert_eq!(state(&work, &snap).await.unwrap().changes, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn lists_branches_pulls_and_switches() {
        let (root, work) = repo("switch").await;
        // origin gets a branch this clone has not, and main moves on there (from a second clone).
        let other = root.join("other");
        sh(&root, &["clone", "-q", root.join("origin.git").to_str().unwrap(), "other"]).await;
        for (k, v) in [("user.email", "t@example.com"), ("user.name", "T")] {
            sh(&other, &["config", k, v]).await;
        }
        sh(&other, &["checkout", "-q", "-b", "fix"]).await;
        sh(&other, &["push", "-q", "origin", "fix"]).await;
        sh(&other, &["checkout", "-q", "main"]).await;
        std::fs::write(other.join("e.txt"), "e\n").unwrap();
        sh(&other, &["add", "-A"]).await;
        sh(&other, &["commit", "-q", "-m", "e"]).await;
        sh(&other, &["push", "-q", "origin", "main"]).await;

        let list = branches(&work).await.unwrap();
        let get = |n: &str| list.iter().find(|b| b.name == n).unwrap_or_else(|| panic!("{n} in {list:?}"));
        assert!(get("feature").current && !get("feature").remote);
        assert!(!get("main").current && !get("main").remote);
        assert!(get("fix").remote);

        // feature tracks nothing: no pull; a change keeps it where it is.
        assert!(pull(&work).await.unwrap_err().to_string().contains("tracks no branch"));
        std::fs::write(work.join("d.txt"), "dirty\n").unwrap();
        assert!(switch(&work, "main").await.unwrap_err().to_string().contains("uncommitted"));
        std::fs::remove_file(work.join("d.txt")).unwrap();
        assert!(switch(&work, "--orphan").await.is_err());

        switch(&work, "main").await.unwrap();
        assert_eq!(pull(&work).await.unwrap(), "main: 1 commit pulled");
        assert!(work.join("e.txt").exists());
        assert_eq!(pull(&work).await.unwrap(), "main already up to date");
        // origin's branch becomes a local one, tracking it.
        switch(&work, "fix").await.unwrap();
        assert_eq!(crate::git::run(&work, &["rev-parse", "--abbrev-ref", "@{u}"]).await.unwrap().trim(), "origin/fix");
        let _ = std::fs::remove_dir_all(root);
    }
}
