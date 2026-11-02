use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::{config::{WorktreeRepo, WorktreesConfig}, git::run as git, state::{Hub, Worktree}, ticket};

const SOURCE: &str = "worktrees";
const FETCH_INTERVAL: Duration = Duration::from_secs(120);
/// A fetch that hangs (the network gone, ssh waiting) is given up after this.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);
/// Untracked files: one walk of the folder per worktree, once a minute.
const SLOW_INTERVAL: Duration = Duration::from_secs(60);
/// Disk size (`du`), slower still: every ten minutes.
const SIZE_EVERY: u64 = 10;

/// Slow measurements of a worktree, done separately from the collection.
#[derive(Clone, Copy, Default)]
struct Slow {
    untracked: Option<u32>,
    size_bytes: Option<u64>,
}
type SlowCache = Arc<Mutex<HashMap<String, Slow>>>;

/// What a worktree's commits gave (behind, ahead, unpushed, last commit), and what they were read
/// for: its HEAD, its base and the remote refs. Unchanged, they are not read again: three git calls
/// a worktree every few seconds, for a score of worktrees, came to a third of a core.
#[derive(Clone, Copy)]
struct Commits {
    key: u64,
    behind: u32,
    ahead: u32,
    unpushed: u32,
    last_commit: Option<DateTime<Utc>>,
}
type CommitCache = Arc<Mutex<HashMap<String, Commits>>>;

/// Queries git locally at a short interval: it is cheap and does not leave the machine. `repos`:
/// those whose worktrees are listed, the primary one first.
pub async fn run(cfg: WorktreesConfig, repos: Vec<WorktreeRepo>, hub: Hub) {
    hub.update(|s| {
        let changed = s.worktree_stale_days != cfg.stale_days;
        s.worktree_stale_days = cfg.stale_days;
        changed
    });
    // Trash of deleted worktrees: what a Prism shutdown would have left to erase.
    for r in &repos {
        if let Ok(common) = git(&r.path, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await {
            empty_trash(Path::new(common.trim()));
        }
    }
    let slow = SlowCache::default();
    prism_base::rt::spawn(measure(hub.clone(), slow.clone()));

    // The fetches go on their own: a few seconds each, minutes when the network hangs, they never
    // hold the list back. Each round done, the lag behind the base is read again at once.
    let fetched = Arc::new(tokio::sync::Notify::new());
    prism_base::rt::spawn(fetch_all(repos.clone(), fetched.clone()));

    // At once, then every `poll_seconds`, slower while neither the window nor a phone looks.
    let every = Duration::from_secs(cfg.poll_seconds.max(1));
    let mut first = true;
    let commits = CommitCache::default();
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = hub.pause(every) => {}
                _ = fetched.notified() => {}
            }
        }
        // The repositories in parallel; one in error does not hide the others' worktrees.
        let results = futures::future::join_all(repos.iter().map(|r| collect(r, &slow, &commits))).await;
        let mut list = Vec::new();
        let mut errors = Vec::new();
        for (r, result) in repos.iter().zip(results) {
            match result {
                Ok(mut l) => list.append(&mut l),
                Err(e) => {
                    tracing::warn!("worktrees: {}: {e:#}", r.name);
                    errors.push(if repos.len() > 1 { format!("{}: {e:#}", r.name) } else { format!("{e:#}") });
                }
            }
        }
        hub.update(|s| {
            let changed = s.worktrees != list;
            s.worktrees = list;
            changed
        });
        // After the data: whoever sees the source as answered also sees what it brought.
        hub.set_error(SOURCE, (!errors.is_empty()).then(|| errors.join("; ")));
    }
}

/// Every `FETCH_INTERVAL`, the repositories fetched in parallel, each given up after
/// `FETCH_TIMEOUT`. Without a fetch, the lag behind the base would be computed against a stale
/// ref; `--prune` removes branches deleted on the forge, hence the "upstream gone" state. The
/// first round waits a moment: the first display does not share the machine with the network.
async fn fetch_all(repos: Vec<WorktreeRepo>, fetched: Arc<tokio::sync::Notify>) {
    tokio::time::sleep(Duration::from_secs(3)).await;
    let mut tick = tokio::time::interval(FETCH_INTERVAL);
    loop {
        tick.tick().await;
        futures::future::join_all(repos.iter().map(|r| async move {
            match tokio::time::timeout(FETCH_TIMEOUT, fetch(r)).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!("worktrees: fetch {}: {e:#}", r.name),
                Err(_) => tracing::warn!("worktrees: fetch {}: no answer within {} s, given up", r.name, FETCH_TIMEOUT.as_secs()),
            }
        }))
        .await;
        fetched.notify_one();
    }
}

/// Fetches the main branch and, with Git Flow, the release branches.
async fn fetch(r: &WorktreeRepo) -> Result<()> {
    // A local project, without a remote: nothing to fetch.
    if !prism_base::git::has_origin(&r.path).await {
        return Ok(());
    }
    let main = main_branch(r).await;
    let mut args = vec!["fetch".to_string(), "--quiet".into(), "--prune".into(), "origin".into(), main];
    // Without a release prefix (no Git Flow), the main branch only.
    if !r.release_prefix.is_empty() {
        args.push(format!("+refs/heads/{0}*:refs/remotes/origin/{0}*", r.release_prefix));
    }
    // Killed when given up (`FETCH_TIMEOUT`), rather than left running; never a prompt to wait on.
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    crate::git::run(&r.path, &args).await?;
    Ok(())
}

/// The repository's main branch: declared, otherwise the remote's default one.
pub async fn main_branch(r: &WorktreeRepo) -> String {
    if r.main_branch.is_empty() {
        super::repos::default_branch(&r.path).await
    } else {
        r.main_branch.clone()
    }
}

/// Untracked files and size of each worktree, in the background: the values join
/// the snapshot at the next collection.
async fn measure(hub: Hub, cache: SlowCache) {
    for n in 0u64..=u64::MAX {
        if n > 0 {
            hub.pause(SLOW_INTERVAL).await;
        }
        let paths: Vec<String> = hub.subscribe().borrow().worktrees.iter().filter(|w| !w.prunable).map(|w| w.path.clone()).collect();
        for path in paths {
            let dir = Path::new(&path);
            let untracked = git(dir, &["status", "--porcelain", "--untracked-files=normal"]).await.ok().map(|s| count_untracked(&s));
            let size_bytes = if n % SIZE_EVERY == 0 { disk_usage(dir).await } else { None };
            let mut cache = cache.lock().unwrap();
            let entry = cache.entry(path).or_default();
            entry.untracked = untracked.or(entry.untracked);
            entry.size_bytes = size_bytes.or(entry.size_bytes);
        }
    }
}

/// Untracked files, excluding those Claude leaves in `.claude/` folders (agent
/// memory, local settings): they are not lost work.
pub fn count_untracked(porcelain: &str) -> u32 {
    porcelain
        .lines()
        .filter_map(|l| l.strip_prefix("?? "))
        .filter(|p| !is_claude_litter(p))
        .count() as u32
}

pub fn is_claude_litter(path: &str) -> bool {
    let p = path.trim_matches('"');
    p.starts_with(".claude/") || p.contains("/.claude/")
}

async fn disk_usage(dir: &Path) -> Option<u64> {
    crate::platform::disk_usage(dir).await
}

/// Where Prism notes the branch on `origin` a worktree's branch was made from
/// (`branch.<name>.prism-base`, gone with the branch): when picked in the launcher, it is not the
/// repository's base.
pub const BASE_KEY: &str = "prism-base";

/// The bases noted for the repository's branches (`BASE_KEY`), by branch.
async fn noted_bases(repo: &Path) -> HashMap<String, String> {
    let out = git(repo, &["config", "--get-regexp", &format!(r"^branch\..*\.{BASE_KEY}$")]).await.unwrap_or_default();
    parse_noted_bases(&out)
}

fn parse_noted_bases(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|l| {
            let (key, base) = l.split_once(' ')?;
            let branch = key.strip_prefix("branch.")?.strip_suffix(&format!(".{BASE_KEY}"))?;
            Some((branch.to_string(), base.trim().to_string())).filter(|(_, b)| !b.is_empty())
        })
        .collect()
}

async fn collect(repo: &WorktreeRepo, slow: &SlowCache, commits: &CommitCache) -> Result<Vec<Worktree>> {
    let porcelain = git(&repo.path, &["worktree", "list", "--porcelain"]).await?;
    let default_base = resolve_base(repo).await?;
    let noted = noted_bases(&repo.path).await;
    let entries: Vec<Entry> = porcelain.split("\n\n").filter_map(parse_entry).collect();
    let cache = slow.lock().unwrap().clone();
    // The remote refs (what `--remotes` and the base are), once for the repository: with each
    // worktree's HEAD, what its commits' counts depend on. Unread, nothing is taken as unchanged.
    let remotes = git(&repo.path, &["for-each-ref", "--format=%(objectname) %(refname)", "refs/remotes"]).await.ok();
    let known = commits.lock().unwrap().clone();

    // Worktrees are read in parallel: about twenty, four git calls each.
    let reads = entries.into_iter().enumerate().map(|(i, e)| {
        let base = e.branch.as_ref().and_then(|b| noted.get(b)).unwrap_or(&default_base).clone();
        let base_ref = format!("origin/{base}");
        let slow = cache.get(&e.path).copied().unwrap_or_default();
        let (remotes, known) = (remotes.as_deref(), &known);
        async move {
            let mut w = Worktree {
                repo: repo.name.clone(),
                ticket: e.branch.as_deref().and_then(|b| ticket::find([b])),
                path: e.path,
                branch: e.branch,
                base,
                behind: 0,
                ahead: 0,
                dirty: false,
                changes: 0,
                untracked: slow.untracked,
                size_bytes: slow.size_bytes,
                unpushed: 0,
                upstream: None,
                upstream_gone: false,
                unpulled: 0,
                head: e.head.as_ref().map(|h| h.chars().take(10).collect()),
                last_commit: None,
                is_main: i == 0,
                prunable: e.prunable,
                locked: e.locked,
            };
            if e.prunable {
                return (w, None);
            }
            let dir = Path::new(&w.path);
            let key = e.head.as_deref().zip(remotes).map(|(head, remotes)| commits_key(head, &base_ref, remotes));
            let kept = key.and_then(|k| known.get(&w.path).filter(|c| c.key == k).copied());
            let status = git(dir, &["status", "--porcelain=v2", "--branch", "--untracked-files=no"]);
            let (status, read) = match kept {
                Some(c) => (status.await, Some(c)),
                None => {
                    let range = format!("{base_ref}...HEAD");
                    let counts_args = ["rev-list", "--left-right", "--count", range.as_str()];
                    let (counts, status, unpushed, last) = tokio::join!(
                        git(dir, &counts_args),
                        status,
                        git(dir, &["rev-list", "--count", "HEAD", "--not", "--remotes"]),
                        git(dir, &["log", "-1", "--format=%cI"]),
                    );
                    // Kept only when all were read: one that failed is asked again next time.
                    let read = match (counts, unpushed, last) {
                        (Ok(counts), Ok(unpushed), Ok(last)) => {
                            let (behind, ahead) = parse_counts(&counts);
                            let unpushed = unpushed.trim().parse().unwrap_or(0);
                            let last_commit = DateTime::parse_from_rfc3339(last.trim()).ok().map(|d| d.with_timezone(&Utc));
                            Some(Commits { key: key.unwrap_or(0), behind, ahead, unpushed, last_commit })
                        }
                        _ => None,
                    };
                    (status, read)
                }
            };
            if let Some(c) = read {
                (w.behind, w.ahead, w.unpushed, w.last_commit) = (c.behind, c.ahead, c.unpushed, c.last_commit);
            }
            // An unreadable worktree does not deprive the others of their state.
            match status {
                Ok(status) => {
                    let st = parse_status(&status);
                    (w.changes, w.upstream, w.upstream_gone, w.unpulled) = (st.changes, st.upstream, st.gone, st.behind);
                    w.dirty = w.changes > 0;
                }
                Err(e) => tracing::warn!("worktrees: {}: {e:#}", w.path),
            }
            (w, read.filter(|_| key.is_some()))
        }
    });
    let read = futures::future::join_all(reads).await;
    let mut cache = commits.lock().unwrap();
    // Those of this repository read again, the gone ones dropped.
    for (w, c) in &read {
        match c {
            Some(c) => cache.insert(w.path.clone(), *c),
            None => cache.remove(&w.path),
        };
    }
    Ok(read.into_iter().map(|(w, _)| w).collect())
}

/// What a worktree's commits are read for: its HEAD, its base, the remote refs.
fn commits_key(head: &str, base_ref: &str, remotes: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (head, base_ref, remotes).hash(&mut h);
    h.finish()
}

/// A block of `git worktree list --porcelain`.
struct Entry {
    path: String,
    head: Option<String>,
    branch: Option<String>,
    prunable: bool,
    locked: bool,
}

fn parse_entry(block: &str) -> Option<Entry> {
    let mut e = Entry { path: String::new(), head: None, branch: None, prunable: false, locked: false };
    let mut bare = false;
    for line in block.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            e.path = p.to_string();
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            e.head = Some(h.to_string());
        } else if let Some(b) = line.strip_prefix("branch ") {
            e.branch = Some(b.trim_start_matches("refs/heads/").to_string());
        } else if line == "bare" {
            bare = true;
        } else if line.starts_with("prunable") {
            e.prunable = true;
        } else if line.starts_with("locked") {
            e.locked = true;
        }
    }
    (!e.path.is_empty() && !bare).then_some(e)
}

struct Status {
    changes: u32,
    upstream: Option<String>,
    gone: bool,
    /// Behind its upstream (`branch.ab +ahead -behind`).
    behind: u32,
}

/// `git status --porcelain=v2 --branch`: a tracked branch without `branch.ab` no longer has a
/// remote ref (deleted after the MR was merged).
fn parse_status(s: &str) -> Status {
    let upstream = s.lines().find_map(|l| l.strip_prefix("# branch.upstream ")).map(String::from);
    let ab = s.lines().find_map(|l| l.strip_prefix("# branch.ab "));
    let behind = ab.and_then(|ab| ab.split_whitespace().find_map(|x| x.strip_prefix('-'))).and_then(|n| n.parse().ok()).unwrap_or(0);
    let changes = s.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count() as u32;
    Status { gone: upstream.is_some() && ab.is_none(), upstream, changes, behind }
}

/// Base of new worktrees: `auto` follows the repository's Git Flow (the most recent release,
/// otherwise the main branch).
pub async fn resolve_base(repo: &WorktreeRepo) -> Result<String> {
    if repo.base_branch != "auto" {
        return Ok(repo.base_branch.clone());
    }
    crate::conventions::latest_release_in(&repo.path, &main_branch(repo).await, &repo.release_prefix).await
}

fn parse_counts(s: &str) -> (u32, u32) {
    let mut it = s.split_whitespace().map(|n| n.parse().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0))
}


/// Trash for removed worktrees, in the common `.git`: same disk as the repository, and
/// invisible to git as well as to tools that walk the repository.
pub fn trash_dir(common: &Path) -> PathBuf {
    common.join("prism-trash")
}

/// Erases the trash's content in the background (`rm -rf`, which outlives the end of a hook). Also
/// at Prism's launch, for whatever a shutdown would have left.
pub fn empty_trash(common: &Path) {
    let Ok(entries) = std::fs::read_dir(trash_dir(common)) else { return };
    let paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    if paths.is_empty() {
        return;
    }
    crate::platform::remove_dirs_detached(&paths);
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_bases_noted_for_branches() {
        let noted = parse_noted_bases("branch.agent/fix-1.2.prism-base main\nbranch.agent/x.prism-base release/Q4-2026\n");
        assert_eq!(noted.get("agent/fix-1.2").map(String::as_str), Some("main"));
        assert_eq!(noted.get("agent/x").map(String::as_str), Some("release/Q4-2026"));
        assert_eq!(noted.len(), 2);
    }

    /// A worktree's commits are read again when its HEAD or the remote refs move, and only then.
    #[tokio::test]
    async fn reads_the_commits_again_when_they_move() {
        let dir = std::env::temp_dir().join(format!("prism-wt-commits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git").args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).current_dir(&dir).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        run(&["init", "-q", "-b", "main"]);
        run(&["commit", "-q", "--allow-empty", "-m", "one"]);
        run(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let repo = WorktreeRepo { name: "r".into(), path: dir.clone(), main_branch: "main".into(), release_prefix: String::new(), base_branch: "main".into() };
        let (slow, commits) = (SlowCache::default(), CommitCache::default());
        let first = collect(&repo, &slow, &commits).await.unwrap();
        assert_eq!((first[0].ahead, first[0].behind, first[0].unpushed), (0, 0, 0));
        let key = commits.lock().unwrap()[&first[0].path].key;
        // Nothing moved: the same, from what was kept.
        let again = collect(&repo, &slow, &commits).await.unwrap();
        assert_eq!(again, first);
        assert_eq!(commits.lock().unwrap()[&first[0].path].key, key);
        // A commit: ahead and unpushed again.
        run(&["commit", "-q", "--allow-empty", "-m", "two"]);
        let moved = collect(&repo, &slow, &commits).await.unwrap();
        assert_eq!((moved[0].ahead, moved[0].unpushed), (1, 1));
        // Pushed (the remote ref moved): nothing ahead any more.
        run(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let pushed = collect(&repo, &slow, &commits).await.unwrap();
        assert_eq!((pushed[0].ahead, pushed[0].unpushed), (0, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_list_and_status() {
        let list = "worktree /r/app\nHEAD abc\nbranch refs/heads/main\n\nworktree /r/x\nHEAD def\nbranch refs/heads/feat/a\nprunable gitdir file points to non-existent location\n\nworktree /r/y\nHEAD 123\ndetached\nlocked\n";
        let e: Vec<Entry> = list.split("\n\n").filter_map(parse_entry).collect();
        assert_eq!(e.len(), 3);
        assert_eq!(e[1].branch.as_deref(), Some("feat/a"));
        assert!(e[1].prunable && !e[0].prunable);
        assert!(e[2].branch.is_none() && e[2].locked);

        let gone = parse_status("# branch.oid abc\n# branch.head feat/a\n# branch.upstream origin/feat/a\n1 .M N... 100644 100644 100644 a b src/x.rs\n");
        assert!(gone.gone && gone.changes == 1);
        let tracked = parse_status("# branch.oid abc\n# branch.head feat/a\n# branch.upstream origin/feat/a\n# branch.ab +1 -3\n");
        assert!(!tracked.gone && tracked.changes == 0 && tracked.behind == 3);
        assert!(!parse_status("# branch.oid abc\n# branch.head local\n").gone);

        assert_eq!(count_untracked("?? .claude/agent-memory/\n?? apps/web/.claude/\n?? src/new.rs\n M a.rs\n"), 1);
    }
}
