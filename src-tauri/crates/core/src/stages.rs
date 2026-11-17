//! The stages a release goes through, under the team's names: their role (`staging`, `preprod`,
//! `production`), their name and their environment's name (in clusters, ArgoCD, Helm paths).
//! Environments, ArgoCD and Settings read them. A block may declare them (`[clusters] stages`,
//! `Block::stages`: a team's delivery model). Without one (`none`, the default), the environments
//! of `[[environments]]` stand for them, their role read from their name.

use std::{collections::HashMap, path::Path, sync::OnceLock};

use anyhow::Result;
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::config::EnvironmentConfig;
use prism_base::{i18n::tr, tr};

/// A stage, as the window sees it too (`Snapshot.stages`), and as a module declares it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "EnvironmentStage"))]
pub struct Stage {
    /// `staging`, `preprod` or `production`; empty for an environment no provider names.
    #[cfg_attr(feature = "ts", ts(type = "'' | 'staging' | 'preprod' | 'production'"))]
    pub role: String,
    pub name: String,
    pub env: String,
}

/// The stages, in their order (branch builds first, production last).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stages(pub Vec<Stage>);

impl Stages {
    /// Every stage's environment name, for what reads them in names (workloads, paths).
    pub fn envs(&self) -> Vec<&str> {
        self.0.iter().map(|s| s.env.as_str()).filter(|e| !e.is_empty()).collect()
    }

    /// Production's environment: that of customers' instances in ArgoCD.
    pub fn production(&self) -> &str {
        self.0.iter().find(|s| s.role == "production").map_or("", |s| s.env.as_str())
    }

    /// Without a provider: `[[environments]]`' environments, in their order, production last; the
    /// role of those named `staging`, `preprod`, `prod` or `production`.
    pub fn of_environments(environments: &[EnvironmentConfig]) -> Self {
        let mut seen = Vec::new();
        for e in environments.iter().map(|e| e.env.trim()).filter(|e| !e.is_empty()) {
            if !seen.contains(&e) {
                seen.push(e);
            }
        }
        fn role(e: &str) -> &str {
            match e {
                "prod" | "production" => "production",
                "staging" | "preprod" => e,
                _ => "",
            }
        }
        seen.sort_by_key(|e| role(e) == "production");
        Self(seen.into_iter().map(|e| Stage { role: role(e).into(), name: e.to_string(), env: e.to_string() }).collect())
    }
}

/// What a provider tells of the workloads in the clusters: the applications (to group them, longest
/// first) and, by application and environment, the version expected there.
#[derive(Debug, Clone, Default)]
pub struct Hints {
    pub apps: Vec<String>,
    pub expected: HashMap<(String, String), String>,
}

/// A source of stages, and of what they say of the workloads, from the primary repository.
pub trait Provider: Send + Sync {
    fn stages(&self) -> Stages;
    fn hints<'a>(&'a self, repo: &'a Path) -> BoxFuture<'a, Result<Hints>>;
}

static CURRENT: prism_base::rt::Swap<(Stages, Option<&'static dyn Provider>)> = prism_base::rt::Swap::new();

/// What `[clusters] stages` comes to: the stages, their provider, and what keeps it from giving
/// them (a setup warning; the stages are then `[[environments]]`' names).
pub struct Resolved {
    pub stages: Stages,
    pub provider: Option<&'static dyn Provider>,
    pub warning: Option<String>,
}

/// The stages of the block named by `[clusters] stages` (`find`: the blocks of this Prism), or,
/// with none (empty, `none`) or one that gives none, the environments' names, and why.
pub fn resolve(name: &str, environments: &[EnvironmentConfig], find: impl Fn(&str) -> Option<&'static dyn crate::blocks::Block>) -> Resolved {
    let fallback = |why: Option<String>| Resolved {
        stages: Stages::of_environments(environments),
        provider: None,
        warning: why.map(|why| format!("{why} {}", tr("Meanwhile they are the names of [[environments]]; stages = \"none\" keeps them without this warning."))),
    };
    let name = name.trim();
    if name.is_empty() || name == "none" {
        return fallback(None);
    }
    let Some(block) = find(name) else {
        return fallback(Some(tr!("[clusters] stages = \"{name}\": no block {name} in this Prism. The module giving it is not installed (Settings › Packs › Modules), or was installed after Prism started: restart Prism.", name = name)));
    };
    let Some(provider) = block.stages() else {
        return fallback(Some(tr!("[clusters] stages = \"{name}\": the block {name} gives no stages (its module does not provide them).", name = name)));
    };
    let stages = provider.stages();
    if stages.0.is_empty() {
        // Its hints (applications, versions expected) may come all the same.
        let why = tr!("[clusters] stages = \"{name}\": the block {name} gives no stages yet. Its module waits for your trust (Settings › Packs › Modules), is held, did not start, or declares none (its log says why).", name = name);
        return Resolved { provider: Some(provider), ..fallback(Some(why)) };
    }
    Resolved { stages, provider: Some(provider), warning: None }
}

/// What the stages are resolved from (`[clusters] stages`, `[[environments]]`), and who is told
/// of them: kept from the last `follow`, for `refresh`.
type Tell = std::sync::Arc<dyn Fn(&Resolved) + Send + Sync>;
static FOLLOWED: std::sync::Mutex<Option<(String, Vec<EnvironmentConfig>, Tell)>> = std::sync::Mutex::new(None);

/// The stages of this config, resolved now and again on each `refresh`; `tell` hears of them
/// each time (the snapshot, the warning).
pub fn follow(name: &str, environments: &[EnvironmentConfig], tell: impl Fn(&Resolved) + Send + Sync + 'static) {
    *FOLLOWED.lock().unwrap() = Some((name.to_string(), environments.to_vec(), std::sync::Arc::new(tell)));
    refresh();
}

/// The stages resolved again, a provider's having changed (a module configured: trusted, started
/// again, installed again), with the config last followed; nothing before it.
pub fn refresh() {
    let Some((name, environments, tell)) = FOLLOWED.lock().unwrap().clone() else { return };
    let resolved = resolve(&name, &environments, crate::blocks::find);
    set(resolved.stages.clone(), resolved.provider);
    tell(&resolved);
}

/// Set at launch, and on each change of the config, before the collectors.
pub fn set(stages: Stages, provider: Option<&'static dyn Provider>) {
    CURRENT.set((stages, provider));
}

/// The stages; before `set` (tests), the delivery model's defaults: `staging`, then `production`.
pub fn get() -> &'static Stages {
    static DEFAULT: OnceLock<Stages> = OnceLock::new();
    match CURRENT.get() {
        Some((stages, _)) => stages,
        None => DEFAULT.get_or_init(|| {
            let stage = |role: &str| Stage { role: role.into(), name: role.into(), env: role.into() };
            Stages(vec![stage("staging"), stage("production")])
        }),
    }
}

pub fn current_provider() -> Option<&'static dyn Provider> {
    CURRENT.get().and_then(|(_, p)| *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environments_stand_for_stages() {
        let env = |e: &str| EnvironmentConfig { env: e.into(), context: String::new(), namespaces: Vec::new() };
        let s = Stages::of_environments(&[env("prod"), env("staging"), env("prod"), env("qa"), env(" ")]);
        assert_eq!(s.envs(), ["staging", "qa", "prod"]);
        assert_eq!((s.production(), s.0[0].role.as_str(), s.0[1].role.as_str()), ("prod", "staging", ""));
    }

    /// A block that gives `stages`, or none (`None`: it does not provide them).
    struct Shipping(Option<Vec<Stage>>);
    impl crate::blocks::Block for Shipping {
        fn id(&self) -> &'static str {
            "shipping"
        }
        fn start(&self, _config: &toml::Table, _ctx: crate::blocks::BlockCtx) -> Result<()> {
            Ok(())
        }
        fn call(&self, _command: String, _args: serde_json::Value) -> BoxFuture<'static, Result<serde_json::Value>> {
            Box::pin(async { Ok(serde_json::Value::Null) })
        }
        fn stages(&self) -> Option<&'static dyn Provider> {
            self.0.is_some().then(|| &*Box::leak(Box::new(Given(Stages(self.0.clone().unwrap_or_default())))) as &'static dyn Provider)
        }
    }
    struct Given(Stages);
    impl Provider for Given {
        fn stages(&self) -> Stages {
            self.0.clone()
        }
        fn hints<'a>(&'a self, _repo: &'a Path) -> BoxFuture<'a, Result<Hints>> {
            Box::pin(async { Ok(Hints::default()) })
        }
    }

    #[test]
    fn a_block_gives_the_stages_or_says_why_not() {
        let envs = [EnvironmentConfig { env: "prod".into(), context: String::new(), namespaces: Vec::new() }];
        let found = |block: Option<Vec<Stage>>| move |id: &str| (id == "shipping").then(|| &*Box::leak(Box::new(Shipping(block.clone()))) as &'static dyn crate::blocks::Block);
        let stage = Stage { role: "production".into(), name: "Prod".into(), env: "production".into() };

        let given = resolve("shipping", &envs, found(Some(vec![stage.clone()])));
        assert_eq!((given.stages.0, given.provider.is_some(), given.warning), (vec![stage], true, None));

        for (name, block, says, provider) in [("other", None, "no block other", false), ("shipping", None, "gives no stages (its module", false), ("shipping", Some(vec![]), "gives no stages yet", true)] {
            let r = resolve(name, &envs, found(block.clone()));
            assert_eq!((r.stages.envs(), r.provider.is_some()), (vec!["prod"], provider), "{name} {block:?}");
            let warning = r.warning.unwrap();
            assert!(warning.contains(says) && warning.contains("[[environments]]"), "{warning}");
        }

        for name in ["", "none", " none "] {
            let r = resolve(name, &envs, found(None));
            assert_eq!((r.stages.envs(), r.warning), (vec!["prod"], None));
        }
    }
}
