//! The config's repositories that have a local clone (`[[repos]]`, the primary one first): current branch,
//! local changes and gap with the remote's default branch, for the Repos menu.
//! Read locally at a short interval; a fetch of the default branch every 2 min.

use std::{path::Path, time::Duration};

use crate::{
    config::RepoConfig,
    state::{Hub, Repo},
};

use crate::git::run as git;

const SOURCE: &str = "repos";
const POLL: Duration = Duration::from_secs(10);
const FETCH_EVERY: u64 = 12;

/// `worktrees`: the worktrees collector already fetches the repositories whose worktrees it lists.
pub async fn run(repos: Vec<RepoConfig>, worktrees: bool, hub: Hub) {
    for n in 0u64..=u64::MAX {
        if n > 0 {
            hub.pause(POLL).await;
        }
        let mut list = Vec::new();
        for r in &repos {
            list.push(collect(r).await);
        }
        hub.update(|s| {
            let changed = s.repos != list;
            s.repos = list;
            changed
        });
        hub.set_error(SOURCE, None);
        if n % FETCH_EVERY == 0 {
            for r in repos.iter().filter(|r| !(worktrees && r.has_worktrees())) {
                if !prism_base::git::has_origin(r.dir()).await {
                    continue;
                }
                let base = default_branch(r.dir()).await;
                if let Err(e) = git(r.dir(), &["fetch", "--quiet", "origin", &base]).await {
                    tracing::warn!("repos: fetch {}: {e:#}", r.name);
                }
            }
        }
    }
}

async fn collect(r: &RepoConfig) -> Repo {
    let mut repo = Repo {
        name: r.name.clone(),
        path: r.dir().display().to_string(),
        project: r.project.clone(),
        branch: None,
        base: crate::conventions::get().main_branch.clone(),
        behind: 0,
        ahead: 0,
        dirty: false,
        is_main: r.primary,
        error: None,
        forge: r.forge,
    };
    let branch = match git(r.dir(), &["rev-parse", "--abbrev-ref", "HEAD"]).await {
        Ok(b) => b.trim().to_string(),
        Err(e) => {
            repo.error = Some(if r.dir().exists() { format!("{e:#}") } else { format!("{} not found", r.dir().display()) });
            return repo;
        }
    };
    repo.branch = (branch != "HEAD").then_some(branch);
    repo.base = default_branch(r.dir()).await;
    if let Ok(counts) = git(r.dir(), &["rev-list", "--left-right", "--count", &format!("origin/{}...HEAD", repo.base)]).await {
        let mut it = counts.split_whitespace().map(|n| n.parse().unwrap_or(0));
        repo.behind = it.next().unwrap_or(0);
        repo.ahead = it.next().unwrap_or(0);
    }
    repo.dirty = git(r.dir(), &["status", "--porcelain", "--untracked-files=no"]).await.is_ok_and(|s| !s.trim().is_empty());
    repo
}

/// Default branch of the remote (`origin/HEAD`), otherwise `main`.
pub async fn default_branch(dir: &Path) -> String {
    git(dir, &["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .await
        .ok()
        .and_then(|r| r.trim().strip_prefix("origin/").map(String::from))
        .unwrap_or_else(|| crate::conventions::get().main_branch.clone())
}
