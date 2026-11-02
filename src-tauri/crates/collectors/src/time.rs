use prism_base::{i18n::tr, tr};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::{
    clickup::{Api, TeamId},
    state::{Hub, TimeEntry, TimeTracking},
    ticket,
};

const SOURCE: &str = "time";
/// Entries can also be made from ClickUp: we resync regularly;
/// those made from Prism are read back immediately.
const POLL: Duration = Duration::from_secs(60);
const JIRA_POLL: Duration = Duration::from_secs(180);

/// ClickUp time tracking: reads the entries since `history_start`, and manual entry
/// from Prism, which immediately resyncs the displayed state.
#[derive(Clone)]
pub struct TimeService {
    hub: Hub,
    team: TeamId,
    /// On Jira, its worklogs instead of ClickUp's entries; chosen on each reading of the config.
    jira: std::sync::Arc<prism_base::rt::Swap<Option<crate::jira::Actions>>>,
}

/// Manual entry sent by the front end, new or edited.
#[derive(Debug, Deserialize)]
pub struct NewEntry {
    pub ticket: String,
    pub start: DateTime<Utc>,
    pub minutes: u32,
    #[serde(default)]
    pub description: String,
}

impl TimeService {
    pub fn new(hub: Hub, team: TeamId) -> Self {
        Self { hub, team, jira: Default::default() }
    }

    /// Time is Jira's worklogs (`Some`), or ClickUp's entries.
    pub fn use_jira(&self, jira: Option<crate::jira::Actions>) {
        self.jira.set(jira);
    }

    fn jira(&self) -> Option<&'static crate::jira::Actions> {
        self.jira.get().and_then(Option::as_ref)
    }

    pub async fn run(self) {
        // First load as soon as the workspace (or Jira's user) is known, then at a regular interval.
        if self.jira().is_none() {
            self.team.ready().await;
        }
        let mut first = true;
        loop {
            if !std::mem::take(&mut first) {
                // Jira's worklogs are read issue by issue: less often.
                self.hub.pause(if self.jira().is_some() { JIRA_POLL } else { POLL }).await;
            }
            if let Err(e) = self.refresh().await {
                tracing::warn!("time: {e:#}");
                self.hub.set_error(SOURCE, Some(format!("{e:#}")));
            }
        }
    }

    fn team_id(&self) -> Result<String> {
        self.team.get().context(tr("ClickUp workspace not resolved yet"))
    }

    pub async fn refresh(&self) -> Result<()> {
        // The dashboard's date picker and the Month view go back that far, and the history is
        // used to suggest recently used tasks.
        let since = history_start(Utc::now(), crate::conventions::get().time_history_days);
        if let Some(jira) = self.jira() {
            let entries = jira.worklogs(since).await?;
            return self.publish(entries);
        }
        let team = self.team_id()?;
        let api = Api::new().await?;
        // Up to and including tomorrow, not just now: an entry chained after the day's last one
        // may start later in the day, and ClickUp filters on the start.
        let until = Utc::now() + chrono::Duration::days(2);
        let path = format!(
            "/team/{team}/time_entries?start_date={}&end_date={}",
            since.timestamp_millis(),
            until.timestamp_millis(),
        );
        let mut entries: Vec<TimeEntry> = api
            .get::<Many>(&path)
            .await?
            .data
            .into_iter()
            .map(TimeEntry::from)
            // A timer started from ClickUp shows up with a negative duration: it is not an entry yet.
            .filter(|e| e.duration_ms > 0)
            .collect();
        entries.sort_by_key(|a| std::cmp::Reverse(a.start));
        self.publish(entries)
    }

    /// The entries, and the tickets they were logged on, most recent first.
    fn publish(&self, entries: Vec<TimeEntry>) -> Result<()> {
        let mut recent_tickets: Vec<String> = Vec::new();
        for t in entries.iter().filter_map(|e| e.ticket.clone()) {
            if !recent_tickets.contains(&t) {
                recent_tickets.push(t);
            }
        }
        let time = TimeTracking { entries, recent_tickets };
        self.hub.update(|s| {
            let changed = s.time != time;
            s.time = time;
            changed
        });
        self.hub.set_error(SOURCE, None);
        Ok(())
    }

    pub async fn log(&self, entry: NewEntry) -> Result<()> {
        check_duration(entry.minutes)?;
        if let Some(jira) = self.jira() {
            jira.log_work(&entry).await?;
            return self.refresh().await;
        }
        let team = self.team_id()?;
        let api = Api::new().await?;
        let task = task_id(&api, &team, &entry.ticket).await?;
        let _: serde_json::Value = api
            .post(
                &format!("/team/{team}/time_entries"),
                json!({
                    "tid": task,
                    "start": entry.start.timestamp_millis(),
                    "duration": i64::from(entry.minutes) * 60_000,
                    "description": entry.description,
                }),
            )
            .await?;
        self.refresh().await
    }

    /// Edits an entry: its ticket, start, duration, description. An empty ticket
    /// keeps the entry's task (an entry without a task stays that way).
    pub async fn update(&self, id: &str, entry: NewEntry) -> Result<()> {
        self.known(id)?;
        check_duration(entry.minutes)?;
        if let Some(jira) = self.jira() {
            jira.update_work(id, &entry).await?;
            return self.refresh().await;
        }
        let team = self.team_id()?;
        let api = Api::new().await?;
        let duration = i64::from(entry.minutes) * 60_000;
        let start = entry.start.timestamp_millis();
        let mut body = json!({
            "start": start,
            "end": start + duration,
            "duration": duration,
            "description": entry.description,
        });
        if !entry.ticket.trim().is_empty() {
            body["tid"] = task_id(&api, &team, &entry.ticket).await?.into();
        }
        tracing::info!("time: update entry {id}");
        let _: serde_json::Value = api.put(&format!("/team/{team}/time_entries/{id}"), body).await?;
        self.refresh().await
    }

    /// Only entries displayed by Prism (mine, over 30 days) are accepted: the
    /// window cannot target an arbitrary id.
    fn known(&self, id: &str) -> Result<()> {
        if !self.hub.subscribe().borrow().time.entries.iter().any(|e| e.id == id) {
            bail!("time entry {id} is not one Prism displays");
        }
        Ok(())
    }

    /// Deletes an entry.
    pub async fn delete(&self, id: &str) -> Result<()> {
        self.known(id)?;
        if let Some(jira) = self.jira() {
            jira.delete_work(id).await?;
            return self.refresh().await;
        }
        let team = self.team_id()?;
        tracing::info!("time: delete entry {id}");
        let _: serde_json::Value = Api::new().await?.delete(&format!("/team/{team}/time_entries/{id}")).await?;
        self.refresh().await
    }
}

fn check_duration(minutes: u32) -> Result<()> {
    if minutes == 0 || minutes > 24 * 60 {
        bail!("{}", tr("duration must be between 1 minute and 24 hours"));
    }
    Ok(())
}

/// `ABC-1234` → internal ClickUp id, via the workspace's custom ids.
async fn task_id(api: &Api, team: &str, ticket: &str) -> Result<String> {
    anyhow::ensure!(!ticket::prefix().is_empty(), "{}", tr("no ticket ids: set [tracker] ticket_prefix"));
    let id = ticket::find([ticket]).with_context(|| tr!("\"{ticket}\" is not a {prefix}- ticket id", ticket = ticket, prefix = ticket::prefix()))?;
    let task: Task = api
        .get(&format!("/task/{id}?custom_task_ids=true&team_id={team}"))
        .await
        .with_context(|| tr!("ticket {id} not found in ClickUp", id = id))?;
    Ok(task.id)
}

#[derive(Deserialize)]
struct Task {
    id: String,
}

#[derive(Deserialize)]
struct Many {
    #[serde(default)]
    data: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    id: String,
    task: Option<RawTask>,
    start: String,
    duration: String,
    #[serde(default)]
    description: Option<String>,
}

/// ClickUp returns `task` as an object, or as an empty string for an entry without a task.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawTask {
    Task { id: String, custom_id: Option<String>, name: Option<String> },
    Other(#[allow(dead_code)] serde_json::Value),
}

impl From<RawEntry> for TimeEntry {
    fn from(e: RawEntry) -> Self {
        let (ticket, task_name, task_url) = match e.task {
            Some(RawTask::Task { id, custom_id, name }) => (custom_id, name, Some(format!("https://app.clickup.com/t/{id}"))),
            _ => (None, None, None),
        };
        TimeEntry {
            id: e.id,
            ticket,
            task_name,
            task_url,
            start: e.start.parse().ok().and_then(DateTime::from_timestamp_millis).unwrap_or_else(Utc::now),
            duration_ms: e.duration.parse().unwrap_or(0),
            description: e.description.unwrap_or_default(),
        }
    }
}

/// First instant read: `[clickup] time_history_days` back, and at least the 1st of the previous
/// month (local time), so that the Month view shows last month whole.
pub fn history_start(now: DateTime<Utc>, days: u32) -> DateTime<Utc> {
    use chrono::{Datelike, Local, Months, TimeZone};
    let back = now - chrono::Duration::days(i64::from(days));
    let today = now.with_timezone(&Local).date_naive();
    let month = today.with_day(1).and_then(|d| d.checked_sub_months(Months::new(1)));
    let month = month.and_then(|d| Local.from_local_datetime(&d.and_hms_opt(0, 0, 0)?).earliest());
    month.map_or(back, |m| back.min(m.with_timezone(&Utc)))
}

#[cfg(test)]
mod history_tests {
    use super::history_start;
    use chrono::{Datelike, Local, TimeZone, Utc};

    #[test]
    fn reaches_the_first_of_last_month() {
        let now = Local.with_ymd_and_hms(2026, 10, 31, 15, 0, 0).unwrap().with_timezone(&Utc);
        let start = history_start(now, 30).with_timezone(&Local);
        assert_eq!((start.year(), start.month(), start.day()), (2026, 9, 1));
    }

    #[test]
    fn keeps_a_longer_history() {
        let now = Local.with_ymd_and_hms(2026, 10, 2, 15, 0, 0).unwrap().with_timezone(&Utc);
        assert_eq!(history_start(now, 90), now - chrono::Duration::days(90));
    }
}
