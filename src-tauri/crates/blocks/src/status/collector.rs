//! Reads the status pages: Atlassian Statuspage (`/api/v2/summary.json`: GitHub, ClickUp, Jira,
//! Claude, `[status] pages`), status.io (GitLab.com) and Google Cloud's incidents feed.

use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use prism_base::tr;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::{Component, Incident, Level, Service, StatusConfig, StatusState};
use crate::BlockCtx;

static REFRESH: OnceLock<tokio::sync::Notify> = OnceLock::new();

/// ↻ in the block: the next read starts now.
pub fn refresh_now() {
    if let Some(n) = REFRESH.get() {
        n.notify_one();
    }
}

/// Where a service's state is read.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    /// An Atlassian Statuspage, by its address.
    Statuspage,
    /// A status.io page, by its id.
    StatusIo(&'static str),
    /// Google Cloud's incidents feed.
    Gcp,
}

#[derive(Debug, Clone, PartialEq)]
struct Source {
    id: String,
    name: String,
    logo: Option<&'static str>,
    url: String,
    kind: Kind,
}

/// The built-in services, by `[status] services` id.
fn builtin(id: &str) -> Option<Source> {
    let (name, logo, url, kind) = match id {
        "github" => ("GitHub", "github", "https://www.githubstatus.com", Kind::Statuspage),
        "gitlab" => ("GitLab.com", "gitlab", "https://status.gitlab.com", Kind::StatusIo("5b36dc6502d06804c08349f7")),
        "clickup" => ("ClickUp", "clickup", "https://status.clickup.com", Kind::Statuspage),
        "jira" => ("Jira", "jira", "https://jira-software.status.atlassian.com", Kind::Statuspage),
        "claude" => ("Claude", "claude", "https://status.claude.com", Kind::Statuspage),
        "gcp" => ("Google Cloud", "gcp", "https://status.cloud.google.com", Kind::Gcp),
        _ => return None,
    };
    Some(Source { id: id.into(), name: name.into(), logo: Some(logo), url: url.into(), kind })
}

/// What Prism reads, for an empty `[status] services`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Reads {
    /// The forge, when it is a public one (GitLab.com, GitHub).
    forge: Option<crate::forge::Provider>,
    gitlab_com: bool,
    tracker: Option<crate::tracker::Provider>,
    claude: bool,
}

fn auto(r: Reads) -> Vec<&'static str> {
    let mut ids = Vec::new();
    match r.forge {
        Some(crate::forge::Provider::Gitlab) if r.gitlab_com => ids.push("gitlab"),
        Some(crate::forge::Provider::Github) => ids.push("github"),
        _ => {}
    }
    match r.tracker {
        Some(crate::tracker::Provider::Clickup) => ids.push("clickup"),
        Some(crate::tracker::Provider::Jira) => ids.push("jira"),
        _ => {}
    }
    if r.claude {
        ids.push("claude");
    }
    ids
}

fn sources(cfg: &StatusConfig, reads: Reads) -> Vec<Source> {
    let ids: Vec<String> = if cfg.services.is_empty() { auto(reads).into_iter().map(String::from).collect() } else { cfg.services.iter().map(|s| s.trim().to_lowercase()).collect() };
    let pages = cfg.pages.iter().filter(|p| !p.url.trim().is_empty()).map(|p| Source {
        id: p.name.clone(),
        name: p.name.clone(),
        logo: None,
        url: p.url.trim().trim_end_matches('/').to_string(),
        kind: Kind::Statuspage,
    });
    ids.iter().filter_map(|id| builtin(id)).chain(pages).collect()
}

pub async fn run(cfg: StatusConfig, agent: String, ctx: BlockCtx) {
    let notify = REFRESH.get_or_init(tokio::sync::Notify::new);
    let every = Duration::from_secs(cfg.poll_seconds.max(60));
    let unknown: Vec<String> = cfg.services.iter().filter(|s| builtin(&s.trim().to_lowercase()).is_none()).cloned().collect();
    // The forge and the tracker are known once their sources have started.
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        let gitlab_com = ctx.env().gitlab.host.trim().eq_ignore_ascii_case("gitlab.com");
        let reads = ctx.snapshot(|s| Reads { forge: ctx.env().code.then_some(s.forge), gitlab_com, tracker: s.tracker, claude: agent == "claude" });
        let list = sources(&cfg, reads);
        let services = futures::future::join_all(list.iter().map(read)).await;
        let failed: Vec<String> = services.iter().filter_map(|s| s.error.as_ref().map(|e| format!("{}: {e}", s.name))).collect();
        ctx.publish(&StatusState { services, alerts: cfg.alerts });
        // Alerts says so only when nothing answers, or for a service it does not know.
        let error = if !unknown.is_empty() {
            Some(format!("[status] services: unknown {} (github, gitlab, clickup, jira, claude, gcp)", unknown.join(", ")))
        } else if !list.is_empty() && failed.len() == list.len() {
            Some(failed.join(" · "))
        } else {
            None
        };
        ctx.set_error(error);
        tokio::select! {
            _ = ctx.pause(every) => {}
            _ = notify.notified() => {}
        }
    }
}

async fn read(s: &Source) -> Service {
    let base = Service { id: s.id.clone(), name: s.name.clone(), logo: s.logo.map(String::from), url: s.url.clone(), checked_at: Some(Utc::now()), ..Default::default() };
    let read = match &s.kind {
        Kind::Statuspage => get::<Summary>(&format!("{}/api/v2/summary.json", s.url)).await.map(|v| statuspage(v, &s.url)),
        Kind::StatusIo(id) => get::<StatusIoAnswer>(&format!("https://api.status.io/1.0/status/{id}")).await.map(|v| status_io(v.result, &s.url, id)),
        Kind::Gcp => get::<Vec<GcpIncident>>(&format!("{}/incidents.json", s.url)).await.map(|v| gcp(v, &s.url)),
    };
    match read {
        Ok(r) => Service { level: r.level, summary: r.summary, incidents: r.incidents, components: r.components, ..base },
        Err(e) => Service { level: Level::Unknown, summary: "Not answering".into(), error: Some(format!("{e:#}")), ..base },
    }
}

async fn get<T: DeserializeOwned>(url: &str) -> Result<T> {
    let resp = crate::http::client().get(url).header("User-Agent", "Prism").timeout(Duration::from_secs(15)).send().await?.error_for_status()?;
    resp.json::<T>().await.with_context(|| tr!("{url}: not a status page", url = url))
}

/// A page read: its overall state, what is under way.
#[derive(Debug, Default, PartialEq)]
struct Read {
    level: Level,
    summary: String,
    incidents: Vec<Incident>,
    components: Vec<Component>,
}

/// An update shortened to its first lines.
fn short(text: &str) -> String {
    const MAX: usize = 400;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(MAX) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None => text,
    }
}

// Atlassian Statuspage.

#[derive(Deserialize)]
struct Summary {
    status: SpStatus,
    #[serde(default)]
    components: Vec<SpComponent>,
    #[serde(default)]
    incidents: Vec<SpIncident>,
    #[serde(default)]
    scheduled_maintenances: Vec<SpIncident>,
}

#[derive(Deserialize)]
struct SpStatus {
    indicator: String,
    description: String,
}

#[derive(Deserialize)]
struct SpComponent {
    name: String,
    status: String,
    #[serde(default)]
    group: bool,
}

#[derive(Deserialize)]
struct SpIncident {
    id: String,
    name: String,
    status: String,
    #[serde(default)]
    impact: String,
    shortlink: Option<String>,
    started_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    incident_updates: Vec<SpUpdate>,
    #[serde(default)]
    components: Vec<SpName>,
}

#[derive(Deserialize)]
struct SpUpdate {
    body: String,
}

#[derive(Deserialize)]
struct SpName {
    name: String,
}

fn sp_level(indicator: &str) -> Level {
    match indicator {
        "minor" => Level::Minor,
        "major" => Level::Major,
        "critical" => Level::Critical,
        "maintenance" | "under_maintenance" => Level::Maintenance,
        "degraded_performance" => Level::Minor,
        "partial_outage" => Level::Major,
        "major_outage" => Level::Critical,
        _ => Level::Operational,
    }
}

fn statuspage(s: Summary, page: &str) -> Read {
    let incident = |i: SpIncident, maintenance: bool| Incident {
        url: i.shortlink.unwrap_or_else(|| format!("{page}/incidents/{}", i.id)),
        impact: if maintenance { Level::Maintenance } else { sp_level(&i.impact) },
        status: i.status.replace('_', " "),
        started_at: i.started_at.or(i.created_at),
        updated_at: i.updated_at,
        update: i.incident_updates.first().map(|u| short(&u.body)).unwrap_or_default(),
        components: i.components.into_iter().map(|c| c.name).collect(),
        id: i.id,
        name: i.name,
        maintenance,
    };
    let mut incidents: Vec<Incident> = s.incidents.into_iter().filter(|i| i.status != "resolved" && i.status != "postmortem").map(|i| incident(i, false)).collect();
    incidents.extend(s.scheduled_maintenances.into_iter().filter(|m| m.status == "in_progress" || m.status == "verifying").map(|m| incident(m, true)));
    let components = s.components.into_iter().filter(|c| !c.group && c.status != "operational").map(|c| Component { level: sp_level(&c.status), name: c.name }).collect();
    Read { level: sp_level(&s.status.indicator), summary: s.status.description, incidents, components }
}

// status.io (GitLab.com).

#[derive(Deserialize)]
struct StatusIoAnswer {
    result: StatusIo,
}

#[derive(Deserialize)]
struct StatusIo {
    status_overall: SioOverall,
    #[serde(default)]
    status: Vec<SioComponent>,
    #[serde(default)]
    incidents: Vec<SioIncident>,
    #[serde(default)]
    maintenance: SioMaintenance,
}

#[derive(Deserialize)]
struct SioOverall {
    status: String,
    status_code: u32,
}

#[derive(Deserialize)]
struct SioComponent {
    name: String,
    status_code: u32,
}

#[derive(Deserialize)]
struct SioIncident {
    #[serde(rename = "_id")]
    id: String,
    name: String,
    datetime_open: Option<DateTime<Utc>>,
    #[serde(default)]
    messages: Vec<SioMessage>,
    #[serde(default)]
    components_affected: Vec<SioName>,
}

#[derive(Deserialize)]
struct SioMessage {
    #[serde(default)]
    details: String,
    #[serde(default)]
    state: u32,
    #[serde(default)]
    status: u32,
    datetime: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct SioName {
    name: String,
}

#[derive(Deserialize, Default)]
struct SioMaintenance {
    #[serde(default)]
    active: Vec<SioIncident>,
}

/// status.io's codes: 100 operational, 200 planned maintenance, 300 degraded performance, 400
/// partial disruption, 500 disruption, 600 security event.
fn sio_level(code: u32) -> Level {
    match code {
        0..=199 => Level::Operational,
        200..=299 => Level::Maintenance,
        300..=399 => Level::Minor,
        400..=499 => Level::Major,
        _ => Level::Critical,
    }
}

fn status_io(s: StatusIo, page: &str, id: &str) -> Read {
    let incident = |i: SioIncident, maintenance: bool| {
        let last = i.messages.last();
        Incident {
            url: format!("{page}/pages/{}/{id}/{}", if maintenance { "maintenance" } else { "incident" }, i.id),
            impact: if maintenance { Level::Maintenance } else { last.map_or(Level::Minor, |m| sio_level(m.status)) },
            status: if maintenance {
                "in progress".into()
            } else {
                match last.map_or(0, |m| m.state) {
                    200 => "identified",
                    300 => "monitoring",
                    _ => "investigating",
                }
                .into()
            },
            started_at: i.datetime_open,
            updated_at: last.and_then(|m| m.datetime),
            update: last.map(|m| short(&m.details)).unwrap_or_default(),
            components: i.components_affected.into_iter().map(|c| c.name).collect(),
            id: i.id,
            name: i.name,
            maintenance,
        }
    };
    let mut incidents: Vec<Incident> = s.incidents.into_iter().map(|i| incident(i, false)).collect();
    incidents.extend(s.maintenance.active.into_iter().map(|m| incident(m, true)));
    let components = s.status.into_iter().filter(|c| c.status_code >= 200).map(|c| Component { level: sio_level(c.status_code), name: c.name }).collect();
    Read { level: sio_level(s.status_overall.status_code), summary: s.status_overall.status, incidents, components }
}

// Google Cloud.

#[derive(Deserialize)]
struct GcpIncident {
    id: String,
    begin: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    modified: Option<DateTime<Utc>>,
    #[serde(default)]
    external_desc: String,
    #[serde(default)]
    status_impact: String,
    uri: String,
    most_recent_update: Option<GcpUpdate>,
    #[serde(default)]
    affected_products: Vec<GcpProduct>,
}

#[derive(Deserialize)]
struct GcpUpdate {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct GcpProduct {
    title: String,
}

fn gcp(all: Vec<GcpIncident>, page: &str) -> Read {
    let incidents: Vec<Incident> = all
        .into_iter()
        .filter(|i| i.end.is_none())
        .map(|i| Incident {
            url: format!("{page}/{}", i.uri),
            impact: match i.status_impact.as_str() {
                "SERVICE_OUTAGE" => Level::Critical,
                "SERVICE_DISRUPTION" => Level::Major,
                _ => Level::Minor,
            },
            status: "ongoing".into(),
            started_at: i.begin,
            updated_at: i.modified,
            update: i.most_recent_update.map(|u| short(&u.text)).unwrap_or_default(),
            components: i.affected_products.into_iter().map(|p| p.title).collect(),
            name: i.external_desc,
            id: i.id,
            maintenance: false,
        })
        .collect();
    let level = incidents.iter().map(|i| i.impact).max().unwrap_or(Level::Operational);
    let summary = if incidents.is_empty() { "All services available".into() } else { format!("{} incident{} under way", incidents.len(), if incidents.len() > 1 { "s" } else { "" }) };
    Read { level, summary, incidents, components: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_statuspage_summary() {
        let s: Summary = serde_json::from_str(
            r#"{"status":{"indicator":"minor","description":"Minor Service Outage"},
            "components":[{"name":"Git Operations","status":"operational"},{"name":"Actions","status":"degraded_performance"},{"name":"Group","status":"major_outage","group":true}],
            "incidents":[{"id":"x1","name":"Delayed Actions","status":"investigating","impact":"minor","shortlink":"https://stspg.io/x1","started_at":"2026-10-14T14:00:00Z","updated_at":"2026-10-14T14:20:00Z","incident_updates":[{"body":"We are  investigating\nreports."}],"components":[{"name":"Actions"}]}],
            "scheduled_maintenances":[{"id":"m1","name":"Planned","status":"scheduled","impact":"maintenance"}]}"#,
        )
        .unwrap();
        let r = statuspage(s, "https://www.githubstatus.com");
        assert_eq!(r.level, Level::Minor);
        assert_eq!(r.components, vec![Component { name: "Actions".into(), level: Level::Minor }]);
        assert_eq!(r.incidents.len(), 1);
        assert_eq!(r.incidents[0].update, "We are investigating reports.");
        assert_eq!(r.incidents[0].url, "https://stspg.io/x1");
    }

    #[test]
    fn reads_status_io() {
        let s: StatusIoAnswer = serde_json::from_str(
            r#"{"result":{"status_overall":{"status":"Partial Service Disruption","status_code":400},
            "status":[{"name":"Website","status_code":100},{"name":"CI/CD","status_code":400}],
            "incidents":[{"_id":"i1","name":"CI/CD pipelines delayed","datetime_open":"2026-10-14T14:33:00.000Z","messages":[{"details":"Investigating","state":100,"status":400,"datetime":"2026-10-14T14:33:00.000Z"},{"details":"Monitoring","state":300,"status":300,"datetime":"2026-10-14T15:22:51.436Z"}],"components_affected":[{"_id":"c","name":"CI/CD"}]}],
            "maintenance":{"active":[],"upcoming":[]}}}"#,
        )
        .unwrap();
        let r = status_io(s.result, "https://status.gitlab.com", "page");
        assert_eq!(r.level, Level::Major);
        assert_eq!(r.components.len(), 1);
        let i = &r.incidents[0];
        assert_eq!((i.impact, i.status.as_str(), i.update.as_str()), (Level::Minor, "monitoring", "Monitoring"));
        assert_eq!(i.url, "https://status.gitlab.com/pages/incident/page/i1");
    }

    #[test]
    fn keeps_google_cloud_incidents_under_way() {
        let all: Vec<GcpIncident> = serde_json::from_str(
            r#"[{"id":"a","begin":"2026-10-08T11:00:00+00:00","end":"2026-10-09T16:46:00+00:00","external_desc":"Over","status_impact":"SERVICE_DISRUPTION","uri":"incidents/a"},
            {"id":"b","begin":"2026-10-14T11:00:00+00:00","end":null,"external_desc":"BigQuery errors","status_impact":"SERVICE_OUTAGE","uri":"incidents/b","affected_products":[{"title":"BigQuery"}]}]"#,
        )
        .unwrap();
        let r = gcp(all, "https://status.cloud.google.com");
        assert_eq!(r.level, Level::Critical);
        assert_eq!(r.incidents.len(), 1);
        assert_eq!(r.incidents[0].components, vec!["BigQuery".to_string()]);
    }

    #[test]
    fn defaults_to_what_prism_reads() {
        let reads = Reads { forge: Some(crate::forge::Provider::Gitlab), gitlab_com: false, tracker: Some(crate::tracker::Provider::Jira), claude: true };
        assert_eq!(auto(reads), vec!["jira", "claude"]);
        assert_eq!(auto(Reads { gitlab_com: true, ..reads }), vec!["gitlab", "jira", "claude"]);
    }

    /// The live pages still read: `cargo test -- --ignored status`.
    #[tokio::test]
    #[ignore = "reads the network"]
    async fn reads_the_live_pages() {
        for id in ["github", "gitlab", "clickup", "jira", "claude", "gcp"] {
            let s = read(&builtin(id).unwrap()).await;
            println!("{id}: {:?} {} · {} incident(s)", s.level, s.summary, s.incidents.len());
            assert_eq!(s.error, None, "{id}");
        }
    }

    #[test]
    fn shortens_long_updates() {
        assert_eq!(short(&"a".repeat(500)).chars().count(), 401);
    }
}
