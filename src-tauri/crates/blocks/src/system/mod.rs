//! The System block (`[system]`): what the computer has under its feet while the agents, the dev
//! servers and Docker run. CPU and memory (macOS's memory pressure), who consumes them, grouped
//! by what Prism knows; the disks' free space; the network's latency and loss towards the services Prism calls, and the
//! Wi-Fi link. Read locally, nothing leaves the machine but a few TCP handshakes. Its window side:
//! `web/src/blocks/system/`.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{sdk::RemoteClass, Block, BlockCtx};

mod collector;
mod demo;
mod probe;

pub struct System;

pub static BLOCK: System = System;

impl Block for System {
    fn id(&self) -> &'static str {
        "system"
    }

    fn remote(&self, _command: &str, _args: &Value) -> RemoteClass {
        RemoteClass::Never
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg: SystemConfig = super::section(config, "system")?;
        if cfg.enabled {
            // The block knows it is on before the first read answers.
            ctx.publish(&SystemState { alerts: cfg.alerts, disk_limits: cfg.disk.clone(), ..Default::default() });
            let agent = config.get("agents").and_then(|a| a.get("provider")).and_then(|p| p.as_str()).unwrap_or("claude").to_string();
            prism_base::rt::spawn(collector::run(cfg, agent, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        super::section::<SystemConfig>(config, "system").ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("system", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move { bail!("the System block has no command {command}") })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }
}

/// `[system]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    /// On by default: local counters, and a TCP handshake to a few hosts every 15 s.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// More hosts to measure the latency to (`git.example.com`, `example.com:8443`), beyond those
    /// Prism calls: its forge, its tracker, its agents' API.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// In Alerts: the memory pressure red or the disk almost full (with a notification), the disk
    /// getting full, the CPU at 90 % or more for 5 minutes, more than 5 % of the handshakes lost.
    #[serde(default = "default_true")]
    pub alerts: bool,
    /// When the home disk is getting full, then almost full.
    #[serde(flatten)]
    pub disk: DiskLimits,
    /// CPU, memory and processes read again this often; 2 at least.
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
}

/// The home disk's free space under either limit: getting full (orange), then almost full (red,
/// notified). 0: that limit is not checked.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DiskLimits {
    pub disk_warn_gb: u64,
    pub disk_warn_percent: u8,
    pub disk_alert_gb: u64,
    pub disk_alert_percent: u8,
}

impl Default for DiskLimits {
    fn default() -> Self {
        Self { disk_warn_gb: 25, disk_warn_percent: 10, disk_alert_gb: 10, disk_alert_percent: 5 }
    }
}

fn default_true() -> bool {
    true
}

fn default_poll() -> u64 {
    5
}

/// What the block shows, every `poll_seconds`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SystemState {
    pub machine: Machine,
    pub cpu: Cpu,
    pub memory: Memory,
    pub network: Network,
    /// The disk the home folder is on first, then the external volumes.
    pub disks: Vec<Disk>,
    /// Who consumes: Prism's agents, Docker, the shells and dev servers, Prism, then the biggest
    /// other applications.
    pub groups: Vec<Group>,
    /// The last 10 minutes, a point every 10 s or so.
    pub history: History,
    /// `[system] alerts`.
    pub alerts: bool,
    /// `[system] disk_warn_gb` and the like.
    pub disk_limits: DiskLimits,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemMachine"))]
pub struct Machine {
    /// The processor's name (`Apple M3 Pro`).
    pub chip: String,
    pub cores: usize,
    /// Bytes.
    pub memory: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemCpu"))]
pub struct Cpu {
    /// Of the whole machine, 0 to 100.
    pub usage: f32,
    /// Over 1, 5 and 15 minutes.
    #[cfg_attr(feature = "ts", ts(type = "[number, number, number]"))]
    pub load: [f64; 3],
    /// How long it has stayed at 90 % or more, in seconds (0: below).
    pub hot_seconds: u64,
}

/// macOS's own reading of the memory: normal, warn, critical.
#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Pressure {
    #[default]
    Unknown,
    Normal,
    Warn,
    Critical,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemMemory"))]
pub struct Memory {
    /// Bytes.
    pub total: u64,
    pub used: u64,
    /// macOS: what applications hold, what the system keeps wired, what is compressed.
    pub app: Option<u64>,
    pub wired: Option<u64>,
    pub compressed: Option<u64>,
    pub swap: u64,
    pub pressure: Pressure,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemNetwork"))]
pub struct Network {
    /// The median of the last handshakes to the hosts, in ms; absent until measured.
    pub latency_ms: Option<f32>,
    /// Handshakes lost over the last 5 minutes, in %.
    pub loss: f32,
    /// How much the latency varies from one handshake to the next, in ms.
    pub jitter_ms: Option<f32>,
    /// Bytes a second, every interface but loopback.
    pub rx: u64,
    pub tx: u64,
    /// The default gateway's answer, in ms.
    pub gateway_ms: Option<f32>,
    /// macOS: the Wi-Fi link, when it is the way out (never the network's name).
    pub wifi: Option<Wifi>,
    pub hosts: Vec<Host>,
}

/// A disk: its volume's name, where it is mounted, and its space.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemDisk"))]
pub struct Disk {
    pub name: String,
    pub mount: String,
    /// Bytes.
    pub total: u64,
    /// What a file can still take (macOS: without the space the system could purge).
    pub available: u64,
    /// An external, removable volume.
    pub removable: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemWifi"))]
pub struct Wifi {
    /// dBm.
    pub rssi: i64,
    /// Mb/s.
    pub rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemHost"))]
pub struct Host {
    pub name: String,
    /// Its last handshake, in ms; absent: lost.
    pub ms: Option<f32>,
    /// Its last handshakes, oldest first (absent: lost).
    pub recent: Vec<Option<f32>>,
    /// Lost over the last 5 minutes, in %.
    pub loss: f32,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemGroup"))]
pub struct Group {
    /// `agents`, `docker`, `shells`, `prism`, or `app` for another application.
    #[cfg_attr(feature = "ts", ts(type = "'agents' | 'docker' | 'shells' | 'prism' | 'app'"))]
    pub kind: String,
    pub name: String,
    /// What it is made of: `3 (claude ×2, codex)`, `41 processes`.
    pub detail: String,
    /// % of one core summed: may pass 100 on several cores.
    pub cpu: f32,
    /// Bytes.
    pub memory: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SystemHistory"))]
pub struct History {
    /// CPU %, memory used % of total, latency in ms (absent: no answer).
    pub cpu: Vec<f32>,
    pub memory: Vec<f32>,
    pub latency: Vec<Option<f32>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disk_limits_default_and_read_flat_in_system() {
        let cfg: SystemConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.disk, DiskLimits::default());
        let cfg: SystemConfig = toml::from_str("disk_warn_gb = 60\ndisk_alert_percent = 0").unwrap();
        assert_eq!(cfg.disk, DiskLimits { disk_warn_gb: 60, disk_alert_percent: 0, ..Default::default() });
        let v = serde_json::to_value(&cfg).unwrap();
        assert_eq!((v["disk_warn_gb"].as_u64(), v["disk_alert_gb"].as_u64()), (Some(60), Some(10)));
    }
}
