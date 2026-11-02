//! My worklogs, as the Time block's entries: those I logged on any issue over the last
//! `time_history_days`, found by JQL (`worklogAuthor = currentUser()`), then read issue by issue.
//! An entry's id is `<issue key>:<worklog id>`; moving an entry to another issue deletes it and
//! logs it again there, as Jira moves no worklog.

use prism_base::i18n::tr;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use super::{adf, date, edit::document, encode, Actions};
use crate::{time::NewEntry, state::TimeEntry};

/// At most this many issues read for their worklogs on a pass.
const MAX_ISSUES: usize = 100;

impl Actions {
    /// My worklogs since `since`, most recent first.
    pub async fn worklogs(&self, since: DateTime<Utc>) -> Result<Vec<TimeEntry>> {
        let api = self.api().await?;
        let me = self.shared.me.read().unwrap().clone().context(tr("Jira user not known yet"))?;
        let days = (Utc::now() - since).num_days().max(1);
        let issues = self.search(&api, &format!("worklogAuthor = currentUser() AND worklogDate >= -{days}d ORDER BY updated DESC")).await?;
        let mut entries = Vec::new();
        for issue in issues.iter().take(MAX_ISSUES) {
            let path = format!("/rest/api/3/issue/{}/worklog?startedAfter={}&maxResults=5000", encode(&issue.id), since.timestamp_millis());
            let logs: Worklogs = api.get(&path).await?;
            entries.extend(logs.worklogs.into_iter().filter(|w| w.author.as_ref().is_some_and(|a| a.account_id == me)).map(|w| TimeEntry {
                id: format!("{}:{}", issue.id, w.id),
                ticket: Some(issue.id.clone()),
                task_name: Some(issue.name.clone()),
                task_url: Some(issue.url.clone()),
                start: w.started.as_deref().and_then(date).unwrap_or_else(Utc::now),
                duration_ms: w.time_spent_seconds * 1000,
                description: w.comment.as_ref().map(adf::markdown).unwrap_or_default(),
            }));
        }
        entries.sort_by_key(|a| std::cmp::Reverse(a.start));
        Ok(entries)
    }

    pub async fn log_work(&self, entry: &NewEntry) -> Result<()> {
        let key = entry.ticket.trim().to_uppercase();
        anyhow::ensure!(!key.is_empty(), "which issue?");
        tracing::info!("jira: {} min logged on {key}", entry.minutes);
        self.api().await?.send(reqwest::Method::POST, &format!("/rest/api/3/issue/{}/worklog", encode(&key)), body(entry)).await?;
        Ok(())
    }

    /// Edits an entry; another issue than its own moves it there. An empty ticket keeps its own.
    pub async fn update_work(&self, id: &str, entry: &NewEntry) -> Result<()> {
        let (key, worklog) = split(id)?;
        let wanted = entry.ticket.trim().to_uppercase();
        if !wanted.is_empty() && wanted != key {
            self.log_work(entry).await?;
            return self.delete_work(id).await;
        }
        tracing::info!("jira: worklog {worklog} of {key} edited");
        self.api().await?.send(reqwest::Method::PUT, &format!("/rest/api/3/issue/{}/worklog/{}", encode(key), encode(worklog)), body(entry)).await?;
        Ok(())
    }

    pub async fn delete_work(&self, id: &str) -> Result<()> {
        let (key, worklog) = split(id)?;
        tracing::info!("jira: worklog {worklog} of {key} deleted");
        self.api().await?.send(reqwest::Method::DELETE, &format!("/rest/api/3/issue/{}/worklog/{}", encode(key), encode(worklog)), serde_json::Value::Null).await?;
        Ok(())
    }
}

fn split(id: &str) -> Result<(&str, &str)> {
    id.split_once(':').with_context(|| format!("\"{id}\" is not a Jira worklog"))
}

/// A worklog's fields: its start in Jira's format, its duration, its comment.
fn body(entry: &NewEntry) -> serde_json::Value {
    let mut body = json!({ "started": started(entry.start), "timeSpentSeconds": u64::from(entry.minutes) * 60 });
    if !entry.description.trim().is_empty() {
        body["comment"] = document(&entry.description);
    }
    body
}

/// `2026-10-11T09:00:00.000+0000`: Jira takes no RFC 3339 offset.
fn started(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string()
}

#[derive(Deserialize)]
struct Worklogs {
    #[serde(default)]
    worklogs: Vec<RawWorklog>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawWorklog {
    id: String,
    author: Option<super::RawUser>,
    started: Option<String>,
    #[serde(default)]
    time_spent_seconds: i64,
    comment: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worklog_starts_as_jira_writes_it() {
        let at = DateTime::parse_from_rfc3339("2026-10-11T09:30:00Z").unwrap().with_timezone(&Utc);
        assert_eq!(started(at), "2026-10-11T09:30:00.000+0000");
        assert_eq!(date(&started(at)), Some(at));
    }

    #[test]
    fn an_entry_id_names_its_issue() {
        assert_eq!(split("ABC-12:10023").unwrap(), ("ABC-12", "10023"));
        assert!(split("10023").is_err());
    }

    #[test]
    fn a_worklog_body_has_its_comment_only_when_there_is_one() {
        let entry = |d: &str| NewEntry { ticket: "ABC-1".into(), start: Utc::now(), minutes: 90, description: d.into() };
        assert_eq!(body(&entry(""))["timeSpentSeconds"], 5400);
        assert!(body(&entry("")).get("comment").is_none());
        assert_eq!(body(&entry("Review"))["comment"]["type"], "doc");
    }
}
