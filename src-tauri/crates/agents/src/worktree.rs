//! Agents' worktrees. Claude requests them (`claude --worktree <name>`), but it is Prism that
//! creates and removes them, through its `WorktreeCreate` / `WorktreeRemove` hooks: next to the repository
//! (`<repo>.worktrees/<name>`, the machine's convention), on an `agent/<name>` branch (`[agents] branch_prefix`) started from
//! the release base, or on an MR's existing branch. Outside `.claude/worktrees/`, neither
//! Claude Desktop's cleanup nor the CLI's purge touch them.
//!
//! Prism describes what it wants before launching the agent (an intent, by name); the hook, called
//! by Claude in a separate process, carries it out. Synchronous code: it runs in that hook.

use std::{
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use prism_base::tr;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::collectors::worktrees::{empty_trash, trash_dir};
use crate::git::run_blocking as git;

/// What the hook must create for a given worktree name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Intent {
    pub dir: PathBuf,
    pub branch: String,
    /// Starting point of a new branch (`origin/release/…`); absent, the branch already exists,
    /// locally or on `origin` (an MR's).
    pub base: Option<String>,
}

pub fn intent_path(work: &Path, name: &str) -> PathBuf {
    work.join("pending").join(format!("{name}.json"))
}

pub fn write_intent(work: &Path, name: &str, intent: &Intent) -> Result<()> {
    let path = intent_path(work, name);
    std::fs::create_dir_all(path.parent().unwrap_or(work))?;
    std::fs::write(&path, serde_json::to_vec_pretty(intent)?)?;
    Ok(())
}

/// A worktree name becomes a folder and a branch: letters, digits, `-`, `_`, `.` only.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && name.len() <= 80 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub use prism_core::agents::{slug, worktrees_dir};

/// What `repos` says of the repository `dir` belongs to: the clone itself or one of its worktrees,
/// whose main repository is the parent of their common `.git` folder; outside git, a clone it is in.
pub fn repo_of<T: Copy>(dir: &Path, repos: &[(PathBuf, T)]) -> Option<T> {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let main = git(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .ok()
        .and_then(|common| PathBuf::from(common.trim()).parent().map(real));
    let dir = real(dir);
    let found = |p: &Path| repos.iter().find(|(path, _)| real(path) == p).map(|(_, v)| *v);
    main.as_deref().and_then(found).or_else(|| repos.iter().find(|(path, _)| dir.starts_with(real(path))).map(|(_, v)| *v))
}

/// Without an intent (a subagent isolated in a worktree, for example): Claude's default
/// behavior, in the repository, from HEAD.
pub fn fallback(cwd: &Path, name: &str) -> Intent {
    Intent { dir: cwd.join(".claude/worktrees").join(name), branch: format!("worktree-{name}"), base: Some("HEAD".into()) }
}


/// Creates the worktree described by `intent` from repository `repo`; returns its path. A worktree
/// already present on the right branch is reused as is (agent relaunched with the same name).
pub fn create(repo: &Path, intent: &Intent) -> Result<PathBuf> {
    // An intent is a file in Prism's folder, which any program of this user can write: its
    // branch goes on git's command line.
    branch_name(&intent.branch)?;
    if let Some(base) = &intent.base {
        branch_name(base)?;
    }
    let dir = intent.dir.display().to_string();
    if intent.dir.exists() {
        let current = git(&intent.dir, &["branch", "--show-current"]).unwrap_or_default();
        if current.trim() == intent.branch {
            return Ok(intent.dir.clone());
        }
        bail!(tr!("{dir} already exists and is not on {branch}", dir = dir, branch = intent.branch));
    }
    if let Some(parent) = intent.dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match &intent.base {
        Some(base) => {
            // The freshest base possible; offline, the last known one will do.
            if let Some(remote) = base.strip_prefix("origin/") {
                let _ = git(repo, &["fetch", "--quiet", "origin", remote]);
            }
            git(repo, &["worktree", "add", "-b", &intent.branch, &dir, base])?;
            // The branch it was made from, for its counts and its MR: `worktrees::BASE_KEY`.
            if let Some(remote) = base.strip_prefix("origin/") {
                let _ = git(repo, &["config", &format!("branch.{}.{}", intent.branch, crate::collectors::worktrees::BASE_KEY), remote]);
            }
        }
        None => {
            let _ = git(repo, &["fetch", "--quiet", "origin", &intent.branch]);
            let local = git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{}", intent.branch)]).is_ok();
            if local {
                git(repo, &["worktree", "add", &dir, &intent.branch])?;
            } else {
                git(repo, &["worktree", "add", "--track", "-b", &intent.branch, &dir, &format!("origin/{}", intent.branch)])?;
            }
        }
    }
    Ok(intent.dir.clone())
}

/// A branch to check out, as git would name one: never an option for the git commands it goes
/// into (`--upload-pack=…`), whoever gave it (an MR's, a phone's launch).
pub fn branch_name(branch: &str) -> Result<String> {
    let ok = !branch.is_empty()
        && !branch.starts_with(['-', '/', '.'])
        && !branch.ends_with(['/', '.'])
        && !branch.ends_with(".lock")
        && !branch.contains("..")
        && !branch.contains("@{")
        && !branch.contains("//")
        && !branch.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'));
    anyhow::ensure!(ok, "{branch:?} is not a branch name");
    Ok(branch.to_string())
}

/// What a launch in a new worktree will create, shown before launching: a name already taken (a
/// folder, the branch `<prefix><name>`, an agent starting with it) gives way to the first free
/// one after it (`-2`, `-3`…); an existing branch (an MR's) already checked out elsewhere is
/// refused, as `git worktree add` would.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "WorktreePlan"))]
pub struct Plan {
    pub name: String,
    pub dir: PathBuf,
    pub branch: String,
    /// Why the name asked for is not the one used.
    pub taken: Option<String>,
    /// Why the worktree cannot be created.
    pub error: Option<String>,
    /// The branch on `origin` a new branch starts from (none for an existing branch), and the
    /// others it could: the repository's base and main branch first, then its releases, then the rest.
    pub base: Option<String>,
    pub bases: Vec<String>,
    /// `base` is a local branch, not on `origin` (an agent's, for what it saw in passing).
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub local_base: bool,
}

/// The `Plan` of a worktree named after `wanted` in `root`, from the repository's `branches`.
pub fn plan(wanted: &str, root: &Path, prefix: &str, branch: Option<&str>, Branches { local, checked_out }: &Branches, starting: impl Fn(&str) -> bool) -> Plan {
    let why = |n: &str| {
        if root.join(n).exists() {
            Some(tr!("the folder {path} exists", path = root.join(n).display()))
        } else if starting(n) {
            Some(tr!("an agent is starting in it"))
        } else if branch.is_none() && local.iter().any(|b| *b == format!("{prefix}{n}")) {
            Some(tr!("the branch {branch} exists", branch = format!("{prefix}{n}")))
        } else {
            None
        }
    };
    let taken = why(wanted).map(|w| tr!("{name} is taken: {why}", name = wanted, why = w));
    let name = (0..100).map(|i| if i == 0 { wanted.to_string() } else { format!("{wanted}-{}", i + 1) }).find(|n| why(n).is_none());
    let Some(name) = name else {
        return Plan { name: wanted.into(), dir: root.join(wanted), branch: branch.map_or_else(|| format!("{prefix}{wanted}"), String::from), taken, error: Some(tr!("no free worktree name after {name}", name = wanted)), base: None, bases: Vec::new(), local_base: false };
    };
    let error = branch.and_then(|b| {
        let (_, dir) = checked_out.iter().find(|(c, _)| c == b)?;
        Some(tr!("{branch} is already checked out in {path}: choose it under Existing worktree", branch = b, path = dir.display()))
    });
    Plan { dir: root.join(&name), branch: branch.map_or_else(|| format!("{prefix}{name}"), String::from), name, taken, error, base: None, bases: Vec::new(), local_base: false }
}

/// The branches on `origin` (as last fetched), the first ones first, then the release ones
/// (`release_prefix`, the most recent first), then the others by name.
pub fn origin_branches(repo: &Path, first: &[&str], release_prefix: &str) -> Vec<String> {
    let all: Vec<String> = git(repo, &["for-each-ref", "--format=%(refname:lstrip=3)", "refs/remotes/origin"])
        .unwrap_or_default()
        .lines()
        .filter(|b| !b.is_empty() && *b != "HEAD")
        .map(String::from)
        .collect();
    order_bases(all, first, release_prefix)
}

fn order_bases(mut all: Vec<String>, first: &[&str], release_prefix: &str) -> Vec<String> {
    let rank = |b: &str| match first.iter().position(|f| *f == b) {
        Some(i) => (0, i),
        None if !release_prefix.is_empty() && b.starts_with(release_prefix) => (1, 0),
        None => (2, 0),
    };
    all.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| if rank(a).0 == 1 { b.cmp(a) } else { a.cmp(b) }));
    all.dedup();
    all
}

/// Local branches of a repository, and where each one already checked out is (the clone or one
/// of its worktrees): `git worktree add` refuses to create the first again, and to check out the
/// others twice.
#[derive(Debug, Default)]
pub struct Branches {
    pub local: Vec<String>,
    pub checked_out: Vec<(String, PathBuf)>,
}

pub fn branches(repo: &Path) -> Result<Branches> {
    let local = git(repo, &["for-each-ref", "--format=%(refname:short)", "refs/heads"])?.lines().map(String::from).collect();
    let porcelain = git(repo, &["worktree", "list", "--porcelain"])?;
    let checked_out = porcelain
        .split("\n\n")
        .filter(|b| !b.lines().any(|l| l.starts_with("prunable")))
        .filter_map(|b| {
            let dir = b.lines().find_map(|l| l.strip_prefix("worktree "))?;
            let branch = b.lines().find_map(|l| l.strip_prefix("branch refs/heads/"))?;
            Some((branch.to_string(), PathBuf::from(dir)))
        })
        .collect();
    Ok(Branches { local, checked_out })
}

/// Worktrees of the repository containing `dir` (existing folders only, not those to "prune").
pub fn list(dir: &Path) -> Result<Vec<String>> {
    let porcelain = git(dir, &["worktree", "list", "--porcelain"])?;
    Ok(porcelain
        .split("\n\n")
        .filter(|b| !b.lines().any(|l| l.starts_with("prunable") || l == "bare"))
        .filter_map(|b| b.lines().find_map(|l| l.strip_prefix("worktree ")).map(String::from))
        .collect())
}

/// Why a worktree cannot be removed without loss: uncommitted changes
/// (outside the `.claude/` folders Claude leaves behind), or commits that are on no
/// remote branch. With `merged_at`, the branch's MR has been merged (squash): its
/// commits prior to the merge are delivered, only those after it count.
pub fn keep_reason(path: &Path, merged_at: Option<DateTime<Utc>>) -> Result<Option<String>> {
    let status = git(path, &["status", "--porcelain"])?;
    let changes = status
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !l.strip_prefix("?? ").is_some_and(crate::collectors::worktrees::is_claude_litter))
        .count();
    if changes > 0 {
        return Ok(Some(if changes > 1 { tr!("{n} uncommitted changes", n = changes) } else { tr!("{n} uncommitted change", n = changes) }));
    }
    let since = merged_at.map(|m| format!("--since={}", m.to_rfc3339()));
    let mut args = vec!["rev-list", "--count", "HEAD", "--not", "--remotes"];
    args.extend(since.as_deref());
    let unpushed: usize = git(path, &args)?.trim().parse().unwrap_or(0);
    if unpushed > 0 {
        return Ok(Some(if unpushed > 1 { tr!("{n} unpushed commits", n = unpushed) } else { tr!("{n} unpushed commit", n = unpushed) }));
    }
    Ok(None)
}

/// Removes the worktree if it contains nothing losable, and its local branch (it is on
/// the remote, merged, or carries no commit of its own). Otherwise, returns the reason for refusal. With
/// `force`, at my explicit request: removed anyway, changes and commits included.
pub fn remove(path: &Path, merged_at: Option<DateTime<Utc>>, force: bool) -> Result<Result<(), String>> {
    if !force
        && let Some(reason) = keep_reason(path, merged_at)?
    {
        return Ok(Err(reason));
    }
    let branch = git(path, &["branch", "--show-current"]).unwrap_or_default().trim().to_string();
    // The main repository, from which the removal is done: the parent of the common `.git` folder.
    let common = PathBuf::from(git(path, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?.trim());
    let repo = common.parent().context("locating the main repository")?.to_path_buf();
    let admin = PathBuf::from(git(path, &["rev-parse", "--path-format=absolute", "--git-dir"])?.trim());
    // A linked worktree only, never a repository itself, whatever path a log or a hook gave.
    anyhow::ensure!(admin != common, "{} is a repository, not a worktree: not removed", path.display());
    // Deleting hundreds of thousands of files (node_modules, coverage…) takes minutes:
    // the folder goes to the trash, on the same disk (instant), and is erased in the background.
    // Otherwise (other disk), `git worktree remove`, slower. `--force`: what remains
    // untracked is only `.claude/`, checked above — or the user chose to lose it.
    if !discard(&common, path, &admin) {
        git(&repo, &["worktree", "remove", "--force", &path.display().to_string()])?;
    }
    if !branch.is_empty() {
        let _ = git(&repo, &["branch", "-D", &branch]);
    }
    Ok(Ok(()))
}

/// Moves the worktree and its metadata (`.git/worktrees/<id>`) to the trash, then
/// starts erasing them in the background. `false` if the move is impossible (other disk):
/// nothing has moved.
fn discard(common: &Path, path: &Path, admin: &Path) -> bool {
    if !admin.starts_with(common.join("worktrees")) {
        return false;
    }
    let trash = trash_dir(common);
    if std::fs::create_dir_all(&trash).is_err() {
        return false;
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "worktree".into());
    let target = trash.join(format!("{name}-{stamp}"));
    if std::fs::rename(path, &target).is_err() {
        return false;
    }
    // Without its metadata, git forgets the worktree, like `git worktree prune` for it alone.
    if std::fs::rename(admin, trash.join(format!("{name}-{stamp}.git"))).is_err() {
        let _ = std::fs::remove_dir_all(admin);
    }
    empty_trash(common);
    true
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_come_base_and_main_first_then_releases_newest_first() {
        let all = ["feat/x", "main", "release/Q3-2026", "develop", "release/Q4-2026", "chore/y"].map(String::from).to_vec();
        assert_eq!(order_bases(all, &["release/Q4-2026", "main"], "release/"), ["release/Q4-2026", "main", "release/Q3-2026", "chore/y", "develop", "feat/x"]);
    }

    /// An MR's branch passes; nothing git would read as an option, or refuse, does.
    #[test]
    fn a_branch_is_never_an_option() {
        for ok in ["feature/ACME-12-fix", "fix_it", "release/1.2"] {
            assert_eq!(branch_name(ok).unwrap(), ok);
        }
        for bad in ["--upload-pack=touch /tmp/x", "-b", "", "a b", "a..b", "a:b", "a/", ".hidden", "x.lock", "a@{1}"] {
            assert!(branch_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn slugs_and_names() {
        assert_eq!(slug("ABC-4661 · Export CSV des mesures"), "abc-4661-export-csv-des-mesures");
        assert_eq!(slug("  Révision: état ! "), "revision-etat");
        assert!(slug(&"x".repeat(100)).len() <= 40);
        assert!(valid_name("abc-4661-export"));
        assert!(!valid_name("../x") && !valid_name(".hidden") && !valid_name("a/b") && !valid_name(""));
        assert_eq!(worktrees_dir(Path::new("/dev/app")), Path::new("/dev/app.worktrees"));
    }

    /// A name whose folder or branch exists gives way to the next free one; an MR's branch
    /// already checked out is refused.
    #[test]
    fn plans_a_free_name() {
        let root = std::env::temp_dir().join(format!("prism-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("fix")).unwrap();
        let branches = Branches { local: vec!["agent/vat".into(), "agent/vat-2".into()], checked_out: vec![("feat/x".into(), PathBuf::from("/r.worktrees/x"))] };
        let none = |_: &str| false;
        let p = plan("vat", &root, "agent/", None, &branches, none);
        assert_eq!((p.name.as_str(), p.branch.as_str()), ("vat-3", "agent/vat-3"));
        assert_eq!(p.taken.as_deref(), Some("vat is taken: the branch agent/vat exists"));
        assert!(plan("fix", &root, "agent/", None, &branches, none).taken.unwrap().contains("the folder"));
        assert_eq!(plan("new", &root, "agent/", None, &branches, |n| n == "new").name, "new-2");
        let ok = plan("other", &root, "agent/", None, &branches, none);
        assert!(ok.taken.is_none() && ok.error.is_none() && ok.dir == root.join("other"));
        let mr = plan("vat", &root, "agent/", Some("feat/x"), &branches, none);
        assert_eq!(mr.name, "vat", "an existing branch: its own name is not asked of git");
        assert!(mr.error.unwrap().starts_with("feat/x is already checked out in /r.worktrees/x"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A folder's repository: the clone, a subfolder, one of its worktrees; none outside.
    #[test]
    fn finds_the_repository_of_a_folder() {
        let root = std::env::temp_dir().join(format!("prism-repo-of-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (repo, other) = (root.join("app"), root.join("lib"));
        std::fs::create_dir_all(&root).unwrap();
        for r in [&repo, &other] {
            git(&root, &["init", "-q", "-b", "main", r.to_str().unwrap()]).unwrap();
            git(r, &["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "init"]).unwrap();
        }
        let wt = worktrees_dir(&repo).join("feat");
        git(&repo, &["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]).unwrap();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let repos = [(repo.clone(), 1), (other.clone(), 2)];
        assert_eq!(repo_of(&repo, &repos), Some(1));
        assert_eq!(repo_of(&repo.join("src"), &repos), Some(1));
        assert_eq!(repo_of(&wt, &repos), Some(1), "a worktree is its repository's");
        assert_eq!(repo_of(&other, &repos), Some(2));
        assert_eq!(repo_of(&root, &repos), None);
        let b = branches(&repo).unwrap();
        assert!(b.local.contains(&"main".to_string()) && b.local.contains(&"feat".to_string()));
        let real = |p: &Path| std::fs::canonicalize(p).unwrap();
        assert!(b.checked_out.iter().any(|(br, dir)| br == "feat" && real(dir) == real(&wt)), "{:?}", b.checked_out);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Creation from a base, reuse, removal refused with changes, then
    /// clean removal; and a worktree on a branch that only exists on the remote.
    #[test]
    fn creates_keeps_and_removes() {
        let root = std::env::temp_dir().join(format!("prism-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (origin, repo) = (root.join("origin.git"), root.join("app"));
        std::fs::create_dir_all(&root).unwrap();
        let sh = |dir: &Path, args: &[&str]| git(dir, args).unwrap();
        sh(&root, &["init", "-q", "--bare", origin.to_str().unwrap()]);
        sh(&root, &["init", "-q", "-b", "main", repo.to_str().unwrap()]);
        for args in [
            &["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "init"][..],
            &["remote", "add", "origin", origin.to_str().unwrap()],
            &["push", "-q", "origin", "main", "main:release/T1", "main:feat/mr-branch"],
            &["fetch", "-q", "origin"],
        ] {
            sh(&repo, args);
        }

        let dir = worktrees_dir(&repo).join("abc-1-test");
        let intent = Intent { dir: dir.clone(), branch: "claude/abc-1-test".into(), base: Some("origin/release/T1".into()) };
        assert_eq!(create(&repo, &intent).unwrap(), dir);
        assert_eq!(git(&dir, &["branch", "--show-current"]).unwrap().trim(), "claude/abc-1-test");
        assert_eq!(create(&repo, &intent).unwrap(), dir, "an existing worktree on the right branch is reused");

        std::fs::write(dir.join("new.txt"), "x").unwrap();
        assert_eq!(remove(&dir, None, false).unwrap(), Err("1 uncommitted change".into()));
        std::fs::create_dir_all(dir.join(".claude/agent-memory")).unwrap();
        std::fs::write(dir.join(".claude/agent-memory/m.md"), "x").unwrap();
        std::fs::remove_file(dir.join("new.txt")).unwrap();
        assert_eq!(remove(&dir, None, false).unwrap(), Ok(()), ".claude/ litter does not keep a worktree");
        assert!(!dir.exists());
        assert!(git(&repo, &["rev-parse", "--verify", "--quiet", "refs/heads/claude/abc-1-test"]).is_err());

        // Commit never pushed, but prior to the MR's merge (squash): delivered.
        let sq = worktrees_dir(&repo).join("squashed");
        create(&repo, &Intent { dir: sq.clone(), branch: "claude/squashed".into(), base: Some("origin/release/T1".into()) }).unwrap();
        sh(&sq, &["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "work"]);
        assert_eq!(remove(&sq, None, false).unwrap(), Err("1 unpushed commit".into()));
        let merged = Utc::now() + chrono::Duration::seconds(5);
        assert_eq!(remove(&sq, Some(merged), false).unwrap(), Ok(()));

        // Changes and a commit never pushed: kept, unless forced.
        let lost = worktrees_dir(&repo).join("lost");
        create(&repo, &Intent { dir: lost.clone(), branch: "claude/lost".into(), base: Some("origin/release/T1".into()) }).unwrap();
        sh(&lost, &["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-q", "--allow-empty", "-m", "work"]);
        std::fs::write(lost.join("draft.txt"), "x").unwrap();
        assert_eq!(remove(&lost, None, false).unwrap(), Err("1 uncommitted change".into()));
        assert_eq!(remove(&lost, None, true).unwrap(), Ok(()));
        assert!(!lost.exists());
        assert!(!git(&repo, &["worktree", "list"]).unwrap().contains("lost"), "git forgets the worktree at once");
        assert!(git(&repo, &["rev-parse", "--verify", "--quiet", "refs/heads/claude/lost"]).is_err());

        let mr = worktrees_dir(&repo).join("mr");
        create(&repo, &Intent { dir: mr.clone(), branch: "feat/mr-branch".into(), base: None }).unwrap();
        assert_eq!(git(&mr, &["branch", "--show-current"]).unwrap().trim(), "feat/mr-branch");
        let _ = std::fs::remove_dir_all(&root);
    }
}
