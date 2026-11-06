//! Jira Cloud as the tracker: the active sprint of a Scrum board (`[jira] board`), or the open
//! issues of a Kanban one, read through the REST and Agile APIs with an API token (Basic auth:
//! `[jira] email` and the token). An issue's key (`ABC-123`) is both its displayed id and its
//! key in Prism; its status is its Jira status, lowercased, as the flow statuses of `[tracker]`
//! name them; the board's columns give their order.

mod adf;
mod edit;
mod queues;
mod worklog;

use prism_base::{i18n::tr, tr};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize};

use crate::{
    http::encode,
    task_view::{Comment, Subtask, TaskView},
    config::JiraConfig,
    state::{Hub, Person, Sprint, SprintBoard, StatusOption, Ticket},
};

const SOURCE: &str = "jira";
/// The slow queues (To pick, Refinement, outside the sprint, next sprint): they answer after the sprint.
pub const QUEUES: &str = "jira queues";
/// The queues change on the scale of a day: re-read every 3 minutes, not on every pass.
const QUEUES_EVERY: Duration = Duration::from_secs(180);
/// System keychain entry read when JIRA_TOKEN is not set.
pub const KEYCHAIN_SERVICE: &str = "prism-jira";
/// Atlassian's API gateway, the only way in for a scoped API token: `<GATEWAY>/<cloudId>/rest/…`.
const GATEWAY: &str = "https://api.atlassian.com/ex/jira";
/// Issues per page (the Agile API's maximum).
const PAGE: usize = 100;
/// The fields a sprint's issues are read with.
const LIST_FIELDS: &str = "summary,status,priority,assignee,issuetype,parent,created,updated,timeoriginalestimate,timespent,labels";
/// The fields of an issue's preview.
const VIEW_FIELDS: &str = "summary,status,priority,assignee,reporter,issuetype,parent,created,updated,duedate,timeoriginalestimate,timespent,description,comment,labels,subtasks";

#[derive(Default)]
struct Shared {
    cfg: JiraConfig,
    /// The board's statuses, in column order, from the last pass.
    statuses: RwLock<Vec<StatusOption>>,
    /// Where the API answered: the site, or Atlassian's gateway for a scoped token.
    api_base: RwLock<Option<String>>,
    /// My Atlassian account, once signed in.
    me: RwLock<Option<String>>,
    /// The board's project key: where new issues go.
    project: RwLock<Option<String>>,
    /// The queues are re-read on the next pass (a refresh, a status change).
    queues_due: std::sync::atomic::AtomicBool,
    passes: super::passes::Passes,
}

/// The Jira tracker, shared by its collector and the window's actions.
#[derive(Clone, Default)]
pub struct Actions {
    shared: Arc<Shared>,
}

impl Actions {
    pub fn new(cfg: &JiraConfig) -> Self {
        Self { shared: Arc::new(Shared { cfg: cfg.clone(), ..Default::default() }) }
    }

    /// Reads the board's sprint now, then every `poll_seconds`, or when asked to.
    pub async fn run(self, hub: Hub) {
        let every = Duration::from_secs(self.shared.cfg.poll_seconds.max(15));
        let base = format!("{}/browse/", self.shared.cfg.base_url());
        hub.update(|s| {
            s.ticket_base = Some(base.clone());
            true
        });
        let mut first = true;
        let mut client: Option<Api> = None;
        let mut me: Option<Person> = None;
        let mut queued: Option<std::time::Instant> = None;
        loop {
            if !std::mem::take(&mut first) {
                tokio::select! {
                    _ = hub.pause(every) => {}
                    _ = self.shared.passes.requested() => {}
                }
            }
            self.shared.passes.begin();
            let result = async {
                let api = match client.take() {
                    Some(api) => api,
                    None => {
                        let (api, user) = self.connect().await?;
                        me = Some(user);
                        api
                    }
                };
                let read = self.read(&api).await?;
                anyhow::Ok((api, read))
            }
            .await;
            let error = match result {
                Ok((api, read)) => {
                    let sprint = read.active.then(|| read.sprint.clone());
                    hub.update(|s| {
                        let changed = s.sprint.as_ref() != Some(&read.sprint) || s.tracker_me != me;
                        s.sprint = Some(read.sprint.clone());
                        s.tracker_me = me.clone();
                        changed
                    });
                    hub.set_error(SOURCE, None);
                    let due = self.shared.queues_due.swap(false, std::sync::atomic::Ordering::SeqCst);
                    if due || queued.is_none_or(|at| at.elapsed() > QUEUES_EVERY) {
                        let board = queues::BoardRead { scrum: read.scrum, name: &read.board, statuses: &read.sprint.statuses, sprint: sprint.as_ref() };
                        match self.queues(&api, &board).await {
                            Ok(q) => {
                                queued = Some(std::time::Instant::now());
                                hub.update(|s| {
                                    let changed = s.refinement != q.refinement || s.pickup != q.pickup || s.outside_sprint != q.outside || s.next_sprint != q.next_sprint;
                                    s.refinement = q.refinement;
                                    s.pickup = q.pickup;
                                    s.outside_sprint = q.outside;
                                    s.next_sprint = q.next_sprint;
                                    changed
                                });
                                hub.set_error(QUEUES, None);
                            }
                            Err(e) => {
                                tracing::warn!("jira: queues: {e:#}");
                                hub.set_error(QUEUES, Some(format!("{e:#}")));
                            }
                        }
                    }
                    client = Some(api);
                    None
                }
                Err(e) => {
                    tracing::warn!("jira: {e:#}");
                    hub.set_error(SOURCE, Some(format!("{e:#}")));
                    Some(format!("{e:#}"))
                }
            };
            self.shared.passes.end(error);
        }
    }

    /// Immediate re-read; returns when it is done, with its error if any.
    pub async fn refresh(&self) -> Result<()> {
        self.shared.queues_due.store(true, std::sync::atomic::Ordering::SeqCst);
        self.shared.passes.request(Duration::from_secs(60), "Jira").await
    }

    /// Signs in: on the site, or, when it refuses the token, through Atlassian's gateway, where
    /// scoped API tokens are accepted (and only there). Returns the API and its user.
    async fn connect(&self) -> Result<(Api, Person)> {
        let cfg = &self.shared.cfg;
        let mut api = Api::new(cfg, None).await?;
        let resp = api.fetch("/rest/api/3/myself").await?;
        let user: RawUser = if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            let tenant: Tenant = api.http.get(format!("{}/_edge/tenant_info", cfg.base_url())).timeout(Duration::from_secs(20)).send().await?.error_for_status()?.json().await.context("unreadable tenant_info")?;
            api.base = format!("{GATEWAY}/{}", tenant.cloud_id);
            let resp = api.fetch("/rest/api/3/myself").await?;
            if matches!(resp.status(), reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN) {
                bail!("{}", tr!("Jira refuses the token for {email}: check [jira] email (the token's account) and the token in the keychain", email = cfg.email.trim()));
            }
            let user = Api::read(resp, "/rest/api/3/myself").await?;
            tracing::info!("jira: scoped token, through the gateway");
            user
        } else {
            Api::read(resp, "/rest/api/3/myself").await?
        };
        *self.shared.api_base.write().unwrap() = Some(api.base.clone());
        *self.shared.me.write().unwrap() = Some(user.account_id.clone());
        Ok((api, user.into()))
    }

    /// The API where it last answered; signs in when it never has.
    async fn api(&self) -> Result<Api> {
        let known = self.shared.api_base.read().unwrap().clone();
        match known {
            Some(base) => Api::new(&self.shared.cfg, Some(base)).await,
            None => Ok(self.connect().await?.0),
        }
    }

    /// A pass: the board, its statuses, and its active sprint's issues (a Kanban board's open ones).
    async fn read(&self, api: &Api) -> Result<Read> {
        let cfg = &self.shared.cfg;
        if cfg.board == 0 {
            bail!("{}", tr("no board: set [jira] board"));
        }
        let id = cfg.board;
        let (board_path, columns_path) = (format!("/rest/agile/1.0/board/{id}"), format!("/rest/agile/1.0/board/{id}/configuration"));
        let (board, columns, all) = tokio::try_join!(
            api.get::<RawBoard>(&board_path),
            api.get::<RawConfiguration>(&columns_path),
            api.get::<Vec<RawStatus>>("/rest/api/3/status"),
        )?;
        let statuses = board_statuses(&columns, &all);
        *self.shared.statuses.write().unwrap() = statuses.clone();
        let url = format!("{}/secure/RapidBoard.jspa?rapidView={id}", cfg.base_url());
        let active = if board.kind == "scrum" {
            let sprints: Page<RawSprint> = api.get(&format!("/rest/agile/1.0/board/{id}/sprint?state=active")).await?;
            sprints.values.into_iter().next()
        } else {
            None
        };
        let (name, list_id, start, end, path) = match &active {
            Some(s) => (s.name.clone(), s.id.to_string(), s.start_date.as_deref().and_then(date), s.end_date.as_deref().and_then(date), format!("/rest/agile/1.0/sprint/{}/issue", s.id)),
            // A Kanban board (or a Scrum one between sprints): its open issues.
            None => (board.name.clone(), String::new(), None, None, format!("/rest/agile/1.0/board/{id}/issue?jql={}", encode("statusCategory != Done"))),
        };
        let tickets = self.issues(api, &path).await?;
        *self.shared.project.write().unwrap() = board.location.as_ref().and_then(|l| l.project_key.clone());
        // The board, for Sprint's +: new issues go to its project.
        let boards = vec![SprintBoard { id: id.to_string(), name: board.name.clone(), statuses: statuses.clone() }];
        let sprint = Sprint { name, list_id, url, start, end, tickets, statuses, boards };
        Ok(Read { sprint, active: active.is_some(), scrum: board.kind == "scrum", board: board.name })
    }

    /// The queues are re-read right away: a status change may move an issue into one.
    fn queues_now(&self) {
        self.shared.queues_due.store(true, std::sync::atomic::Ordering::SeqCst);
        self.shared.passes.poke();
    }

    /// Every issue of an Agile API listing, page after page.
    async fn issues(&self, api: &Api, path: &str) -> Result<Vec<Ticket>> {
        let base = self.shared.cfg.base_url();
        let join = if path.contains('?') { '&' } else { '?' };
        let mut out = Vec::new();
        loop {
            let page: Issues = api.get(&format!("{path}{join}fields={LIST_FIELDS}&maxResults={PAGE}&startAt={}", out.len())).await?;
            let read = page.issues.len();
            out.extend(page.issues.into_iter().map(|i| i.ticket(&base)));
            if read == 0 || out.len() >= page.total {
                return Ok(out);
            }
        }
    }

    /// The issues of the board's project (any project before the first pass) whose text has the
    /// words typed, the latest changed first: one page, read by Jira's text search.
    pub async fn search_text(&self, query: &str) -> Result<Vec<Ticket>> {
        // Letters and digits only: the rest is Lucene's syntax, refused or misread.
        let words: Vec<String> = query.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let api = self.api().await?;
        let text = words.iter().map(|w| format!("text ~ \"{w}*\"")).collect::<Vec<_>>().join(" AND ");
        let project = self.shared.project.read().unwrap().clone();
        let jql = match project {
            Some(p) => format!("project = \"{p}\" AND {text} ORDER BY updated DESC"),
            None => format!("{text} ORDER BY updated DESC"),
        };
        let base = self.shared.cfg.base_url();
        let page: queues::Found = api.get(&format!("/rest/api/3/search/jql?jql={}&fields={LIST_FIELDS}&maxResults={}", encode(&jql), crate::tracker::SEARCH_LIMIT)).await?;
        Ok(page.issues.into_iter().map(|i| i.ticket(&base)).collect())
    }

    /// An issue's preview: `id` is its key.
    pub async fn ticket(&self, id: &str) -> Result<TaskView> {
        let api = self.api().await?;
        let raw: RawIssue = api.get(&format!("/rest/api/3/issue/{}?fields={VIEW_FIELDS}", encode(id))).await?;
        let statuses = self.shared.statuses.read().unwrap().clone();
        Ok(raw.view(&self.shared.cfg.base_url(), statuses))
    }
}

pub async fn token() -> Result<String> {
    if let Ok(t) = std::env::var("JIRA_TOKEN")
        && !t.trim().is_empty()
    {
        return Ok(t.trim().to_string());
    }
    // The active profile's entry: `prism-jira`, `prism-jira-<profile>`.
    let service = crate::profile::service(KEYCHAIN_SERVICE);
    if let Some(token) = crate::platform::secret(&service).await? {
        return Ok(token);
    }
    let how = if cfg!(target_os = "macos") { "keychain entry" } else { "keyring entry (prism jira-token)" };
    bail!("missing token: set JIRA_TOKEN or the \"{service}\" {how}")
}

/// Who the token signs in as on `site` with `email` (its display name), read fresh: the
/// Settings' and the wizard's check, with the form's values.
pub async fn whoami(site: &str, email: &str) -> Result<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Me {
        display_name: String,
    }
    let cfg: JiraConfig = serde_json::from_value(serde_json::json!({ "site": site, "email": email }))?;
    Ok(Api::new(&cfg, None).await?.get::<Me>("/rest/api/3/myself").await?.display_name)
}

struct Api {
    http: Client,
    base: String,
    email: String,
    token: String,
}

impl Api {
    /// On `base`, or the site.
    async fn new(cfg: &JiraConfig, base: Option<String>) -> Result<Self> {
        if cfg.site.trim().is_empty() || cfg.email.trim().is_empty() {
            bail!("{}", tr("set [jira] site and email"));
        }
        cfg.check_site()?;
        Ok(Api { http: crate::http::client(), base: base.unwrap_or_else(|| cfg.base_url()), email: cfg.email.trim().to_string(), token: token().await? })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self.fetch(path).await?;
        Self::read(resp, path).await
    }

    async fn fetch(&self, path: &str) -> Result<reqwest::Response> {
        Ok(self
            .http
            .get(format!("{}{path}", self.base))
            .basic_auth(&self.email, Some(&self.token))
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(20))
            .send()
            .await?)
    }

    /// A write; its answer, `null` when Jira answers without a body (204).
    async fn send(&self, method: reqwest::Method, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
        let resp = self
            .http
            .request(method, format!("{}{path}", self.base))
            .basic_auth(&self.email, Some(&self.token))
            .header("Accept", "application/json")
            .timeout(Duration::from_secs(20));
        // A deletion has no body.
        let resp = if body.is_null() { resp } else { resp.json(&body) }.send().await?;
        Self::read(resp, path).await
    }

    async fn read<T: DeserializeOwned>(resp: reqwest::Response, path: &str) -> Result<T> {
        crate::http::json("Jira", path, &crate::http::body("Jira", path, resp).await?)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tenant {
    cloud_id: String,
}

/// The board's statuses, column after column; a status of the "done" category is closed.
fn board_statuses(cfg: &RawConfiguration, all: &[RawStatus]) -> Vec<StatusOption> {
    let by_id: HashMap<&str, &RawStatus> = all.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut out: Vec<StatusOption> = Vec::new();
    for s in cfg.column_config.columns.iter().flat_map(|c| &c.statuses) {
        let Some(raw) = by_id.get(s.id.as_str()) else { continue };
        let status = raw.name.to_lowercase();
        if !out.iter().any(|o| o.status == status) {
            out.push(StatusOption { status, color: None, closed: raw.done() });
        }
    }
    out
}

/// Jira's priorities as Prism's; a name of its own (an instance's), none.
fn priority(name: &str) -> Option<crate::state::Priority> {
    use crate::state::Priority::*;
    Some(match name.to_lowercase().as_str() {
        "highest" | "blocker" | "critical" | "urgent" => Urgent,
        "high" | "major" => High,
        "medium" | "normal" => Normal,
        "low" | "lowest" | "minor" | "trivial" => Low,
        _ => return None,
    })
}

/// Jira's dates: `2026-10-01T09:12:33.123+0200`, or RFC 3339 (sprints).
fn date(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text).or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f%z")).ok().map(|d| d.with_timezone(&Utc))
}

/// A stable number for an Atlassian account id (Prism's people have numeric ids): FNV-1a.
fn person_id(account: &str) -> u64 {
    account.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// A pass on the board: its sprint (its open issues, for a Kanban board), and what the queues need.
struct Read {
    sprint: Sprint,
    /// The sprint is an active sprint.
    active: bool,
    scrum: bool,
    board: String,
}

#[derive(Deserialize)]
struct RawBoard {
    name: String,
    #[serde(rename = "type", default)]
    kind: String,
    location: Option<RawLocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLocation {
    project_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawConfiguration {
    column_config: RawColumns,
}

#[derive(Deserialize)]
struct RawColumns {
    columns: Vec<RawColumn>,
}

#[derive(Deserialize)]
struct RawColumn {
    #[serde(default)]
    statuses: Vec<RawId>,
}

#[derive(Deserialize)]
struct RawId {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStatus {
    #[serde(default)]
    id: String,
    name: String,
    status_category: Option<RawCategory>,
}

impl RawStatus {
    fn done(&self) -> bool {
        self.status_category.as_ref().is_some_and(|c| c.key == "done")
    }
}

#[derive(Deserialize)]
struct RawCategory {
    key: String,
}

#[derive(Deserialize)]
struct Page<T> {
    values: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSprint {
    id: u64,
    name: String,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Deserialize)]
struct Issues {
    issues: Vec<RawIssue>,
    #[serde(default)]
    total: usize,
}

#[derive(Deserialize)]
struct RawIssue {
    key: String,
    fields: RawFields,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawFields {
    summary: String,
    status: Option<RawStatus>,
    priority: Option<RawName>,
    assignee: Option<RawUser>,
    reporter: Option<RawUser>,
    issuetype: Option<RawType>,
    parent: Option<RawParent>,
    created: Option<String>,
    updated: Option<String>,
    duedate: Option<String>,
    /// In seconds.
    timeoriginalestimate: Option<u64>,
    /// Everyone's worklogs, in seconds.
    timespent: Option<u64>,
    description: Option<serde_json::Value>,
    comment: Option<RawComments>,
    labels: Vec<String>,
    subtasks: Vec<RawParent>,
}

#[derive(Deserialize)]
struct RawName {
    name: String,
}

#[derive(Deserialize)]
struct RawType {
    name: String,
    #[serde(default)]
    subtask: bool,
}

#[derive(Deserialize)]
struct RawParent {
    key: String,
    #[serde(default)]
    fields: Option<RawParentFields>,
}

#[derive(Deserialize)]
struct RawParentFields {
    #[serde(default)]
    summary: String,
    status: Option<RawStatus>,
}

#[derive(Deserialize)]
struct RawComments {
    #[serde(default)]
    comments: Vec<RawComment>,
}

#[derive(Deserialize)]
struct RawComment {
    id: String,
    author: Option<RawUser>,
    body: Option<serde_json::Value>,
    created: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUser {
    account_id: String,
    #[serde(default)]
    display_name: String,
}

impl From<RawUser> for Person {
    fn from(u: RawUser) -> Self {
        let initials: String = u.display_name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
        Person { id: person_id(&u.account_id), name: u.display_name, initials: initials.to_uppercase(), color: None, avatar: None }
    }
}

impl RawIssue {
    fn ticket(self, base: &str) -> Ticket {
        let f = self.fields;
        let status = f.status.as_ref();
        Ticket {
            fields: Vec::new(),
            waiting_on: Vec::new(),
            blocking: Vec::new(),
            url: format!("{base}/browse/{}", self.key),
            id: self.key.clone(),
            task_id: self.key,
            parent: f.parent.map(|p| p.key),
            priority: f.priority.and_then(|p| priority(&p.name)),
            // An ordinary issue has no type in Prism, as an ordinary ClickUp task.
            task_type: f.issuetype.filter(|t| !t.subtask && !matches!(t.name.as_str(), "Task" | "Tâche")).map(|t| t.name),
            task_type_icon: None,
            list: None,
            repo: None,
            description: None,
            time_estimate: f.timeoriginalestimate.map(|s| s * 1000),
            time_spent: f.timespent.filter(|s| *s > 0).map(|s| s * 1000),
            tags: f.labels,
            name: f.summary,
            status: status.map(|s| s.name.to_lowercase()).unwrap_or_default(),
            closed: status.is_some_and(RawStatus::done),
            assignees: f.assignee.into_iter().map(Person::from).collect(),
            created_at: f.created.as_deref().and_then(date),
            updated_at: f.updated.as_deref().and_then(date),
        }
    }

    fn view(self, base: &str, statuses: Vec<StatusOption>) -> TaskView {
        let f = self.fields;
        let status = f.status.as_ref();
        TaskView {
            fields: Vec::new(),
            url: format!("{base}/browse/{}", self.key),
            task_id: self.key.clone(),
            id: self.key,
            name: f.summary,
            task_type: f.issuetype.map(|t| t.name),
            task_type_icon: None,
            status: status.map(|s| s.name.to_lowercase()).unwrap_or_default(),
            status_color: None,
            statuses,
            closed: status.is_some_and(RawStatus::done),
            priority: f.priority.and_then(|p| priority(&p.name)),
            assignees: f.assignee.into_iter().map(Person::from).collect(),
            creator: f.reporter.map(Person::from),
            tags: f.labels,
            list: None,
            list_id: None,
            parent: f.parent.map(|p| p.key),
            description: f.description.as_ref().map(adf::markdown).unwrap_or_default(),
            created_at: f.created.as_deref().and_then(date),
            updated_at: f.updated.as_deref().and_then(date),
            due_at: f.duedate.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).and_then(|d| d.and_hms_opt(0, 0, 0)).map(|d| d.and_utc()),
            time_estimate: f.timeoriginalestimate.map(|s| s * 1000),
            time_spent: f.timespent.filter(|s| *s > 0).map(|s| s * 1000),
            checklists: Vec::new(),
            subtasks: f
                .subtasks
                .into_iter()
                .map(|s| {
                    let (name, status, closed) = match s.fields {
                        Some(f) => (f.summary, f.status.as_ref().map(|s| s.name.to_lowercase()).unwrap_or_default(), f.status.as_ref().is_some_and(RawStatus::done)),
                        None => (String::new(), String::new(), false),
                    };
                    Subtask { task_id: s.key.clone(), id: s.key, name, status, closed }
                })
                .collect(),
            comments: f
                .comment
                .map(|c| c.comments)
                .unwrap_or_default()
                .into_iter()
                .map(|c| Comment { id: c.id, author: c.author.map(Person::from), text: c.body.as_ref().map(adf::markdown).unwrap_or_default(), date: c.created.as_deref().and_then(date), replies: 0, mentions: Vec::new() })
                .collect(),
            attachments: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token's check builds its config from the form's site and account alone.
    #[test]
    fn a_config_from_the_site_and_account() {
        let cfg: JiraConfig = serde_json::from_value(serde_json::json!({ "site": "team.atlassian.net/", "email": "a@b.c" })).unwrap();
        assert_eq!((cfg.base_url().as_str(), cfg.email.as_str()), ("https://team.atlassian.net", "a@b.c"));
    }

    /// The token goes to an Atlassian Cloud site only, whatever the site says.
    #[test]
    fn the_token_goes_to_atlassian_only() {
        let site = |s: &str| serde_json::from_value::<JiraConfig>(serde_json::json!({ "site": s })).unwrap().check_site().is_ok();
        assert!(site("team.atlassian.net") && site("https://Team.Atlassian.net/") && site("old.jira.com"));
        assert!(!site("evil.example") && !site("team.atlassian.net@evil.example") && !site("team.atlassian.net.evil.example") && !site("evil.example/team.atlassian.net"));
    }

    fn issue(json: serde_json::Value) -> RawIssue {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn an_issue_is_a_ticket() {
        let t = issue(serde_json::json!({
            "key": "ABC-12",
            "fields": {
                "summary": "Export the report",
                "status": { "id": "3", "name": "In Progress", "statusCategory": { "key": "indeterminate" } },
                "priority": { "name": "Highest" },
                "assignee": { "accountId": "5b10ac8d82e05b22cc7d4ef5", "displayName": "Ada Lovelace" },
                "issuetype": { "name": "Bug", "subtask": false },
                "parent": { "key": "ABC-1" },
                "created": "2026-10-01T09:12:33.123+0200",
                "timeoriginalestimate": 7200
            }
        }))
        .ticket("https://team.atlassian.net");
        assert_eq!((t.id.as_str(), t.task_id.as_str(), t.status.as_str(), t.closed), ("ABC-12", "ABC-12", "in progress", false));
        assert_eq!((t.priority, t.task_type.as_deref(), t.parent.as_deref()), (Some(crate::state::Priority::Urgent), Some("Bug"), Some("ABC-1")));
        assert_eq!(t.url, "https://team.atlassian.net/browse/ABC-12");
        assert_eq!(t.time_estimate, Some(7_200_000));
        assert_eq!((t.assignees[0].initials.as_str(), t.assignees[0].id), ("AL", person_id("5b10ac8d82e05b22cc7d4ef5")));
        assert_eq!(t.created_at.unwrap().to_rfc3339(), "2026-10-01T07:12:33.123+00:00");
    }

    #[test]
    fn a_done_issue_is_closed_and_a_task_has_no_type() {
        let t = issue(serde_json::json!({
            "key": "ABC-3",
            "fields": { "summary": "x", "status": { "name": "Done", "statusCategory": { "key": "done" } }, "issuetype": { "name": "Task" } }
        }))
        .ticket("https://team.atlassian.net");
        assert!(t.closed);
        assert_eq!(t.task_type, None);
    }

    #[test]
    fn the_board_orders_its_statuses_by_column() {
        let cfg: RawConfiguration = serde_json::from_value(serde_json::json!({
            "columnConfig": { "columns": [
                { "name": "To Do", "statuses": [{ "id": "1" }] },
                { "name": "Doing", "statuses": [{ "id": "3" }, { "id": "4" }] },
                { "name": "Done", "statuses": [{ "id": "5" }] }
            ] }
        }))
        .unwrap();
        let all: Vec<RawStatus> = serde_json::from_value(serde_json::json!([
            { "id": "5", "name": "Done", "statusCategory": { "key": "done" } },
            { "id": "4", "name": "In Review", "statusCategory": { "key": "indeterminate" } },
            { "id": "3", "name": "In Progress", "statusCategory": { "key": "indeterminate" } },
            { "id": "1", "name": "To Do", "statusCategory": { "key": "new" } }
        ]))
        .unwrap();
        let s = board_statuses(&cfg, &all);
        assert_eq!(s.iter().map(|s| s.status.as_str()).collect::<Vec<_>>(), ["to do", "in progress", "in review", "done"]);
        assert_eq!(s.iter().map(|s| s.closed).collect::<Vec<_>>(), [false, false, false, true]);
    }

    #[test]
    fn the_site_is_written_any_way() {
        let cfg = |site: &str| JiraConfig { site: site.into(), ..Default::default() };
        assert_eq!(cfg("team.atlassian.net").base_url(), "https://team.atlassian.net");
        assert_eq!(cfg("https://team.atlassian.net/").base_url(), "https://team.atlassian.net");
    }

    #[test]
    fn sprint_dates_are_rfc3339() {
        assert_eq!(date("2026-10-05T08:00:00.000Z").unwrap().to_rfc3339(), "2026-10-05T08:00:00+00:00");
    }
}
