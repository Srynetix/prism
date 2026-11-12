//! The block contract, as Prism's core has it (`prism_core::blocks`): the same trait and context,
//! their effects going through the component's runtime instead of Prism's hub.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{Alert, ChangeEvent, ImageTarget, Provider, Stage};

/// A block with a source of its own: Prism's `Block`, hook for hook.
pub trait Block: Send + Sync {
    /// Its identifier: its config table, its key in the snapshot, the window's block id.
    fn id(&self) -> &'static str;

    /// Reads what the core needs of it before anything starts (the stages it declares), demo
    /// included.
    fn configure(&self, _config: &toml::Table) -> Result<()> {
        Ok(())
    }

    /// Its messages in another language (`tr!`), the English text the key.
    fn messages(&self, _lang: crate::Lang) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// Its settings as it reads them from `config`, by table. Not read from a module yet.
    fn settings(&self, _config: &toml::Table) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The JSON schema of its settings, by table. Not read from a module yet.
    fn schema(&self) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The repositories it reads (Prism reads it in the module's manifest, `repos`).
    fn repos(&self) -> RepoUse {
        RepoUse::None
    }

    /// Its sources read apart (`BlockCtx::part`): each a part Prism resumes on its own.
    fn parts(&self) -> &'static [&'static str] {
        &[]
    }

    /// Reads its settings and, when on, spawns its collector (`sdk::spawn`) with `ctx`.
    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()>;

    /// A change of the config configures it anew (Prism reads it in the manifest, `reloads`).
    fn reloads(&self) -> bool {
        false
    }

    /// A call from the window: `command` and its arguments, its answer as JSON.
    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>>;

    /// Its state in the demo profile, as of `now`; `None`: off there.
    fn demo(&self, _now: DateTime<Utc>) -> Option<Value> {
        None
    }

    /// The stages it declares (`provides = ["stages"]` in the manifest).
    fn stages(&self) -> Option<&'static dyn StagesProvider> {
        None
    }

    /// The images it names for Vulnerabilities (`provides = ["images"]`).
    fn images(&self) -> Option<&'static dyn ImageProvider> {
        None
    }

    /// What it adds to the Changes timeline, from its state as it published it.
    fn changes(&self, _state: &Value, _now: DateTime<Utc>) -> Vec<ChangeEvent> {
        Vec::new()
    }

    /// Its lines in Alerts, from its state as it published it.
    fn alerts(&self, _state: &Value) -> Vec<Alert> {
        Vec::new()
    }

    /// A call in the demo profile: from sample data, or `None` to refuse it (read-only).
    fn demo_call(&self, _command: &str, _args: &Value) -> Option<Result<Value>> {
        None
    }

    /// The class of `command` called from a phone (Prism reads it in the manifest, `remote`).
    fn remote(&self, _command: &str, _args: &Value) -> RemoteClass {
        RemoteClass::Never
    }
}

/// The class of a block's command called from a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteClass {
    Read,
    Act,
    Critical,
    Never,
}

/// The repositories a block reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepoUse {
    None,
    One,
    Many,
}

/// A repository a block reads: its `[[repos]]` name, project and local clone.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BlockRepo {
    pub name: String,
    pub project: Option<String>,
    pub clone: Option<PathBuf>,
}

/// `[gitlab]`, as a block reads it: the instance, the first repository's project.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GitlabConfig {
    pub project: String,
    pub host: String,
}

impl Default for GitlabConfig {
    fn default() -> Self {
        Self { project: String::new(), host: "gitlab.com".into() }
    }
}

/// A repository with a local clone, as a provider may read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalRepo {
    pub name: String,
    pub clone: PathBuf,
}

/// What Prism resolved for a block as it starts: the forge, and the repositories it reads.
#[derive(Debug, Clone, Deserialize)]
pub struct BlockEnv {
    #[serde(default)]
    pub forge: Provider,
    #[serde(default)]
    pub code: bool,
    #[serde(default)]
    pub repos: Vec<BlockRepo>,
    pub project: Option<String>,
    pub clone: Option<PathBuf>,
    #[serde(default)]
    pub gitlab: GitlabConfig,
    #[serde(default)]
    pub clones: Vec<LocalRepo>,
}

/// What a block reaches of Prism: its place in the snapshot, its errors, its pace.
#[derive(Clone)]
pub struct BlockCtx {
    id: &'static str,
    source: &'static str,
    env: Arc<BlockEnv>,
}

impl BlockCtx {
    pub(crate) fn new(id: &'static str, env: Arc<BlockEnv>) -> Self {
        Self { id, source: id, env }
    }

    /// The same block, its errors under `source`: one per source it reads apart.
    pub fn part(&self, source: &'static str) -> Self {
        Self { source, ..self.clone() }
    }

    pub fn env(&self) -> &BlockEnv {
        &self.env
    }

    /// Its state, sent to the window when it changed.
    pub fn publish(&self, state: &impl Serialize) {
        match serde_json::to_value(state) {
            Ok(v) => crate::runtime::publish(self.id, v),
            Err(e) => crate::host::log(0, &format!("{}: state not serializable: {e}", self.id)),
        }
    }

    /// Its read's error, or `None` after a successful one.
    pub fn set_error(&self, err: Option<String>) {
        crate::runtime::set_error(self.source, err);
    }

    /// Wait between two reads: Prism resumes this loop `every` later.
    pub fn pause(&self, every: Duration) -> Pause {
        Pause { every, source: self.source, waited: false }
    }
}

/// `BlockCtx::pause`: pending once (control goes back to Prism, which resumes it `every`
/// later), then ready.
pub struct Pause {
    every: Duration,
    source: &'static str,
    waited: bool,
}

impl std::future::Future for Pause {
    type Output = ();
    fn poll(mut self: std::pin::Pin<&mut Self>, _: &mut std::task::Context<'_>) -> std::task::Poll<()> {
        if self.waited {
            return std::task::Poll::Ready(());
        }
        self.waited = true;
        crate::runtime::paused(self.source, self.every);
        std::task::Poll::Pending
    }
}

/// A block's settings: its table of the config, defaults for what it leaves out.
pub fn section<T: serde::de::DeserializeOwned>(config: &toml::Table, id: &str) -> Result<T> {
    let table = config.get(id).cloned().unwrap_or_else(|| toml::Value::Table(Default::default()));
    Ok(table.try_into()?)
}

/// The stages, in their order (branch builds first, production last).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stages(pub Vec<Stage>);

impl Stages {
    /// Every stage's environment name.
    pub fn envs(&self) -> Vec<&str> {
        self.0.iter().map(|s| s.env.as_str()).filter(|e| !e.is_empty()).collect()
    }

    /// Production's environment.
    pub fn production(&self) -> &str {
        self.0.iter().find(|s| s.role == "production").map_or("", |s| s.env.as_str())
    }
}

/// What a provider tells of the workloads: the applications, and by application and environment
/// the version expected there.
#[derive(Debug, Clone, Default)]
pub struct Hints {
    pub apps: Vec<String>,
    pub expected: HashMap<(String, String), String>,
}

/// The stages a block declares (`Block::stages`).
pub trait StagesProvider: Send + Sync {
    fn stages(&self) -> Stages;
    fn hints<'a>(&'a self, repo: &'a Path) -> BoxFuture<'a, Result<Hints>>;
}

/// The images a block names for Vulnerabilities (`Block::images`).
pub trait ImageProvider: Send + Sync {
    fn images<'a>(&'a self, repo: &'a Path, project: &'a str, local: &'a [LocalRepo]) -> BoxFuture<'a, Result<Vec<ImageTarget>>>;
}
