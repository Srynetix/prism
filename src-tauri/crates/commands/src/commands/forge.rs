//! Merge requests and pipelines, whatever the forge.


use super::State;


use crate::app::AppState;

use super::{gitlab, Cmd};

/// Rebase of a GitLab MR, requested from the window.
#[prism_macros::command]
pub async fn rebase_mr(state: State<'_, AppState>, repo: Option<String>, iid: u64) -> Cmd {
    state.forge().rebase_mr(&state.hub, repo.as_deref(), iid).await.map_err(Into::into)
}

/// Merge queue label set or removed on a GitLab MR.
#[prism_macros::command]
pub async fn set_merge_queue(state: State<'_, AppState>, repo: Option<String>, iid: u64, on: bool) -> Cmd {
    gitlab(&state)?.set_merge_queue(&state.hub, repo.as_deref(), iid, on).await?;
    state.hub.forge_changed();
    Ok(())
}

/// Merge priority label set (`priority`: its number) or removed (`None`) on a GitLab MR.
#[prism_macros::command]
pub async fn set_merge_priority(state: State<'_, AppState>, repo: Option<String>, iid: u64, priority: Option<u32>) -> Cmd {
    gitlab(&state)?.set_merge_priority(&state.hub, repo.as_deref(), iid, priority).await?;
    state.hub.forge_changed();
    Ok(())
}

/// Merges a GitLab MR now, from the window (two clicks: it cannot be undone).
#[prism_macros::command]
pub async fn merge_mr(state: State<'_, AppState>, repo: Option<String>, iid: u64) -> Cmd {
    state.forge().merge_mr(&state.hub, repo.as_deref(), iid).await.map_err(Into::into)
}

/// Auto-merge of a GitLab MR, turned on or off from the window.
#[prism_macros::command]
pub async fn set_auto_merge(state: State<'_, AppState>, repo: Option<String>, iid: u64, enable: bool) -> Cmd {
    state.forge().set_auto_merge(&state.hub, repo.as_deref(), iid, enable).await.map_err(Into::into)
}

/// Description, discussions and approvals of an MR, read when its pane opens.
#[prism_macros::command]
pub async fn mr_detail(state: State<'_, AppState>, repo: Option<String>, iid: u64) -> Cmd<crate::forge::MrDetail> {
    if crate::demo::active() {
        return crate::demo::details::mr_detail(&state.hub.subscribe().borrow(), repo.as_deref(), iid).map_err(Into::into);
    }
    state.forge().mr_detail(&state.hub, repo.as_deref(), iid).await.map_err(Into::into)
}

/// Diff of an MR, read when its tab is shown.
#[prism_macros::command]
pub async fn mr_diff(state: State<'_, AppState>, repo: Option<String>, iid: u64) -> Cmd<crate::forge::MrDiff> {
    if crate::demo::active() {
        return crate::demo::details::mr_diff(&state.hub.subscribe().borrow(), repo.as_deref(), iid).map_err(Into::into);
    }
    state.forge().mr_diff(&state.hub, repo.as_deref(), iid).await.map_err(Into::into)
}

/// Commits of an MR, newest first, read when its tab is shown.
#[prism_macros::command]
pub async fn mr_commits(state: State<'_, AppState>, repo: Option<String>, iid: u64) -> Cmd<Vec<crate::forge::MrCommit>> {
    if crate::demo::active() {
        return crate::demo::details::mr_commits(&state.hub.subscribe().borrow(), repo.as_deref(), iid).map_err(Into::into);
    }
    state.forge().mr_commits(&state.hub, repo.as_deref(), iid).await.map_err(Into::into)
}

/// A file of an MR, whole, on its source branch: the Markdown preview of the diff.
#[prism_macros::command]
pub async fn mr_file(state: State<'_, AppState>, repo: Option<String>, iid: u64, path: String) -> Cmd<String> {
    if crate::demo::active() {
        return crate::demo::details::mr_file(&state.hub.subscribe().borrow(), repo.as_deref(), iid, &path).map_err(Into::into);
    }
    state.forge().mr_file(&state.hub, repo.as_deref(), iid, &path).await.map_err(Into::into)
}

/// An image of an MR's diff, as a `data:` URL: its new version, or (`old`) the target branch's.
#[prism_macros::command]
pub async fn mr_image(state: State<'_, AppState>, repo: Option<String>, iid: u64, path: String, old: bool) -> Cmd<String> {
    if crate::demo::active() {
        return crate::demo::details::mr_image(&state.hub.subscribe().borrow(), repo.as_deref(), iid, &path, old).map_err(Into::into);
    }
    state.forge().mr_image(&state.hub, repo.as_deref(), iid, &path, old).await.map_err(Into::into)
}

/// Approve an MR, or remove my approval.
#[prism_macros::command]
pub async fn approve_mr(state: State<'_, AppState>, repo: Option<String>, iid: u64, approve: bool) -> Cmd {
    state.forge().approve_mr(&state.hub, repo.as_deref(), iid, approve).await.map_err(Into::into)
}

/// Request changes on an MR (with a comment), or take my request back.
#[prism_macros::command]
pub async fn request_changes_mr(state: State<'_, AppState>, repo: Option<String>, iid: u64, on: bool, body: String) -> Cmd {
    state.forge().request_changes(&state.hub, repo.as_deref(), iid, on, &body).await.map_err(Into::into)
}

/// Comment on an MR, or reply to one of its threads.
#[prism_macros::command]
pub async fn comment_mr(state: State<'_, AppState>, repo: Option<String>, iid: u64, discussion: Option<String>, body: String) -> Cmd {
    state.forge().comment_mr(&state.hub, repo.as_deref(), iid, discussion.as_deref(), &body).await.map_err(Into::into)
}

/// New thread on a line of an MR's diff.
#[prism_macros::command]
pub async fn comment_line(state: State<'_, AppState>, repo: Option<String>, iid: u64, line: crate::forge::LineRef, body: String) -> Cmd {
    state.forge().comment_line(&state.hub, repo.as_deref(), iid, line, &body).await.map_err(Into::into)
}

/// Resolve a discussion thread of an MR, or reopen it.
#[prism_macros::command]
pub async fn resolve_thread(state: State<'_, AppState>, repo: Option<String>, iid: u64, discussion: String, resolved: bool) -> Cmd {
    state.forge().resolve_thread(&state.hub, repo.as_deref(), iid, &discussion, resolved).await.map_err(Into::into)
}

/// Mark an MR as draft, or mark it ready.
#[prism_macros::command]
pub async fn set_draft(state: State<'_, AppState>, repo: Option<String>, iid: u64, draft: bool) -> Cmd {
    state.forge().set_draft(&state.hub, repo.as_deref(), iid, draft).await.map_err(Into::into)
}

/// Members of an MR's project (`repo`: a non-primary repository): the assignee and reviewer choices.
#[prism_macros::command]
pub async fn forge_members(state: State<'_, AppState>, repo: Option<String>) -> Cmd<Vec<crate::forge::Member>> {
    if crate::demo::active() {
        return Ok(crate::demo::details::members());
    }
    state.forge().members(repo.as_deref()).await.map_err(Into::into)
}

/// Assignees or reviewers of an open MR (the whole list; empty: nobody).
#[prism_macros::command]
pub async fn set_mr_people(state: State<'_, AppState>, repo: Option<String>, iid: u64, reviewers: bool, usernames: Vec<String>) -> Cmd {
    state.forge().set_people(&state.hub, repo.as_deref(), iid, reviewers, usernames).await.map_err(Into::into)
}

/// Labels of an MR's project (`repo`: a non-primary repository): the choices for its labels.
#[prism_macros::command]
pub async fn forge_labels(state: State<'_, AppState>, repo: Option<String>) -> Cmd<Vec<crate::forge::ForgeLabel>> {
    if crate::demo::active() {
        return Ok(crate::demo::details::labels(&state.hub.subscribe().borrow()));
    }
    state.forge().labels(repo.as_deref()).await.map_err(Into::into)
}

/// A repository's followed branches as its forge has them, from what Settings › Repositories shows,
/// saved or not: its project, main branch (empty: the project's default one), release prefix and
/// list (empty: the main and release ones); and the entries the forge has no branch for.
#[prism_macros::command]
pub async fn pipeline_branches(state: State<'_, AppState>, repo: String, project: String, main_branch: String, release_prefix: String, branches: Vec<String>) -> Cmd<crate::config::PipelineBranches> {
    let target = crate::config::PipelineRepo { name: repo.clone(), repo: Some(repo), project, main_branch, release_prefix, branches };
    if crate::demo::active() {
        return Ok(crate::collectors::forge::branches::resolve_against(&target, crate::demo::BRANCHES, "main"));
    }
    let got = match state.forge().provider() {
        crate::forge::Provider::Gitlab => crate::collectors::gitlab::pipeline_branches(&target).await,
        crate::forge::Provider::Github => crate::collectors::github::pipeline_branches(&target).await,
    };
    got.map_err(Into::into)
}

/// Labels of an open MR (the whole list; empty: none).
#[prism_macros::command]
pub async fn set_mr_labels(state: State<'_, AppState>, repo: Option<String>, iid: u64, labels: Vec<String>) -> Cmd {
    state.forge().set_labels(&state.hub, repo.as_deref(), iid, labels).await.map_err(Into::into)
}

/// The custom emoji of a forge text's project (GitLab groups'; none on GitHub nor in the demo).
#[prism_macros::command]
pub async fn forge_custom_emoji(state: State<'_, AppState>, project: Option<String>) -> Cmd<Vec<crate::forge::CustomEmoji>> {
    if crate::demo::active() {
        return Ok(Vec::new());
    }
    state.forge().custom_emoji(project.as_deref()).await.map_err(Into::into)
}

/// A custom emoji's picture, as a `data:` URL: only from the GitLab instance.
#[prism_macros::command]
pub async fn forge_emoji_image(url: String) -> Cmd<String> {
    crate::forge::emoji_image(&url, &crate::conventions::get().gitlab_host).await.map_err(Into::into)
}

/// Image attached to a text of the forge (description, comment), as a `data:` URL.
#[prism_macros::command]
pub async fn forge_upload(state: State<'_, AppState>, project: Option<String>, path: String) -> Cmd<String> {
    state.forge().upload(project.as_deref(), &path).await.map_err(Into::into)
}

/// A person's profile picture on the forge (MR author, member), as a `data:` URL.
#[prism_macros::command]
pub async fn forge_avatar(url: String) -> Cmd<String> {
    crate::forge::avatar(&url, &crate::conventions::get().gitlab_host).await.map_err(Into::into)
}

/// Jobs of a GitLab pipeline, read when the logs pane opens.
#[prism_macros::command]
pub async fn pipeline_jobs(state: State<'_, AppState>, project: Option<String>, pipeline: u64) -> Cmd<Vec<crate::forge::JobInfo>> {
    if crate::demo::active() {
        return crate::demo::details::pipeline_jobs(&state.hub.subscribe().borrow(), pipeline).map_err(Into::into);
    }
    state.forge().pipeline_jobs(project.as_deref(), pipeline).await.map_err(Into::into)
}

/// Retry of a failed or canceled GitLab job, from an MR's details or the logs pane.
#[prism_macros::command]
pub async fn retry_job(state: State<'_, AppState>, project: Option<String>, job: u64) -> Cmd<crate::forge::JobInfo> {
    state.forge().retry_job(&state.hub, project.as_deref(), job).await.map_err(Into::into)
}

/// Log of a GitLab job, read on demand (and reread while the job runs).
#[prism_macros::command]
pub async fn job_log(state: State<'_, AppState>, project: Option<String>, job: u64) -> Cmd<crate::forge::JobLog> {
    if crate::demo::active() {
        return crate::demo::details::job_log(&state.hub.subscribe().borrow(), job).map_err(Into::into);
    }
    state.forge().job_log(project.as_deref(), job).await.map_err(Into::into)
}

/// Immediate run of the Renovate schedule.
#[prism_macros::command]
pub async fn play_renovate(state: State<'_, AppState>) -> Cmd {
    state.forge().play_renovate(&state.hub).await.map_err(Into::into)
}

/// Checking an item of Renovate's Dependency Dashboard (`action=branch`).
#[prism_macros::command]
pub async fn check_renovate(state: State<'_, AppState>, marker: String) -> Cmd {
    state.forge().check_renovate(&state.hub, &marker).await.map_err(Into::into)
}

/// Run of a manual GitLab job, requested from the window.
#[prism_macros::command]
pub async fn play_job(state: State<'_, AppState>, id: u64) -> Cmd {
    state.forge().play_job(&state.hub, id).await.map_err(Into::into)
}

/// Retry of a GitLab pipeline's failed or canceled jobs, requested from the window.
#[prism_macros::command]
pub async fn retry_pipeline(state: State<'_, AppState>, id: u64) -> Cmd {
    state.forge().retry_pipeline(&state.hub, id).await.map_err(Into::into)
}

/// A new pipeline for an open GitLab MR, requested from the window (its rules read the MR's labels now).
#[prism_macros::command]
pub async fn run_mr_pipeline(state: State<'_, AppState>, repo: Option<String>, iid: u64) -> Cmd {
    state.forge().run_mr_pipeline(&state.hub, repo.as_deref(), iid).await.map_err(Into::into)
}

/// Cancel of a GitLab pipeline, requested from the window.
#[prism_macros::command]
pub async fn cancel_pipeline(state: State<'_, AppState>, id: u64) -> Cmd {
    state.forge().cancel_pipeline(&state.hub, id).await.map_err(Into::into)
}
