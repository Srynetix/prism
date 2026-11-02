//! Review of an MR from its panel: description, discussions, approval and diff, read on
//! demand when the panel displays them (never collected in the background nor kept in the
//! snapshot); approve, revoke one's approval, comment and reply to a thread.

use prism_base::{i18n::tr, tr};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::{api, encode, glab, glab_bytes, glab_input};
use crate::state::MrState;
use crate::forge::cli::Graphql;
use crate::forge::shown::{self, is};
// Percent-encoding of a URL segment (a file path, its slashes included, or a branch).
use crate::http::encode as escape;
use crate::{
    forge::{approval_history, ApprovalEvent, ApprovalKind, Discussion, FileDiff, ForgeLabel, LineRef, Member, MrCommit, MrDetail, MrDiff, Note},
    state::Hub,
};

const PER_PAGE: usize = 100;
/// Max pages of discussions read (100 threads each).
const DISCUSSION_PAGES: usize = 5;

/// The MR must be one of those Prism displays: the window cannot target an arbitrary number.
fn known(hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> {
    if shown::mr(hub, repo, iid).is_none() {
        bail!("{}", tr!("!{iid} is not a merge request Prism displays", iid = iid));
    }
    Ok(())
}

pub async fn detail(project: &str, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<MrDetail> {
    known(hub, repo, iid)?;
    let p = encode(project);
    let mr: RawMr = api(&format!("projects/{p}/merge_requests/{iid}")).await?;
    let approvals: RawApprovals = api(&format!("projects/{p}/merge_requests/{iid}/approvals")).await?;
    let mut discussions = Vec::new();
    let mut history = Vec::new();
    for page in 1..=DISCUSSION_PAGES {
        let batch: Vec<RawDiscussion> = api(&format!("projects/{p}/merge_requests/{iid}/discussions?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        history.extend(batch.iter().flat_map(|d| &d.notes).filter_map(approval_event));
        discussions.extend(batch.into_iter().filter_map(discussion));
        if last {
            break;
        }
    }
    let (me, changes_requested_by) = review_states(project, iid).await?;
    Ok(MrDetail {
        user_requested_changes: me.as_ref().is_some_and(|me| changes_requested_by.contains(me)),
        changes_requested_by,
        description: mr.description.unwrap_or_default(),
        discussions,
        approved_by: approvals.approved_by.into_iter().map(|a| a.user.username).collect(),
        approvals_left: approvals.approvals_left.unwrap_or(0),
        user_has_approved: approvals.user_has_approved.unwrap_or(false),
        user_can_approve: approvals.user_can_approve.unwrap_or(false),
        approval_history: approval_history(history),
    })
}

/// A system note of the approval history: "approved this merge request", "unapproved this merge
/// request", "requested changes", "reset approvals from @a and @b by pushing to the branch",
/// "added 3 commits"; any other (labels, description…) is not.
fn approval_event(n: &RawNote) -> Option<ApprovalEvent> {
    if !n.system {
        return None;
    }
    let body = n.body.trim();
    let first = body.lines().next().unwrap_or_default().trim();
    let (kind, whom, commits) = if first.starts_with("approved this merge request") {
        (ApprovalKind::Approved, None, 0)
    } else if first.starts_with("unapproved this merge request") {
        (ApprovalKind::Unapproved, None, 0)
    } else if first.starts_with("requested changes") {
        (ApprovalKind::ChangesRequested, None, 0)
    } else if let Some(rest) = first.strip_prefix("reset approvals from ").or_else(|| first.strip_prefix("resetted approvals from ")) {
        let whom = rest.split(" by ").next().unwrap_or(rest).trim();
        (ApprovalKind::Reset, Some(whom.to_string()).filter(|w| !w.is_empty()), 0)
    } else {
        let rest = first.strip_prefix("added ")?;
        let count = rest.split_whitespace().next()?.parse().ok()?;
        rest.split_whitespace().nth(1).filter(|w| w.starts_with("commit"))?;
        (ApprovalKind::Pushed, None, count)
    };
    Some(ApprovalEvent { at: n.created_at, kind, author: n.author.username.clone(), author_name: n.author.name.clone(), whom, commits })
}

/// A file of the MR, whole, as it is on its source branch: the Markdown preview of the diff.
pub async fn file(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, path: &str) -> Result<String> {
    let branch = hub
        .subscribe()
        .borrow()
        .merge_requests
        .iter()
        .find(|m| is(m, repo, iid))
        .map(|m| m.source_branch.clone());
    let Some(branch) = branch else { bail!("{}", tr!("!{iid} is not a merge request Prism displays", iid = iid)) };
    let bytes = glab_bytes(&format!("projects/{}/repository/files/{}/raw?ref={}", encode(project), escape(path), escape(&branch))).await?;
    if bytes.len() > shown::FILE_MAX_BYTES {
        bail!("{}", tr!("{path}: too large", path = path));
    }
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!(tr!("{path}: not text", path = path)))
}

/// An image of an MR's diff as a `data:` URL: on its source branch, or (`old`) on its target branch.
pub async fn file_image(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, path: &str, old: bool) -> Result<String> {
    let Some(m) = shown::mr(hub, repo, iid) else { bail!("{}", tr!("!{iid} is not a merge request Prism displays", iid = iid)) };
    let branch = if old { m.target_branch } else { m.source_branch };
    let bytes = glab_bytes(&format!("projects/{}/repository/files/{}/raw?ref={}", encode(project), escape(path), escape(&branch))).await?;
    crate::forge::image_data_url(&bytes)
}

pub async fn diff(project: &str, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<MrDiff> {
    known(hub, repo, iid)?;
    let p = encode(project);
    let mut diff = shown::Diff::default();
    for page in 1.. {
        let batch: Vec<RawDiff> = api(&format!("projects/{p}/merge_requests/{iid}/diffs?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        for d in batch {
            diff.push(FileDiff {
                collapsed: d.too_large.unwrap_or(false) || d.collapsed.unwrap_or(false) || d.diff.is_empty(),
                diff: d.diff,
                old_path: d.old_path,
                new_path: d.new_path,
                new_file: d.new_file,
                deleted_file: d.deleted_file,
                renamed_file: d.renamed_file,
            });
        }
        if last || page >= 50 {
            break;
        }
    }
    Ok(diff.finish())
}

#[derive(Deserialize)]
struct RawCommit {
    id: String,
    title: String,
    author_name: String,
    authored_date: Option<DateTime<Utc>>,
    web_url: Option<String>,
}

/// Its commits, newest first (as GitLab lists them).
pub async fn commits(project: &str, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<Vec<MrCommit>> {
    known(hub, repo, iid)?;
    let p = encode(project);
    let mut out = Vec::new();
    for page in 1..=10 {
        let batch: Vec<RawCommit> = api(&format!("projects/{p}/merge_requests/{iid}/commits?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        out.extend(batch.into_iter().map(|c| MrCommit { sha: c.id, title: c.title, author: c.author_name, at: c.authored_date, url: c.web_url }));
        if last {
            break;
        }
    }
    Ok(out)
}

/// Approves the MR, or revokes my approval; the snapshot follows right away (approvals, remaining).
pub async fn approve(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, on: bool) -> Result<()> {
    let ok = hub.subscribe().borrow().merge_requests.iter().any(|m| is(m, repo, iid) && m.state == MrState::Opened);
    if !ok {
        bail!("{}", tr!("!{iid} is not an open merge request Prism displays", iid = iid));
    }
    let p = encode(project);
    tracing::info!("gitlab: !{iid} {}", if on { "approve" } else { "unapprove" });
    let _: serde_json::Value = glab(&[
        "api".into(), "--method".into(), "POST".into(),
        format!("projects/{p}/merge_requests/{iid}/{}", if on { "approve" } else { "unapprove" }),
    ])
    .await?;
    follow_approvals(project, hub, repo, iid).await
}

/// The MR's approvals read again, into the snapshot at once (approvals, remaining).
async fn follow_approvals(project: &str, hub: &Hub, repo: Option<&str>, iid: u64) -> Result<()> {
    let approvals: RawApprovals = api(&format!("projects/{}/merge_requests/{iid}/approvals", encode(project))).await?;
    let by: Vec<String> = approvals.approved_by.into_iter().map(|a| a.user.username).collect();
    let left = approvals.approvals_left.unwrap_or(0);
    hub.update(|s| {
        let mut changed = false;
        for m in s.merge_requests.iter_mut().filter(|m| is(m, repo, iid)) {
            changed |= m.approved_by != by || m.approvals_left != left;
            m.approved_by.clone_from(&by);
            m.approvals_left = left;
        }
        changed
    });
    Ok(())
}

const REVIEW_STATES: &str = "query($path: ID!, $iid: String!) { currentUser { username } project(fullPath: $path) { mergeRequest(iid: $iid) { reviewers { nodes { username mergeRequestInteraction { reviewState } } } } } }";

/// The current user, and the reviewers whose state is "changes requested".
async fn review_states(project: &str, iid: u64) -> Result<(Option<String>, Vec<String>)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Data {
        current_user: Option<User>,
        project: Option<Project>,
    }
    #[derive(Deserialize)]
    struct User {
        username: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Project {
        merge_request: Option<Mr>,
    }
    #[derive(Deserialize)]
    struct Mr {
        reviewers: Nodes<Reviewer>,
    }
    #[derive(Deserialize)]
    struct Nodes<T> {
        nodes: Vec<T>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reviewer {
        username: String,
        merge_request_interaction: Option<Interaction>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Interaction {
        review_state: Option<String>,
    }
    let resp: Graphql<Data> = glab(&[
        "api".into(), "graphql".into(),
        "-f".into(), format!("query={REVIEW_STATES}"),
        "-f".into(), format!("path={project}"),
        "-f".into(), format!("iid={iid}"),
    ])
    .await?;
    let data = resp.into_data()?;
    let reviewers = data.project.and_then(|p| p.merge_request).map(|m| m.reviewers.nodes).unwrap_or_default();
    let requested = reviewers
        .into_iter()
        .filter(|r| r.merge_request_interaction.as_ref().and_then(|i| i.review_state.as_deref()) == Some("REQUESTED_CHANGES"))
        .map(|r| r.username)
        .collect();
    Ok((data.current_user.map(|u| u.username), requested))
}

/// Requests changes (my reviewer state becomes "changes requested", and my approval goes), with
/// `body` as a comment first if there is one; or takes my request back.
pub async fn request_changes(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, on: bool, body: &str) -> Result<()> {
    let ok = hub.subscribe().borrow().merge_requests.iter().any(|m| is(m, repo, iid) && m.state == MrState::Opened);
    if !ok {
        bail!("{}", tr!("!{iid} is not an open merge request Prism displays", iid = iid));
    }
    if on && !body.trim().is_empty() {
        comment(project, hub, repo, iid, None, body).await?;
    }
    let mutation = if on { "mergeRequestRequestChanges" } else { "mergeRequestDestroyRequestedChanges" };
    tracing::info!("gitlab: !{iid} {mutation}");
    #[derive(Deserialize)]
    struct Payload {
        errors: Vec<String>,
    }
    let resp: Graphql<std::collections::HashMap<String, Option<Payload>>> = glab(&[
        "api".into(), "graphql".into(),
        "-f".into(), format!("query=mutation($path: ID!, $iid: String!) {{ {mutation}(input: {{projectPath: $path, iid: $iid}}) {{ errors }} }}"),
        "-f".into(), format!("path={project}"),
        "-f".into(), format!("iid={iid}"),
    ])
    .await?;
    let errors = resp.into_data()?.remove(mutation).flatten().map(|p| p.errors).unwrap_or_default();
    if !errors.is_empty() {
        bail!("GitLab: {}", errors.join("; "));
    }
    // The snapshot follows: the requests for changes, and my approval, which a request removes.
    let (_, requested) = review_states(project, iid).await?;
    hub.update(|s| {
        let mut changed = false;
        for m in s.merge_requests.iter_mut().filter(|m| is(m, repo, iid)) {
            changed |= m.changes_requested_by != requested;
            m.changes_requested_by.clone_from(&requested);
        }
        changed
    });
    follow_approvals(project, hub, repo, iid).await
}

/// New thread on a diff line, anchored on the SHAs of the MR's current version.
pub async fn comment_line(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, line: LineRef, body: &str) -> Result<()> {
    known(hub, repo, iid)?;
    let body = body.trim();
    if body.is_empty() {
        bail!("{}", tr("empty comment"));
    }
    if line.old_line.is_none() && line.new_line.is_none() {
        bail!("{}", tr("no line to comment on"));
    }
    let p = encode(project);
    let mr: RawMr = api(&format!("projects/{p}/merge_requests/{iid}")).await?;
    let refs = mr.diff_refs.ok_or_else(|| anyhow::anyhow!("GitLab gave no diff refs for !{iid}"))?;
    let mut position = serde_json::json!({
        "position_type": "text",
        "base_sha": refs.base_sha,
        "start_sha": refs.start_sha,
        "head_sha": refs.head_sha,
        "old_path": line.old_path,
        "new_path": line.new_path,
    });
    if let Some(n) = line.old_line {
        position["old_line"] = n.into();
    }
    if let Some(n) = line.new_line {
        position["new_line"] = n.into();
    }
    // On several lines: GitLab's range, from the first line to this one, each by its line code.
    if let (Some(start), Some(pos)) = (&line.start, line.pos) {
        position["line_range"] = serde_json::json!({
            "start": range_end(&line.new_path, start.old_line, start.new_line, start.pos),
            "end": range_end(&line.new_path, line.old_line, line.new_line, pos),
        });
    }
    tracing::info!("gitlab: !{iid} comment on {}:{}{}", line.new_path, line.start.as_ref().and_then(|s| s.new_line.or(s.old_line)).map(|n| format!("{n}-")).unwrap_or_default(), line.new_line.or(line.old_line).unwrap_or(0));
    let _: serde_json::Value = glab_input("POST", &format!("projects/{p}/merge_requests/{iid}/discussions"), &serde_json::json!({ "body": body, "position": position })).await?;
    Ok(())
}

/// An end of a range for GitLab: the line's numbers, its type, and its line code — the SHA-1 of
/// the file's path, then its place in the old and the new file (`pos`: a line added has one in
/// the old file too, the next old line's).
fn range_end(path: &str, old_line: Option<u64>, new_line: Option<u64>, pos: (u64, u64)) -> serde_json::Value {
    use sha1::{Digest, Sha1};
    let kind = match (old_line, new_line) {
        (None, Some(_)) => serde_json::json!("new"),
        (Some(_), None) => serde_json::json!("old"),
        _ => serde_json::Value::Null,
    };
    let hash = Sha1::digest(path.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>();
    serde_json::json!({ "line_code": format!("{hash}_{}_{}", pos.0, pos.1), "type": kind, "old_line": old_line, "new_line": new_line })
}

/// Resolves a thread, or reopens it; the MR's open thread count follows right away.
pub async fn resolve(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, discussion: &str, resolved: bool) -> Result<()> {
    known(hub, repo, iid)?;
    if discussion.is_empty() || !discussion.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("\"{discussion}\" is not a discussion id");
    }
    let p = encode(project);
    tracing::info!("gitlab: !{iid} {} thread", if resolved { "resolve" } else { "reopen" });
    let _: serde_json::Value = glab(&[
        "api".into(), "--method".into(), "PUT".into(),
        format!("projects/{p}/merge_requests/{iid}/discussions/{discussion}"),
        "-F".into(), format!("resolved={resolved}"),
    ])
    .await?;
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| is(m, repo, iid)) else { return false };
        m.open_threads = if resolved { (m.open_threads - 1).max(0) } else { m.open_threads + 1 };
        true
    });
    Ok(())
}

/// Turns an open MR into a draft, or marks it ready: GitLab reads it from the title (`Draft:`).
/// The title is re-read from GitLab before being rewritten, so as not to lose a recent
/// edit; the snapshot follows right away.
pub async fn set_draft(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, draft: bool) -> Result<()> {
    let ok = hub.subscribe().borrow().merge_requests.iter().any(|m| is(m, repo, iid) && m.state == MrState::Opened);
    if !ok {
        bail!("{}", tr!("!{iid} is not an open merge request Prism displays", iid = iid));
    }
    let p = encode(project);
    let mr: RawMr = api(&format!("projects/{p}/merge_requests/{iid}")).await?;
    let title = draft_title(&mr.title, draft);
    if title == mr.title {
        return Ok(());
    }
    tracing::info!("gitlab: !{iid} {}", if draft { "draft" } else { "ready" });
    let updated: RawTitle = glab(&[
        "api".into(), "--method".into(), "PUT".into(),
        format!("projects/{p}/merge_requests/{iid}"),
        "-f".into(), format!("title={title}"),
    ])
    .await?;
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| is(m, repo, iid)) else { return false };
        m.title.clone_from(&updated.title);
        m.draft = updated.draft;
        true
    });
    Ok(())
}

/// Title with or without the draft prefix; the forms GitLab recognizes (`Draft:`, `[Draft]`,
/// `(Draft)`, uppercase or not) are removed.
fn draft_title(title: &str, draft: bool) -> String {
    static PREFIX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"(?i)^\s*(\[draft\]|\(draft\)|draft:|draft\s-)\s*").unwrap());
    let bare = PREFIX.replace(title, "").into_owned();
    if draft { format!("Draft: {bare}") } else { bare }
}

/// Image uploaded to the project, read through the API (`/projects/:id/uploads/:secret/:filename`) and
/// returned as a `data:` URL. Only `/uploads/<secret>/<file>` paths are accepted, and
/// only PNG, JPEG, GIF and WebP are displayed (never an SVG, which can carry script).
pub async fn upload(project: &str, path: &str) -> Result<String> {
    let (secret, file) = upload_parts(path).ok_or_else(|| anyhow::anyhow!("not a GitLab upload: {path}"))?;
    let bytes = glab_bytes(&format!("projects/{}/uploads/{secret}/{file}", encode(project))).await?;
    crate::forge::image_data_url(&bytes)
}

/// Secret and file name of a `…/uploads/<secret>/<file>` path (relative or full URL).
fn upload_parts(path: &str) -> Option<(String, String)> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"/uploads/([0-9a-f]{32})/([^/?#]+)(?:[?#].*)?$").unwrap());
    let caps = RE.captures(path)?;
    Some((caps[1].to_string(), caps[2].to_string()))
}

/// Comment on the MR, or reply to a thread (`discussion`).
pub async fn comment(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, discussion: Option<&str>, body: &str) -> Result<()> {
    known(hub, repo, iid)?;
    let body = body.trim();
    if body.is_empty() {
        bail!("{}", tr("empty comment"));
    }
    let p = encode(project);
    let path = match discussion {
        Some(id) if id.chars().all(|c| c.is_ascii_hexdigit()) && !id.is_empty() => format!("projects/{p}/merge_requests/{iid}/discussions/{id}/notes"),
        Some(id) => bail!("\"{id}\" is not a discussion id"),
        None => format!("projects/{p}/merge_requests/{iid}/notes"),
    };
    tracing::info!("gitlab: !{iid} comment{}", if discussion.is_some() { " (reply)" } else { "" });
    let _: serde_json::Value = glab(&["api".into(), "--method".into(), "POST".into(), path, "-f".into(), format!("body={body}")]).await?;
    Ok(())
}

/// A thread without a human note (system notes only: pushed commits, labels…) is omitted.
fn discussion(d: RawDiscussion) -> Option<Discussion> {
    let first = d.notes.iter().find(|n| !n.system)?;
    let (path, line, old_side) = first
        .position
        .as_ref()
        .map(|p| (p.new_path.clone().or(p.old_path.clone()), p.new_line.or(p.old_line), p.new_line.is_none() && p.old_line.is_some()))
        .unwrap_or_default();
    let start_line = first.position.as_ref().and_then(|p| p.line_range.as_ref()).and_then(|r| r.start.new_line.or(r.start.old_line)).filter(|&n| Some(n) != line);
    let resolvable = d.notes.iter().any(|n| n.resolvable);
    let resolved = resolvable && d.notes.iter().filter(|n| n.resolvable).all(|n| n.resolved);
    let notes = d
        .notes
        .into_iter()
        .filter(|n| !n.system)
        .map(|n| Note { id: n.id, author_name: n.author.name, author: n.author.username, body: n.body, created_at: n.created_at })
        .collect();
    Some(Discussion { id: d.id, individual: d.individual_note, resolvable, resolved, path, line, old_side, start_line, notes })
}

#[derive(Deserialize)]
struct RawTitle {
    title: String,
    #[serde(default)]
    draft: bool,
}

#[derive(Deserialize)]
struct RawMr {
    #[serde(default)]
    title: String,
    description: Option<String>,
    diff_refs: Option<RawDiffRefs>,
}

#[derive(Deserialize)]
struct RawDiffRefs {
    base_sha: String,
    start_sha: String,
    head_sha: String,
}

#[derive(Deserialize)]
struct RawApprovals {
    #[serde(default)]
    approved_by: Vec<RawApprover>,
    approvals_left: Option<i64>,
    user_has_approved: Option<bool>,
    user_can_approve: Option<bool>,
}

#[derive(Deserialize)]
struct RawApprover {
    user: RawUser,
}

#[derive(Deserialize)]
struct RawUser {
    username: String,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct RawDiscussion {
    id: String,
    #[serde(default)]
    individual_note: bool,
    #[serde(default)]
    notes: Vec<RawNote>,
}

#[derive(Deserialize)]
struct RawNote {
    id: u64,
    body: String,
    author: RawUser,
    created_at: DateTime<Utc>,
    #[serde(default)]
    system: bool,
    #[serde(default)]
    resolvable: bool,
    #[serde(default)]
    resolved: bool,
    position: Option<RawPosition>,
}

#[derive(Deserialize)]
struct RawPosition {
    new_path: Option<String>,
    old_path: Option<String>,
    new_line: Option<u64>,
    old_line: Option<u64>,
    #[serde(default)]
    line_range: Option<RawRange>,
}

#[derive(Deserialize)]
struct RawRange {
    start: RawRangeEnd,
}

#[derive(Deserialize)]
struct RawRangeEnd {
    new_line: Option<u64>,
    old_line: Option<u64>,
}

#[derive(Deserialize)]
struct RawDiff {
    old_path: String,
    new_path: String,
    #[serde(default)]
    diff: String,
    #[serde(default)]
    new_file: bool,
    #[serde(default)]
    deleted_file: bool,
    #[serde(default)]
    renamed_file: bool,
    too_large: Option<bool>,
    collapsed: Option<bool>,
}

/* ---------- Assignees and reviewers ---------- */

/// A project member, as GitLab gives it: its state says whether it is active.
#[derive(Deserialize)]
struct RawMember {
    id: u64,
    username: String,
    name: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    state: String,
}

/// Members change little: re-read at most every hour.
const MEMBERS_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
static MEMBERS: std::sync::Mutex<Option<(std::time::Instant, Vec<Member>)>> = std::sync::Mutex::new(None);

/// The project's active members, parent groups included (`members/all`), without project
/// or group bots; sorted by name.
pub async fn members(project: &str) -> Result<Vec<Member>> {
    if let Some((at, list)) = MEMBERS.lock().unwrap().as_ref()
        && at.elapsed() < MEMBERS_TTL
    {
        return Ok(list.clone());
    }
    let p = encode(project);
    let mut all: Vec<RawMember> = Vec::new();
    for page in 1..=10 {
        let batch: Vec<RawMember> = api(&format!("projects/{p}/members/all?per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        all.extend(batch);
        if last {
            break;
        }
    }
    let mut list = people_of(all);
    list.sort_by_key(|m| m.name.to_lowercase());
    *MEMBERS.lock().unwrap() = Some((std::time::Instant::now(), list.clone()));
    Ok(list)
}

/// Active, without bots (`project_123_bot_…`, `group_45_bot_…`), once each (an inherited member
/// can come back through several groups).
fn people_of(all: Vec<RawMember>) -> Vec<Member> {
    let mut seen = std::collections::HashSet::new();
    all.into_iter()
        .filter(|m| m.state == "active" && !m.username.contains("_bot") && !m.username.ends_with("-bot"))
        .filter(|m| seen.insert(m.id))
        .map(|m| Member { id: m.id, username: m.username, name: m.name, avatar_url: m.avatar_url })
        .collect()
}

#[derive(Deserialize)]
struct RawPeople {
    #[serde(default)]
    assignees: Vec<RawPerson>,
    #[serde(default)]
    reviewers: Vec<RawPerson>,
}

#[derive(Deserialize)]
struct RawPerson {
    username: String,
}

pub async fn set_people(project: &str, hub: &Hub, repo: Option<&str>, iid: u64, reviewers: bool, usernames: Vec<String>) -> Result<()> {
    let ok = hub.subscribe().borrow().merge_requests.iter().any(|m| is(m, repo, iid) && m.state == MrState::Opened);
    if !ok {
        bail!("{}", tr!("!{iid} is not an open merge request Prism displays", iid = iid));
    }
    let known = members(project).await?;
    let ids = usernames
        .iter()
        .map(|u| known.iter().find(|m| &m.username == u).map(|m| m.id).ok_or_else(|| anyhow::anyhow!(tr!("{u} is not a member of the project", u = u))))
        .collect::<Result<Vec<u64>>>()?;
    let field = if reviewers { "reviewer_ids" } else { "assignee_ids" };
    tracing::info!("gitlab: !{iid} {field} = {usernames:?}");
    let p = encode(project);
    let updated: RawPeople = glab_input("PUT", &format!("projects/{p}/merge_requests/{iid}"), &serde_json::json!({ field: ids })).await?;
    // Only the list set: the other one, in GitLab's answer, may predate a change of it sent meanwhile.
    let now: Vec<String> = if reviewers { updated.reviewers } else { updated.assignees }.into_iter().map(|u| u.username).collect();
    hub.update(|s| {
        let Some(m) = s.merge_requests.iter_mut().find(|m| is(m, repo, iid)) else { return false };
        let list = if reviewers { &mut m.reviewers } else { &mut m.assignees };
        let changed = *list != now;
        *list = now;
        changed
    });
    Ok(())
}

/* ---------- Labels ---------- */

#[derive(Deserialize)]
struct RawLabel {
    name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// Labels change little: re-read at most every ten minutes, per project (or at once after Prism created one).
const LABELS_TTL: std::time::Duration = std::time::Duration::from_secs(600);
type LabelCache = std::collections::HashMap<String, (std::time::Instant, Vec<ForgeLabel>)>;
static LABELS: std::sync::Mutex<Option<LabelCache>> = std::sync::Mutex::new(None);

/// The project's labels, its groups' included (`include_ancestor_groups`), by name.
pub async fn labels(project: &str) -> Result<Vec<ForgeLabel>> {
    if let Some((_, list)) = LABELS.lock().unwrap().as_ref().and_then(|c| c.get(project)).filter(|(at, _)| at.elapsed() < LABELS_TTL) {
        return Ok(list.clone());
    }
    let p = encode(project);
    let mut list: Vec<ForgeLabel> = Vec::new();
    for page in 1..=10 {
        let batch: Vec<RawLabel> = api(&format!("projects/{p}/labels?include_ancestor_groups=true&per_page={PER_PAGE}&page={page}")).await?;
        let last = batch.len() < PER_PAGE;
        list.extend(batch.into_iter().map(|l| ForgeLabel { name: l.name, color: l.color, description: l.description.filter(|d| !d.trim().is_empty()) }));
        if last {
            break;
        }
    }
    list.sort_by_key(|l| l.name.to_lowercase());
    list.dedup_by(|a, b| a.name == b.name);
    LABELS.lock().unwrap().get_or_insert_with(Default::default).insert(project.to_string(), (std::time::Instant::now(), list.clone()));
    Ok(list)
}

const CUSTOM_EMOJI: &str = "query($path: ID!, $after: String) { group(fullPath: $path) { customEmoji(includeAncestorGroups: true, first: 100, after: $after) { nodes { name url } pageInfo { hasNextPage endCursor } } } }";

/// Custom emoji change rarely: re-read at most every hour, per group.
const EMOJI_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
type EmojiCache = std::collections::HashMap<String, (std::time::Instant, Vec<crate::forge::CustomEmoji>)>;
static EMOJI: std::sync::Mutex<Option<EmojiCache>> = std::sync::Mutex::new(None);

/// The custom emoji of the project's group and of the groups above it (they are made on a group);
/// none for a project of a user's namespace, which has no group.
pub async fn custom_emoji(project: &str) -> Result<Vec<crate::forge::CustomEmoji>> {
    let Some((group, _)) = project.rsplit_once('/') else { return Ok(Vec::new()) };
    if let Some((_, list)) = EMOJI.lock().unwrap().as_ref().and_then(|c| c.get(group)).filter(|(at, _)| at.elapsed() < EMOJI_TTL) {
        return Ok(list.clone());
    }
    #[derive(Deserialize)]
    struct Data {
        group: Option<Group>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Group {
        custom_emoji: Option<Page>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Page {
        nodes: Vec<crate::forge::CustomEmoji>,
        page_info: PageInfo,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PageInfo {
        has_next_page: bool,
        end_cursor: Option<String>,
    }
    let mut list = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..10 {
        let mut args = vec!["api".into(), "graphql".into(), "-f".into(), format!("query={CUSTOM_EMOJI}"), "-f".into(), format!("path={group}")];
        if let Some(a) = &after {
            args.extend(["-f".into(), format!("after={a}")]);
        }
        let resp: Graphql<Data> = glab(&args).await?;
        // A user's namespace (no group by that path): none.
        let Some(page) = resp.into_data()?.group.and_then(|g| g.custom_emoji) else { break };
        list.extend(page.nodes.into_iter().filter(|e| e.url.starts_with("https://")));
        after = page.page_info.end_cursor.filter(|_| page.page_info.has_next_page);
        if after.is_none() {
            break;
        }
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list.dedup_by(|a, b| a.name == b.name);
    EMOJI.lock().unwrap().get_or_insert_with(Default::default).insert(group.to_string(), (std::time::Instant::now(), list.clone()));
    Ok(list)
}

/// After a label was created by adding it to an MR: the next read sees it.
pub fn forget_labels(project: &str) {
    if let Some(c) = LABELS.lock().unwrap().as_mut() {
        c.remove(project);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_a_file_path_and_a_branch() {
        assert_eq!(escape("docs/demo/READ ME.md"), "docs%2Fdemo%2FREAD%20ME.md");
        assert_eq!(escape("feat/ABC-12_x~y"), "feat%2FABC-12_x~y");
        assert_eq!(escape("é"), "%C3%A9");
    }

    #[test]
    fn keeps_human_notes_and_thread_state() {
        let raw: RawDiscussion = serde_json::from_value(serde_json::json!({
            "id": "6a9c1750b37d513a43987b574953fceb50b03ce7",
            "individual_note": false,
            "notes": [
                { "id": 1, "body": "added 1 commit", "author": { "username": "bot" }, "created_at": "2026-10-06T10:00:00Z", "system": true },
                { "id": 2, "body": "Why not a stream?", "author": { "username": "alex", "name": "Alex" }, "created_at": "2026-10-06T11:00:00Z",
                  "resolvable": true, "resolved": false, "position": { "new_path": "api/export.py", "new_line": 42 } },
                { "id": 3, "body": "Done", "author": { "username": "denis", "name": "Denis" }, "created_at": "2026-10-06T12:00:00Z",
                  "resolvable": true, "resolved": true }
            ]
        }))
        .unwrap();
        let d = discussion(raw).unwrap();
        assert_eq!(d.notes.len(), 2);
        assert_eq!((d.path.as_deref(), d.line, d.old_side), (Some("api/export.py"), Some(42), false));
        assert!(d.resolvable && !d.resolved);
    }

    #[test]
    fn reads_a_thread_on_several_lines() {
        let raw: RawDiscussion = serde_json::from_value(serde_json::json!({
            "id": "6a9c1750b37d513a43987b574953fceb50b03ce8",
            "notes": [{ "id": 4, "body": "This block", "author": { "username": "alex" }, "created_at": "2026-10-06T11:00:00Z",
                "position": { "new_path": "api/export.py", "new_line": 48, "line_range": { "start": { "new_line": 42 }, "end": { "new_line": 48 } } } }]
        }))
        .unwrap();
        let d = discussion(raw).unwrap();
        assert_eq!((d.start_line, d.line), (Some(42), Some(48)));
    }

    #[test]
    fn codes_the_ends_of_a_range_as_gitlab_does() {
        let end = range_end("api/export.py", None, Some(42), (40, 42));
        assert_eq!(end, serde_json::json!({ "line_code": "41c02c9c5fcee96876b8012659103720d7439f62_40_42", "type": "new", "old_line": null, "new_line": 42 }));
        assert_eq!(range_end("api/export.py", Some(7), Some(9), (7, 9))["type"], serde_json::Value::Null);
        assert_eq!(range_end("api/export.py", Some(7), None, (7, 8))["type"], "old");
    }

    #[test]
    fn toggles_the_draft_prefix() {
        assert_eq!(draft_title("feat: export | ABC-9012", true), "Draft: feat: export | ABC-9012");
        assert_eq!(draft_title("Draft: feat: export", true), "Draft: feat: export");
        assert_eq!(draft_title("Draft: feat: export", false), "feat: export");
        assert_eq!(draft_title("[Draft] fix", false), "fix");
        assert_eq!(draft_title("(draft) fix", false), "fix");
        assert_eq!(draft_title("Drafting the API", false), "Drafting the API");
    }

    #[test]
    fn reads_upload_paths() {
        let secret = "0123456789abcdef0123456789abcdef";
        assert_eq!(upload_parts(&format!("/uploads/{secret}/screen%20shot.png")), Some((secret.into(), "screen%20shot.png".into())));
        assert_eq!(upload_parts(&format!("https://gitlab.example.com/g/p/uploads/{secret}/a.png?w=1")), Some((secret.into(), "a.png".into())));
        assert_eq!(upload_parts("/uploads/../../etc/passwd"), None);
        assert_eq!(upload_parts("https://example.com/a.png"), None);
    }

    #[test]
    fn reads_the_approval_history_from_system_notes() {
        let note = |id: u64, body: &str, at: &str| serde_json::from_value::<RawNote>(serde_json::json!({ "id": id, "body": body, "author": { "username": "ana", "name": "Ana" }, "created_at": at, "system": true })).unwrap();
        let notes = [
            note(1, "approved this merge request", "2026-10-06T10:00:00Z"),
            note(2, "added 2 commits\n\n<ul><li>abc - fix</li></ul>", "2026-10-06T11:00:00Z"),
            note(3, "reset approvals from @ana by pushing to the branch", "2026-10-06T11:00:01Z"),
            note(4, "unapproved this merge request", "2026-10-06T12:00:00Z"),
            note(5, "added ~bug label", "2026-10-06T12:30:00Z"),
            note(6, "requested changes", "2026-10-06T13:00:00Z"),
        ];
        let history = approval_history(notes.iter().filter_map(approval_event).collect());
        let kinds: Vec<_> = history.iter().map(|e| (e.kind, e.commits, e.whom.as_deref())).collect();
        assert_eq!(kinds, [
            (ApprovalKind::Approved, 0, None),
            (ApprovalKind::Pushed, 2, None),
            (ApprovalKind::Reset, 0, Some("@ana")),
            (ApprovalKind::Unapproved, 0, None),
            (ApprovalKind::ChangesRequested, 0, None),
        ]);
        let human = serde_json::from_value::<RawNote>(serde_json::json!({ "id": 7, "body": "approved this merge request", "author": { "username": "x" }, "created_at": "2026-10-06T10:00:00Z" })).unwrap();
        assert!(approval_event(&human).is_none(), "a comment saying so is not an approval");
    }

    #[test]
    fn drops_system_only_threads() {
        let raw: RawDiscussion = serde_json::from_value(serde_json::json!({
            "id": "ab", "notes": [{ "id": 1, "body": "changed the description", "author": { "username": "x" }, "created_at": "2026-10-06T10:00:00Z", "system": true }]
        }))
        .unwrap();
        assert!(discussion(raw).is_none());
    }

    #[test]
    fn members_without_bots_nor_duplicates() {
        let m = |id: u64, username: &str, state: &str| RawMember { id, username: username.into(), name: username.into(), avatar_url: None, state: state.into() };
        let people = people_of(vec![m(1, "alice", "active"), m(2, "project_42_bot_abc", "active"), m(3, "bob", "blocked"), m(1, "alice", "active"), m(4, "renovate-bot", "active"), m(5, "carol", "active")]);
        assert_eq!(people.iter().map(|p| p.username.as_str()).collect::<Vec<_>>(), ["alice", "carol"]);
    }
}

