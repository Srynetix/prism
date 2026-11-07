//! Changes made to Jira issues from the window: status (through a transition of its workflow),
//! priority, comments. Each change is reflected in the snapshot without waiting for the next pass.

use prism_base::{i18n::tr, tr};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{priority, Actions};
use crate::{task_view::update_ticket, state::Hub};

impl Actions {
    /// Moves an issue to `status` (lowercased, as Prism names it), by the transition of its
    /// workflow that leads there: Jira sets no status directly.
    pub async fn set_status(&self, hub: &Hub, key: &str, status: &str) -> Result<()> {
        let api = self.api().await?;
        let path = format!("/rest/api/3/issue/{}/transitions", super::encode(key));
        let offered: Transitions = api.get(&path).await?;
        let Some(t) = offered.transitions.iter().find(|t| t.to.name.eq_ignore_ascii_case(status)) else {
            let names: Vec<String> = offered.transitions.iter().map(|t| t.to.name.to_lowercase()).collect();
            bail!("{}", tr!("Jira's workflow does not lead {key} to \"{status}\" from where it is; it offers: {offers}", key = key, status = status, offers = if names.is_empty() { tr("nothing").into() } else { names.join(", ") }));
        };
        tracing::info!("jira: {key} → {status}");
        api.send(reqwest::Method::POST, &path, json!({ "transition": { "id": t.id } })).await?;
        let closed = t.to.done();
        let status = t.to.name.to_lowercase();
        update_ticket(hub, key, |ticket| {
            let changed = ticket.status != status || ticket.closed != closed;
            ticket.status.clone_from(&status);
            ticket.closed = closed;
            changed
        });
        self.queues_now();
        Ok(())
    }

    /// Sets an issue's priority: the first of the instance's priorities (most urgent first) that
    /// reads as `wanted` in Prism. A Jira issue always has one: `None` is refused.
    pub async fn set_priority(&self, hub: &Hub, key: &str, wanted: Option<crate::state::Priority>) -> Result<()> {
        let wanted = wanted.context("a Jira issue always has a priority")?;
        let api = self.api().await?;
        let all: Vec<Priority> = api.get("/rest/api/3/priority").await?;
        let chosen = all.iter().find(|p| priority(&p.name) == Some(wanted)).with_context(|| tr!("no Jira priority reads as \"{wanted}\" (Jira has {all})", wanted = wanted.name(), all = all.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))?;
        tracing::info!("jira: {key} priority → {}", chosen.name);
        api.send(reqwest::Method::PUT, &format!("/rest/api/3/issue/{}", super::encode(key)), json!({ "fields": { "priority": { "id": chosen.id } } })).await?;
        let now = Some(wanted);
        update_ticket(hub, key, |t| {
            let changed = t.priority != now;
            t.priority.clone_from(&now);
            changed
        });
        Ok(())
    }

    /// Sets an issue's original estimate, in hours (Jira's time tracking, to the minute). Jira keeps
    /// one once set: `None` is refused, 0 is not.
    pub async fn set_estimate(&self, hub: &Hub, key: &str, hours: Option<f64>) -> Result<()> {
        let hours = hours.context(tr("Jira keeps an original estimate once set: enter 0 instead"))?;
        let minutes = (hours * 60.0).round() as u64;
        let text = match (minutes / 60, minutes % 60) {
            (0, m) => format!("{m}m"),
            (h, 0) => format!("{h}h"),
            (h, m) => format!("{h}h {m}m"),
        };
        tracing::info!("jira: {key} estimate → {text}");
        let api = self.api().await?;
        api.send(reqwest::Method::PUT, &format!("/rest/api/3/issue/{}", super::encode(key)), json!({ "fields": { "timetracking": { "originalEstimate": text } } })).await?;
        let now = Some(minutes * 60_000);
        update_ticket(hub, key, |t| {
            let changed = t.time_estimate != now;
            t.time_estimate = now;
            changed
        });
        Ok(())
    }

    /// Assigns an issue to me, or unassigns it (`assign` false; only when it is mine). A Jira
    /// issue has one assignee: assigning replaces the one it had.
    pub async fn set_mine(&self, hub: &Hub, key: &str, assign: bool) -> Result<()> {
        let account = self.shared.me.read().unwrap().clone().context(tr("Jira user not known yet"))?;
        let me = hub.subscribe().borrow().tracker_me.clone().context(tr("Jira user not known yet"))?;
        tracing::info!("jira: {key} {}", if assign { "assigned to me" } else { "unassigned" });
        let api = self.api().await?;
        api.send(reqwest::Method::PUT, &format!("/rest/api/3/issue/{}/assignee", super::encode(key)), json!({ "accountId": if assign { Value::from(account) } else { Value::Null } })).await?;
        update_ticket(hub, key, |t| {
            let now = if assign { vec![me.clone()] } else { Vec::new() };
            let changed = t.assignees != now;
            t.assignees = now;
            changed
        });
        Ok(())
    }

    /// Moves an issue into the active sprint (a Jira issue is in one sprint at most).
    pub async fn add_to_sprint(&self, hub: &Hub, key: &str) -> Result<()> {
        let sprint = hub.subscribe().borrow().sprint.as_ref().map(|s| s.list_id.clone()).filter(|id| !id.is_empty()).context(tr("the board has no active sprint"))?;
        tracing::info!("jira: {key} → sprint {sprint}");
        let api = self.api().await?;
        api.send(reqwest::Method::POST, &format!("/rest/agile/1.0/sprint/{sprint}/issue"), json!({ "issues": [key] })).await?;
        self.queues_now();
        Ok(())
    }

    /// Creates an issue in the board's project (`[jira] issue_type`), its description in ADF,
    /// assigned to me if asked, in the active sprint if there is one, then moves it to the status
    /// asked if it starts elsewhere; returns its key.
    pub async fn create(&self, hub: &Hub, new: &crate::tracker::NewTicket) -> Result<String> {
        anyhow::ensure!(new.board == self.shared.cfg.board.to_string(), "board {} is not [jira] board", new.board);
        let title = new.title.trim();
        anyhow::ensure!(!title.is_empty(), "{}", tr("a title is needed"));
        let project = self.shared.project.read().unwrap().clone().context(tr("the board's project is not known: is it a board of one project?"))?;
        let me = if new.assign_me { Some(self.shared.me.read().unwrap().clone().context(tr("Jira user not known yet"))?) } else { None };
        let api = self.api().await?;
        let fields = new_fields(&project, self.shared.cfg.issue_type.trim(), title, &new.description, me.as_deref());
        let created: Created = serde_json::from_value(api.send(reqwest::Method::POST, "/rest/api/3/issue", json!({ "fields": fields })).await?).context("unreadable answer to the creation")?;
        tracing::info!("jira: {} created", created.key);
        let sprint = hub.subscribe().borrow().sprint.as_ref().map(|s| s.list_id.clone()).filter(|id| !id.is_empty());
        if let Some(sprint) = sprint {
            api.send(reqwest::Method::POST, &format!("/rest/agile/1.0/sprint/{sprint}/issue"), json!({ "issues": [created.key] })).await?;
        }
        let path = format!("/rest/api/3/issue/{}/transitions", super::encode(&created.key));
        let offered: Transitions = api.get(&path).await?;
        if let Some(t) = offered.transitions.iter().find(|t| t.to.name.eq_ignore_ascii_case(&new.status)) {
            api.send(reqwest::Method::POST, &path, json!({ "transition": { "id": t.id } })).await?;
        }
        // Read back now: the sprint shows it without waiting for the next pass.
        self.queues_now();
        Ok(created.key)
    }

    pub async fn add_comment(&self, key: &str, text: &str) -> Result<()> {
        tracing::info!("jira: comment on {key}");
        let api = self.api().await?;
        api.send(reqwest::Method::POST, &format!("/rest/api/3/issue/{}/comment", super::encode(key)), json!({ "body": document(text) })).await?;
        Ok(())
    }

    /// Replaces the text of one of my comments (Jira refuses another's).
    pub async fn edit_comment(&self, key: &str, comment: &str, text: &str) -> Result<()> {
        tracing::info!("jira: comment {comment} of {key} edited");
        let api = self.api().await?;
        api.send(reqwest::Method::PUT, &format!("/rest/api/3/issue/{}/comment/{}", super::encode(key), super::encode(comment)), json!({ "body": document(text) })).await?;
        Ok(())
    }
}

/// The fields of a new issue: its description (none if empty) and assignee (`me`, an account id).
fn new_fields(project: &str, issue_type: &str, title: &str, description: &str, me: Option<&str>) -> Value {
    let mut fields = json!({ "project": { "key": project }, "summary": title, "issuetype": { "name": issue_type } });
    if !description.trim().is_empty() {
        fields["description"] = document(description);
    }
    if let Some(me) = me {
        fields["assignee"] = json!({ "accountId": me });
    }
    fields
}

/// A comment's text as an ADF document: a paragraph per block of lines, its lines kept apart;
/// a fenced block (```) as code.
pub(super) fn document(text: &str) -> Value {
    let mut content = Vec::new();
    let mut lines = text.trim().lines().peekable();
    let mut para: Vec<&str> = Vec::new();
    let flush = |para: &mut Vec<&str>, content: &mut Vec<Value>| {
        if para.is_empty() {
            return;
        }
        let mut inline = Vec::new();
        for (n, line) in para.drain(..).enumerate() {
            if n > 0 {
                inline.push(json!({ "type": "hardBreak" }));
            }
            if !line.is_empty() {
                inline.push(json!({ "type": "text", "text": line }));
            }
        }
        content.push(json!({ "type": "paragraph", "content": inline }));
    };
    while let Some(line) = lines.next() {
        if let Some(lang) = line.trim_start().strip_prefix("```") {
            flush(&mut para, &mut content);
            let mut code = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                code.push(l);
            }
            let mut block = json!({ "type": "codeBlock", "content": [{ "type": "text", "text": code.join("\n") }] });
            if !lang.trim().is_empty() {
                block["attrs"] = json!({ "language": lang.trim() });
            }
            content.push(block);
        } else if line.trim().is_empty() {
            flush(&mut para, &mut content);
        } else {
            para.push(line);
        }
    }
    flush(&mut para, &mut content);
    json!({ "type": "doc", "version": 1, "content": content })
}

#[derive(Deserialize)]
struct Transitions {
    transitions: Vec<Transition>,
}

#[derive(Deserialize)]
struct Transition {
    id: String,
    to: super::RawStatus,
}

#[derive(Deserialize)]
struct Created {
    key: String,
}

#[derive(Deserialize)]
struct Priority {
    id: String,
    name: String,
}

#[cfg(test)]
mod tests {
    use super::{document, new_fields};
    use crate::jira::adf::markdown;

    #[test]
    fn a_comment_reads_back_the_same() {
        let text = "First line\nsecond line\n\n```sh\ntask deploy\n```\n\nThanks";
        assert_eq!(markdown(&document(text)), text);
    }

    #[test]
    fn a_comment_is_a_document_of_paragraphs() {
        let doc = document("a\nb");
        assert_eq!(doc["content"][0]["content"].as_array().unwrap().len(), 3);
        assert_eq!(doc["content"][0]["content"][1]["type"], "hardBreak");
    }

    /// A new issue: its description as ADF and me as assignee when given, neither otherwise.
    #[test]
    fn a_new_issue_carries_its_description_and_assignee() {
        let f = new_fields("PRJ", "Task", "Fix it", "Why\n\nHow", Some("acc-1"));
        assert_eq!((f["project"]["key"].as_str(), f["summary"].as_str(), f["issuetype"]["name"].as_str()), (Some("PRJ"), Some("Fix it"), Some("Task")));
        assert_eq!(f["description"], document("Why\n\nHow"));
        assert_eq!(f["assignee"]["accountId"], "acc-1");
        let bare = new_fields("PRJ", "Task", "Fix it", "  ", None);
        assert!(bare.get("description").is_none() && bare.get("assignee").is_none(), "{bare}");
    }
}
