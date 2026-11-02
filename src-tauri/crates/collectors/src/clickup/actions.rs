//! ClickUp's actions on a ticket, behind the tracker: its content (`task_view::TaskView`), read
//! when it is opened, and the changes made from the window.

use prism_base::{i18n::tr, tr};
use std::sync::LazyLock;

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::Deserialize;

use super::{list_statuses, ms, task_type, task_type_icon, Api, RawStatus, RawUser, TeamId};
use crate::{
    task_view::{all_tickets, update_ticket, Attachment, Checklist, ChecklistItem, Comment, Subtask, TaskView},
    state::{Hub, Person, Priority, StatusOption},
    ticket,
};

/// Internal ClickUp id (`86c2abcde`), the one used by subtasks and parents.
static TASK_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]{6,16}$").unwrap());

/// ClickUp ticket actions triggered from the window. The workspace is shared with the collector,
/// which resolves it, and with time tracking.
#[derive(Clone, Default)]
pub struct Actions {
    team: TeamId,
}

impl Actions {
    pub fn new(team: TeamId) -> Self {
        Self { team }
    }

    /// Immediate, full re-read of ClickUp (lists, sprint, slow queues).
    pub async fn refresh(&self) -> Result<()> {
        self.team.refresh().await
    }

    /// `id`: `ABC-1234` or an internal id (subtask, parent).
    pub async fn ticket(&self, id: &str) -> Result<TaskView> {
        fetch(&self.team, id).await
    }

    /// The tickets of the followed spaces whose id or name has the words typed.
    pub async fn search(&self, query: &str) -> Result<Vec<crate::state::Ticket>> {
        super::search::search(&self.team, query).await
    }

    pub async fn image(&self, url: &str) -> Result<String> {
        image(url).await
    }

    pub async fn set_status(&self, hub: &Hub, task_id: &str, status: &str) -> Result<()> {
        set_status(hub, &self.team, task_id, status).await
    }

    pub async fn set_priority(&self, hub: &Hub, task_id: &str, priority: Option<Priority>) -> Result<()> {
        set_priority(hub, task_id, priority).await
    }

    pub async fn set_mine(&self, hub: &Hub, task_id: &str, assign: bool) -> Result<()> {
        set_mine(hub, task_id, assign).await
    }

    /// A ticket's estimate in hours (`None`: cleared): its time estimate (`field` `None`), or one of
    /// its number custom fields.
    pub async fn set_estimate(&self, hub: &Hub, task_id: &str, field: Option<&str>, hours: Option<f64>) -> Result<()> {
        match field {
            None => set_time_estimate(hub, task_id, hours).await,
            Some(field) => set_estimate_field(hub, task_id, field, hours).await,
        }
    }

    pub async fn add_to_sprint(&self, hub: &Hub, task_id: &str) -> Result<()> {
        add_to_sprint(hub, task_id).await
    }

    pub async fn add_comment(&self, task_id: &str, text: &str) -> Result<()> {
        add_comment(task_id, text).await
    }

    pub async fn edit_comment(&self, hub: &Hub, task_id: &str, comment: &str, text: &str) -> Result<()> {
        edit_comment(hub, task_id, comment, text).await
    }
}

async fn fetch(team: &TeamId, id: &str) -> Result<TaskView> {
    let api = Api::new().await?;
    let path = match ticket::find([id]) {
        Some(custom) => {
            let team = team.get().context(tr("ClickUp workspace not resolved yet"))?;
            format!("/task/{custom}?custom_task_ids=true&team_id={team}&include_subtasks=true&include_markdown_description=true")
        }
        None if TASK_ID.is_match(id) => format!("/task/{id}?include_subtasks=true&include_markdown_description=true"),
        None => bail!("\"{id}\" is not a ClickUp ticket id"),
    };
    let task: RawTask = api.get(&path).await?;
    // Comments and statuses are a bonus: their failure must not deprive us of the ticket.
    let comments = match api.get::<Comments>(&format!("/task/{}/comment", task.id)).await {
        Ok(c) => c.comments,
        Err(e) => {
            tracing::warn!("clickup: comments of {id}: {e:#}");
            Vec::new()
        }
    };
    let statuses = match &task.list {
        Some(list) => match list_statuses(&api, &list.id).await {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("clickup: statuses of {id}: {e:#}");
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    Ok(view(task, comments, statuses))
}

/// Changes a ticket's status, then reflects it in the snapshot right away without waiting for the
/// next collection.
async fn set_status(hub: &Hub, team: &TeamId, task_id: &str, status: &str) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    tracing::info!("clickup: {task_id} → {status}");
    let task: StatusOnly = Api::new().await?.put(&format!("/task/{task_id}"), serde_json::json!({ "status": status })).await?;
    let (status, closed) = (task.status.status.to_lowercase(), task.status.is_closed());
    // A ticket that left the refinement statuses stays in the queue until it is re-read: the
    // front end filters it by its new status.
    update_ticket(hub, task_id, |t| {
        let changed = t.status != status || t.closed != closed;
        t.status.clone_from(&status);
        t.closed = closed;
        changed
    });
    // The ticket may enter a queue read filtered by status ("To pick"): re-read immediately.
    team.refresh_queues();
    Ok(())
}

/// Sets a ticket's priority (`None`: none), then reflects it in the snapshot. ClickUp numbers its
/// four from the most urgent: 1 to 4.
async fn set_priority(hub: &Hub, task_id: &str, priority: Option<Priority>) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    let level = priority.map(|p| p.rank() + 1);
    tracing::info!("clickup: {task_id} priority → {}", priority.map_or("none", Priority::name));
    let task: PriorityOnly = Api::new().await?.put(&format!("/task/{task_id}"), serde_json::json!({ "priority": level })).await?;
    let now = task.priority.and_then(|p| Priority::of(&p.priority));
    update_ticket(hub, task_id, |t| {
        let changed = t.priority != now;
        t.priority.clone_from(&now);
        changed
    });
    Ok(())
}

/// Sets a ticket's time estimate (ClickUp's own, in milliseconds), then reflects it in the snapshot.
async fn set_time_estimate(hub: &Hub, task_id: &str, hours: Option<f64>) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    let ms = hours.map(|h| (h * 3_600_000.0).round() as u64);
    tracing::info!("clickup: {task_id} time estimate → {}", hours.map_or("none".into(), |h| format!("{h} h")));
    let task: EstimateOnly = Api::new().await?.put(&format!("/task/{task_id}"), serde_json::json!({ "time_estimate": ms })).await?;
    let now = task.time_estimate.filter(|ms| *ms > 0);
    update_ticket(hub, task_id, |t| {
        let changed = t.time_estimate != now;
        t.time_estimate = now;
        changed
    });
    Ok(())
}

/// Sets one of a ticket's number custom fields (by name) to an estimate in hours, or clears it; the
/// field is found on the task itself, never written to when it is not a number.
async fn set_estimate_field(hub: &Hub, task_id: &str, field: &str, hours: Option<f64>) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    let api = Api::new().await?;
    let task: FieldsOnly = api.get(&format!("/task/{task_id}")).await?;
    let raw = task.custom_fields.iter().find(|f| f.named(field)).with_context(|| tr!("the ticket has no custom field \"{field}\"", field = field))?;
    if !super::fields::holds_hours(raw.kind()) {
        bail!("{}", tr!("\"{field}\" is a {kind} field, not a number: Prism does not write to it", field = field, kind = raw.kind()));
    }
    let path = format!("/task/{task_id}/field/{}", raw.id);
    tracing::info!("clickup: {task_id} {field} → {}", hours.map_or("none".into(), |h| format!("{h} h")));
    let _: serde_json::Value = match hours {
        Some(h) => api.post(&path, serde_json::json!({ "value": h })).await?,
        None => api.delete(&path).await?,
    };
    let label = hours.map(hours_label);
    update_ticket(hub, task_id, |t| {
        let Some(f) = t.fields.iter_mut().find(|f| f.name.eq_ignore_ascii_case(field.trim())) else { return false };
        let now: Vec<_> = label.iter().map(|l| crate::state::FieldValue { label: l.clone(), color: None, order: None }).collect();
        let changed = f.values != now;
        f.values = now;
        changed
    });
    Ok(())
}

/// Hours as ClickUp shows a number: `7`, `3.5`.
fn hours_label(h: f64) -> String {
    if h.fract() == 0.0 { format!("{h:.0}") } else { format!("{}", (h * 100.0).round() / 100.0) }
}

/// Assigns a ticket to me, or removes me from it (`assign` false). Other assignees are unchanged.
async fn set_mine(hub: &Hub, task_id: &str, assign: bool) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    let me = hub.subscribe().borrow().tracker_me.clone().context(tr("ClickUp user not known yet"))?;
    tracing::info!("clickup: {task_id} {} {}", if assign { "assigned to" } else { "unassigned from" }, me.name);
    let change = if assign { serde_json::json!({ "add": [me.id], "rem": [] }) } else { serde_json::json!({ "add": [], "rem": [me.id] }) };
    let task: AssigneesOnly = Api::new().await?.put(&format!("/task/{task_id}"), serde_json::json!({ "assignees": change })).await?;
    let assignees: Vec<Person> = task.assignees.into_iter().map(Person::from).collect();
    update_ticket(hub, task_id, |t| {
        let changed = t.assignees != assignees;
        t.assignees.clone_from(&assignees);
        changed
    });
    Ok(())
}

/// Adds a ticket to the current sprint, on top of its list (ClickUp's "Tasks in Multiple
/// Lists"): it stays in Architecture, Bugs… and also shows up in the sprint, with its status.
/// The snapshot follows right away: the ticket joins the sprint without leaving the other blocks.
async fn add_to_sprint(hub: &Hub, task_id: &str) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    let snap = hub.subscribe().borrow().clone();
    let sprint = snap.sprint.as_ref().context(tr("no current sprint"))?;
    if sprint.list_id.is_empty() {
        bail!("{}", tr("the current sprint list is not known"));
    }
    if sprint.tickets.iter().any(|t| t.task_id == task_id) {
        bail!("{}", tr!("already in {sprint}", sprint = sprint.name));
    }
    let ticket = all_tickets(&snap).find(|t| t.task_id == task_id).cloned().context(tr("ticket not found in the dashboard"))?;
    tracing::info!("clickup: {} added to {}", ticket.id, sprint.name);
    let _: serde_json::Value = Api::new().await?.post(&format!("/list/{}/task/{task_id}", sprint.list_id), serde_json::json!({})).await?;
    hub.update(|s| {
        let Some(sp) = s.sprint.as_mut() else { return false };
        sp.tickets.push(ticket.clone());
        true
    });
    Ok(())
}

/// Id of a ClickUp comment: digits.
fn comment_id(id: &str) -> Result<()> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
        bail!("\"{id}\" is not a ClickUp comment id");
    }
    Ok(())
}

/// Comments on a ticket (plain text, without notifying everyone).
async fn add_comment(task_id: &str, text: &str) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    let text = text.trim();
    if text.is_empty() {
        bail!("{}", tr("empty comment"));
    }
    tracing::info!("clickup: comment on {task_id}");
    let _: serde_json::Value = Api::new().await?.post(&format!("/task/{task_id}/comment"), serde_json::json!({ "comment_text": text, "notify_all": false })).await?;
    Ok(())
}

/// Edits one of my comments on this ticket. The ticket's comments are re-read first:
/// only a comment that belongs to it, and that I authored, is accepted. ClickUp
/// then replaces the text: the original rich formatting (mentions, bold) is lost.
async fn edit_comment(hub: &Hub, task_id: &str, comment: &str, text: &str) -> Result<()> {
    if !TASK_ID.is_match(task_id) {
        bail!("\"{task_id}\" is not a ClickUp task id");
    }
    comment_id(comment)?;
    let text = text.trim();
    if text.is_empty() {
        bail!("{}", tr("empty comment"));
    }
    let me = hub.subscribe().borrow().tracker_me.clone().context(tr("ClickUp user not known yet"))?;
    let api = Api::new().await?;
    let all: Comments = api.get(&format!("/task/{task_id}/comment")).await?;
    let found = all.comments.into_iter().find(|c| c.id == comment).context(tr("comment not found on this ticket"))?;
    if found.user.map(Person::from).is_none_or(|p| p.id != me.id) {
        bail!("{}", tr("only your own comments can be edited"));
    }
    tracing::info!("clickup: edit comment {comment} on {task_id}");
    let _: serde_json::Value = api.put(&format!("/comment/{comment}"), serde_json::json!({ "comment_text": text })).await?;
    Ok(())
}

fn view(t: RawTask, comments: Vec<RawComment>, statuses: Vec<StatusOption>) -> TaskView {
    let description = t
        .markdown_description
        .filter(|d| !d.trim().is_empty())
        .or(t.text_content)
        .unwrap_or_default()
        .trim()
        .to_string();
    TaskView {
        id: t.custom_id.clone().unwrap_or_else(|| t.id.clone()),
        task_id: t.id,
        name: t.name,
        task_type: task_type(t.custom_item_id),
        task_type_icon: task_type_icon(t.custom_item_id),
        closed: t.status.is_closed(),
        status: t.status.status.to_lowercase(),
        status_color: t.status.color,
        statuses,
        priority: t.priority.and_then(|p| Priority::of(&p.priority)),
        assignees: t.assignees.into_iter().map(Person::from).collect(),
        creator: t.creator.map(Person::from),
        tags: t.tags.into_iter().map(|t| t.name).collect(),
        list: t.list.map(|l| l.name),
        parent: t.parent,
        url: t.url,
        description,
        created_at: ms(t.date_created.as_deref()),
        updated_at: ms(t.date_updated.as_deref()),
        due_at: ms(t.due_date.as_deref()),
        time_estimate: t.time_estimate,
        time_spent: t.time_spent.filter(|ms| *ms > 0),
        fields: super::fields::shown(&t.custom_fields),
        checklists: t
            .checklists
            .into_iter()
            .map(|c| Checklist {
                name: c.name,
                items: c.items.into_iter().map(|i| ChecklistItem { name: i.name, resolved: i.resolved }).collect(),
            })
            .collect(),
        subtasks: t
            .subtasks
            .into_iter()
            .map(|s| Subtask {
                id: s.custom_id.unwrap_or_else(|| s.id.clone()),
                task_id: s.id,
                name: s.name,
                closed: s.status.is_closed(),
                status: s.status.status.to_lowercase(),
            })
            .collect(),
        attachments: t
            .attachments
            .into_iter()
            .map(|a| {
                let ext = a.extension.map(|e| e.to_lowercase());
                let image = a.mimetype.as_deref().is_some_and(|m| m.starts_with("image/"))
                    || ext.as_deref().is_some_and(|e| IMAGE_EXT.contains(&e));
                Attachment {
                    title: a.title.unwrap_or_else(|| "attachment".into()),
                    url: a.url,
                    extension: ext,
                    image,
                    size: a.size.and_then(|s| s.as_u64().or_else(|| s.as_str()?.parse().ok())),
                    thumbnail: a.thumbnail_large.or(a.thumbnail_medium).filter(|_| image),
                    date: ms(a.date.as_deref()),
                }
            })
            .collect(),
        // ClickUp returns them newest first.
        comments: comments
            .into_iter()
            .rev()
            .map(|c| Comment {
                id: c.id,
                author: c.user.map(Person::from),
                text: c.comment_text.trim().to_string(),
                date: ms(c.date.as_deref()),
                replies: c.reply_count.unwrap_or(0),
            })
            .filter(|c| !c.text.is_empty())
            .collect(),
    }
}

#[derive(Deserialize)]
struct RawTask {
    id: String,
    custom_id: Option<String>,
    name: String,
    status: RawStatus,
    #[serde(default)]
    assignees: Vec<RawUser>,
    creator: Option<RawUser>,
    #[serde(default)]
    tags: Vec<RawTag>,
    list: Option<RawList>,
    parent: Option<String>,
    custom_item_id: Option<u64>,
    url: String,
    text_content: Option<String>,
    markdown_description: Option<String>,
    priority: Option<RawPriority>,
    date_created: Option<String>,
    date_updated: Option<String>,
    due_date: Option<String>,
    time_estimate: Option<u64>,
    #[serde(default)]
    time_spent: Option<u64>,
    #[serde(default)]
    custom_fields: Vec<super::fields::RawField>,
    #[serde(default)]
    checklists: Vec<RawChecklist>,
    #[serde(default)]
    subtasks: Vec<RawSubtask>,
    #[serde(default)]
    attachments: Vec<RawAttachment>,
}

#[derive(Deserialize)]
struct RawAttachment {
    title: Option<String>,
    url: String,
    extension: Option<String>,
    mimetype: Option<String>,
    /// Number or string depending on the API version.
    size: Option<serde_json::Value>,
    thumbnail_large: Option<String>,
    thumbnail_medium: Option<String>,
    date: Option<String>,
}

const IMAGE_EXT: [&str; 7] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];

/// Domains ClickUp files come from: only those are downloaded.
fn clickup_host(host: &str) -> bool {
    host == "clickup.com"
        || host.ends_with(".clickup.com")
        || host == "clickup-attachments.com"
        || host.ends_with(".clickup-attachments.com")
}

/// Beyond this, the image is not displayed (link only).
const IMAGE_MAX_BYTES: usize = 15 * 1024 * 1024;

/// A ticket image (attachment or description image), as a `data:` URL for the window,
/// whose CSP allows no external image. Nothing is written to disk. Without authentication
/// first; with the ClickUp token if the file requires it (the token only goes to ClickUp).
async fn image(url: &str) -> Result<String> {
    use base64::Engine;
    let parsed = reqwest::Url::parse(url).context("bad image URL")?;
    if parsed.scheme() != "https" || !parsed.host_str().is_some_and(clickup_host) {
        bail!("not a ClickUp file: {}", parsed.host_str().unwrap_or("?"));
    }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?;
    let mut resp = http.get(parsed.clone()).send().await?;
    if matches!(resp.status().as_u16(), 401 | 403) {
        let token = crate::clickup::token().await?;
        resp = http.get(parsed).header("Authorization", token).send().await?;
    }
    if !resp.status().is_success() {
        bail!("ClickUp file: HTTP {}", resp.status());
    }
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or(v).trim().to_lowercase())
        .unwrap_or_default();
    // An SVG can carry script: never displayed inline.
    if !mime.starts_with("image/") || mime == "image/svg+xml" {
        bail!("{}", tr!("not a displayable image ({mime})", mime = mime));
    }
    if resp.content_length().is_some_and(|n| n as usize > IMAGE_MAX_BYTES) {
        bail!("{}", tr("image too large to preview"));
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > IMAGE_MAX_BYTES {
        bail!("{}", tr("image too large to preview"));
    }
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes)))
}

#[derive(Deserialize)]
struct RawPriority {
    priority: String,
}

#[derive(Deserialize)]
struct RawTag {
    name: String,
}

#[derive(Deserialize)]
struct RawList {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct StatusOnly {
    status: RawStatus,
}

#[derive(Deserialize)]
struct EstimateOnly {
    time_estimate: Option<u64>,
}

#[derive(Deserialize)]
struct FieldsOnly {
    #[serde(default)]
    custom_fields: Vec<super::fields::RawField>,
}

#[derive(Deserialize)]
struct PriorityOnly {
    priority: Option<RawPriority>,
}

#[derive(Deserialize)]
struct AssigneesOnly {
    #[serde(default)]
    assignees: Vec<RawUser>,
}

#[derive(Deserialize)]
struct RawChecklist {
    name: String,
    #[serde(default)]
    items: Vec<RawChecklistItem>,
}

#[derive(Deserialize)]
struct RawChecklistItem {
    name: String,
    #[serde(default)]
    resolved: bool,
}

#[derive(Deserialize)]
struct RawSubtask {
    id: String,
    custom_id: Option<String>,
    name: String,
    status: RawStatus,
}

#[derive(Deserialize)]
struct Comments {
    #[serde(default)]
    comments: Vec<RawComment>,
}

#[derive(Deserialize)]
struct RawComment {
    #[serde(default)]
    id: String,
    #[serde(default)]
    comment_text: String,
    user: Option<RawUser>,
    date: Option<String>,
    reply_count: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_clickup_task() {
        let task: RawTask = serde_json::from_value(serde_json::json!({
            "id": "86c2abcde", "custom_id": "ABC-42", "name": "Export", "url": "https://app.clickup.com/t/86c2abcde",
            "status": { "status": "In Review", "color": "#7c4dff", "type": "custom" },
            "priority": null, "assignees": [], "tags": [{ "name": "backend" }], "list": { "id": "901", "name": "Sprint 24" },
            "text_content": "plain", "markdown_description": "**rich**", "time_estimate": 7200000,
            "checklists": [{ "name": "DoD", "items": [{ "name": "tests", "resolved": true }] }],
            "subtasks": [{ "id": "86c2abcdf", "custom_id": null, "name": "API", "status": { "status": "done", "type": "closed" } }]
        }))
        .unwrap();
        let comments: Comments = serde_json::from_value(serde_json::json!({ "comments": [
            { "id": "2", "comment_text": "second", "date": "2000" }, { "id": "1", "comment_text": "first", "date": "1000" }, { "comment_text": " " }
        ]}))
        .unwrap();
        let v = view(task, comments.comments, Vec::new());
        assert_eq!(v.id, "ABC-42");
        assert_eq!(v.status, "in review");
        assert_eq!(v.description, "**rich**");
        assert_eq!(v.tags, ["backend"]);
        assert_eq!(v.subtasks[0].id, "86c2abcdf");
        assert!(v.subtasks[0].closed);
        assert_eq!(v.comments.iter().map(|c| (c.id.as_str(), c.text.as_str())).collect::<Vec<_>>(), [("1", "first"), ("2", "second")]);
    }

    #[test]
    fn reads_a_priority_change() {
        let set: PriorityOnly = serde_json::from_value(serde_json::json!({ "priority": { "id": "2", "priority": "High", "color": "#ffcc00" } })).unwrap();
        assert_eq!(set.priority.and_then(|p| Priority::of(&p.priority)), Some(Priority::High));
        let cleared: PriorityOnly = serde_json::from_value(serde_json::json!({ "priority": null })).unwrap();
        assert!(cleared.priority.is_none());
    }

    #[test]
    fn checks_comment_ids() {
        assert!(comment_id("90120012345678").is_ok());
        assert!(comment_id("").is_err());
        assert!(comment_id("12/../34").is_err());
    }

    #[test]
    fn only_clickup_hosts_are_fetched() {
        assert!(clickup_host("t9015.p.clickup-attachments.com"));
        assert!(clickup_host("attachments.clickup.com"));
        assert!(!clickup_host("clickup.com.evil.io"));
        assert!(!clickup_host("evilclickup.com"));
    }

    #[test]
    fn task_ids_are_checked_before_reaching_the_url() {
        assert!(TASK_ID.is_match("86c2abcde"));
        assert!(!TASK_ID.is_match("../team"));
        assert!(!TASK_ID.is_match("86c2?x=1"));
    }
}
