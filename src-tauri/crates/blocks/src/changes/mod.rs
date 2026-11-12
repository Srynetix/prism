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

use super::{Block, BlockCtx};
use prism_core::remote::Class as RemoteClass;
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

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                // Reads the sources again now.
                "refresh" => {
                    collector::refresh();
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
        (command == "refresh").then_some(Ok(Value::Null))
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
    /// The pair Compare opens on (`["staging", "prod"]`, the one deployed first first). None: the
    /// environment shown and the next one in the roles' order (dev → staging → preprod → prod).
    #[serde(default)]
    pub compare: Vec<String>,
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

/// An alert of an environment, as the journal keeps it: Argo CD's and the clusters', those an agent
/// can investigate (`env_alerts`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvAlert {
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

/// The alerts of an environment (`Snapshot::all_alerts`: the window's of the modules' blocks too), by key: Argo CD's and the clusters', those an
/// agent can investigate; one that names none is left out. `None` before the clusters or Argo CD
/// have been read: their alerts would look gone.
pub fn env_alerts(s: &crate::state::Snapshot) -> Option<Vec<EnvAlert>> {
    use prism_core::alerts::{Fix, OpenTarget};
    if s.environments.is_none() && s.argocd.is_none() {
        return None;
    }
    let mut out = Vec::new();
    for a in s.all_alerts() {
        let fixes = a.fixes.as_deref().unwrap_or_default();
        let investigate = fixes.iter().find_map(|f| match f {
            Fix::Investigate { workload, env, .. } => Some((workload, env)),
            _ => None,
        });
        let changes = fixes.iter().find_map(|f| match f {
            Fix::Changes { env, app } => Some((env, app)),
            _ => None,
        });
        if a.kind != "Infra" && a.kind != "ArgoCD" && investigate.is_none() {
            continue;
        }
        let app = match &a.open {
            Some(OpenTarget::Argo { app, .. }) => s.argocd.iter().flat_map(|x| &x.instances).flat_map(|i| &i.apps).find(|x| app.ends_with(&format!("/{}", x.name))),
            _ => None,
        };
        let env = changes
            .and_then(|(env, _)| env.clone())
            .or_else(|| app.and_then(|x| x.env.clone()))
            .or_else(|| investigate.filter(|(_, env)| s.environments.iter().flat_map(|e| &e.envs).any(|e| e.env == **env)).map(|(_, env)| env.clone()));
        // Runners and CI jobs are no environment's.
        let Some(env) = env.filter(|e| !e.is_empty()) else { continue };
        let name = changes.map(|(_, app)| app.clone()).or_else(|| app.and_then(|x| x.app.clone())).or_else(|| investigate.map(|(w, _)| w.clone()));
        let rule = a.rule.and_then(|r| serde_json::to_value(r).ok()).and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| a.kind.clone());
        let object = match &a.open {
            Some(OpenTarget::Mr { iid, repo }) => Some(format!("mr:{}:{iid}", repo.as_deref().unwrap_or_default())),
            Some(OpenTarget::Ticket { id }) => Some(format!("ticket:{id}")),
            Some(OpenTarget::Pipeline { pipeline, project, .. }) => Some(format!("pipeline:{}:{pipeline}", project.as_deref().unwrap_or_default())),
            Some(OpenTarget::Argo { cluster, app }) => Some(format!("argo:{cluster}:{app}")),
            None => None,
        };
        let id = format!("{rule}|{}", object.clone().unwrap_or_else(|| a.text.clone()));
        // Its object's when it names one (an Argo CD application); otherwise its environment and
        // application, not its text, which changes with a count (restarts) or the language.
        let key = if object.is_some() { id } else { format!("{rule}|{env}|{}", name.clone().unwrap_or(id)) };
        out.push(EnvAlert { key, severity: a.severity.name().into(), text: a.text.clone(), env: Some(env), app: name, url: a.url.clone() });
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Some(out)
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
    /// `[changes] compare`: the pair Compare opens on; empty: chosen by the environments' roles.
    pub compare: Vec<String>,
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
        assert!(none.compare.is_empty());
        assert_eq!(read("[changes]\ncompare = [\"staging\", \"prod\"]\n").compare, ["staging", "prod"]);
    }
}
