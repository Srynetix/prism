//! What the window does to a pull request: rebase, merge, auto-merge, approve, comment, resolve
//! a thread, draft, assignees and reviewers. Each one only on a pull request Prism displays, in
//! the state the action needs; the snapshot follows right away, without waiting for the next
//! collection.

use prism_base::{i18n::tr, tr};
use anyhow::{bail, ensure, Context, Result};
use chrono::Utc;
use serde::Deserialize;

use super::{approvals_left, approvers, gh_bytes, gh_input, graphql, review::known, reviewers, split, Login, Nodes, Opinion, ReviewRequest, Var};
use crate::state::MrState;
use crate::{forge::LineRef, state::Hub};

const STATE_QUERY: &str = include_str!("pull_request_state.graphql");

#[derive(Deserialize)]
struct Data {
    repository: Option<Repository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    viewer_default_merge_method: String,
    pull_request: Option<PrState>,
}

/// A pull request as it is now on GitHub: its id for the mutations, and what the snapshot shows.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrState {
    id: String,
    state: String,
    is_draft: bool,
    title: String,
    head_ref_oid: String,
    review_decision: Option<String>,
    #[serde(default)]
    base_ref: Option<super::BaseRef>,
    author: Option<Login>,
    auto_merge_request: Option<serde_json::Value>,
    assignees: Nodes<Login>,
    review_requests: Nodes<ReviewRequest>,
    latest_opinionated_reviews: Nodes<Opinion>,
}

/// The pull request, re-read from GitHub, and the repository's merge method for me.
async fn state(project: &str, number: u64) -> Result<(PrState, String)> {
    let (owner, name) = split(project)?;
    let data: Data = graphql(STATE_QUERY, &[("owner", owner.into()), ("name", name.into()), ("number", Var::Raw(number.to_string()))]).await?;
    let repo = data.repository.context(tr("repository not found or not accessible"))?;
    let pr = repo.pull_request.with_context(|| tr!("#{number} not found", number = number))?;
    Ok((pr, repo.viewer_default_merge_method))
}

/// An open pull request Prism displays, as GitHub has it now.
async fn open(project: &str, hub: &Hub, repo: Option<&str>, number: u64) -> Result<(PrState, String)> {
    let shown = known(hub, repo, number)?;
    ensure!(shown.state == MrState::Opened, "{}", tr!("#{number} is not an open pull request", number = number));
    let (pr, method) = state(project, number).await?;
    ensure!(pr.state == "OPEN", "{}", tr!("#{number} is no longer open on GitHub", number = number));
    Ok((pr, method))
}

/// A mutation on the pull request; GitHub's answer is not used.
async fn mutate(mutation: &str, vars: &[(&str, Var<'_>)]) -> Result<()> {
    let _: serde_json::Value = graphql(mutation, vars).await?;
    Ok(())
}

/// The snapshot's pull request after a change: its people, approvals and draft state as GitHub has them.
fn follow(hub: &Hub, repo: Option<&str>, number: u64, pr: PrState) {
    let author = pr.author.map(|a| a.login).unwrap_or_default();
    let approved_by = approvers(&pr.latest_opinionated_reviews.nodes);
    let changes_requested_by = super::requesters(&pr.latest_opinionated_reviews.nodes);
    let reviewers = reviewers(pr.review_requests.nodes, &pr.latest_opinionated_reviews.nodes, &author);
    let assignees: Vec<String> = pr.assignees.nodes.into_iter().map(|a| a.login).collect();
    let left = approvals_left(pr.review_decision.as_deref(), pr.base_ref.as_ref().and_then(super::BaseRef::required), approved_by.len());
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| m.iid == number && m.repo.as_deref() == repo) else { return false };
        let changed = m.approved_by != approved_by || m.changes_requested_by != changes_requested_by || m.reviewers != reviewers || m.assignees != assignees || m.approvals_left != left || m.draft != pr.is_draft || m.title != pr.title || m.auto_merge != pr.auto_merge_request.is_some();
        m.approved_by = approved_by;
        m.changes_requested_by = changes_requested_by;
        m.reviewers = reviewers;
        m.assignees = assignees;
        m.approvals_left = left;
        m.draft = pr.is_draft;
        m.title = pr.title;
        m.auto_merge = pr.auto_merge_request.is_some();
        changed
    });
}

/// Rebases the pull request's branch on its base, on GitHub's side. Only one Prism sees behind
/// its base and without conflict.
pub async fn rebase(project: &str, hub: &Hub, repo: Option<&str>, number: u64) -> Result<()> {
    let m = known(hub, repo, number)?;
    ensure!(m.state == MrState::Opened && !m.conflicts && (m.needs_rebase || m.merge_status == "need_rebase"), "{}", tr!("#{number} is not an open pull request that needs a rebase", number = number));
    let (pr, _) = open(project, hub, repo, number).await?;
    tracing::info!("github: rebase #{number}");
    mutate(
        "mutation($id: ID!, $head: GitObjectID!) { updatePullRequestBranch(input: {pullRequestId: $id, expectedHeadOid: $head, updateMethod: REBASE}) { clientMutationId } }",
        &[("id", pr.id.as_str().into()), ("head", pr.head_ref_oid.as_str().into())],
    )
    .await?;
    // The branch moves: the pull request is no longer "to rebase" while waiting for the collection.
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| m.iid == number && m.repo.as_deref() == repo) else { return false };
        m.needs_rebase = false;
        m.merge_status = "checking".into();
        true
    });
    Ok(())
}

/// Merges the pull request now, with the method GitHub offers me for this repository. Only one
/// GitHub sees as mergeable; it checks again, against the head Prism saw.
pub async fn merge(project: &str, hub: &Hub, repo: Option<&str>, number: u64) -> Result<()> {
    let m = known(hub, repo, number)?;
    ensure!(m.state == MrState::Opened && !m.draft && m.merge_status == "mergeable", "{}", tr!("#{number} is not an open pull request ready to merge", number = number));
    let (pr, method) = open(project, hub, repo, number).await?;
    tracing::info!("github: merge #{number} ({method})");
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Merged {
        merge_pull_request: Option<Payload>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Payload {
        pull_request: Option<StateOnly>,
    }
    #[derive(Deserialize)]
    struct StateOnly {
        state: String,
    }
    let merged: Merged = graphql(
        "mutation($id: ID!, $head: GitObjectID!, $method: PullRequestMergeMethod!) { mergePullRequest(input: {pullRequestId: $id, expectedHeadOid: $head, mergeMethod: $method}) { pullRequest { state } } }",
        &[("id", pr.id.as_str().into()), ("head", pr.head_ref_oid.as_str().into()), ("method", method.as_str().into())],
    )
    .await?;
    let state = merged.merge_pull_request.and_then(|p| p.pull_request).map(|p| p.state).unwrap_or_default();
    ensure!(state == "MERGED", "{}", tr!("GitHub did not merge #{number} (state: {state})", number = number, state = state));
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| m.iid == number && m.repo.as_deref() == repo) else { return false };
        m.state = MrState::Merged;
        m.merged_at = Some(Utc::now());
        m.auto_merge = false;
        true
    });
    Ok(())
}

/// Enables or disables auto-merge. GitHub refuses it on a pull request already mergeable: it is
/// then merged right away, as GitLab does.
pub async fn set_auto_merge(project: &str, hub: &Hub, repo: Option<&str>, number: u64, enable: bool) -> Result<()> {
    let m = known(hub, repo, number)?;
    let ok = m.state == MrState::Opened && if enable { !m.draft && !m.conflicts && !m.auto_merge } else { m.auto_merge };
    if !ok {
        if enable {
            bail!("{}", tr!("#{number} is not an open, non-draft pull request without conflicts and auto-merge", number = number));
        }
        bail!("{}", tr!("#{number} has no auto-merge to cancel", number = number));
    }
    if enable && m.merge_status == "mergeable" {
        return merge(project, hub, repo, number).await;
    }
    let (pr, method) = open(project, hub, repo, number).await?;
    tracing::info!("github: auto-merge #{number} {}", if enable { "on" } else { "off" });
    if enable {
        mutate(
            "mutation($id: ID!, $method: PullRequestMergeMethod!) { enablePullRequestAutoMerge(input: {pullRequestId: $id, mergeMethod: $method}) { clientMutationId } }",
            &[("id", pr.id.as_str().into()), ("method", method.as_str().into())],
        )
        .await?;
    } else {
        mutate("mutation($id: ID!) { disablePullRequestAutoMerge(input: {pullRequestId: $id}) { clientMutationId } }", &[("id", pr.id.as_str().into())]).await?;
    }
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| m.iid == number && m.repo.as_deref() == repo) else { return false };
        m.auto_merge = enable;
        true
    });
    Ok(())
}

/// Approves the pull request. GitHub has no taking back of an approval: only dismissing the
/// review, which needs rights on the repository, on GitHub.
pub async fn approve(project: &str, hub: &Hub, repo: Option<&str>, number: u64, on: bool) -> Result<()> {
    if !on {
        bail!("{}", tr("GitHub cannot take back an approval: dismiss your review on GitHub"));
    }
    let (pr, _) = open(project, hub, repo, number).await?;
    tracing::info!("github: approve #{number}");
    mutate("mutation($id: ID!) { addPullRequestReview(input: {pullRequestId: $id, event: APPROVE}) { clientMutationId } }", &[("id", pr.id.as_str().into())]).await?;
    follow(hub, repo, number, state(project, number).await?.0);
    Ok(())
}

/// Requests changes: a review with that verdict, `body` its summary (GitHub requires one). GitHub
/// has no taking it back but dismissing the review, on GitHub.
pub async fn request_changes(project: &str, hub: &Hub, repo: Option<&str>, number: u64, on: bool, body: &str) -> Result<()> {
    if !on {
        bail!("{}", tr("GitHub cannot take back a request for changes: dismiss your review on GitHub"));
    }
    let body = body.trim();
    ensure!(!body.is_empty(), "{}", tr("GitHub requires a comment to request changes"));
    let (pr, _) = open(project, hub, repo, number).await?;
    tracing::info!("github: request changes on #{number}");
    mutate(
        "mutation($id: ID!, $body: String!) { addPullRequestReview(input: {pullRequestId: $id, event: REQUEST_CHANGES, body: $body}) { clientMutationId } }",
        &[("id", pr.id.as_str().into()), ("body", body.into())],
    )
    .await?;
    follow(hub, repo, number, state(project, number).await?.0);
    Ok(())
}

/// A review thread's id (`PRRT_…`): the only discussion one replies to or resolves on GitHub.
fn thread_id(id: &str) -> Result<&str> {
    ensure!(id.starts_with("PRRT_") && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'), "\"{id}\" is not a review thread id");
    Ok(id)
}

/// Comment on the pull request, or reply to a review thread. A conversation comment or a review
/// summary has no thread on GitHub: replying to one is a new comment.
pub async fn comment(project: &str, hub: &Hub, repo: Option<&str>, number: u64, discussion: Option<&str>, body: &str) -> Result<()> {
    known(hub, repo, number)?;
    let body = body.trim();
    ensure!(!body.is_empty(), "{}", tr("empty comment"));
    let thread = discussion.filter(|d| !(d.starts_with("comment-") || d.starts_with("review-"))).map(thread_id).transpose()?;
    tracing::info!("github: #{number} comment{}", if thread.is_some() { " (reply)" } else { "" });
    match thread {
        Some(id) => {
            mutate(
                "mutation($id: ID!, $body: String!) { addPullRequestReviewThreadReply(input: {pullRequestReviewThreadId: $id, body: $body}) { clientMutationId } }",
                &[("id", id.into()), ("body", body.into())],
            )
            .await
        }
        None => {
            split(project)?;
            let _: serde_json::Value = gh_input("POST", &format!("repos/{project}/issues/{number}/comments"), &serde_json::json!({ "body": body })).await?;
            Ok(())
        }
    }
}

/// New review thread on a diff line: the new file's line, or the old one's for a deleted line.
pub async fn comment_line(project: &str, hub: &Hub, repo: Option<&str>, number: u64, line: LineRef, body: &str) -> Result<()> {
    known(hub, repo, number)?;
    let body = body.trim();
    ensure!(!body.is_empty(), "{}", tr("empty comment"));
    let (path, n, side) = match (line.new_line, line.old_line) {
        (Some(n), _) => (line.new_path, n, "RIGHT"),
        (None, Some(n)) => (line.old_path, n, "LEFT"),
        (None, None) => bail!("{}", tr("no line to comment on")),
    };
    let (pr, _) = state(project, number).await?;
    let mut vars: Vec<(&str, Var<'_>)> = vec![("id", pr.id.as_str().into()), ("body", body.into()), ("path", path.as_str().into()), ("line", Var::Raw(n.to_string())), ("side", side.into())];
    // On several lines: the first one, on its own side (a range may start on a removed line).
    let start = line.start.as_ref().and_then(|s| match (s.new_line, s.old_line) {
        (Some(n), _) => Some((n, "RIGHT")),
        (None, Some(n)) => Some((n, "LEFT")),
        (None, None) => None,
    });
    tracing::info!("github: #{number} comment on {path}:{}{n}", start.map(|(s, _)| format!("{s}-")).unwrap_or_default());
    let mutation = if let Some((s, side)) = start {
        vars.push(("startLine", Var::Raw(s.to_string())));
        vars.push(("startSide", side.into()));
        "mutation($id: ID!, $body: String!, $path: String!, $line: Int!, $side: DiffSide!, $startLine: Int!, $startSide: DiffSide!) { addPullRequestReviewThread(input: {pullRequestId: $id, body: $body, path: $path, line: $line, side: $side, startLine: $startLine, startSide: $startSide}) { clientMutationId } }"
    } else {
        "mutation($id: ID!, $body: String!, $path: String!, $line: Int!, $side: DiffSide!) { addPullRequestReviewThread(input: {pullRequestId: $id, body: $body, path: $path, line: $line, side: $side}) { clientMutationId } }"
    };
    mutate(mutation, &vars).await
}

/// Resolves a review thread, or reopens it; the open thread count follows right away.
pub async fn resolve(hub: &Hub, repo: Option<&str>, number: u64, discussion: &str, resolved: bool) -> Result<()> {
    known(hub, repo, number)?;
    let id = thread_id(discussion)?;
    tracing::info!("github: #{number} {} thread", if resolved { "resolve" } else { "reopen" });
    let mutation = if resolved {
        "mutation($id: ID!) { resolveReviewThread(input: {threadId: $id}) { clientMutationId } }"
    } else {
        "mutation($id: ID!) { unresolveReviewThread(input: {threadId: $id}) { clientMutationId } }"
    };
    mutate(mutation, &[("id", id.into())]).await?;
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| m.iid == number && m.repo.as_deref() == repo) else { return false };
        m.open_threads = if resolved { (m.open_threads - 1).max(0) } else { m.open_threads + 1 };
        true
    });
    Ok(())
}

/// Turns the pull request into a draft, or marks it ready for review: a state of its own on
/// GitHub, the title does not change.
pub async fn set_draft(project: &str, hub: &Hub, repo: Option<&str>, number: u64, draft: bool) -> Result<()> {
    let (pr, _) = open(project, hub, repo, number).await?;
    if pr.is_draft == draft {
        follow(hub, repo, number, pr);
        return Ok(());
    }
    tracing::info!("github: #{number} {}", if draft { "draft" } else { "ready" });
    let mutation = if draft {
        "mutation($id: ID!) { convertPullRequestToDraft(input: {pullRequestId: $id}) { clientMutationId } }"
    } else {
        "mutation($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { clientMutationId } }"
    };
    mutate(mutation, &[("id", pr.id.as_str().into())]).await?;
    follow(hub, repo, number, state(project, number).await?.0);
    Ok(())
}

/// Assignees (the whole list) or reviewers of an open pull request. Reviewers: those added are
/// asked for a review, those removed no longer are; one who already reviewed stays, GitHub
/// keeps their review.
pub async fn set_people(project: &str, hub: &Hub, repo: Option<&str>, number: u64, as_reviewers: bool, logins: Vec<String>) -> Result<()> {
    let (pr, _) = open(project, hub, repo, number).await?;
    let assignable = super::review::members(project).await?;
    if let Some(u) = logins.iter().find(|u| !assignable.iter().any(|m| &m.username == *u)) {
        bail!("{}", tr!("{u} cannot be assigned in {project}", u = u, project = project));
    }
    split(project)?;
    if as_reviewers {
        let requested: Vec<String> = pr.review_requests.nodes.into_iter().filter_map(|r| r.requested_reviewer?.login).collect();
        let reviewed: Vec<String> = pr.latest_opinionated_reviews.nodes.iter().filter_map(|r| r.author.as_ref()?.login.clone()).collect();
        let add: Vec<&String> = logins.iter().filter(|u| !requested.contains(u) && !reviewed.contains(u)).collect();
        let remove: Vec<&String> = requested.iter().filter(|u| !logins.contains(u)).collect();
        tracing::info!("github: #{number} reviewers +{add:?} -{remove:?}");
        let path = format!("repos/{project}/pulls/{number}/requested_reviewers");
        if !add.is_empty() {
            let _: serde_json::Value = gh_input("POST", &path, &serde_json::json!({ "reviewers": add })).await?;
        }
        if !remove.is_empty() {
            let _: serde_json::Value = gh_input("DELETE", &path, &serde_json::json!({ "reviewers": remove })).await?;
        }
    } else {
        tracing::info!("github: #{number} assignees = {logins:?}");
        let _: serde_json::Value = gh_input("PATCH", &format!("repos/{project}/issues/{number}"), &serde_json::json!({ "assignees": logins })).await?;
    }
    follow(hub, repo, number, state(project, number).await?.0);
    Ok(())
}

/// An open pull request's labels, the whole list: only those that move are sent, so a label set
/// elsewhere meanwhile stays; a new name creates the label (in GitHub's default color).
pub async fn set_labels(project: &str, hub: &Hub, repo: Option<&str>, number: u64, labels: Vec<String>) -> Result<()> {
    crate::forge::check_labels(&labels)?;
    let shown = known(hub, repo, number)?;
    ensure!(shown.state == MrState::Opened, "{}", tr!("#{number} is not an open pull request", number = number));
    let (add, remove) = crate::forge::label_changes(&shown.labels, &labels);
    if add.is_empty() && remove.is_empty() {
        return Ok(());
    }
    split(project)?;
    tracing::info!("github: #{number} labels +{add:?} -{remove:?}");
    let path = format!("repos/{project}/issues/{number}/labels");
    if !add.is_empty() {
        let _: serde_json::Value = gh_input("POST", &path, &serde_json::json!({ "labels": add })).await?;
        super::review::forget_labels(project);
    }
    for l in &remove {
        // A label named `.` or `..` (set elsewhere) escaped whole: as is, it would be a path of the API.
        let name = if matches!(*l, "." | "..") { l.replace('.', "%2E") } else { crate::http::encode(l) };
        gh_bytes(&["api".into(), "--method".into(), "DELETE".into(), format!("{path}/{name}")]).await?;
    }
    let now: Vec<super::review::RawLabel> = super::api(&path).await?;
    let now: Vec<String> = now.into_iter().map(|l| l.name).collect();
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| m.iid == number && m.repo.as_deref() == repo) else { return false };
        let changed = m.labels != now;
        m.labels = now;
        changed
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_review_threads_are_threads() {
        assert!(thread_id("PRRT_kwDOABCD12-x_9").is_ok());
        assert!(thread_id("comment-12").is_err());
        assert!(thread_id("PRRT_a b").is_err());
        assert!(thread_id("PRRT_x\") { evil }").is_err());
    }

    /// Against a pull request of yours (`PRISM_GH_TEST=owner/repo#number`, open, with write
    /// access): only what can be undone — draft and back, assignees and back.
    #[tokio::test]
    #[ignore = "needs gh logged in, the network, and PRISM_GH_TEST=owner/repo#number"]
    async fn changes_a_pull_request_and_back() {
        let target = std::env::var("PRISM_GH_TEST").expect("PRISM_GH_TEST=owner/repo#number");
        let (project, number) = target.split_once('#').unwrap();
        let number: u64 = number.parse().unwrap();
        let mut identity = super::super::Identity::default();
        identity.refresh().await;
        let mut followed = super::super::Followed { repo: None, project: project.into(), merged: None, open: Vec::new() };
        let mrs = followed.read(Utc::now(), &identity).await.unwrap().mrs;
        let me = identity.login.unwrap();
        let hub = Hub::new();
        hub.update(|s| {
            s.merge_requests = mrs;
            true
        });
        let shown = |hub: &Hub| known(hub, None, number).unwrap();
        let before = shown(&hub);
        set_draft(project, &hub, None, number, !before.draft).await.unwrap();
        assert_eq!(shown(&hub).draft, !before.draft);
        set_draft(project, &hub, None, number, before.draft).await.unwrap();
        assert_eq!(shown(&hub).draft, before.draft);
        let mut with_me = before.assignees.clone();
        if !with_me.contains(&me) {
            with_me.push(me.clone());
        }
        set_people(project, &hub, None, number, false, with_me).await.unwrap();
        assert!(shown(&hub).assignees.contains(&me));
        set_people(project, &hub, None, number, false, before.assignees.clone()).await.unwrap();
        assert_eq!(shown(&hub).assignees, before.assignees);
        assert!(approve(project, &hub, None, number, false).await.is_err(), "no taking back an approval");
        assert!(resolve(&hub, None, number, "comment-1", true).await.is_err(), "not a thread");
    }
}
