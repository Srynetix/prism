//! A project's local branches (Projects › Branches, `prism_agents::branches`): listed with their
//! verdict, deleted one by one (its worktree with it), a worktree opened on one, the clone switched
//! to one, and a tidy: origin pruned, then the safe ones deleted, each checked again.

use std::path::PathBuf;

use prism_base::{i18n::tr, tr};
use serde::Serialize;

use super::{Cmd, State};
use crate::agents::branches::{self, Facts, LocalBranch, Need};
use crate::app::AppState;
use crate::state::{AgentState, MrState};

/// A project's clone and what the snapshot says of its branches.
fn clone_of(state: &AppState, repo: &str) -> Result<(PathBuf, Facts), String> {
    let snap = state.hub.subscribe();
    let snap = snap.borrow();
    let r = snap.repos.iter().find(|r| r.name == repo && r.error.is_none()).ok_or_else(|| tr!("{repo}: no clone of it on this computer", repo = repo))?;
    let mut mrs: std::collections::HashMap<String, (Option<chrono::DateTime<chrono::Utc>>, bool)> = Default::default();
    for m in snap.merge_requests.iter().filter(|m| snap.mr_in_repo(m, repo)) {
        let e = mrs.entry(m.source_branch.clone()).or_default();
        if m.state == MrState::Opened {
            e.1 = true;
        }
        e.0 = e.0.max(m.merged_at);
    }
    for (merged, open) in mrs.values_mut() {
        if *open {
            *merged = None;
        }
    }
    // An agent at work in a folder: its worktree, one it adopted, its other repositories' (`also`),
    // or below it.
    let mut busy = std::collections::HashMap::new();
    for w in snap.worktrees.iter().filter(|w| w.repo == repo) {
        let inside = |dir: &str| dir == w.path || dir.starts_with(&format!("{}/", w.path));
        if let Some(a) = snap.agents.iter().find(|a| a.state == AgentState::Running && (a.worktree.as_deref() == Some(&w.path) || a.adopted.contains(&w.path) || a.also.contains(&w.path) || inside(&a.cwd))) {
            busy.insert(w.path.clone(), a.title.clone());
        }
    }
    Ok((PathBuf::from(&r.path), Facts { base: r.base.clone(), conventions: snap.conventions.clone(), mrs, busy }))
}

async fn list(state: &AppState, repo: &str, need: Need) -> Cmd<(PathBuf, Vec<LocalBranch>)> {
    let (path, facts) = clone_of(state, repo)?;
    let all = branches::list(&path, &facts, need).await?;
    Ok((path, all))
}

/// A branch deleted, its worktree with it when it has one (`remove_worktree`'s checks).
async fn delete_one(state: &AppState, path: &std::path::Path, b: &LocalBranch, force: bool) -> Cmd {
    if let Some(why) = b.locked.as_ref().or(b.keep.as_ref().filter(|_| !force)) {
        return Err(tr!("{branch} is kept: {why}", branch = b.name, why = why).into());
    }
    match &b.worktree {
        // Its worktree goes with it; one whose folder is gone, the branch alone (`branches::delete`).
        Some(w) if std::path::Path::new(w).is_dir() => super::agents::remove_worktree(state, w.clone(), force).await,
        _ => branches::delete(path, b, force).await.map_err(Into::into),
    }
}

/// A project's local branches, most recent first, with where each stands and its verdict.
#[prism_macros::command]
pub async fn project_branches(state: State<'_, AppState>, repo: String) -> Cmd<Vec<LocalBranch>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    Ok(list(&state, &repo, Need::Shown).await?.1)
}

/// A project's branch deleted, its worktree with it: nothing to lose, or `force` (never a locked
/// one: protected, the clone's, one an agent works on).
#[prism_macros::command]
pub async fn project_branch_delete(state: State<'_, AppState>, repo: String, branch: String, force: bool) -> Cmd {
    let (path, all) = list(&state, &repo, Need::Action).await?;
    let b = all.iter().find(|b| b.name == branch).ok_or_else(|| tr!("{branch}: no such branch in {repo}", branch = branch, repo = repo))?;
    delete_one(&state, &path, b, force).await?;
    tracing::info!("front: branch {branch} of {repo} deleted{}", if force { " (forced)" } else { "" });
    Ok(())
}

/// A worktree opened on a project's branch, next to its clone, without an agent: its path.
#[prism_macros::command]
pub async fn project_branch_worktree(state: State<'_, AppState>, repo: String, branch: String) -> Cmd<String> {
    let (path, all) = list(&state, &repo, Need::Action).await?;
    let b = all.iter().find(|b| b.name == branch).ok_or_else(|| tr!("{branch}: no such branch in {repo}", branch = branch, repo = repo))?;
    if let Some(w) = &b.worktree {
        return Err(tr!("{branch} is already checked out in {path}", branch = branch, path = w).into());
    }
    let made = tokio::task::spawn_blocking(move || branches::add_worktree(&path, &branch)).await??;
    tracing::info!("front: worktree {} opened for a branch of {repo}", made.display());
    Ok(made.display().to_string())
}

/// A project's clone switched to one of its branches: nothing uncommitted, no agent in it.
#[prism_macros::command]
pub async fn project_switch(state: State<'_, AppState>, repo: String, branch: String) -> Cmd<String> {
    let (path, _) = clone_of(&state, &repo)?;
    state.agents.folder_switch(&path, &branch).await.map_err(Into::into)
}

/// A tidy's first step: origin read again and pruned, worktrees whose folder is gone forgotten;
/// the branches as they now stand (the safe ones are what `project_tidy` deletes).
#[prism_macros::command]
pub async fn project_tidy_prepare(state: State<'_, AppState>, repo: String) -> Cmd<Vec<LocalBranch>> {
    let (path, _) = clone_of(&state, &repo)?;
    branches::refresh(&path).await?;
    Ok(list(&state, &repo, Need::Action).await?.1)
}

/// What a tidy did: the branches deleted, those kept and why.
#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TidyReport {
    pub deleted: Vec<String>,
    pub kept: Vec<TidyKept>,
}

#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TidyKept {
    pub branch: String,
    pub why: String,
}

/// A tidy: the branches picked from its list deleted (their worktrees with them), each checked
/// again and only when safe (done, nothing to lose, not locked). Never forced.
#[prism_macros::command]
pub async fn project_tidy(state: State<'_, AppState>, repo: String, branches: Vec<String>) -> Cmd<TidyReport> {
    let (path, all) = list(&state, &repo, Need::Action).await?;
    let mut report = TidyReport { deleted: Vec::new(), kept: Vec::new() };
    for name in branches {
        let Some(b) = all.iter().find(|b| b.name == name) else {
            report.kept.push(TidyKept { branch: name, why: tr("no such branch any more").into() });
            continue;
        };
        if !b.safe() {
            let why = b.locked.clone().or(b.keep.clone()).unwrap_or_else(|| tr("not done yet").into());
            report.kept.push(TidyKept { branch: name, why });
            continue;
        }
        match delete_one(&state, &path, b, false).await {
            Ok(()) => report.deleted.push(name),
            Err(e) => report.kept.push(TidyKept { branch: name, why: e.to_string() }),
        }
    }
    tracing::info!("front: tidy of {repo}: {} deleted, {} kept", report.deleted.len(), report.kept.len());
    Ok(report)
}
