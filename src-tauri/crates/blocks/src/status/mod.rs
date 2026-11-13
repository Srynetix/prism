//! The Status pages block (`[status]`): whether the services Prism leans on (GitHub, GitLab.com,
//! ClickUp, Jira, Claude, Google Cloud, any Atlassian Statuspage) have an incident under way, read
//! from their public status pages. No token: they are public. Its window side:
//! `web/src/blocks/status/`.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Block, BlockCtx};
use prism_core::remote::Class as RemoteClass;

mod collector;
mod demo;

pub struct Status;

pub static BLOCK: Status = Status;

impl Block for Status {
    fn id(&self) -> &'static str {
        "status"
    }

    fn remote(&self, command: &str, _args: &Value) -> RemoteClass {
        if command == "refresh" { RemoteClass::Read } else { RemoteClass::Never }
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg: StatusConfig = super::section(config, "status")?;
        if cfg.enabled {
            // The block knows it is on before the first read answers.
            ctx.publish(&StatusState { alerts: cfg.alerts, ..Default::default() });
            let agent = config.get("agents").and_then(|a| a.get("provider")).and_then(|p| p.as_str()).unwrap_or("claude").to_string();
            prism_base::rt::spawn(collector::run(cfg, agent, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        super::section::<StatusConfig>(config, "status").ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("status", v)]).unwrap_or_default()
    }

    /// The incidents under way.
    fn alerts(&self, state: &Value) -> Vec<prism_core::alerts::Alert> {
        crate::alerts::status(state)
    }

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                // ↻ in the block: read the pages now.
                "refresh" => {
                    collector::refresh_now();
                    Ok(Value::Null)
                }
                other => bail!("the Status pages block has no command {other}"),
            }
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }
}

/// `[status]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatusConfig {
    /// On by default: a few public pages, read every `poll_seconds`.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Built-in services read: `github`, `gitlab`, `clickup`, `jira`, `claude`, `gcp`. Empty: those
    /// Prism reads (its forge on gitlab.com or GitHub, its tracker, Claude for the agents).
    #[serde(default)]
    pub services: Vec<String>,
    /// Other Atlassian Statuspage pages (`https://status.example.com`, which answers
    /// `/api/v2/summary.json`), by name.
    #[serde(default)]
    pub pages: Vec<StatusPage>,
    /// An incident under way in Alerts: major and critical ones with a notification, minor ones
    /// without.
    #[serde(default = "default_true")]
    pub alerts: bool,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatusPage {
    pub name: String,
    pub url: String,
}

fn default_true() -> bool {
    true
}

fn default_poll() -> u64 {
    180
}

/// How bad: a page's overall state, an incident's impact, a component's state.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "StatusLevel"))]
pub enum Level {
    #[default]
    Operational,
    Maintenance,
    Minor,
    Major,
    Critical,
    /// The page did not answer.
    Unknown,
}

/// The pages read, in the order of `[status] services` then `pages`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StatusState {
    pub services: Vec<Service>,
    /// `[status] alerts`.
    pub alerts: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "StatusService"))]
pub struct Service {
    /// `github`…, or the page's name for one of `[status] pages`.
    pub id: String,
    pub name: String,
    /// Its logo in the window (`Logo`), for a built-in one.
    #[cfg_attr(feature = "ts", ts(type = "'github' | 'gitlab' | 'clickup' | 'jira' | 'claude' | 'gcp' | null"))]
    pub logo: Option<String>,
    /// The page people read.
    pub url: String,
    pub level: Level,
    /// The page's own words: `All Systems Operational`, `Partial Service Disruption`.
    pub summary: String,
    /// Incidents under way, and maintenance in progress.
    pub incidents: Vec<Incident>,
    /// Components not operational.
    pub components: Vec<Component>,
    /// The page's read failed: why. `level` is then `unknown`.
    pub error: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "StatusIncident"))]
pub struct Incident {
    pub id: String,
    pub name: String,
    pub impact: Level,
    /// `investigating`, `identified`, `monitoring`, `in progress` (maintenance)…
    pub status: String,
    pub url: String,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    /// Its latest update, shortened.
    pub update: String,
    /// The components it affects.
    pub components: Vec<String>,
    pub maintenance: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "StatusComponent"))]
pub struct Component {
    pub name: String,
    pub level: Level,
}
