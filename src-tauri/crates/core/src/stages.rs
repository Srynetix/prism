//! The stages a release goes through, under the team's names: their role (`staging`, `preprod`,
//! `production`), their name and their environment's name (in clusters, ArgoCD, Helm paths).
//! Environments, ArgoCD and Settings read them. A block may declare them (`[clusters] stages`,
//! `Block::stages`: a team's delivery model). Without one (`none`, the default), the environments
//! of `[[environments]]` stand for them, their role read from their name.

use std::{collections::HashMap, path::Path, sync::OnceLock};

use anyhow::Result;
use futures::future::BoxFuture;
use serde::Serialize;

use crate::config::EnvironmentConfig;

/// A stage, as the window sees it too (`Snapshot.stages`).
#[derive(Debug, Clone, Serialize, PartialEq)]
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

/// The provider named by `[clusters] stages` (empty: none), or an error for an unknown name.
pub fn provider(name: &str) -> Result<Option<&'static dyn Provider>> {
    match name.trim() {
        "" | "none" => Ok(None),
        // A block that declares stages (`Block::stages`).
        other => crate::blocks::find(other).and_then(|b| b.stages()).map(Some).ok_or_else(|| anyhow::anyhow!("[clusters] stages: no block {other} declaring stages in this Prism (none: [[environments]])")),
    }
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
}
