//! The block Hello, written as a block of Prism (`impl Block`) against `crate::blocks::sdk` only:
//! the same file compiles inside Prism (`prism_blocks::sdk`, a test of Prism checks it) and in
//! this module (`prism_module::sdk`).
//!
//! It reads its repository's last commit with git, every `[hello] every` seconds, answers `ping`
//! and, on purpose, panics on `panic`; it declares stages and names an image.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::blocks::sdk::{self, conventions, git, section, tr, Block, BlockCtx, Hints, ImageProvider, ImageRef, ImageTag, ImageTarget, Lang, LocalRepo, RemoteClass, RepoUse, Stage, Stages, StagesProvider, Swap};

/// `[hello]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    /// Seconds between two reads.
    pub every: u64,
    /// The image Vulnerabilities scans for it.
    pub image: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true, every: 30, image: "alpine:3.20".into() }
    }
}

/// What the window shows.
#[derive(Debug, Clone, Serialize)]
pub struct HelloState {
    pub reads: u64,
    pub at: i64,
    pub commit: Option<String>,
    pub project: Option<String>,
    pub branch: String,
}

static SETTINGS: Swap<Settings> = Swap::new();
static READS: AtomicU64 = AtomicU64::new(0);

pub struct Hello;
pub static HELLO: Hello = Hello;

static FR: &[(&str, &str)] = &[("no commit in {repo}", "pas de commit dans {repo}")];

async fn run(clone: Option<PathBuf>, project: Option<String>, every: u64, ctx: BlockCtx) {
    let reads = ctx.part("hello_reads");
    loop {
        let n = READS.fetch_add(1, Ordering::Relaxed) + 1;
        // Its repository's last commit: git, in its own clone only.
        let commit = match &clone {
            Some(c) => match git(c, &["log", "-1", "--format=%s"]).await {
                Ok(out) if !out.trim().is_empty() => {
                    reads.set_error(None);
                    Some(out.trim().to_string())
                }
                Ok(_) => {
                    reads.set_error(Some(tr!("no commit in {repo}", repo = c.display())));
                    None
                }
                Err(e) => {
                    reads.set_error(Some(format!("{e:#}")));
                    None
                }
            },
            None => None,
        };
        ctx.publish(&HelloState { reads: n, at: Utc::now().timestamp_millis(), commit, project: project.clone(), branch: conventions::get().main_branch.clone() });
        reads.pause(Duration::from_secs(every.max(5))).await;
    }
}

impl Block for Hello {
    fn id(&self) -> &'static str {
        "hello"
    }

    fn messages(&self, lang: Lang) -> &'static [(&'static str, &'static str)] {
        match lang {
            Lang::Fr => FR,
            _ => &[],
        }
    }

    fn configure(&self, config: &toml::Table) -> Result<()> {
        SETTINGS.set(section(config, "hello")?);
        Ok(())
    }

    fn repos(&self) -> RepoUse {
        RepoUse::One
    }

    fn parts(&self) -> &'static [&'static str] {
        &["hello_reads"]
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let settings: Settings = section(config, "hello")?;
        if !settings.enabled {
            return Ok(());
        }
        let env = ctx.env().clone();
        sdk::spawn(run(env.clone, env.project, settings.every, ctx));
        Ok(())
    }

    fn reloads(&self) -> bool {
        true
    }

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                "ping" => Ok(json!({ "pong": READS.load(Ordering::Relaxed), "demo": false })),
                // On purpose: Prism says so on the block, and starts the component again.
                "panic" => panic!("hello panicked on purpose"),
                other => anyhow::bail!("no command {other}"),
            }
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        Some(json!({ "reads": 42, "at": now.timestamp_millis(), "commit": "Add the checkout page", "project": "example-org/shop", "branch": "main" }))
    }

    fn demo_call(&self, command: &str, _args: &Value) -> Option<Result<Value>> {
        (command == "ping").then(|| Ok(json!({ "pong": 42, "demo": true })))
    }

    fn stages(&self) -> Option<&'static dyn StagesProvider> {
        Some(&HELLO)
    }

    fn images(&self) -> Option<&'static dyn ImageProvider> {
        Some(&HELLO)
    }

    fn remote(&self, command: &str, _args: &Value) -> RemoteClass {
        if command == "ping" { RemoteClass::Read } else { RemoteClass::Never }
    }
}

impl StagesProvider for Hello {
    fn stages(&self) -> Stages {
        let stage = |role: &str, name: &str, env: &str| Stage { role: role.into(), name: name.into(), env: env.into() };
        Stages(vec![stage("staging", "recette", "staging"), stage("production", "production", "prod")])
    }

    fn hints<'a>(&'a self, _repo: &'a Path) -> BoxFuture<'a, Result<Hints>> {
        Box::pin(async { Ok(Hints::default()) })
    }
}

impl ImageProvider for Hello {
    fn images<'a>(&'a self, _repo: &'a Path, _project: &'a str, _local: &'a [LocalRepo]) -> BoxFuture<'a, Result<Vec<ImageTarget>>> {
        Box::pin(async move {
            let image = SETTINGS.get().map_or_else(|| Settings::default().image, |s| s.image.clone());
            let tag = ImageTag { tag: image.clone(), shown: image.clone(), at: vec![ImageRef { image: image.clone(), gcp: false }] };
            Ok(vec![ImageTarget { label: format!("hello · {image}"), tags: vec![tag], ignore: None, error: None, repo: None, base: false }])
        })
    }
}
