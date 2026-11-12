//! The Dev servers block (`[dev_servers]`): the local servers your agents' folders run, found
//! rather than declared. A server is a program, in an agent's shell (or the agent itself), that
//! listens on a TCP port; a Compose container whose working directory is an agent's folder counts
//! too, when it started after the agent was launched (one already running is not the agent's). What to start comes from the agent's Tasks (`agents::tasks`), on the window side. The
//! ports held outside Prism (a forgotten `node`, a Homebrew Postgres) are listed apart. Its window
//! side: `web/src/blocks/dev_servers/`.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Block, BlockCtx};
use prism_core::remote::Class as RemoteClass;

mod collector;
mod demo;


pub struct DevServers;

pub static BLOCK: DevServers = DevServers;

impl Block for DevServers {
    fn id(&self) -> &'static str {
        "dev_servers"
    }

    fn remote(&self, command: &str, _args: &Value) -> RemoteClass {
        match command {
            "refresh" => RemoteClass::Read,
            "stop" | "dismiss" => RemoteClass::Act,
            "kill" => RemoteClass::Critical,
            _ => RemoteClass::Never,
        }
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg = settings(config)?;
        if cfg.enabled {
            ctx.publish(&DevServersState { scan_outside: cfg.scan_outside, ..Default::default() });
            prism_base::rt::spawn(collector::run(cfg, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        settings(config).ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("dev_servers", v)]).unwrap_or_default()
    }

    /// A server stopped on an error.
    fn alerts(&self, state: &Value) -> Vec<prism_core::alerts::Alert> {
        crate::alerts::dev_servers(state)
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            let arg = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
            match command.as_str() {
                // Ctrl-C in the shell running a server (`key`), or a Compose container stopped.
                "stop" => collector::stop(&arg("key")).await?,
                // A process holding a port outside Prism, by pid (only one the block shows).
                "kill" => collector::kill(args.get("pid").and_then(Value::as_u64).unwrap_or_default() as u32).await?,
                // A stopped server's line forgotten.
                "dismiss" => collector::dismiss(&arg("key")),
                // Reads again now (after a start).
                "refresh" => collector::refresh(),
                other => bail!("the Dev servers block has no command {other}"),
            }
            Ok(Value::Null)
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }

    fn demo_call(&self, command: &str, _args: &Value) -> Option<Result<Value>> {
        (command == "refresh").then_some(Ok(Value::Null))
    }
}

fn settings(config: &toml::Table) -> Result<DevServersConfig> {
    super::section(config, "dev_servers")
}

/// `[dev_servers]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DevServersConfig {
    /// Off by default: on, the processes and their listening ports are read every `poll_seconds`.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    /// Also list the ports held by programs Prism did not start (system and app processes aside).
    #[serde(default = "yes")]
    pub scan_outside: bool,
}

fn default_poll() -> u64 {
    5
}

fn yes() -> bool {
    true
}

/// The servers found, published under `Snapshot.blocks.dev_servers`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DevServersState {
    pub servers: Vec<DevServer>,
    /// Ports held outside Prism (`scan_outside`).
    pub outside: Vec<Listener>,
    pub scan_outside: bool,
    /// The last read; absent before the first.
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DevServer {
    /// What stops it: a shell's terminal key (`shell:<agent>:<n>`), the agent's id (a server it
    /// started itself), or `docker:<container>`.
    pub key: String,
    pub agent: String,
    /// The agent's shell running it; absent: the agent's own terminal, or a container.
    pub shell: Option<u32>,
    /// The line that started it: the task run from Prism, otherwise the process's command line.
    pub command: String,
    /// `command` is the line Prism typed (a task): Run again types it again. Otherwise it is read
    /// from the process (its argv, joined), which a program can rewrite: never typed back.
    pub typed: bool,
    /// The program in the shell's foreground (`node`, `uv`), or the container's image.
    pub program: String,
    pub ports: Vec<u16>,
    /// `running`, `starting` (a server task with no port yet) or `stopped` (it had ports, its
    /// shell is back at its prompt).
    #[cfg_attr(feature = "ts", ts(type = "'running' | 'starting' | 'stopped'"))]
    pub state: String,
    /// A stopped server's last lines: why it stopped.
    pub tail: Option<String>,
    /// The tail reads as an error (a traceback, `EADDRINUSE`, `exited 1`…).
    pub failed: bool,
    pub docker: bool,
    /// When Prism first saw it.
    pub since: Option<DateTime<Utc>>,
}

/// A port held by a program Prism did not start.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "DevListener"))]
pub struct Listener {
    pub port: u16,
    pub pid: u32,
    /// Its program (`node`, `postgres`), or the container's name.
    pub program: String,
    /// Its command line, or the container's image.
    pub command: String,
    pub docker: bool,
}
