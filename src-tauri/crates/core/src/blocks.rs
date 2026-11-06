//! Blocks that bring their own source: their config table, their collector, their state in the
//! snapshot, their commands and their demo data, behind one contract (`Block`). The core knows
//! none of them by name: it starts each one with the merged config, keeps what it publishes in
//! `Snapshot.blocks` under its id, and routes the window's calls to it (`block_call`). The window
//! side of a block lives in `web/src/blocks/<id>/`.
//!
//! A block absent from `Snapshot.blocks` is off; present (even with an empty state), it is on.
//!
//! This is the blocks' contract; the blocks themselves are in `prism-blocks` (Prism's own) and in
//! the app's crate (a team's, `PRISM_EXTRA_BLOCKS`), which registers them all at launch.

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::Serialize;
use serde_json::Value;

use crate::state::Hub;

/// A block with a source of its own.
pub trait Block: Send + Sync {
    /// Its identifier: its config table (`[docker]`), its key in `Snapshot.blocks` and in
    /// `errors`, the `BlockId` of the window.
    fn id(&self) -> &'static str;

    /// Reads what the core needs of it before anything starts (the stages it declares), demo
    /// included.
    fn configure(&self, _config: &toml::Table) -> Result<()> {
        Ok(())
    }

    /// Its messages in another language (`prism_base::tr!`): a table as Prism's own
    /// (`prism_base::i18n`'s `fr/`), the English text the key; Prism's tables win over it.
    fn messages(&self, _lang: prism_base::i18n::Lang) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// Its settings as it reads them from `config`, by table (`docker`; a few keys of another
    /// table, such as `[gitlab]` keys of its own, merge into it): Settings shows them, and their
    /// defaults (from an empty config).
    fn settings(&self, _config: &toml::Table) -> Vec<(&'static str, Value)> {
        Vec::new()
    }

    /// The JSON schema of its settings, by table (as `settings`): what `prism mcp` gives an agent
    /// and validates against. By default inferred from its settings' defaults (`settings` of an
    /// empty config: the types of the values); a block with a struct gives `schemars`'s
    /// (`blocks::sdk::schemars`), its doc comments as descriptions.
    fn schema(&self) -> Vec<(&'static str, Value)> {
        let inferred = |v: &Value| {
            let mut s = infer_schema(v);
            s["x-prism-inferred"] = Value::Bool(true);
            s
        };
        self.settings(&toml::Table::new()).into_iter().map(|(t, v)| (t, inferred(&v))).collect()
    }

    /// The repositories it reads, chosen in its table: `One`, its `repo` key (`[shipping] repo =
    /// "app"`); `Many`, its `repos` list. Either way the primary repository when it names none;
    /// `BlockEnv` gives them. `None` (default): it reads none of its own, `BlockEnv` gives the
    /// primary repository.
    fn repos(&self) -> RepoUse {
        RepoUse::None
    }

    /// Its sources read apart (`BlockCtx::part`): in the demo, they answer with it.
    fn parts(&self) -> &'static [&'static str] {
        &[]
    }

    /// Reads its settings in the merged config (packs, then `config.toml`) and, when on, starts
    /// its collector with `ctx`. Called at launch, outside the demo, and again on each change of
    /// the config when it `reloads`.
    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()>;

    /// Its collector may be stopped and started again when the config changes: `start` then
    /// starts it anew, what it spawns going through `prism_base::rt::spawn`, and none of its
    /// statics keeps the first start's settings. Otherwise it runs on from launch, and a change of
    /// its settings waits for a restart of Prism.
    fn reloads(&self) -> bool {
        false
    }

    /// A call from the window (`block_call`): `command` and its arguments, its answer as JSON.
    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>>;

    /// Its state in the demo profile, as of `now`; `None`: off there.
    fn demo(&self, _now: DateTime<Utc>) -> Option<Value> {
        None
    }

    /// The stages it declares, when `[clusters] stages` names it (`stages`).
    fn stages(&self) -> Option<&'static dyn crate::stages::Provider> {
        None
    }

    /// The images it names for Vulnerabilities, when `[vulnerabilities] images` names it.
    fn images(&self) -> Option<&'static dyn crate::images::ImageProvider> {
        None
    }

    /// What it adds to the Changes timeline (`[changes]`), from its state as it published it
    /// (`Snapshot.blocks`): its events, each with an id that stays the same from one read to the
    /// next, and the date its source gives (`seen` when there is none). Read when `[changes]
    /// sources` names `packs` or its id.
    fn changes(&self, _state: &Value, _now: DateTime<Utc>) -> Vec<crate::changes::ChangeEvent> {
        Vec::new()
    }

    /// A call in the demo profile, which reads nothing: its answer from sample data, or `None` to
    /// refuse it (read-only).
    fn demo_call(&self, _command: &str, _args: &Value) -> Option<Result<Value>> {
        None
    }

    /// The class of `command` called from a phone (`crate::remote`), its arguments included (an
    /// action may be one of several); `Never` unless the block says otherwise, so a block that
    /// declares nothing is never reached from a phone.
    fn remote(&self, _command: &str, _args: &Value) -> crate::remote::Class {
        crate::remote::Class::Never
    }
}

/// The blocks compiled into this Prism (Prism's own, then a team's), given once at launch
/// (`register`) before anything reads them: none until then.
static ALL: std::sync::OnceLock<Vec<&'static dyn Block>> = std::sync::OnceLock::new();

pub fn register(blocks: Vec<&'static dyn Block>) {
    let _ = ALL.set(blocks);
}

pub fn all() -> &'static [&'static dyn Block] {
    ALL.get().map_or(&[], Vec::as_slice)
}

pub fn find(id: &str) -> Option<&'static dyn Block> {
    all().iter().copied().find(|b| b.id() == id)
}

/// The repositories a block reads (`Block::repos`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum RepoUse {
    None,
    One,
    Many,
}

/// A repository a block reads: its `[[repos]]` name, project and local clone.
#[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
#[derive(Debug, Clone, PartialEq)]
pub struct BlockRepo {
    pub name: String,
    pub project: Option<String>,
    pub clone: Option<std::path::PathBuf>,
}

/// What the core resolved for a block as it starts: the forge, and the repositories it reads.
#[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
#[derive(Debug, Clone)]
pub struct BlockEnv {
    /// `[forge] provider`.
    pub forge: crate::forge::Provider,
    /// The forge is read (`[forge] enabled`).
    pub code: bool,
    /// The repositories it reads (`Block::repos`): those its table names, otherwise the primary one.
    pub repos: Vec<BlockRepo>,
    /// The first of them: its project (`group/project`), and its local clone.
    pub project: Option<String>,
    pub clone: Option<std::path::PathBuf>,
    /// `[gitlab]`, the first one's project filled in.
    pub gitlab: crate::config::GitlabConfig,
}

impl BlockEnv {
    /// The environment of a block reading `repos` (the first one's project and clone at hand).
    pub fn of(cfg: &crate::config::Config, repos: Vec<crate::config::RepoConfig>) -> Self {
        let first = repos.first();
        let project = first.and_then(|r| r.project.clone());
        Self {
            forge: cfg.forge.provider,
            code: cfg.forge.enabled,
            project: project.clone(),
            clone: first.and_then(|r| r.path.clone()),
            gitlab: crate::config::GitlabConfig { project: project.unwrap_or_default(), ..cfg.gitlab.clone() },
            repos: repos.into_iter().map(|r| BlockRepo { name: r.name, project: r.project, clone: r.path }).collect(),
        }
    }
}

/// A block that reads repositories, for Settings and the window: where it names them, and those
/// it reads.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RepoBlock {
    /// Its id: a block's (`shipping`), or `ci_health`, `renovate`, `vulnerabilities`.
    pub id: String,
    /// One repository, or several.
    pub uses: RepoUse,
    /// Its key in the config: `["shipping", "repo"]`, `["forge", "ci_health_repo"]`.
    pub key: Vec<String>,
    /// The repositories it reads (the primary one when its key names none), in order.
    pub repos: Vec<String>,
}

/// Every block that reads repositories: the core's (CI health, Renovate, Vulnerabilities), then
/// those of `all` that declare it (`Block::repos`).
pub fn repo_blocks(cfg: &crate::config::Config) -> Vec<RepoBlock> {
    let names = |list: Vec<crate::config::RepoConfig>| list.into_iter().map(|r| r.name).collect::<Vec<_>>();
    let key = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    let core = [
        RepoBlock { id: "ci_health".into(), uses: RepoUse::One, key: key("forge", "ci_health_repo"), repos: names(cfg.ci_health_repo().into_iter().collect()) },
        RepoBlock { id: "renovate".into(), uses: RepoUse::One, key: key("renovate", "repo"), repos: names(cfg.renovate_repo().into_iter().collect()) },
        RepoBlock { id: "vulnerabilities".into(), uses: RepoUse::Many, key: key("vulnerabilities", "repos"), repos: names(cfg.vulnerability_repos()) },
    ];
    let declared = all().iter().filter(|b| b.repos() != RepoUse::None).map(|b| RepoBlock {
        id: b.id().into(),
        uses: b.repos(),
        key: key(b.id(), if b.repos() == RepoUse::Many { "repos" } else { "repo" }),
        repos: names(cfg.block_repos(b.id()).0),
    });
    core.into_iter().chain(declared).collect()
}

/// The environment `block` starts with: the repositories it reads, or the primary one.
pub fn env_of(cfg: &crate::config::Config, block: &dyn Block) -> BlockEnv {
    let repos = match block.repos() {
        RepoUse::None => cfg.primary().into_iter().collect(),
        RepoUse::One => cfg.block_repos(block.id()).0.into_iter().take(1).collect(),
        RepoUse::Many => cfg.block_repos(block.id()).0,
    };
    BlockEnv::of(cfg, repos)
}

/// The repositories the blocks name that no `[[repos]]` entry has: `[shipping] repo: no
/// repository is named app`.
pub fn missing_repos(cfg: &crate::config::Config) -> Vec<String> {
    all()
        .iter()
        .filter(|b| b.repos() != RepoUse::None)
        .flat_map(|b| cfg.block_repos(b.id()).1.into_iter().map(move |name| prism_base::tr!("[{block}] repo: no repository is named {name} ([[repos]])", block = b.id(), name = name)))
        .collect()
}

/// What a block reaches of the core: its own place in the snapshot, the collectors' pace, and
/// what the core resolved (`env`).
#[derive(Clone)]
pub struct BlockCtx {
    id: &'static str,
    /// The source its errors and answers go under: its id, or one of its parts (`part`).
    source: &'static str,
    hub: Hub,
    #[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
    env: std::sync::Arc<BlockEnv>,
}

impl BlockCtx {
    pub fn new(id: &'static str, hub: Hub, env: std::sync::Arc<BlockEnv>) -> Self {
        Self { id, source: id, hub, env }
    }

    #[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
    /// The same block, its errors under `source`: one per source a block reads apart (a delivery
    /// block's deployments and rollouts), each answering on its own.
    pub fn part(&self, source: &'static str) -> Self {
        Self { source, ..self.clone() }
    }

    #[allow(dead_code, reason = "for the blocks of another repository (`blocks::sdk`)")]
    pub fn env(&self) -> &BlockEnv {
        &self.env
    }

    /// Its state, sent to the window when it changed.
    pub fn publish(&self, state: &impl Serialize) {
        let value = match serde_json::to_value(state) {
            Ok(v) => v,
            Err(e) => return tracing::error!("{}: state not serializable: {e}", self.id),
        };
        self.hub.update(|s| s.blocks.insert(self.id.to_string(), value.clone()).as_ref() != Some(&value));
    }

    /// Its read's error, or `None` after a successful one; either way, it has answered.
    pub fn set_error(&self, err: Option<String>) {
        self.hub.set_error(self.source, err);
    }

    /// Wait between two reads (slower while the window is hidden: see `Hub::pause`).
    pub async fn pause(&self, every: Duration) {
        self.hub.pause(every).await
    }

    /// What `f` reads of the current snapshot (the other sources' data: MRs, tickets, agenda…).
    pub fn snapshot<T>(&self, f: impl FnOnce(&crate::state::Snapshot) -> T) -> T {
        f(&self.hub.subscribe().borrow())
    }

    /// The active profile's data folder (the app's: notes, layout), for what a block keeps.
    pub fn data_dir(&self) -> std::path::PathBuf {
        crate::profile::data_dir(&dirs::data_dir().unwrap_or_default().join(crate::profile::IDENTIFIER))
    }
}

/// The blocks' settings as they read them from `config`, merged into the core's (`settings::read`).
pub fn settings(into: &mut Value, config: &toml::Table) {
    for b in all() {
        for (table, value) in b.settings(config) {
            match (into.get_mut(table), value) {
                (Some(Value::Object(existing)), Value::Object(keys)) => existing.extend(keys),
                (_, value) => into[table] = value,
            }
        }
    }
}

/// Lets every block read what the core needs of it first (`Block::configure`), and adds its
/// messages' translations (`Block::messages`).
pub fn configure(config: &toml::Table, hub: &Hub) {
    for b in all() {
        // Each language Prism speaks besides English.
        prism_base::i18n::add(prism_base::i18n::Lang::Fr, b.messages(prism_base::i18n::Lang::Fr));
        if let Err(e) = b.configure(config) {
            tracing::error!("{}: {e:#}", b.id());
            hub.set_error(b.id(), Some(format!("[{}]: {e:#}", b.id())));
        }
    }
}

/// Starts the blocks on the merged config; a block whose settings do not read says so in Alerts.
/// `reloading`: only those that `reload`, the others running on from launch, outside the
/// collectors' generation. A block that reloads starts through `within`, in its own generation,
/// which may leave it running as it is (its settings unchanged).
pub fn start(config: &toml::Table, env: &dyn Fn(&dyn Block) -> BlockEnv, hub: &Hub, reloading: bool, within: &dyn Fn(&'static str, &mut dyn FnMut())) {
    for b in all() {
        let env = std::sync::Arc::new(env(*b));
        let ctx = || BlockCtx::new(b.id(), hub.clone(), env.clone());
        let mut started = None;
        match b.reloads() {
            true => within(b.id(), &mut || started = Some(b.start(config, ctx()))),
            false if reloading => continue,
            false => started = Some(prism_base::rt::Generation::outside(|| b.start(config, ctx()))),
        }
        if let Some(Err(e)) = started {
            tracing::error!("{}: {e:#}", b.id());
            hub.set_error(b.id(), Some(format!("[{}]: {e:#}", b.id())));
        }
    }
}

/// The blocks that do not reload whose settings differ from `old` to `new`: they apply once Prism
/// restarts.
pub fn waiting_restart(old: &toml::Table, new: &toml::Table) -> Vec<&'static str> {
    all().iter().filter(|b| !b.reloads() && b.settings(old) != b.settings(new)).map(|b| b.id()).collect()
}

/// The blocks that do not reload, and their sources: their state and errors outlive a reload.
pub fn kept(id: &str) -> bool {
    all().iter().any(|b| !b.reloads() && (b.id() == id || b.parts().contains(&id)))
}

/// The demo profile: the blocks with sample data (written by `demo`) have answered at once.
pub fn demo(hub: &Hub, now: DateTime<Utc>) {
    for b in all().iter().filter(|b| b.demo(now).is_some()) {
        for source in std::iter::once(b.id()).chain(b.parts().iter().copied()) {
            hub.set_error(source, None);
        }
    }
}

/// A block's settings: its table of the merged config, defaults for what it leaves out.
pub fn section<T: serde::de::DeserializeOwned>(config: &toml::Table, id: &str) -> Result<T> {
    let table = config.get(id).cloned().unwrap_or_else(|| toml::Value::Table(Default::default()));
    Ok(table.try_into()?)
}

/// A JSON schema guessed from a value: its type, the keys of an object, the items of a list
/// (from its first), the value as its default.
pub fn infer_schema(value: &Value) -> Value {
    let mut schema = match value {
        Value::Null => serde_json::json!({}),
        Value::Bool(_) => serde_json::json!({ "type": "boolean" }),
        Value::Number(n) if n.is_i64() || n.is_u64() => serde_json::json!({ "type": "integer" }),
        Value::Number(_) => serde_json::json!({ "type": "number" }),
        Value::String(_) => serde_json::json!({ "type": "string" }),
        Value::Array(items) => serde_json::json!({ "type": "array", "items": items.first().map(infer_schema).unwrap_or_else(|| serde_json::json!({})) }),
        Value::Object(keys) => serde_json::json!({ "type": "object", "properties": keys.iter().map(|(k, v)| (k.clone(), infer_schema(v))).collect::<serde_json::Map<_, _>>() }),
    };
    if !value.is_null() && !value.is_object() {
        schema["default"] = value.clone();
    }
    schema
}
