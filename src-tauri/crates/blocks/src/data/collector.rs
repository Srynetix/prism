//! The Data block's collector, on GCP (`[data]`): Cloud Run jobs and their Cloud Scheduler triggers,
//! freshness of the declared BigQuery tables, scheduled queries, Datastream streams (and their lag,
//! read from Cloud Monitoring), estimated BigQuery cost. All over REST, read-only, with the
//! `gcloud auth print-access-token` token (the machine's login, like the image scan).
//!
//! Only metadata, and only what Prism displays: never a table row nor its schema
//! (`fields` keeps them out of the response), never a query's SQL (`projection=minimal`), never
//! a log line nor an execution's message (only the exit code is drawn from it), never
//! a person's address (only service accounts are named). The calls are free:
//! no BigQuery query is run.

use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Result};
use prism_base::i18n::tr;
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc};
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use tokio::sync::Notify;

use super::{BqCost, BqDay, BqJob, DataConfig, DataStream, DataTable, GcpData, RunExecution, RunJob, ScheduledQuery, SchedulerTrigger, TableFreshness};
use crate::BlockCtx;
/// The gcloud token lives one hour: requested again well before.
const TOKEN_TTL: Duration = Duration::from_secs(40 * 60);
/// Cost moves little on the scale of a minute: BigQuery jobs are re-read every 5 minutes.
const COST_EVERY: Duration = Duration::from_secs(5 * 60);
/// Executions kept per job: enough to compute a median and draw the history.
const EXECUTIONS: usize = 14;
/// Days of cost kept, for the median.
const COST_DAYS: i64 = 14;
const TOP_JOBS: usize = 8;
/// Max pages per list: a safeguard, not an expected limit.
const MAX_PAGES: usize = 200;

/// "Refresh" of the Data blocks: re-reads everything, cost included, without waiting for the next pass.
static REFRESH: LazyLock<Notify> = LazyLock::new(Notify::new);
/// End of a pass, for whoever awaits their refresh.
static READ: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Re-reads GCP now; returns once the reading is in the snapshot (2 min at most).
pub async fn refresh() -> Result<()> {
    let read = READ.notified();
    REFRESH.notify_one();
    tokio::time::timeout(Duration::from_secs(120), read).await.map_err(|_| anyhow!(tr("GCP did not answer within 2 min")))
}

pub async fn run(cfg: DataConfig, ctx: BlockCtx) {
    if cfg.projects.is_empty() {
        ctx.set_error(Some(tr("no GCP project in [data] projects").into()));
        return;
    }
    let http = match Client::builder().timeout(Duration::from_secs(30)).build() {
        Ok(http) => http,
        Err(e) => {
            ctx.set_error(Some(format!("HTTP client: {e:#}")));
            READ.notify_waiters();
            return;
        }
    };
    let mut gcp = Gcp { http, token: None };
    let mut executions = HashMap::new();
    let mut book = CostBook::default();
    let mut cost: Option<BqCost> = None;
    let mut cost_read: Option<Instant> = None;
    let mut first = true;
    let mut shown: Option<GcpData> = None;
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = ctx.pause(Duration::from_secs(cfg.poll_seconds.max(15))) => {}
                _ = REFRESH.notified() => {
                    // A requested refresh also re-reads the cost, and requests the token again.
                    cost_read = None;
                    gcp.token = None;
                }
            }
        }
        if let Err(e) = gcp.token().await {
            ctx.set_error(Some(format!("{e:#}")));
            READ.notify_waiters();
            continue;
        }
        let mut data = GcpData { thresholds: cfg.thresholds.clone(), ..Default::default() };
        let mut read = 0;
        let mut note = |data: &mut GcpData, what: &str, project: &str, r: Result<()>| match r {
            Ok(()) => read += 1,
            Err(e) => data.errors.push(format!("{project} · {what}: {}", short(&e))),
        };
        for project in &cfg.projects {
            if cfg.cloud_run_jobs {
                let r = run_jobs(&mut gcp, project, &cfg.region, &mut executions).await.map(|(jobs, trigger_error)| {
                    data.jobs.extend(jobs);
                    if let Some(e) = trigger_error {
                        data.errors.push(format!("{project} · Cloud Scheduler: {e}"));
                    }
                });
                note(&mut data, "Cloud Run", project, r);
            }
            if cfg.scheduled_queries {
                let r = transfers(&mut gcp, project, &cfg.bigquery_location).await.map(|t| data.transfers.extend(t));
                note(&mut data, "scheduled queries", project, r);
            }
            if cfg.datastream {
                let r = streams(&mut gcp, project, &cfg.region).await.map(|s| data.streams.extend(s));
                note(&mut data, "Datastream", project, r);
            }
        }
        if cfg.bigquery_freshness {
            for t in &cfg.tables {
                let f = table(&mut gcp, t, &cfg.projects[0]).await;
                if f.error.is_none() {
                    read += 1;
                }
                data.tables.push(f);
            }
        }
        if cfg.bigquery_cost {
            if cost_read.is_none_or(|t| t.elapsed() >= COST_EVERY) {
                cost_read = Some(Instant::now());
                for project in &cfg.projects {
                    if let Err(e) = book.refresh(&mut gcp, project, Utc::now()).await {
                        data.errors.push(format!("{project} · BigQuery jobs: {}", short(&e)));
                    }
                }
                cost = book.any().then(|| BqCost { partial: !book.complete(&cfg.projects), ..book.summary(cfg.price_per_tib, Local::now()) });
            }
            // Between two readings, the last state stays displayed.
            if cost.is_some() {
                read += 1;
            }
            data.cost = cost.clone();
        }
        data.jobs.sort_by(|a, b| (&a.project, &a.name).cmp(&(&b.project, &b.name)));
        data.streams.sort_by(|a, b| (&a.project, &a.display_name).cmp(&(&b.project, &b.display_name)));
        // The source is in error only when nothing could be read.
        ctx.set_error((read == 0 && !data.errors.is_empty()).then(|| data.errors[0].clone()));
        if shown.as_ref() != Some(&data) {
            ctx.publish(&data);
            shown = Some(data);
        }
        READ.notify_waiters();
    }
}

/// Google API client, with the gcloud token.
struct Gcp {
    http: Client,
    token: Option<(String, Instant)>,
}

impl Gcp {
    async fn token(&mut self) -> Result<String> {
        if let Some((t, at)) = &self.token
            && at.elapsed() < TOKEN_TTL
        {
            return Ok(t.clone());
        }
        let out = crate::platform::command("gcloud").args(["auth", "print-access-token", "--quiet"]).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.map_err(|e| anyhow!("gcloud: {e}"))?;
        if !out.status.success() {
            bail!(tr("gcloud not logged in (gcloud auth login)"));
        }
        let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
        self.token = Some((token.clone(), Instant::now()));
        Ok(token)
    }

    /// A GET; a refused token (expired earlier than expected) is requested again once.
    async fn get(&mut self, url: &str, query: &[(&str, String)]) -> Result<Value> {
        for retry in [false, true] {
            let token = self.token().await?;
            let res = self.http.get(url).query(query).bearer_auth(token).send().await?;
            let status = res.status();
            if status == StatusCode::UNAUTHORIZED && !retry {
                self.token = None;
                continue;
            }
            let body: Value = res.json().await.unwrap_or(Value::Null);
            if !status.is_success() {
                let message = body["error"]["message"].as_str().unwrap_or_default();
                bail!("{} {}", status.as_u16(), message.lines().next().unwrap_or_default());
            }
            return Ok(body);
        }
        unreachable!()
    }

    /// A paginated list (`nextPageToken`): the items of `field`, across all pages.
    async fn list(&mut self, url: &str, field: &str, query: &[(&str, String)]) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut q = query.to_vec();
            if let Some(p) = &page {
                q.push(("pageToken", p.clone()));
            }
            let mut body = self.get(url, &q).await?;
            if let Some(items) = body[field].as_array_mut() {
                out.append(items);
            }
            page = body["nextPageToken"].as_str().filter(|p| !p.is_empty()).map(String::from);
            if page.is_none() {
                break;
            }
        }
        Ok(out)
    }
}

/* ---------- Cloud Run jobs ---------- */

/// Already-read executions of a job, kept as long as its last execution has neither changed nor is running.
type Executions = HashMap<String, (String, Vec<RunExecution>)>;

/// A project's jobs, their executions and their trigger; the Cloud Scheduler error kept separate,
/// as it does not prevent displaying the jobs.
async fn run_jobs(gcp: &mut Gcp, project: &str, region: &str, cache: &mut Executions) -> Result<(Vec<RunJob>, Option<String>)> {
    let base = format!("https://run.googleapis.com/v2/projects/{project}/locations/{region}/jobs");
    let items = gcp.list(&base, "jobs", &[]).await?;
    let (triggers, trigger_error) = match scheduler(gcp, project, region).await {
        Ok(t) => (t, None),
        Err(e) => (HashMap::new(), Some(short(&e))),
    };
    let mut jobs = Vec::new();
    for v in &items {
        let mut job = run_job(v, project, region);
        job.trigger = triggers.get(&job.name).cloned();
        let full = v["name"].as_str().unwrap_or_default().to_string();
        let latest = v["latestCreatedExecution"]["name"].as_str().unwrap_or_default().to_string();
        let fresh = cache.get(&full).filter(|(l, e)| *l == latest && !e.iter().any(|x| x.status == "running"));
        job.executions = match fresh {
            Some((_, e)) => e.clone(),
            None if latest.is_empty() => Vec::new(),
            None => {
                let list = gcp.get(&format!("https://run.googleapis.com/v2/{full}/executions"), &[("pageSize", EXECUTIONS.to_string())]).await?;
                let mut e: Vec<RunExecution> = list["executions"].as_array().into_iter().flatten().filter_map(execution).collect();
                e.sort_by_key(|a| std::cmp::Reverse(a.created_at));
                e.truncate(EXECUTIONS);
                cache.insert(full, (latest, e.clone()));
                e
            }
        };
        jobs.push(job);
    }
    Ok((jobs, trigger_error))
}

fn run_job(v: &Value, project: &str, region: &str) -> RunJob {
    RunJob {
        project: project.to_string(),
        region: region.to_string(),
        name: last_segment(&v["name"]),
        image: v["template"]["template"]["containers"][0]["image"].as_str().map(short_image),
        updated_at: time(&v["updateTime"]),
        trigger: None,
        executions: Vec::new(),
    }
}

static EXIT_CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)exit(?:ed with)? code(?: of)? (-?\d+)").unwrap());

fn execution(v: &Value) -> Option<RunExecution> {
    let count = |k: &str| v[k].as_u64().unwrap_or(0) as u32;
    let conditions: Vec<&Value> = v["conditions"].as_array().into_iter().flatten().collect();
    let finished_at = time(&v["completionTime"]);
    let failed_tasks = count("failedCount");
    let failed_condition = conditions.iter().any(|c| c["type"] == "Completed" && c["state"] == "CONDITION_FAILED");
    let status = if finished_at.is_none() {
        "running"
    } else if failed_tasks > 0 || failed_condition {
        "failed"
    } else if count("cancelledCount") > 0 {
        "cancelled"
    } else {
        "succeeded"
    };
    // Only the exit code is drawn from the messages; the message itself is not kept.
    let exit_code = conditions.iter().filter_map(|c| c["message"].as_str()).find_map(|m| EXIT_CODE.captures(m)).and_then(|c| c[1].parse().ok());
    Some(RunExecution {
        name: last_segment(&v["name"]),
        created_at: time(&v["createTime"])?,
        started_at: time(&v["startTime"]),
        finished_at,
        status: status.into(),
        tasks: count("taskCount"),
        failed_tasks,
        exit_code: if status == "failed" { exit_code } else { None },
    })
}

static RUN_TARGET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/jobs/([A-Za-z0-9_-]+):run\b").unwrap());

/// Cloud Scheduler triggers that run a Cloud Run job, by job name.
async fn scheduler(gcp: &mut Gcp, project: &str, region: &str) -> Result<HashMap<String, SchedulerTrigger>> {
    let url = format!("https://cloudscheduler.googleapis.com/v1/projects/{project}/locations/{region}/jobs");
    let items = gcp.list(&url, "jobs", &[]).await?;
    Ok(items.iter().filter_map(trigger).collect())
}

fn trigger(v: &Value) -> Option<(String, SchedulerTrigger)> {
    let uri = v["httpTarget"]["uri"].as_str()?;
    let job = RUN_TARGET.captures(uri)?[1].to_string();
    let code = v["status"]["code"].as_i64().unwrap_or(0);
    let t = SchedulerTrigger {
        name: last_segment(&v["name"]),
        schedule: v["schedule"].as_str().unwrap_or_default().to_string(),
        time_zone: v["timeZone"].as_str().map(String::from),
        state: v["state"].as_str().unwrap_or("UNKNOWN").to_string(),
        last_attempt: time(&v["lastAttemptTime"]),
        next_run: time(&v["scheduleTime"]),
        error: (code != 0).then(|| rpc_code(code)),
    };
    Some((job, t))
}

/// Name of a `google.rpc.Code` code.
fn rpc_code(code: i64) -> String {
    const NAMES: [&str; 17] = [
        "OK", "CANCELLED", "UNKNOWN", "INVALID_ARGUMENT", "DEADLINE_EXCEEDED", "NOT_FOUND", "ALREADY_EXISTS", "PERMISSION_DENIED", "RESOURCE_EXHAUSTED",
        "FAILED_PRECONDITION", "ABORTED", "OUT_OF_RANGE", "UNIMPLEMENTED", "INTERNAL", "UNAVAILABLE", "DATA_LOSS", "UNAUTHENTICATED",
    ];
    usize::try_from(code).ok().and_then(|i| NAMES.get(i)).map_or_else(|| format!("code {code}"), |n| n.to_string())
}

/* ---------- BigQuery: freshness and scheduled queries ---------- */

/// `project.dataset.table`; without a project, the default one.
fn table_ref(t: &str, default_project: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = t.trim().split(['.', ':']).collect();
    match parts.as_slice() {
        [p, d, t] => Some((p.to_string(), d.to_string(), t.to_string())),
        [d, t] => Some((default_project.to_string(), d.to_string(), t.to_string())),
        _ => None,
    }
}

async fn table(gcp: &mut Gcp, cfg: &DataTable, default_project: &str) -> TableFreshness {
    let mut out = TableFreshness { table: cfg.table.trim().to_string(), max_age_hours: cfg.max_age_hours, fed_by: cfg.fed_by.clone(), last_modified: None, rows: None, error: None };
    let Some((p, d, t)) = table_ref(&cfg.table, default_project) else {
        out.error = Some("expected project.dataset.table".into());
        return out;
    };
    out.table = format!("{p}.{d}.{t}");
    // `fields`: neither schema nor preview in the response, only the age and the row count.
    let url = format!("https://bigquery.googleapis.com/bigquery/v2/projects/{p}/datasets/{d}/tables/{t}");
    match gcp.get(&url, &[("fields", "lastModifiedTime,numRows".into())]).await {
        Ok(v) => {
            out.last_modified = millis(&v["lastModifiedTime"]);
            out.rows = number(&v["numRows"]);
        }
        Err(e) => out.error = Some(short(&e)),
    }
    out
}

async fn transfers(gcp: &mut Gcp, project: &str, location: &str) -> Result<Vec<ScheduledQuery>> {
    let url = format!("https://bigquerydatatransfer.googleapis.com/v1/projects/{project}/locations/{location}/transferConfigs");
    // `fields`: never `params`, which contains the SQL.
    let query = [
        ("dataSourceIds", "scheduled_query".to_string()),
        ("fields", "transferConfigs(name,displayName,schedule,state,disabled,updateTime,nextRunTime),nextPageToken".into()),
    ];
    let items = gcp.list(&url, "transferConfigs", &query).await?;
    Ok(items.iter().map(|v| scheduled_query(v, project)).collect())
}

fn scheduled_query(v: &Value, project: &str) -> ScheduledQuery {
    ScheduledQuery {
        project: project.to_string(),
        name: v["displayName"].as_str().map(String::from).unwrap_or_else(|| last_segment(&v["name"])),
        schedule: v["schedule"].as_str().map(String::from),
        state: v["state"].as_str().unwrap_or("UNKNOWN").to_string(),
        disabled: v["disabled"].as_bool().unwrap_or(false),
        updated_at: time(&v["updateTime"]),
        next_run: time(&v["nextRunTime"]),
    }
}

/* ---------- Datastream ---------- */

async fn streams(gcp: &mut Gcp, project: &str, region: &str) -> Result<Vec<DataStream>> {
    let url = format!("https://datastream.googleapis.com/v1/projects/{project}/locations/{region}/streams");
    let items = gcp.list(&url, "streams", &[]).await?;
    let mut out: Vec<DataStream> = items.iter().map(|v| stream(v, project, region)).collect();
    if out.is_empty() {
        return Ok(out);
    }
    // The lag comes from Cloud Monitoring; without it, the streams are displayed anyway.
    match freshness(gcp, project).await {
        Ok(by_stream) => {
            for s in &mut out {
                if let Some(points) = by_stream.get(&s.name) {
                    let (at, seconds, history) = current_freshness(points, Utc::now());
                    (s.freshness_at, s.freshness_seconds, s.freshness_history) = (at, seconds, history);
                }
            }
        }
        Err(e) => tracing::warn!("data: {project} stream freshness: {e:#}"),
    }
    Ok(out)
}

fn stream(v: &Value, project: &str, region: &str) -> DataStream {
    // `postgresqlSourceConfig` → `postgresql`; only the type is kept, not the table list.
    let kind = |config: &Value, suffix: &str| {
        config.as_object().and_then(|o| o.keys().find_map(|k| k.strip_suffix(suffix).map(|k| k.to_lowercase())))
    };
    let name = last_segment(&v["name"]);
    DataStream {
        project: project.to_string(),
        region: region.to_string(),
        display_name: v["displayName"].as_str().map(String::from).unwrap_or_else(|| name.clone()),
        name,
        state: v["state"].as_str().unwrap_or("UNKNOWN").to_string(),
        source: kind(&v["sourceConfig"], "SourceConfig"),
        destination: kind(&v["destinationConfig"], "DestinationConfig"),
        errors: v["errors"].as_array().into_iter().flatten().filter_map(|e| e["reason"].as_str().map(String::from)).collect(),
        freshness_seconds: None,
        freshness_at: None,
        freshness_history: Vec::new(),
    }
}

/// Window read from Cloud Monitoring, and the bucket of each point.
const FRESHNESS_WINDOW: chrono::Duration = chrono::Duration::hours(6);
const FRESHNESS_STEP: chrono::Duration = chrono::Duration::minutes(10);
/// An older point no longer tells the current lag: the source has written nothing since.
const FRESHNESS_STALE: chrono::Duration = chrono::Duration::minutes(20);

/// Points of a metric: end of the bucket, value; oldest first.
type Points = Vec<(DateTime<Utc>, f64)>;

/// Lag points of each stream over 6 h, in seconds.
async fn freshness(gcp: &mut Gcp, project: &str) -> Result<HashMap<String, Points>> {
    let end = Utc::now();
    let start = end - FRESHNESS_WINDOW;
    let url = format!("https://monitoring.googleapis.com/v3/projects/{project}/timeSeries");
    let query = [
        ("filter", r#"metric.type = "datastream.googleapis.com/stream/freshness""#.to_string()),
        ("interval.startTime", start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        ("interval.endTime", end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        ("aggregation.alignmentPeriod", "600s".into()),
        ("aggregation.perSeriesAligner", "ALIGN_MAX".into()),
    ];
    let series = gcp.list(&url, "timeSeries", &query).await?;
    Ok(series.iter().filter_map(freshness_series).collect())
}

fn freshness_series(v: &Value) -> Option<(String, Points)> {
    let id = v["resource"]["labels"]["stream_id"].as_str()?.to_string();
    let mut points: Points = v["points"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((time(&p["interval"]["endTime"])?, p["value"]["doubleValue"].as_f64().or_else(|| number(&p["value"]["int64Value"]).map(|n| n as f64))?)))
        .collect();
    points.sort_by_key(|(t, _)| *t);
    Some((id, points))
}

/// The current lag (only from a recent point), the time of the last point, and the history in
/// regular buckets: Datastream does not write the metric when it has nothing to read, and a
/// bucket without a point is then worth 0, like the metric itself in that case.
fn current_freshness(points: &[(DateTime<Utc>, f64)], now: DateTime<Utc>) -> (Option<DateTime<Utc>>, Option<f64>, Vec<f64>) {
    let last = points.last().copied();
    let current = last.filter(|(t, _)| now - *t <= FRESHNESS_STALE).map(|(_, v)| v);
    let slots = (FRESHNESS_WINDOW.num_seconds() / FRESHNESS_STEP.num_seconds()) as usize;
    let history = (0..slots)
        .map(|i| {
            let slot_end = now - FRESHNESS_STEP * (slots - 1 - i) as i32;
            let slot_start = slot_end - FRESHNESS_STEP;
            points.iter().filter(|(t, _)| *t > slot_start && *t <= slot_end).map(|(_, v)| *v).fold(0.0, f64::max)
        })
        .collect();
    (last.map(|(t, _)| t), current, history)
}

/* ---------- BigQuery cost ---------- */

#[derive(Debug, Clone)]
struct CostEntry {
    project: String,
    id: String,
    location: Option<String>,
    service_account: Option<String>,
    created: DateTime<Utc>,
    bytes: u64,
    slot_ms: u64,
    statement: Option<String>,
    tables: Vec<String>,
    done: bool,
}

/// BigQuery jobs of the window (the current month and the last 14 days), re-read in
/// chunks: after the first pass, only the jobs created since the previous one, and those that
/// were still running.
#[derive(Default)]
struct CostBook {
    jobs: HashMap<(String, String), CostEntry>,
    /// Per project: minimum creation time of the next read.
    since: HashMap<String, DateTime<Utc>>,
    read: HashSet<String>,
}

impl CostBook {
    fn window_start(now: DateTime<Local>) -> DateTime<Utc> {
        let today = now.date_naive();
        let month = today.with_day(1).unwrap_or(today);
        let first = month.min(today - chrono::Duration::days(COST_DAYS - 1));
        local_midnight(first)
    }

    async fn refresh(&mut self, gcp: &mut Gcp, project: &str, now: DateTime<Utc>) -> Result<()> {
        let start = Self::window_start(now.with_timezone(&Local));
        let pending = self.jobs.values().filter(|j| j.project == project && !j.done).map(|j| j.created).min();
        let from = [Some(*self.since.get(project).unwrap_or(&start)), pending].into_iter().flatten().min().unwrap_or(start).max(start);
        let url = format!("https://bigquery.googleapis.com/bigquery/v2/projects/{project}/jobs");
        let query = [
            ("allUsers", "true".to_string()),
            // Without the job configuration, hence without its SQL.
            ("projection", "minimal".into()),
            ("minCreationTime", from.timestamp_millis().to_string()),
            ("maxResults", "1000".into()),
        ];
        let items = gcp.list(&url, "jobs", &query).await?;
        for v in &items {
            if let Some(e) = cost_entry(v, project) {
                self.jobs.insert((e.project.clone(), e.id.clone()), e);
            }
        }
        // A margin for clocks and for jobs created during the read.
        self.since.insert(project.to_string(), now - chrono::Duration::minutes(10));
        self.read.insert(project.to_string());
        self.jobs.retain(|_, j| j.created >= start);
        Ok(())
    }

    fn any(&self) -> bool {
        !self.read.is_empty()
    }

    fn complete(&self, projects: &[String]) -> bool {
        projects.iter().all(|p| self.read.contains(p))
    }

    fn summary(&self, price_per_tib: f64, now: DateTime<Local>) -> BqCost {
        let today = now.date_naive();
        let day_of = |j: &CostEntry| j.created.with_timezone(&Local).date_naive();
        let days = (0..COST_DAYS)
            .rev()
            .map(|n| {
                let day = today - chrono::Duration::days(n);
                let of_day = self.jobs.values().filter(|j| day_of(j) == day);
                let (bytes, jobs) = of_day.fold((0u64, 0u32), |(b, n), j| (b + j.bytes, n + 1));
                BqDay { day, bytes, jobs }
            })
            .collect();
        let month = today.with_day(1).unwrap_or(today);
        let month_bytes = self.jobs.values().filter(|j| day_of(j) >= month).map(|j| j.bytes).sum();
        let mut top: Vec<&CostEntry> = self.jobs.values().filter(|j| day_of(j) == today && j.bytes > 0).collect();
        top.sort_by_key(|a| std::cmp::Reverse(a.bytes));
        let top = top
            .into_iter()
            .take(TOP_JOBS)
            .map(|j| BqJob {
                project: j.project.clone(),
                id: j.id.clone(),
                location: j.location.clone(),
                service_account: j.service_account.clone(),
                created_at: j.created,
                bytes_billed: j.bytes,
                slot_ms: j.slot_ms,
                statement: j.statement.clone(),
                tables: j.tables.clone(),
            })
            .collect();
        BqCost { price_per_tib, days, month_bytes, top, partial: false }
    }
}

fn local_midnight(day: NaiveDate) -> DateTime<Utc> {
    let midnight = day.and_hms_opt(0, 0, 0).unwrap_or_default();
    Local.from_local_datetime(&midnight).earliest().map(|t| t.with_timezone(&Utc)).unwrap_or_else(|| Utc.from_utc_datetime(&midnight))
}

fn cost_entry(v: &Value, project: &str) -> Option<CostEntry> {
    let stats = &v["statistics"];
    let id = v["jobReference"]["jobId"].as_str()?.to_string();
    // A person is never named: their address is discarded on read.
    let service_account = v["user_email"].as_str().filter(|e| e.ends_with(".gserviceaccount.com")).map(String::from);
    let tables = stats["query"]["referencedTables"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| Some(format!("{}.{}.{}", t["projectId"].as_str()?, t["datasetId"].as_str()?, t["tableId"].as_str()?)))
        .collect();
    Some(CostEntry {
        project: project.to_string(),
        id,
        location: v["jobReference"]["location"].as_str().map(String::from),
        service_account,
        created: millis(&stats["creationTime"])?,
        bytes: number(&stats["query"]["totalBytesBilled"]).unwrap_or(0),
        slot_ms: number(&stats["totalSlotMs"]).unwrap_or(0),
        statement: stats["query"]["statementType"].as_str().map(String::from),
        tables,
        done: v["state"] == "DONE",
    })
}

/* ---------- Tools ---------- */

fn last_segment(v: &Value) -> String {
    v.as_str().and_then(|n| n.rsplit('/').next()).unwrap_or_default().to_string()
}

fn time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Utc))
}

/// A number Google returns as a string (`"1234"`) or as a number.
fn number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

/// Milliseconds since the epoch (BigQuery).
fn millis(v: &Value) -> Option<DateTime<Utc>> {
    number(v).and_then(|ms| DateTime::from_timestamp_millis(ms as i64))
}

/// `europe-docker.pkg.dev/p/r/ingest:4f2a1c9` → `4f2a1c9`; a digest, its first 12 characters.
fn short_image(image: &str) -> String {
    if let Some((_, digest)) = image.split_once("@sha256:") {
        return digest.chars().take(12).collect();
    }
    let last = image.rsplit('/').next().unwrap_or(image);
    last.split_once(':').map_or(last, |(_, tag)| tag).to_string()
}

fn short(e: &anyhow::Error) -> String {
    format!("{e:#}").lines().next().unwrap_or_default().chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_a_job_and_its_executions() {
        let job = run_job(
            &json!({
                "name": "projects/data-prod/locations/us-central1/jobs/ingest-daily",
                "updateTime": "2026-10-07T16:12:00Z",
                "template": { "template": { "containers": [{ "image": "europe-docker.pkg.dev/x/data/ingest:4f2a1c9", "env": [{ "name": "DB_PASSWORD", "value": "secret" }] }] } }
            }),
            "data-prod",
            "us-central1",
        );
        assert_eq!((job.name.as_str(), job.image.as_deref()), ("ingest-daily", Some("4f2a1c9")));
        assert!(!serde_json::to_string(&job).unwrap().contains("secret"), "no env var is kept");

        let failed = execution(&json!({
            "name": "projects/p/locations/r/jobs/ingest-daily/executions/ingest-daily-x7k2p",
            "createTime": "2026-10-08T05:00:01Z", "startTime": "2026-10-08T05:00:03Z", "completionTime": "2026-10-08T05:02:44Z",
            "taskCount": 3, "failedCount": 2, "succeededCount": 1,
            "conditions": [{ "type": "Completed", "state": "CONDITION_FAILED", "message": "Task ingest-daily-x7k2p-task0 failed with message: The container exited with an exit code of 1. Customer ACME row 42 invalid" }]
        }))
        .unwrap();
        assert_eq!((failed.status.as_str(), failed.tasks, failed.failed_tasks, failed.exit_code), ("failed", 3, 2, Some(1)));
        assert!(!serde_json::to_string(&failed).unwrap().contains("ACME"), "the message is never kept");

        let running = execution(&json!({ "name": "e2", "createTime": "2026-10-08T08:00:00Z", "startTime": "2026-10-08T08:00:02Z", "taskCount": 1 })).unwrap();
        assert_eq!(running.status, "running");
        let ok = execution(&json!({ "name": "e3", "createTime": "2026-10-07T05:00:00Z", "completionTime": "2026-10-07T05:12:00Z", "taskCount": 1, "succeededCount": 1 })).unwrap();
        assert_eq!((ok.status.as_str(), ok.exit_code), ("succeeded", None));
    }

    #[test]
    fn scheduler_trigger_of_a_run_job() {
        let (job, t) = trigger(&json!({
            "name": "projects/p/locations/us-central1/jobs/compute-kpis-trigger",
            "schedule": "30 6 * * *", "timeZone": "Europe/Paris", "state": "ENABLED",
            "lastAttemptTime": "2026-10-08T04:30:00Z", "scheduleTime": "2026-10-09T04:30:00Z",
            "status": { "code": 7, "message": "denied" },
            "httpTarget": { "uri": "https://us-central1-run.googleapis.com/apis/run.googleapis.com/v1/namespaces/123/jobs/compute-kpis:run", "httpMethod": "POST" }
        }))
        .unwrap();
        assert_eq!((job.as_str(), t.error.as_deref(), t.schedule.as_str()), ("compute-kpis", Some("PERMISSION_DENIED"), "30 6 * * *"));
        let v2 = trigger(&json!({ "name": "x", "httpTarget": { "uri": "https://run.googleapis.com/v2/projects/p/locations/r/jobs/export_daily:run" } })).unwrap();
        assert_eq!((v2.0.as_str(), v2.1.error), ("export_daily", None));
        assert!(trigger(&json!({ "name": "y", "httpTarget": { "uri": "https://example.com/hook" } })).is_none());
        assert_eq!(rpc_code(99), "code 99");
    }

    #[test]
    fn table_refs() {
        assert_eq!(table_ref("data-prod.reporting.daily", "x"), Some(("data-prod".into(), "reporting".into(), "daily".into())));
        assert_eq!(table_ref("reporting.daily", "x"), Some(("x".into(), "reporting".into(), "daily".into())));
        assert_eq!(table_ref("data-prod:reporting.daily", "x"), Some(("data-prod".into(), "reporting".into(), "daily".into())));
        assert_eq!(table_ref("daily", "x"), None);
    }

    #[test]
    fn stream_keeps_types_and_reasons_only() {
        let s = stream(
            &json!({
                "name": "projects/p/locations/r/streams/pg-audit-to-bq", "displayName": "pg-audit-to-bq", "state": "RUNNING",
                "sourceConfig": { "sourceConnectionProfile": "x", "postgresqlSourceConfig": { "includeObjects": { "postgresqlSchemas": [{ "schema": "customer_acme" }] } } },
                "destinationConfig": { "bigqueryDestinationConfig": {} },
                "errors": [{ "reason": "BIGQUERY_QUOTA_EXCEEDED", "message": "table customer_acme.orders" }]
            }),
            "p",
            "r",
        );
        assert_eq!((s.source.as_deref(), s.destination.as_deref()), (Some("postgresql"), Some("bigquery")));
        assert_eq!(s.errors, vec!["BIGQUERY_QUOTA_EXCEEDED".to_string()]);
        assert!(!serde_json::to_string(&s).unwrap().contains("acme"));

        let now = Utc::now();
        let at = |min: i64| (now - chrono::Duration::minutes(min)).to_rfc3339();
        let (id, points) = freshness_series(&json!({
            "resource": { "labels": { "stream_id": "pg-audit-to-bq" } },
            "points": [
                { "interval": { "endTime": at(5) }, "value": { "doubleValue": 1080.0 } },
                { "interval": { "endTime": at(15) }, "value": { "int64Value": "600" } },
                { "interval": { "endTime": at(25) }, "value": { "doubleValue": 42.0 } }
            ]
        }))
        .unwrap();
        assert_eq!(id, "pg-audit-to-bq");
        assert_eq!(points.iter().map(|(_, v)| *v).collect::<Vec<_>>(), vec![42.0, 600.0, 1080.0], "oldest first");
        let (_, current, history) = current_freshness(&points, now);
        assert_eq!(current, Some(1080.0));
        assert_eq!(history.len(), 36);
        assert_eq!(&history[33..], &[42.0, 600.0, 1080.0]);
    }

    /// Caught up at 9 h, then nothing more to read: no more points, hence no lag displayed.
    #[test]
    fn stale_freshness_is_not_a_lag() {
        let now = Utc::now();
        let caught_up = now - chrono::Duration::hours(3);
        let points = vec![(caught_up - chrono::Duration::minutes(10), 32_000.0), (caught_up, 32_400.0)];
        let (at, current, history) = current_freshness(&points, now);
        assert_eq!((at, current), (Some(caught_up), None));
        assert_eq!(history.last(), Some(&0.0), "nothing to read since: 0, as the metric itself says");
        assert!(history.contains(&32_400.0), "the catch-up stays visible in the history");
    }

    #[test]
    fn scheduled_query_without_sql() {
        let q = scheduled_query(&json!({ "name": "projects/1/locations/eu/transferConfigs/abc", "displayName": "weekly_rollup", "state": "FAILED", "params": { "query": "SELECT secret" } }), "p");
        assert_eq!((q.name.as_str(), q.state.as_str()), ("weekly_rollup", "FAILED"));
        assert!(!serde_json::to_string(&q).unwrap().contains("secret"));
    }

    #[test]
    fn cost_of_the_day_without_people() {
        let job = |id: &str, email: &str, created: DateTime<Utc>, bytes: u64| {
            json!({
                "jobReference": { "jobId": id, "location": "EU" }, "state": "DONE", "user_email": email,
                "statistics": { "creationTime": created.timestamp_millis().to_string(), "totalSlotMs": "1000",
                    "query": { "totalBytesBilled": bytes.to_string(), "statementType": "SELECT", "referencedTables": [{ "projectId": "p", "datasetId": "raw", "tableId": "events" }] } }
            })
        };
        let now = Local::now();
        let today = now.with_timezone(&Utc) - chrono::Duration::minutes(1);
        let mut book = CostBook::default();
        for v in [job("a", "looker@p.iam.gserviceaccount.com", today, 4 << 40), job("b", "someone@example.com", today, 1 << 40), job("c", "etl@p.iam.gserviceaccount.com", today - chrono::Duration::days(3), 2 << 40)] {
            let e = cost_entry(&v, "p").unwrap();
            book.jobs.insert((e.project.clone(), e.id.clone()), e);
        }
        book.read.insert("p".into());
        let c = book.summary(6.25, now);
        assert_eq!(c.days.len(), COST_DAYS as usize);
        assert_eq!((c.days.last().unwrap().bytes, c.days.last().unwrap().jobs), (5 << 40, 2));
        assert_eq!(c.top[0].service_account.as_deref(), Some("looker@p.iam.gserviceaccount.com"));
        assert_eq!(c.top[1].service_account, None, "a person is never named");
        assert_eq!(c.top[0].tables, vec!["p.raw.events".to_string()]);
        assert!(!serde_json::to_string(&c).unwrap().contains("someone"));
        assert!(CostBook::window_start(now) <= local_midnight(now.date_naive() - chrono::Duration::days(COST_DAYS - 1)));
    }

    #[test]
    fn short_images() {
        assert_eq!(short_image("europe-docker.pkg.dev/p/r/ingest:4f2a1c9"), "4f2a1c9");
        assert_eq!(short_image("gcr.io/p/ingest@sha256:0123456789abcdef0123"), "0123456789ab");
        assert_eq!(short_image("localhost:5000/ingest"), "ingest");
    }
}
