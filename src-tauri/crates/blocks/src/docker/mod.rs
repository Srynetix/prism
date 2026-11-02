//! The Docker block (`[docker]`): the containers, images, volumes and disk of a Docker daemon
//! (Docker Desktop, OrbStack, colima…), read through the `docker` CLI. Its window side:
//! `web/src/blocks/docker/`.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{sdk::RemoteClass, Block, BlockCtx};

mod collector;
mod demo;

pub struct Docker;

pub static BLOCK: Docker = Docker;

impl Block for Docker {
    fn id(&self) -> &'static str {
        "docker"
    }

    fn remote(&self, command: &str, args: &Value) -> RemoteClass {
        match command {
            "logs" => RemoteClass::Read,
            "container" if args.get("action").and_then(Value::as_str) == Some("remove") => RemoteClass::Critical,
            "container" => RemoteClass::Act,
            "remove_image" | "prune" => RemoteClass::Critical,
            _ => RemoteClass::Never,
        }
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg = settings(config)?;
        if cfg.enabled {
            // The block knows it is on before the first read answers.
            ctx.publish(&DockerState { context: cfg.context.trim().to_string(), reclaimable_gb: cfg.reclaimable_gb, ..Default::default() });
            prism_base::rt::spawn(collector::run(cfg, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        settings(config).ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("docker", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
            Ok(match command.as_str() {
                // Starts, stops, restarts or removes a container.
                "container" => {
                    collector::container_action(&arg("id"), &arg("action")).await?;
                    Value::Null
                }
                // The last lines of a container's log.
                "logs" => json!(collector::logs(&arg("id")).await?),
                "remove_image" => {
                    collector::remove_image(&arg("id")).await?;
                    Value::Null
                }
                // Unused data of a kind (`images`, `containers`, `volumes`, `build_cache`): Docker's summary.
                "prune" => json!(collector::prune(&arg("kind")).await?),
                other => bail!("the Docker block has no command {other}"),
            })
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }
}

/// `[docker]`; the threshold formerly `[alerts] docker_reclaimable_gb` still read there when `[docker]` does not say.
fn settings(config: &toml::Table) -> Result<DockerConfig> {
    let mut cfg: DockerConfig = super::section(config, "docker")?;
    let legacy = config.get("alerts").and_then(|a| a.get("docker_reclaimable_gb")).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
    if let Some(gb) = legacy.filter(|_| config.get("docker").and_then(|d| d.get("reclaimable_gb")).is_none()) {
        cfg.reclaimable_gb = gb;
    }
    Ok(cfg)
}

/// `[docker]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DockerConfig {
    /// Off by default: on, the `docker` CLI must reach a daemon (Docker Desktop, OrbStack, colima…).
    #[serde(default)]
    pub enabled: bool,
    /// The `docker context` read. Empty: the current one.
    #[serde(default)]
    pub context: String,
    /// Containers and their usage are re-read this often; the disk, every few minutes.
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    /// Docker's reclaimable disk (unused images, volumes, build cache) above this: flagged in Alerts. 0: never.
    #[serde(default = "default_reclaimable_gb")]
    pub reclaimable_gb: f64,
}

fn default_poll() -> u64 {
    15
}

fn default_reclaimable_gb() -> f64 {
    10.0
}

/// A Docker daemon, as the `docker` CLI reports it. No environment variable, no mount path.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DockerState {
    /// The `docker context` read.
    pub context: String,
    /// The daemon's version; absent while it does not answer.
    pub server: Option<String>,
    pub containers: Vec<DockerContainer>,
    pub images: Vec<DockerImage>,
    pub volumes: Vec<DockerVolume>,
    /// Disk per kind (images, containers, volumes, build cache), from `docker system df`.
    pub disk: Vec<DockerDisk>,
    /// When the disk was last measured (it is slower: every few minutes).
    pub disk_at: Option<DateTime<Utc>>,
    /// Reclaimable disk beyond this many GB is flagged in Alerts (`[docker] reclaimable_gb`); 0: never.
    pub reclaimable_gb: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    /// `running`, `exited`, `restarting`, `paused`, `created`, `dead`.
    pub state: String,
    /// Docker's own words: `Up 3 days (healthy)`, `Exited (1) 2 hours ago`.
    pub status: String,
    /// `healthy`, `unhealthy`, `starting`; absent without a health check.
    #[cfg_attr(feature = "ts", ts(type = "'healthy' | 'unhealthy' | 'starting' | null"))]
    pub health: Option<String>,
    /// The exit code of an exited container.
    pub exit_code: Option<i32>,
    /// Its Compose project, from its labels.
    pub project: Option<String>,
    /// Published ports: `8080→80/tcp`.
    pub ports: Vec<String>,
    pub created_at: Option<DateTime<Utc>>,
    /// Its writable layer, in bytes.
    pub size_bytes: Option<u64>,
    /// From `docker stats`, while it runs.
    pub cpu_percent: Option<f64>,
    pub mem_bytes: Option<u64>,
    pub mem_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DockerImage {
    pub id: String,
    /// `repository:tag`, or `<none>` for a dangling one.
    pub name: String,
    pub size_bytes: u64,
    /// The part no other image shares: what removing it frees.
    pub unique_bytes: Option<u64>,
    /// Containers using it, running or not.
    pub containers: u32,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DockerVolume {
    pub name: String,
    pub size_bytes: Option<u64>,
    /// Containers using it (0: orphan, removed by a volume prune).
    pub links: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DockerDisk {
    /// `images`, `containers`, `volumes`, `build_cache`.
    #[cfg_attr(feature = "ts", ts(type = "'images' | 'containers' | 'volumes' | 'build_cache'"))]
    pub kind: String,
    pub total: u32,
    pub active: u32,
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_threshold_moved_from_alerts() {
        let read = |text: &str| settings(&toml::from_str(text).unwrap()).unwrap();
        let none = read("");
        assert_eq!((none.enabled, none.poll_seconds, none.reclaimable_gb), (false, 15, 10.0));
        assert_eq!(read("[alerts]\ndocker_reclaimable_gb = 25\n").reclaimable_gb, 25.0);
        assert_eq!(read("[alerts]\ndocker_reclaimable_gb = 25\n[docker]\nreclaimable_gb = 0.0\n").reclaimable_gb, 0.0);
    }

    #[test]
    fn removing_from_a_phone_is_critical() {
        let class = |command: &str, action: &str| Docker.remote(command, &json!({ "action": action }));
        assert_eq!(class("container", "restart"), RemoteClass::Act);
        assert_eq!(class("container", "remove"), RemoteClass::Critical);
        assert_eq!(class("prune", ""), RemoteClass::Critical);
        assert_eq!(class("logs", ""), RemoteClass::Read);
        assert_eq!(class("unknown", ""), RemoteClass::Never);
    }
}
