//! A module's blocks as the core sees them: ordinary `Block`s whose every hook asks the module's
//! component (`worker`), or reads what its manifest declares.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use prism_core::blocks::{Block, BlockCtx, BlockEnv, RepoUse};
use prism_core::images::{ImageProvider, ImageTarget, LocalRepo};
use prism_core::remote::Class;
use prism_core::stages::{Hints, Provider, Stages};
use serde_json::{json, Value};

use crate::manifest::{BlockSpec, Repos};
use crate::worker::Module;

struct WasmBlock {
    id: &'static str,
    parts: &'static [&'static str],
    spec: BlockSpec,
    module: Arc<Module>,
    stages: Option<&'static Of>,
    images: Option<&'static Of>,
}

/// The stages or the images a block `provides`: its module answers.
struct Of {
    module: Arc<Module>,
}

/// The blocks of `module`, leaked: the core keeps them for the life of Prism.
pub fn of(module: &Arc<Module>) -> Vec<&'static dyn Block> {
    module.warm();
    let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };
    module
        .spec
        .blocks
        .iter()
        .map(|spec| {
            let of = |what: &str| spec.provides.iter().any(|p| p == what).then(|| &*Box::leak(Box::new(Of { module: module.clone() })));
            let parts: Vec<&'static str> = spec.parts.iter().map(|p| leak(p)).collect();
            let block = WasmBlock { id: leak(&spec.id), parts: Box::leak(parts.into_boxed_slice()), spec: spec.clone(), module: module.clone(), stages: of("stages"), images: of("images") };
            &*Box::leak(Box::new(block)) as &'static dyn Block
        })
        .collect()
}

/// What the core resolved for a block, as its component reads it.
pub(crate) fn env_json(block: &str, env: &BlockEnv) -> Value {
    json!({
        "block": block,
        "forge": env.forge,
        "code": env.code,
        "project": env.project,
        "clone": env.clone,
        "repos": env.repos.iter().map(|r| json!({ "name": r.name, "project": r.project, "clone": r.clone })).collect::<Vec<_>>(),
        "gitlab_host": env.gitlab.host,
        "main_branch": prism_core::conventions::get().main_branch,
    })
}

impl Block for WasmBlock {
    fn id(&self) -> &'static str {
        self.id
    }

    fn configure(&self, config: &toml::Table) -> Result<()> {
        self.module.configure(config)
    }

    fn repos(&self) -> RepoUse {
        match self.spec.repos {
            Repos::None => RepoUse::None,
            Repos::One => RepoUse::One,
            Repos::Many => RepoUse::Many,
        }
    }

    fn parts(&self) -> &'static [&'static str] {
        self.parts
    }

    fn start(&self, _config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let env = env_json(self.id, ctx.env());
        self.module.start(self.id, ctx, env)
    }

    fn reloads(&self) -> bool {
        self.module.reloads()
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        let (module, id) = (self.module.clone(), self.id);
        Box::pin(async move { module.call(id, command, args, false).await })
    }

    fn demo(&self, _now: DateTime<Utc>) -> Option<Value> {
        self.module.demo(self.id)
    }

    fn call_in_demo(&self, command: String, args: Value) -> BoxFuture<'static, Option<Result<Value>>> {
        let (module, id) = (self.module.clone(), self.id);
        Box::pin(async move { Some(module.call(id, command, args, true).await) })
    }

    fn stages(&self) -> Option<&'static dyn Provider> {
        self.stages.map(|s| s as &'static dyn Provider)
    }

    fn images(&self) -> Option<&'static dyn ImageProvider> {
        self.images.map(|s| s as &'static dyn ImageProvider)
    }

    fn changes(&self, _state: &Value, _now: DateTime<Utc>) -> Vec<prism_core::changes::ChangeEvent> {
        self.module.shared.lock().unwrap().changes.get(self.id).cloned().unwrap_or_default()
    }

    fn alerts(&self, _state: &Value) -> Vec<prism_core::alerts::Alert> {
        self.module.shared.lock().unwrap().alerts.get(self.id).cloned().unwrap_or_default()
    }

    fn remote(&self, command: &str, _args: &Value) -> Class {
        match self.spec.remote.get(command).map(String::as_str) {
            Some("read") => Class::Read,
            Some("act") => Class::Act,
            Some("critical") => Class::Critical,
            _ => Class::Never,
        }
    }
}

impl Provider for Of {
    fn stages(&self) -> Stages {
        self.module.shared.lock().unwrap().stages.clone().unwrap_or_default()
    }

    fn hints<'a>(&'a self, repo: &'a Path) -> BoxFuture<'a, Result<Hints>> {
        Box::pin(self.module.hints(repo.to_path_buf()))
    }
}

impl ImageProvider for Of {
    fn images<'a>(&'a self, repo: &'a Path, project: &'a str, local: &'a [LocalRepo]) -> BoxFuture<'a, Result<Vec<ImageTarget>>> {
        Box::pin(self.module.images(repo.to_path_buf(), project.to_string(), local.to_vec()))
    }
}
