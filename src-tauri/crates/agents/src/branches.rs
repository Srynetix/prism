//! A project's local branches (Projects › Branches): each with where it stands and whether deleting
//! it loses anything; deleted one by one, or in a tidy (`fetch --prune`, `worktree prune`, then
//! those done and safe). Local branches only: nothing is deleted on the forge.
//!
//! Nothing to lose: no commit on no remote branch (after its MR's merge, a squash's commits count as
//! delivered), no change in its worktree. Done: its MR merged, its upstream gone from origin, or
//! nothing of its own beyond the base (merged into it, or never committed on). A tidy deletes only what is both.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use prism_base::{i18n::tr, tr};
use prism_core::conventions::Conventions;
use serde::Serialize;

/// A local branch of a project's clone.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LocalBranch {
    pub name: String,
    /// Its last commit: abbreviated, its date, author and subject.
    pub head: String,
    pub last_commit: Option<DateTime<Utc>>,
    pub author: String,
    pub subject: String,
    /// Its branch on origin; gone from it (deleted after a merge, `fetch --prune`).
    pub upstream: Option<String>,
    pub upstream_gone: bool,
    /// Commits not on its upstream yet, and the upstream's not here.
    pub to_push: u32,
    pub to_pull: u32,
    /// Against the clone's base (`origin/main`): its commits of its own, and the base's it lacks;
    /// absent when the base cannot be read.
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// The folder it is checked out in: a worktree, or the clone itself (`current`).
    pub worktree: Option<String>,
    /// The clone is on it.
    pub current: bool,
    /// Never deleted, even forced: a protected branch, the clone's, one an agent works on (why).
    pub locked: Option<String>,
    /// What deleting it would lose (commits on no remote branch, its worktree's changes, an open
    /// MR); absent: nothing.
    pub keep: Option<String>,
    /// Why it is done (MR merged, gone from origin, already in the base); empty: not done.
    pub done: Vec<String>,
}

impl LocalBranch {
    /// What a tidy deletes: done, with nothing to lose, not locked.
    pub fn safe(&self) -> bool {
        self.locked.is_none() && self.keep.is_none() && !self.done.is_empty()
    }
}

/// What the snapshot says of a project's branches, read by `list`.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The clone's base (`main`), compared against on origin.
    pub base: String,
    pub conventions: Conventions,
    /// By branch: its MRs' latest merge (none open), and whether one is open.
    pub mrs: HashMap<String, (Option<DateTime<Utc>>, bool)>,
    /// By folder: the agent working in it.
    pub busy: HashMap<String, String>,
}

/// `git` without hooks, its output trimmed.
async fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let mut all = vec!["-c", "core.hooksPath=/dev/null"];
    all.extend_from_slice(args);
    Ok(crate::git::run(dir, &all).await?.trim().to_string())
}

/// A count git printed (`rev-list --count`), 0 when unreadable.
fn count(s: &str) -> u32 {
    s.trim().parse().unwrap_or(0)
}

/// `%(upstream:track,nobracket)`: `gone`, `ahead 1, behind 2`, `ahead 1`, `behind 2`, or nothing.
fn track(s: &str) -> (bool, u32, u32) {
    if s.trim() == "gone" {
        return (true, 0, 0);
    }
    let mut ahead = 0;
    let mut behind = 0;
    for part in s.split(',').map(str::trim) {
        if let Some(n) = part.strip_prefix("ahead ") {
            ahead = count(n);
        } else if let Some(n) = part.strip_prefix("behind ") {
            behind = count(n);
        }
    }
    (false, ahead, behind)
}

const FORMAT: &str = "%(refname:short)%00%(objectname:short)%00%(committerdate:iso-strict)%00%(authorname)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(worktreepath)%00%(subject)";

/// A list's per-branch reads run at once, at most: each is a git process, its pipes among the open
/// files. And every background list together (the Cleanup tab reads every project), at most
/// `BACKGROUND`; a list an action needs (a deletion, a tidy) does not queue behind them.
const AT_ONCE: usize = 8;
static BACKGROUND: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(32);

/// Whom a list is for: shown (queued with the others), or an action the user waits on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Need {
    Shown,
    Action,
}

/// One line of `for-each-ref`, read.
struct Ref {
    name: String,
    head: String,
    date: String,
    author: String,
    upstream: String,
    track: String,
    worktree: String,
    subject: String,
    /// `%(ahead-behind:<base>)`, when git has it (2.41).
    counts: Option<(u32, u32)>,
}

fn parse_ref(line: &str, counted: bool) -> Option<Ref> {
    let f: Vec<&str> = line.split('\0').collect();
    if f.len() < 8 {
        return None;
    }
    let counts = counted.then(|| f.get(8).map(|c| {
        let mut n = c.split_whitespace().map(count);
        (n.next().unwrap_or(0), n.next().unwrap_or(0))
    })).flatten();
    Some(Ref { name: f[0].into(), head: f[1].into(), date: f[2].into(), author: f[3].into(), upstream: f[4].into(), track: f[5].into(), worktree: f[6].into(), subject: f[7].into(), counts })
}

/// The local branches of the clone `repo`, most recent first.
pub async fn list(repo: &Path, facts: &Facts, need: Need) -> Result<Vec<LocalBranch>> {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let here = real(Path::new(&git(repo, &["rev-parse", "--show-toplevel"]).await?));
    let base_name = facts.base.trim_start_matches("origin/").to_string();
    let base = format!("origin/{base_name}");
    let has_base = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{base}")]).await.is_ok();
    // The counts against the base in the same read where git has `ahead-behind` (2.41); one read
    // per branch otherwise.
    let with_counts = format!("--format={FORMAT}%00%(ahead-behind:{base})");
    let (refs, counted) = match has_base {
        true => match git(repo, &["for-each-ref", "--sort=-committerdate", &with_counts, "refs/heads"]).await {
            Ok(out) => (out, true),
            Err(_) => (git(repo, &["for-each-ref", "--sort=-committerdate", &format!("--format={FORMAT}"), "refs/heads"]).await?, false),
        },
        false => (git(repo, &["for-each-ref", "--sort=-committerdate", &format!("--format={FORMAT}"), "refs/heads"]).await?, false),
    };
    let refs: Vec<Ref> = refs.lines().filter_map(|l| parse_ref(l, counted)).collect();

    let mine = std::sync::Arc::new(tokio::sync::Semaphore::new(AT_ONCE));
    let mut tasks = tokio::task::JoinSet::new();
    for (i, r) in refs.into_iter().enumerate() {
        let (repo, facts, here, base, mine) = (repo.to_path_buf(), facts.clone(), here.clone(), base.clone(), mine.clone());
        let base_name = base_name.clone();
        tasks.spawn(async move {
            let _mine = mine.acquire_owned().await;
            let _shared = match need {
                Need::Shown => Some(BACKGROUND.acquire().await),
                Need::Action => None,
            };
            (i, branch(&repo, &facts, &here, has_base.then_some(base.as_str()), &base_name, r).await)
        });
    }
    let mut out: Vec<(usize, LocalBranch)> = Vec::new();
    while let Some(done) = tasks.join_next().await {
        out.push(done?);
    }
    out.sort_by_key(|(i, _)| *i);
    Ok(out.into_iter().map(|(_, b)| b).collect())
}

/// One branch: its counts, its verdict.
async fn branch(repo: &Path, facts: &Facts, here: &Path, base: Option<&str>, base_name: &str, r: Ref) -> LocalBranch {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let name = r.name;
    let (upstream_gone, to_push, to_pull) = track(&r.track);
    let worktree = Some(r.worktree.as_str()).filter(|p| !p.is_empty()).map(PathBuf::from);
    let current = worktree.as_deref().is_some_and(|w| real(w) == here);
    let (ahead, behind) = match (r.counts, base) {
        (Some((a, b)), _) => (Some(a), Some(b)),
        (None, Some(base)) => {
            let lr = git(repo, &["rev-list", "--left-right", "--count", &format!("{base}...refs/heads/{name}")]).await.unwrap_or_default();
            let mut n = lr.split_whitespace().map(count);
            let behind = n.next();
            (n.next(), behind)
        }
        (None, None) => (None, None),
    };
    // Its MR's merge counts only for a branch pushed to origin under its own name (gone since or
    // not): a fork's MR, or an old one whose name this branch reuses, says nothing of its commits.
    let pushed_as_itself = r.upstream == format!("origin/{name}");
    let (merged_at, mr_open) = facts.mrs.get(&name).copied().unwrap_or((None, false));
    let merged_at = merged_at.filter(|_| pushed_as_itself);
    let worktree_path = worktree.as_ref().map(|w| w.display().to_string());
    let locked = if crate::vcs::protected(&name, &facts.conventions) || name == base_name {
        Some(tr("a protected branch").to_string())
    } else if current {
        Some(tr("the clone is on it").to_string())
    } else {
        worktree_path.as_ref().and_then(|w| facts.busy.get(w)).map(|title| tr!("the agent “{title}” works on it", title = title))
    };
    // What it would lose: its worktree's changes and commits (`keep_reason`), or its commits on
    // no remote branch (its worktree's folder gone too); then an open MR.
    let keep = match (&worktree, current) {
        (Some(w), false) if w.is_dir() => {
            let (w, at) = (w.clone(), merged_at);
            tokio::task::spawn_blocking(move || crate::worktree::keep_reason(&w, at)).await.map_err(anyhow::Error::from).and_then(|r| r).unwrap_or_else(|e| Some(e.to_string()))
        }
        _ => {
            let since = merged_at.map(|m| format!("--since={}", m.to_rfc3339()));
            let branch = format!("refs/heads/{name}");
            let mut args = vec!["rev-list", "--count", branch.as_str(), "--not", "--remotes"];
            args.extend(since.as_deref());
            let unpushed = count(&git(repo, &args).await.unwrap_or_default());
            (unpushed > 0).then(|| if unpushed > 1 { tr!("{n} unpushed commits", n = unpushed) } else { tr!("{n} unpushed commit", n = unpushed) })
        }
    };
    let keep = keep.or_else(|| mr_open.then(|| tr("its MR is open").to_string()));
    let mut done = Vec::new();
    if merged_at.is_some() {
        done.push(tr("MR merged").to_string());
    }
    if upstream_gone {
        done.push(tr("gone from origin").to_string());
    }
    if ahead == Some(0) && !mr_open {
        done.push(tr!("already in {base}", base = base_name));
    }
    LocalBranch {
        name,
        head: r.head,
        last_commit: DateTime::parse_from_rfc3339(&r.date).ok().map(|d| d.with_timezone(&Utc)),
        author: r.author,
        subject: r.subject,
        upstream: Some(r.upstream).filter(|u| !u.is_empty()),
        upstream_gone,
        to_push,
        to_pull,
        ahead,
        behind,
        worktree: worktree_path,
        current,
        locked,
        keep,
        done,
    }
}

/// Origin read again, its deleted branches forgotten (`fetch --prune`), and the worktrees whose
/// folder is gone (`worktree prune`): before a tidy's list. A failed fetch (offline) is no failure.
pub async fn refresh(repo: &Path) -> Result<()> {
    if let Err(e) = git(repo, &["fetch", "--quiet", "--prune", "origin"]).await {
        tracing::info!("branches: fetch before a tidy: {e:#}");
    }
    git(repo, &["worktree", "prune"]).await?;
    Ok(())
}

/// `branch` deleted from the clone `repo`: refused when locked, or when it would lose something
/// unless `force`. A branch checked out in a worktree is the caller's (the worktree goes with it),
/// unless that worktree's folder is gone.
pub async fn delete(repo: &Path, branch: &LocalBranch, force: bool) -> Result<()> {
    if let Some(why) = &branch.locked {
        bail!(tr!("{branch} is kept: {why}", branch = branch.name, why = why));
    }
    if !force && let Some(why) = &branch.keep {
        bail!(tr!("{branch} is kept: {why}", branch = branch.name, why = why));
    }
    if let Some(w) = &branch.worktree {
        let w = Path::new(w);
        if w.is_dir() {
            bail!("{} is checked out in a worktree: remove it with its worktree", branch.name);
        }
        // Its worktree's folder deleted by hand: what git keeps of it, forgotten (that one only).
        crate::worktree::forget_stale(repo, w);
    }
    crate::worktree::branch_name(&branch.name)?;
    git(repo, &["branch", "-D", "--", &branch.name]).await?;
    tracing::info!("branches: {} deleted from {}", branch.name, repo.display());
    Ok(())
}

/// A worktree made for `branch` next to the clone (`<repo>.worktrees/<name>`, the first free
/// name), without an agent: its path.
pub fn add_worktree(repo: &Path, branch: &str) -> Result<PathBuf> {
    crate::worktree::branch_name(branch)?;
    let root = crate::worktree::worktrees_dir(repo);
    let stem = crate::worktree::slug(branch.rsplit('/').next().unwrap_or(branch));
    let stem = if stem.is_empty() { "branch".to_string() } else { stem };
    let dir = (1..100).map(|n| if n == 1 { root.join(&stem) } else { root.join(format!("{stem}-{n}")) }).find(|d| !d.exists()).unwrap_or_else(|| root.join(format!("{stem}-{}", uuid_like())));
    crate::worktree::create(repo, &crate::worktree::Intent { dir, branch: branch.to_string(), base: None })
}

fn uuid_like() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_tracks_are_read() {
        assert_eq!(track("gone"), (true, 0, 0));
        assert_eq!(track("ahead 2, behind 3"), (false, 2, 3));
        assert_eq!(track("behind 1"), (false, 0, 1));
        assert_eq!(track(""), (false, 0, 0));
    }

    fn sh(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A clone of a bare origin: `main`; `merged` pushed then merged into main; `squashed` whose
    /// MR was merged by squash, its origin branch deleted; `local` never pushed; `empty` with no
    /// commit of its own (already in main); `wip` in a worktree with a change.
    #[tokio::test]
    async fn verdicts_and_deletion() {
        let root = std::env::temp_dir().join(format!("prism-branches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (origin, repo) = (root.join("origin.git"), root.join("app"));
        std::fs::create_dir_all(&root).unwrap();
        sh(&root, &["init", "-q", "--bare", "-b", "main", origin.to_str().unwrap()]);
        sh(&root, &["clone", "-q", origin.to_str().unwrap(), repo.to_str().unwrap()]);
        let commit = |m: &str| sh(&repo, &["commit", "-q", "--allow-empty", "-m", m]);
        sh(&repo, &["checkout", "-q", "-b", "main"]);
        commit("init");
        sh(&repo, &["push", "-q", "-u", "origin", "main"]);
        for b in ["merged", "squashed"] {
            sh(&repo, &["checkout", "-q", "-b", b, "main"]);
            commit(b);
            sh(&repo, &["push", "-q", "-u", "origin", b]);
        }
        sh(&repo, &["checkout", "-q", "main"]);
        sh(&repo, &["merge", "-q", "--no-ff", "-m", "merge", "merged"]);
        commit("squash of squashed");
        sh(&repo, &["push", "-q", "origin", "main", ":squashed"]);
        sh(&repo, &["checkout", "-q", "-b", "local", "main"]);
        commit("mine");
        sh(&repo, &["branch", "empty", "main"]);
        sh(&repo, &["checkout", "-q", "main"]);
        let wt = root.join("app.worktrees/wip");
        sh(&repo, &["worktree", "add", "-q", "-b", "wip", wt.to_str().unwrap(), "main"]);
        std::fs::write(wt.join("note.txt"), "x").unwrap();
        refresh(&repo).await.unwrap();

        let merged_at = Utc::now() + chrono::Duration::seconds(5);
        let facts = Facts { base: "main".into(), conventions: Conventions { main_branch: "main".into(), ..Default::default() }, mrs: HashMap::from([("squashed".to_string(), (Some(merged_at), false))]), busy: HashMap::new() };
        let all = list(&repo, &facts, Need::Shown).await.unwrap();
        let get = |n: &str| all.iter().find(|b| b.name == n).unwrap_or_else(|| panic!("{n}: {all:?}")).clone();
        let main = get("main");
        assert!(main.current && main.locked.is_some() && !main.safe());
        let merged = get("merged");
        assert!(merged.keep.is_none() && merged.upstream.is_some(), "{merged:?}");
        assert_eq!(merged.done, ["already in main"], "merged into main: nothing of its own beyond it");
        assert!(merged.safe());
        let squashed = get("squashed");
        assert!(squashed.upstream_gone && squashed.safe(), "{squashed:?}");
        let local = get("local");
        assert_eq!(local.keep.as_deref(), Some("1 unpushed commit"));
        assert!(!local.safe());
        let empty = get("empty");
        assert_eq!((empty.ahead, empty.safe()), (Some(0), true), "{empty:?}");
        let wip = get("wip");
        assert!(wip.worktree.is_some() && wip.keep.as_deref().is_some_and(|k| k.contains("change")), "{wip:?}");

        assert!(delete(&repo, &local, false).await.unwrap_err().to_string().contains("unpushed"));
        assert!(delete(&repo, &main, true).await.is_err(), "never the clone's, even forced");
        delete(&repo, &squashed, false).await.unwrap();
        delete(&repo, &local, true).await.unwrap();
        let left: Vec<String> = list(&repo, &facts, Need::Shown).await.unwrap().into_iter().map(|b| b.name).collect();
        assert!(!left.contains(&"squashed".into()) && !left.contains(&"local".into()), "{left:?}");

        // A same-named MR merged (a fork's, an old one) says nothing of a branch never pushed as itself.
        sh(&repo, &["checkout", "-q", "-b", "patch-1", "main"]);
        commit("never pushed");
        sh(&repo, &["checkout", "-q", "main"]);
        let mut fork = facts.clone();
        fork.mrs.insert("patch-1".into(), (Some(merged_at), false));
        let patch = list(&repo, &fork, Need::Shown).await.unwrap().into_iter().find(|b| b.name == "patch-1").unwrap();
        assert_eq!(patch.keep.as_deref(), Some("1 unpushed commit"), "{patch:?}");
        assert!(!patch.done.iter().any(|d| d == "MR merged") && !patch.safe());

        // The repository's own base is never deleted, whatever the conventions' main branch.
        sh(&repo, &["branch", "develop", "main"]);
        sh(&repo, &["push", "-q", "origin", "develop"]);
        let on_develop = Facts { base: "develop".into(), ..facts.clone() };
        let develop = list(&repo, &on_develop, Need::Action).await.unwrap().into_iter().find(|b| b.name == "develop").unwrap();
        assert!(develop.locked.is_some() && !develop.safe(), "{develop:?}");

        // A worktree whose folder was deleted by hand: its branch deleted all the same.
        let gone = root.join("app.worktrees/gone");
        sh(&repo, &["worktree", "add", "-q", "-b", "gone", gone.to_str().unwrap(), "main"]);
        std::fs::remove_dir_all(&gone).unwrap();
        let g = list(&repo, &facts, Need::Shown).await.unwrap().into_iter().find(|b| b.name == "gone").unwrap();
        assert!(g.keep.is_none() && g.worktree.is_some(), "{g:?}");
        delete(&repo, &g, false).await.unwrap();
        assert!(!list(&repo, &facts, Need::Shown).await.unwrap().iter().any(|b| b.name == "gone"));

        let made = add_worktree(&repo, "merged").unwrap();
        assert_eq!(made.file_name().unwrap(), "merged");
        assert!(list(&repo, &facts, Need::Shown).await.unwrap().iter().any(|b| b.name == "merged" && b.worktree.is_some()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
