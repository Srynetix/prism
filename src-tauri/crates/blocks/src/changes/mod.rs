//! The Changes block (`[changes]`): what touched an environment, on one timeline. Argo CD syncs,
//! merge requests merged into the branch that feeds it, pods restarting or crashing, alerts and
//! vulnerabilities, and what a pack's blocks add (`Block::changes`). Each of them Prism already
//! reads, but every block shows only the present: this one keeps the transitions in a journal
//! (`changes.jsonl` in the profile's data folder) over `retention_days`, and links a crash to the
//! change just before it on the same application. Its window side: `web/src/blocks/changes/`.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{sdk::RemoteClass, Block, BlockCtx};
pub use prism_core::changes::{ChangeEvent, ChangeKind, ChangeLink, ChangeMr, ChangeSeverity};

mod collector;
mod demo;
mod observe;

pub struct Changes;

pub static BLOCK: Changes = Changes;

impl Block for Changes {
    fn id(&self) -> &'static str {
        "changes"
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
            prism_base::rt::spawn(collector::run(cfg, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        settings(config).ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("changes", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                // Reads the sources again now.
                "refresh" => {
                    collector::refresh();
                    Ok(Value::Null)
                }
                // The window's alerts (computed there), each time they change.
                "alerts" => {
                    let alerts: Vec<WindowAlert> = serde_json::from_value(args.get("alerts").cloned().unwrap_or_default())?;
                    collector::alerts(alerts);
                    Ok(Value::Null)
                }
                other => bail!("the Changes block has no command {other}"),
            }
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }

    fn demo_call(&self, command: &str, _args: &Value) -> Option<Result<Value>> {
        matches!(command, "refresh" | "alerts").then_some(Ok(Value::Null))
    }
}

fn settings(config: &toml::Table) -> Result<ChangesConfig> {
    super::section(config, "changes")
}

/// `[changes]`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ChangesConfig {
    /// On by default: it reads only what Prism already reads (and `kube_events`).
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Days of changes kept (the journal, in the profile's data folder).
    #[serde(default = "default_retention")]
    pub retention_days: u32,
    /// A crash is linked to a sync or a rollout of the same application this many minutes before it.
    #[serde(default = "default_correlate")]
    pub correlate_minutes: u32,
    /// The period the block opens on: `1h`, `since_yesterday`, `24h`, `7d`.
    #[serde(default = "default_period")]
    pub default_period: String,
    /// The sources read: `argocd`, `mrs`, `pods`, `alerts`, `vulnerabilities`, and `packs` (every
    /// block of a pack that adds changes) or a block's id.
    #[serde(default = "default_sources")]
    pub sources: Vec<String>,
    /// Also reads the Kubernetes events (BackOff, OOMKilling, FailedScheduling) of each
    /// environment: an hour kept by the cluster, and the right to `list events`.
    #[serde(default)]
    pub kube_events: bool,
    /// The branch that feeds each environment (`{ env = "prod", branch = "release/*" }`; a table
    /// `{ staging = "main" }` is read too): the MRs merged into it go on its timeline. None: every
    /// merged MR, on every environment.
    #[serde(default, deserialize_with = "branches")]
    pub branches: Vec<EnvBranch>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct EnvBranch {
    pub env: String,
    /// `*` for any characters.
    pub branch: String,
}

/// `[{ env, branch }]`, or `{ env = branch }`.
fn branches<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EnvBranch>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        List(Vec<EnvBranch>),
        Map(BTreeMap<String, String>),
    }
    Ok(match Either::deserialize(d)? {
        Either::List(l) => l,
        Either::Map(m) => m.into_iter().map(|(env, branch)| EnvBranch { env, branch }).collect(),
    })
}

fn yes() -> bool {
    true
}

fn default_retention() -> u32 {
    14
}

fn default_correlate() -> u32 {
    30
}

fn default_period() -> String {
    "since_yesterday".into()
}

pub(crate) const SOURCES: [&str; 6] = ["argocd", "mrs", "pods", "alerts", "vulnerabilities", "packs"];

fn default_sources() -> Vec<String> {
    SOURCES.iter().map(|s| s.to_string()).collect()
}

impl ChangesConfig {
    fn reads(&self, source: &str) -> bool {
        self.sources.iter().any(|s| s == source)
    }
}

/// An alert as the window computed it, sent each time they change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowAlert {
    /// The same alert from one computation to the next.
    pub key: String,
    /// `ko`, `warn`, `info`.
    pub severity: String,
    pub text: String,
    #[serde(default)]
    pub env: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// What the window shows: the journal, oldest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChangesState {
    pub events: Vec<ChangeEvent>,
    /// The environments known, in the order of `[[environments]]`, then Argo CD's.
    pub envs: Vec<String>,
    /// `[changes] sources`.
    pub sources: Vec<String>,
    /// When Prism was closed (nothing seen then: alerts, vulnerabilities, restarts), oldest first.
    pub closed: Vec<Closed>,
    /// `[changes] correlate_minutes`, `default_period`, `retention_days`.
    pub correlate_minutes: u32,
    pub default_period: String,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "ChangesClosed"))]
pub struct Closed {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let read = |text: &str| settings(&toml::from_str(text).unwrap()).unwrap();
        let none = read("");
        assert_eq!((none.enabled, none.retention_days, none.correlate_minutes, none.default_period.as_str(), none.kube_events), (true, 14, 30, "since_yesterday", false));
        assert!(SOURCES.iter().all(|s| none.reads(s)));
        let some = read("[changes]\nsources = [\"argocd\"]\nbranches = { staging = \"main\", prod = \"release/*\" }\n");
        assert!(some.reads("argocd") && !some.reads("pods"));
        assert_eq!(some.branches[0], EnvBranch { env: "prod".into(), branch: "release/*".into() });
        let rows = read("[changes]\nbranches = [{ env = \"staging\", branch = \"main\" }]\n");
        assert_eq!(rows.branches, [EnvBranch { env: "staging".into(), branch: "main".into() }]);
    }
}
