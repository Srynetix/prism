//! The Data block (`[data]`) and the BigQuery cost block, read from GCP: Cloud Run jobs and their
//! Cloud Scheduler triggers, freshness of the declared BigQuery tables, scheduled queries,
//! Datastream streams, estimated BigQuery cost. One source for both blocks (`data`); their
//! window side: `web/src/blocks/data/`.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Block, BlockCtx};
use prism_core::remote::Class as RemoteClass;

mod collector;

pub struct Data;

pub static BLOCK: Data = Data;

impl Block for Data {
    fn id(&self) -> &'static str {
        "data"
    }

    fn remote(&self, command: &str, _args: &Value) -> RemoteClass {
        if command == "refresh" { RemoteClass::Read } else { RemoteClass::Never }
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg = settings(config)?;
        if cfg.enabled {
            // The blocks know they are on before the first read answers.
            ctx.publish(&GcpData { thresholds: cfg.thresholds.clone(), ..Default::default() });
            prism_base::rt::spawn(collector::run(cfg, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        settings(config).ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("data", v)]).unwrap_or_default()
    }

    /// Jobs, tables and streams broken or drifting, a cost spike.
    fn alerts(&self, state: &Value) -> Vec<prism_core::alerts::Alert> {
        crate::alerts::data(state)
    }

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                // Rereads GCP now, cost included; answers once the read is in the snapshot.
                "refresh" => collector::refresh().await.map(|()| Value::Null),
                other => bail!("the Data block has no command {other}"),
            }
        })
    }
}

/// `[data]`; the thresholds formerly in `[alerts]` still read there when `[data]` does not say.
fn settings(config: &toml::Table) -> Result<DataConfig> {
    let mut cfg: DataConfig = crate::section(config, "data")?;
    let own = |key: &str| config.get("data").and_then(|d| d.get(key)).is_some();
    let legacy = |key: &str| config.get("alerts").and_then(|a| a.get(key)).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
    let t = &mut cfg.thresholds;
    if let Some(v) = legacy("data_slow_factor").filter(|_| !own("slow_factor")) {
        t.slow_factor = v;
    }
    if let Some(v) = legacy("data_stream_lag_minutes").filter(|_| !own("stream_lag_minutes")) {
        t.stream_lag_minutes = v as u32;
    }
    if let Some(v) = legacy("data_cost_spike_factor").filter(|_| !own("cost_spike_factor")) {
        t.cost_spike_factor = v;
    }
    Ok(cfg)
}

/// What drifts: a job slower than this multiple of its median, a stream further behind, a day's
/// cost above this multiple of the 14-day median.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DataThresholds"))]
pub struct Thresholds {
    pub slow_factor: f64,
    pub stream_lag_minutes: u32,
    pub cost_spike_factor: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { slow_factor: 2.0, stream_lag_minutes: 5, cost_spike_factor: 3.0 }
    }
}

/// Data pipelines on GCP (`[data]`), read with the gcloud login (`gcloud auth login`),
/// read-only. One switch per source; everything is reread on next launch.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct DataConfig {
    /// Off by default: only the Data profile, or this setting, turns it on.
    pub enabled: bool,
    /// GCP projects read.
    pub projects: Vec<String>,
    /// Region of the Cloud Run jobs, Cloud Scheduler and Datastream.
    pub region: String,
    /// BigQuery location of the scheduled queries and jobs (`us`, `eu`, or a region).
    pub bigquery_location: String,
    pub poll_seconds: u64,
    pub cloud_run_jobs: bool,
    pub bigquery_freshness: bool,
    pub scheduled_queries: bool,
    pub datastream: bool,
    /// Cost estimated from the jobs' billed bytes (on-demand pricing).
    pub bigquery_cost: bool,
    /// On-demand price, in dollars per TiB billed.
    pub price_per_tib: f64,
    /// Tables expected to be updated regularly.
    pub tables: Vec<DataTable>,
    /// What drifts (formerly `[alerts] data_slow_factor`, `data_stream_lag_minutes`, `data_cost_spike_factor`).
    #[serde(flatten)]
    pub thresholds: Thresholds,
}

impl Default for DataConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            projects: Vec::new(),
            region: "us-central1".into(),
            bigquery_location: "us".into(),
            poll_seconds: 60,
            cloud_run_jobs: true,
            bigquery_freshness: true,
            scheduled_queries: true,
            datastream: true,
            bigquery_cost: true,
            price_per_tib: 6.25,
            tables: Vec::new(),
            thresholds: Thresholds::default(),
        }
    }
}

/// A followed table (`[[data.tables]]`): late when its last modification is older
/// than `max_age_hours`. `fed_by`: the Cloud Run job feeding it, declared (never guessed).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct DataTable {
    /// `project.dataset.table`, or `dataset.table` in the first project.
    pub table: String,
    #[serde(default = "default_max_age_hours")]
    pub max_age_hours: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fed_by: Option<String>,
}

fn default_max_age_hours() -> u32 {
    26
}


/// Data pipelines on GCP (`[data]`): Cloud Run jobs, BigQuery table freshness, scheduled
/// queries, Datastream streams and estimated BigQuery cost. Only metadata: no rows, no
/// SQL, no logs, no identity of people.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GcpData {
    pub jobs: Vec<RunJob>,
    pub tables: Vec<TableFreshness>,
    pub transfers: Vec<ScheduledQuery>,
    pub streams: Vec<DataStream>,
    /// Absent without `bigquery_cost`, or before the first read of BigQuery jobs.
    pub cost: Option<BqCost>,
    /// A read that failed (`data-prod · Cloud Run: 403 …`); the others are shown anyway.
    pub errors: Vec<String>,
    /// The thresholds of what drifts (`[data]`), for the window's states and alerts.
    pub thresholds: Thresholds,
}

/// A Cloud Run job and its latest executions.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunJob {
    pub project: String,
    pub region: String,
    pub name: String,
    /// End of the deployed image (tag, or start of the digest).
    pub image: Option<String>,
    /// Last modification of the job: a deployment.
    pub updated_at: Option<DateTime<Utc>>,
    /// Cloud Scheduler trigger that launches it, if there is one.
    pub trigger: Option<SchedulerTrigger>,
    /// Most recent first, 14 at most.
    pub executions: Vec<RunExecution>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SchedulerTrigger {
    pub name: String,
    /// Cron expression (`0 5 * * *`) and its time zone.
    pub schedule: String,
    pub time_zone: Option<String>,
    /// `ENABLED`, `PAUSED`, `DISABLED`…
    pub state: String,
    pub last_attempt: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
    /// Code of the last attempt when it failed (`PERMISSION_DENIED`): the execution did not happen.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RunExecution {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    /// `running`, `succeeded`, `failed`, `cancelled`.
    #[cfg_attr(feature = "ts", ts(type = "'running' | 'succeeded' | 'failed' | 'cancelled'"))]
    pub status: String,
    pub tasks: u32,
    pub failed_tasks: u32,
    /// Container exit code when Cloud Run gives it; never the message.
    pub exit_code: Option<i32>,
}

/// A table declared in `[[data.tables]]`, and its age.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TableFreshness {
    /// `project.dataset.table`.
    pub table: String,
    pub max_age_hours: u32,
    pub fed_by: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
    pub rows: Option<u64>,
    pub error: Option<String>,
}

/// A scheduled query (Data Transfer Service): its state, never its SQL.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ScheduledQuery {
    pub project: String,
    pub name: String,
    pub schedule: Option<String>,
    /// State of the last run: `SUCCEEDED`, `FAILED`, `RUNNING`, `PENDING`, `CANCELLED`.
    pub state: String,
    pub disabled: bool,
    pub updated_at: Option<DateTime<Utc>>,
    pub next_run: Option<DateTime<Utc>>,
}

/// A Datastream stream.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DataStream {
    pub project: String,
    pub region: String,
    pub name: String,
    pub display_name: String,
    /// `RUNNING`, `PAUSED`, `FAILED`, `FAILED_PERMANENTLY`, `STARTING`, `DRAINING`…
    pub state: String,
    /// `postgresql`, `mysql`, `oracle`, `sqlserver`, `salesforce`…
    pub source: Option<String>,
    /// `bigquery`, `gcs`.
    pub destination: Option<String>,
    /// Reasons of the stream's errors (`BIGQUERY_UNSUPPORTED_PRIMARY_KEY_CHANGE`), without their message.
    pub errors: Vec<String>,
    /// Current replication lag, in seconds (Cloud Monitoring `stream/freshness`). Absent without
    /// a recent point: Datastream stops writing the metric when the source has nothing new.
    pub freshness_seconds: Option<f64>,
    /// Time of the metric's last point, even if too old to tell the current lag.
    pub freshness_at: Option<DateTime<Utc>>,
    /// The lag over the last 6 hours, one point per 10 minutes, oldest first; a
    /// slice without a point counts as 0 (nothing to read).
    pub freshness_history: Vec<f64>,
}

/// Estimated BigQuery cost: bytes billed by jobs × on-demand price (`price_per_tib`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BqCost {
    pub price_per_tib: f64,
    /// Bytes billed and jobs per day (local time), the last 14 days, today last.
    pub days: Vec<BqDay>,
    /// Since the 1st of the month.
    pub month_bytes: u64,
    /// Today's most expensive, 8 at most.
    pub top: Vec<BqJob>,
    /// Older days are not all read yet (first pass).
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BqDay {
    pub day: chrono::NaiveDate,
    pub bytes: u64,
    pub jobs: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BqJob {
    pub project: String,
    pub id: String,
    pub location: Option<String>,
    /// The service account that launched it; absent for a person, never named.
    pub service_account: Option<String>,
    pub created_at: DateTime<Utc>,
    pub bytes_billed: u64,
    pub slot_ms: u64,
    /// `SELECT`, `MERGE`, `INSERT`… (the statement type, not its text).
    pub statement: Option<String>,
    /// Tables read (`project.dataset.table`).
    pub tables: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[data]` off by default; a price or a factor written as an integer is read too; the
    /// thresholds formerly in `[alerts]` still read there.
    #[test]
    fn data_section() {
        let read = |text: &str| settings(&toml::from_str(text).unwrap()).unwrap();
        let off = read("");
        assert!(!off.enabled && off.cloud_run_jobs && off.price_per_tib == 6.25 && off.thresholds == Thresholds::default());
        let on = read("[data]\nenabled = true\nprojects = [\"data-prod\"]\nprice_per_tib = 6\n[[data.tables]]\ntable = \"data-prod.reporting.daily\"\nfed_by = \"ingest\"\n[alerts]\ndata_slow_factor = 3\n");
        assert!(on.enabled && on.price_per_tib == 6.0 && on.thresholds.slow_factor == 3.0);
        assert_eq!((on.tables[0].max_age_hours, on.tables[0].fed_by.as_deref()), (26, Some("ingest")));
        let own = read("[data]\nslow_factor = 4\nstream_lag_minutes = 10\n[alerts]\ndata_slow_factor = 3\ndata_cost_spike_factor = 5\n");
        assert_eq!((own.thresholds.slow_factor, own.thresholds.stream_lag_minutes, own.thresholds.cost_spike_factor), (4.0, 10, 5.0));
    }
}
