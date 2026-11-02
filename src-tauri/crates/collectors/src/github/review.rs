//! Review of a pull request from its panel: description, discussions, approvals, diff and files,
//! read on demand when the panel displays them (never collected in the background nor kept in
//! the snapshot). Conversation comments and review summaries are standalone notes; review
//! threads, placed on the diff, are the resolvable discussions.

use prism_base::{i18n::tr, tr};
use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{api, approvals_left, approvers, gh_bytes, graphql, split, Actor, Nodes, Opinion};
use crate::forge::shown;
// Percent-encoding of a URL segment (a file path, its slashes included, or a branch).
use crate::http::encode as escape;
use crate::{
    forge::{approval_history, ApprovalEvent, ApprovalKind, Discussion, FileDiff, ForgeLabel, Member, MrCommit, MrDetail, MrDiff, Note},
    state::Hub,
};

const DETAIL_QUERY: &str = include_str!("pull_request.graphql");
const PER_PAGE: usize = 100;

/// The pull request must be one of those Prism displays: the window cannot target an arbitrary number.
pub(super) fn known(hub: &Hub, repo: Option<&str>, number: u64) -> Result<crate::state::MergeRequest> {
    shown::mr(hub, repo, number).with_context(|| tr!("#{number} is not a pull request Prism displays", number = number))
}

#[derive(Deserialize)]
struct Data {
    viewer: Option<Actor>,
    repository: Option<Repository>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    pull_request: Option<RawPr>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPr {
    body: String,
    state: String,
    review_decision: Option<String>,
    #[serde(default)]
    base_ref: Option<super::BaseRef>,
    author: Option<Actor>,
    latest_opinionated_reviews: Nodes<Opinion>,
    comments: Nodes<RawComment>,
    reviews: Nodes<RawComment>,
    #[serde(default = "no_items")]
    timeline_items: Nodes<RawItem>,
    review_threads: Nodes<RawThread>,
}

fn no_items() -> Nodes<RawItem> {
    Nodes { nodes: Vec::new() }
}

/// A timeline item of the approval history: a commit, a force push, a dismissed review.
#[derive(Deserialize)]
#[serde(tag = "__typename")]
enum RawItem {
    PullRequestCommit { commit: RawCommit },
    #[serde(rename_all = "camelCase")]
    HeadRefForcePushedEvent { created_at: DateTime<Utc>, actor: Option<Actor> },
    #[serde(rename_all = "camelCase")]
    ReviewDismissedEvent { created_at: DateTime<Utc>, actor: Option<Actor>, review: Option<RawDismissed> },
    /// Whatever else GitHub would send: left out rather than failing the whole pane.
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCommit {
    committed_date: DateTime<Utc>,
    author: Option<RawCommitAuthor>,
}

#[derive(Deserialize)]
struct RawCommitAuthor {
    name: Option<String>,
    user: Option<Actor>,
}

#[derive(Deserialize)]
struct RawDismissed {
    author: Option<Actor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    database_id: Option<u64>,
    /// A review's: APPROVED, CHANGES_REQUESTED, COMMENTED, DISMISSED…
    #[serde(default)]
    state: Option<String>,
    body: String,
    created_at: DateTime<Utc>,
    author: Option<Actor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawThread {
    id: String,
    is_resolved: bool,
    path: Option<String>,
    line: Option<u64>,
    original_line: Option<u64>,
    #[serde(default)]
    start_line: Option<u64>,
    #[serde(default)]
    original_start_line: Option<u64>,
    diff_side: Option<String>,
    comments: Nodes<RawComment>,
}

impl From<RawComment> for Note {
    fn from(c: RawComment) -> Self {
        let author = c.author.unwrap_or(Actor { login: None, name: None, team: None, avatar: None });
        let login = author.login.unwrap_or_else(|| "ghost".into());
        Note { id: c.database_id.unwrap_or_default(), author_name: author.name.filter(|n| !n.is_empty()).unwrap_or_else(|| login.clone()), author: login, body: c.body, created_at: c.created_at }
    }
}

/// A standalone note: a conversation comment, or a review's summary.
fn standalone(prefix: &str, c: RawComment) -> Discussion {
    Discussion { id: format!("{prefix}-{}", c.database_id.unwrap_or_default()), individual: true, resolvable: false, resolved: false, path: None, line: None, old_side: false, start_line: None, notes: vec![c.into()] }
}

pub async fn detail(project: &str, hub: &Hub, repo: Option<&str>, number: u64) -> Result<MrDetail> {
    known(hub, repo, number)?;
    let (owner, name) = split(project)?;
    let data: Data = graphql(DETAIL_QUERY, &[("owner", owner.into()), ("name", name.into()), ("number", super::Var::Raw(number.to_string()))]).await?;
    let pr = data.repository.and_then(|r| r.pull_request).with_context(|| tr!("#{number} not found", number = number))?;
    let me = data.viewer.and_then(|v| v.login);
    let author = pr.author.and_then(|a| a.login);
    let approved_by = approvers(&pr.latest_opinionated_reviews.nodes);
    let changes_requested_by = super::requesters(&pr.latest_opinionated_reviews.nodes);
    let history = approval_history(history_of(&pr.reviews.nodes, pr.timeline_items.nodes));
    let mut discussions: Vec<Discussion> = pr.comments.nodes.into_iter().map(|c| standalone("comment", c)).collect();
    // A review's summary, when it has one: its line comments are in the threads.
    discussions.extend(pr.reviews.nodes.into_iter().filter(|r| !r.body.trim().is_empty()).map(|r| standalone("review", r)));
    discussions.extend(pr.review_threads.nodes.into_iter().map(|t| Discussion {
        id: t.id,
        individual: false,
        resolvable: true,
        resolved: t.is_resolved,
        path: t.path,
        line: t.line.or(t.original_line),
        old_side: t.diff_side.as_deref() == Some("LEFT"),
        start_line: t.start_line.or(t.original_start_line),
        notes: t.comments.nodes.into_iter().map(Note::from).collect(),
    }));
    discussions.retain(|d| !d.notes.is_empty());
    discussions.sort_by_key(|d| d.notes[0].created_at);
    Ok(MrDetail {
        description: pr.body,
        user_has_approved: me.as_ref().is_some_and(|me| approved_by.contains(me)),
        user_requested_changes: me.as_ref().is_some_and(|me| changes_requested_by.contains(me)),
        changes_requested_by,
        // GitHub refuses an author's own approval.
        user_can_approve: pr.state == "OPEN" && me.is_some() && me != author,
        approvals_left: approvals_left(pr.review_decision.as_deref(), pr.base_ref.as_ref().and_then(super::BaseRef::required), approved_by.len()),
        approved_by,
        discussions,
        approval_history: history,
    })
}

/// The approval history: the reviews that approve or ask for changes (a dismissed one still
/// counts as given, its dismissal follows), the commits pushed, the dismissals.
fn history_of(reviews: &[RawComment], items: Vec<RawItem>) -> Vec<ApprovalEvent> {
    let login = |a: Option<&Actor>| a.and_then(|a| a.login.clone()).unwrap_or_default();
    let event = |at, kind, author: String, whom: Option<String>, commits| ApprovalEvent { at, kind, author_name: author.clone(), author, whom, commits };
    let mut out: Vec<ApprovalEvent> = reviews
        .iter()
        .filter_map(|r| {
            let kind = match r.state.as_deref()? {
                "APPROVED" => ApprovalKind::Approved,
                "CHANGES_REQUESTED" => ApprovalKind::ChangesRequested,
                _ => return None,
            };
            let name = r.author.as_ref().and_then(|a| a.name.clone()).filter(|n| !n.is_empty());
            let author = login(r.author.as_ref());
            Some(ApprovalEvent { author_name: name.unwrap_or_else(|| author.clone()), ..event(r.created_at, kind, author, None, 0) })
        })
        .collect();
    out.extend(items.into_iter().filter_map(|i| match i {
        RawItem::PullRequestCommit { commit } => {
            let user = commit.author.as_ref().and_then(|a| a.user.as_ref());
            let author = login(user);
            let name = commit.author.and_then(|a| a.name).filter(|n| !n.is_empty()).unwrap_or_else(|| author.clone());
            Some(ApprovalEvent { author_name: name, ..event(commit.committed_date, ApprovalKind::Pushed, author, None, 1) })
        }
        RawItem::HeadRefForcePushedEvent { created_at, actor } => Some(event(created_at, ApprovalKind::Pushed, login(actor.as_ref()), None, 0)),
        RawItem::ReviewDismissedEvent { created_at, actor, review } => {
            Some(event(created_at, ApprovalKind::Dismissed, login(actor.as_ref()), review.and_then(|r| r.author).and_then(|a| a.login), 0))
        }
        RawItem::Other => None,
    }));
    out
}

#[derive(Deserialize)]
struct RawFile {
    filename: String,
    previous_filename: Option<String>,
    status: String,
    patch: Option<String>,
}

pub async fn diff(project: &str, hub: &Hub, repo: Option<&str>, number: u64) -> Result<MrDiff> {
    known(hub, repo, number)?;
    split(project)?;
    let mut diff = shown::Diff::default();
    // GitHub lists at most 3000 files.
    for page in 1..=30 {
        let batch: Vec<RawFile> = api(&format!("repos/{project}/pulls/{number}/files?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        for f in batch {
            let patch = f.patch.unwrap_or_default();
            diff.push(FileDiff {
                // No patch: a binary file, or too big for GitHub.
                collapsed: patch.is_empty(),
                diff: patch,
                old_path: f.previous_filename.unwrap_or_else(|| f.filename.clone()),
                new_file: f.status == "added",
                deleted_file: f.status == "removed",
                renamed_file: f.status == "renamed",
                new_path: f.filename,
            });
        }
        if last {
            break;
        }
    }
    Ok(diff.finish())
}

#[derive(Deserialize)]
struct PrCommit {
    sha: String,
    commit: PrCommitInfo,
    html_url: Option<String>,
    author: Option<PrLogin>,
}
#[derive(Deserialize)]
struct PrCommitInfo {
    message: String,
    author: Option<PrSignature>,
}
#[derive(Deserialize)]
struct PrSignature {
    name: String,
    date: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
struct PrLogin {
    login: String,
}

/// Its commits, newest first (GitHub lists them oldest first, 250 at most).
pub async fn commits(project: &str, hub: &Hub, repo: Option<&str>, number: u64) -> Result<Vec<MrCommit>> {
    known(hub, repo, number)?;
    split(project)?;
    let mut out = Vec::new();
    for page in 1..=3 {
        let batch: Vec<PrCommit> = api(&format!("repos/{project}/pulls/{number}/commits?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        out.extend(batch.into_iter().map(|c| {
            let (name, at) = c.commit.author.map(|a| (a.name, a.date)).unwrap_or_default();
            MrCommit {
                title: c.commit.message.lines().next().unwrap_or_default().to_string(),
                author: c.author.map(|a| a.login).unwrap_or(name),
                at,
                url: c.html_url,
                sha: c.sha,
            }
        }));
        if last {
            break;
        }
    }
    out.reverse();
    Ok(out)
}

/// A file's path in the contents API, each part escaped: none empty, `.` or `..`, which would
/// climb out of `contents/` to another of the API's paths.
fn content_path(path: &str) -> Result<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == "..") {
        bail!("{}", tr!("{path}: not a file", path = path));
    }
    Ok(parts.into_iter().map(escape).collect::<Vec<_>>().join("/"))
}

/// A file of the pull request, whole, as it is on its head branch: the Markdown preview of the diff.
pub async fn file(project: &str, hub: &Hub, repo: Option<&str>, number: u64, path: &str) -> Result<String> {
    let branch = known(hub, repo, number)?.source_branch;
    split(project)?;
    let path = content_path(path)?;
    let bytes = gh_bytes(&["api".into(), "-H".into(), "Accept: application/vnd.github.raw".into(), format!("repos/{project}/contents/{path}?ref={}", escape(&branch))]).await?;
    if bytes.len() > shown::FILE_MAX_BYTES {
        bail!("{}", tr!("{path}: too large", path = path));
    }
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!(tr!("{path}: not text", path = path)))
}

/// An image of a pull request's diff as a `data:` URL: on its head branch, or (`old`) on its base branch.
pub async fn file_image(project: &str, hub: &Hub, repo: Option<&str>, number: u64, path: &str, old: bool) -> Result<String> {
    let m = known(hub, repo, number)?;
    let branch = if old { m.target_branch } else { m.source_branch };
    split(project)?;
    let path = content_path(path)?;
    let bytes = gh_bytes(&["api".into(), "-H".into(), "Accept: application/vnd.github.raw".into(), format!("repos/{project}/contents/{path}?ref={}", escape(&branch))]).await?;
    crate::forge::image_data_url(&bytes)
}

/* ---------- Assignees and reviewers ---------- */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Assignable {
    repository: Option<AssignableRepo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignableRepo {
    assignable_users: AssignablePage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignablePage {
    nodes: Vec<RawUser>,
    page_info: super::PageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUser {
    database_id: Option<u64>,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

/// Those who can be assigned change little: re-read at most every hour, per repository.
const MEMBERS_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
type Cache = HashMap<String, (std::time::Instant, Vec<Member>)>;
static MEMBERS: std::sync::Mutex<Option<Cache>> = std::sync::Mutex::new(None);

/// Those who can be assigned in the repository (users only: GitHub leaves bots out), by name;
/// one without a name goes by their login.
pub async fn members(project: &str) -> Result<Vec<Member>> {
    if let Some((_, list)) = MEMBERS.lock().unwrap().as_ref().and_then(|c| c.get(project)).filter(|(at, _)| at.elapsed() < MEMBERS_TTL) {
        return Ok(list.clone());
    }
    const QUERY: &str = "query($owner: String!, $name: String!, $after: String) { repository(owner: $owner, name: $name) { assignableUsers(first: 100, after: $after) { pageInfo { hasNextPage endCursor } nodes { databaseId login name avatarUrl } } } }";
    let (owner, name) = split(project)?;
    let (mut list, mut after) = (Vec::new(), None::<String>);
    for _ in 0..10 {
        let mut vars = vec![("owner", owner.into()), ("name", name.into())];
        if let Some(cursor) = &after {
            vars.push(("after", cursor.as_str().into()));
        }
        let data: Assignable = graphql(QUERY, &vars).await?;
        let page = data.repository.context(tr("repository not found or not accessible"))?.assignable_users;
        list.extend(page.nodes.into_iter().map(|u| Member {
            id: u.database_id.unwrap_or_default(),
            name: u.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| u.login.clone()),
            username: u.login,
            avatar_url: u.avatar_url,
        }));
        match page.page_info {
            super::PageInfo { has_next_page: true, end_cursor: Some(c) } => after = Some(c),
            _ => break,
        }
    }
    list.sort_by_key(|m| m.name.to_lowercase());
    MEMBERS.lock().unwrap().get_or_insert_with(HashMap::new).insert(project.to_string(), (std::time::Instant::now(), list.clone()));
    Ok(list)
}

/* ---------- Labels ---------- */

#[derive(Deserialize)]
pub(super) struct RawLabel {
    pub name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Labels change little: re-read at most every ten minutes, per repository (or at once after Prism created one).
const LABELS_TTL: std::time::Duration = std::time::Duration::from_secs(600);
type LabelCache = HashMap<String, (std::time::Instant, Vec<ForgeLabel>)>;
static LABELS: std::sync::Mutex<Option<LabelCache>> = std::sync::Mutex::new(None);

/// The repository's labels, by name; GitHub's colors come without `#`.
pub async fn labels(project: &str) -> Result<Vec<ForgeLabel>> {
    if let Some((_, list)) = LABELS.lock().unwrap().as_ref().and_then(|c| c.get(project)).filter(|(at, _)| at.elapsed() < LABELS_TTL) {
        return Ok(list.clone());
    }
    split(project)?;
    let mut list: Vec<ForgeLabel> = Vec::new();
    for page in 1..=10 {
        let batch: Vec<RawLabel> = api(&format!("repos/{project}/labels?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        list.extend(batch.into_iter().map(|l| ForgeLabel {
            name: l.name,
            color: l.color.filter(|c| !c.is_empty()).map(|c| format!("#{}", c.trim_start_matches('#'))),
            description: l.description.filter(|d| !d.trim().is_empty()),
        }));
        if last {
            break;
        }
    }
    list.sort_by_key(|l| l.name.to_lowercase());
    LABELS.lock().unwrap().get_or_insert_with(HashMap::new).insert(project.to_string(), (std::time::Instant::now(), list.clone()));
    Ok(list)
}

/// After a label was created by adding it to a pull request: the next read sees it.
pub fn forget_labels(project: &str) {
    if let Some(c) = LABELS.lock().unwrap().as_mut() {
        c.remove(project);
    }
}

/* ---------- Images ---------- */

/// An image attached on GitHub: `github.com/user-attachments/…` or a `*.githubusercontent.com`
/// address; no other URL is read.
fn attachment(url: &str) -> bool {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^https://(github\.com/user-attachments/|([a-z0-9-]+\.)?githubusercontent\.com/)[^\s]+$").unwrap());
    RE.is_match(url)
}

/// An attached image of `project`'s texts. A public repository's is read as is; a private one's
/// is behind GitHub's login: GitHub renders it, for the user `gh` answers for, under a signed
/// address valid a few minutes (`private-user-images.githubusercontent.com/…?jwt=…`), which is read.
pub async fn image(project: &str, url: &str) -> Result<String> {
    if !attachment(url) {
        bail!("not a GitHub attachment: {url}");
    }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build()?;
    let resp = http.get(url).send().await?;
    if resp.status().is_success() {
        return crate::forge::image_data_url(&resp.bytes().await?);
    }
    let first = resp.status();
    let signed = signed(project, url).await?.with_context(|| format!("GitHub answered {first}, and gave no address to read it"))?;
    let resp = http.get(&signed).send().await?;
    if !resp.status().is_success() {
        bail!("GitHub answered {} for the signed image", resp.status());
    }
    crate::forge::image_data_url(&resp.bytes().await?)
}

/// The signed address GitHub gives an attached image when it renders it in the repository.
async fn signed(project: &str, url: &str) -> Result<Option<String>> {
    split(project)?;
    let html = gh_bytes(&["api".into(), "-X".into(), "POST".into(), "markdown".into(), "-f".into(), format!("text=![]({url})"), "-f".into(), "mode=gfm".into(), "-f".into(), format!("context={project}")]).await?;
    Ok(signed_src(&String::from_utf8_lossy(&html), url))
}

/// In GitHub's rendering, the signed address standing for `url`: the same asset (its id), on
/// GitHub's image host.
fn signed_src(html: &str, url: &str) -> Option<String> {
    static SRC: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#"src="(https://[a-z0-9-]*\.?githubusercontent\.com/[^"]+)""#).unwrap());
    let asset = url.trim_end_matches('/').rsplit('/').next().filter(|a| !a.is_empty())?;
    SRC.captures_iter(html).map(|c| c[1].replace("&amp;", "&")).find(|src| src.contains(asset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_contents_path_stays_in_contents() {
        assert_eq!(content_path("docs/a b.png").unwrap(), "docs/a%20b.png");
        for p in ["../../user", "docs/../x", "./a", "a//b", ""] {
            assert!(content_path(p).is_err(), "{p}");
        }
    }

    #[test]
    fn the_approval_history_of_a_pull_request() {
        let pr: RawPr = serde_json::from_value(serde_json::json!({
            "body": "", "state": "OPEN", "reviewDecision": null, "author": { "login": "me" },
            "latestOpinionatedReviews": { "nodes": [] }, "comments": { "nodes": [] }, "reviewThreads": { "nodes": [] },
            "reviews": { "nodes": [
                { "databaseId": 1, "body": "", "createdAt": "2026-10-06T10:00:00Z", "state": "APPROVED", "author": { "login": "ana", "name": "Ana" } },
                { "databaseId": 2, "body": "nit", "createdAt": "2026-10-06T10:30:00Z", "state": "COMMENTED", "author": { "login": "bob" } },
                { "databaseId": 3, "body": "", "createdAt": "2026-10-06T13:00:00Z", "state": "CHANGES_REQUESTED", "author": { "login": "bob" } }
            ] },
            "timelineItems": { "nodes": [
                { "__typename": "PullRequestCommit", "commit": { "committedDate": "2026-10-06T11:00:00Z", "author": { "name": "Me", "user": { "login": "me" } } } },
                { "__typename": "PullRequestCommit", "commit": { "committedDate": "2026-10-06T11:05:00Z", "author": { "name": "Me", "user": { "login": "me" } } } },
                { "__typename": "ReviewDismissedEvent", "createdAt": "2026-10-06T12:00:00Z", "actor": { "login": "me" }, "review": { "author": { "login": "ana" } } },
                { "__typename": "SomethingNew" }
            ] }
        }))
        .unwrap();
        let history = approval_history(history_of(&pr.reviews.nodes, pr.timeline_items.nodes));
        let got: Vec<_> = history.iter().map(|e| (e.kind, e.author.as_str(), e.commits, e.whom.as_deref())).collect();
        assert_eq!(got, [
            (ApprovalKind::Approved, "ana", 0, None),
            (ApprovalKind::Pushed, "me", 2, None),
            (ApprovalKind::Dismissed, "me", 0, Some("ana")),
            (ApprovalKind::ChangesRequested, "bob", 0, None),
        ]);
        assert_eq!(history[0].author_name, "Ana");
    }

    #[test]
    fn the_signed_address_of_an_asset() {
        let html = r#"<p><a href="x"><img src="https://private-user-images.githubusercontent.com/1/634-e7d6.png?jwt=a.b&amp;c=d" alt=""></a><img src="https://private-user-images.githubusercontent.com/1/9-other.png?jwt=z"></p>"#;
        assert_eq!(signed_src(html, "https://github.com/user-attachments/assets/e7d6").as_deref(), Some("https://private-user-images.githubusercontent.com/1/634-e7d6.png?jwt=a.b&c=d"));
        assert_eq!(signed_src(html, "https://github.com/user-attachments/assets/abcd"), None);
        assert_eq!(signed_src(r#"<img src="https://evil.example/e7d6.png">"#, "https://github.com/user-attachments/assets/e7d6"), None);
    }

    /// Against GitHub itself: the signed address of a public attachment reads the image.
    #[tokio::test]
    #[ignore = "needs gh logged in, and the network"]
    async fn reads_an_attachment_through_its_signed_address() {
        let url = "https://github.com/user-attachments/assets/e7d69315-2274-4873-a343-7c087c9f8543";
        let signed = signed("cli/cli", url).await.unwrap().expect("a signed address");
        assert!(signed.contains("jwt="), "{signed}");
        let bytes = reqwest::get(&signed).await.unwrap().bytes().await.unwrap();
        assert!(crate::forge::image_data_url(&bytes).unwrap().starts_with("data:image/png;base64,"));
        assert!(image("cli/cli", url).await.unwrap().starts_with("data:image/"));
    }

    #[test]
    fn only_github_attachments_are_read() {
        assert!(attachment("https://github.com/user-attachments/assets/0a1b2c3d-4e5f-6789-abcd-ef0123456789"));
        assert!(attachment("https://private-user-images.githubusercontent.com/1/2.png?jwt=x"));
        assert!(attachment("https://user-images.githubusercontent.com/1/2.png"));
        assert!(!attachment("https://github.com/o/r/blob/main/a.png"));
        assert!(!attachment("https://evil.example/githubusercontent.com/a.png"));
        assert!(!attachment("http://user-images.githubusercontent.com/1/2.png"));
        assert!(!attachment("https://githubusercontent.com.evil.example/a.png"));
    }
}
