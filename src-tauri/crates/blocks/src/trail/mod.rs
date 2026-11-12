//! The Day trail block (`[trail]`): what you did, day by day, without writing it down. It keeps a
//! journal of your traces (commits in the local clones, your merge requests opened, merged, approved
//! and commented on the forge, your tickets changing status, your meetings once over) over
//! `keep_days`; the window adds your agents and logged time, and shows a day as a timeline, by
//! ticket (to log time), or as a daily. Nothing leaves the machine but the forge's read of your own
//! events. Its window side: `web/src/blocks/trail/`. My sprint's retro reads it too (`retro`).

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Block, BlockCtx};
use prism_core::remote::Class as RemoteClass;

mod collector;
mod demo;
pub mod retro;

pub struct Trail;

pub static BLOCK: Trail = Trail;

impl Block for Trail {
    fn id(&self) -> &'static str {
        "trail"
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
        settings(config).ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("trail", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                // Reads the sources again now.
                "refresh" => {
                    collector::refresh();
                    Ok(Value::Null)
                }
                other => bail!("the Day trail block has no command {other}"),
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

fn settings(config: &toml::Table) -> Result<TrailConfig> {
    super::section(config, "trail")
}

/// `[trail]`.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct TrailConfig {
    /// On by default: it reads your commits locally and your own events on the forge.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The sources are read again this often.
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
    /// Days of traces kept (the journal, in the profile's data folder).
    #[serde(default = "default_keep")]
    pub keep_days: u32,
    /// A stretch of a working day this long without any trace is flagged (time to log). 0: never.
    #[serde(default = "default_gap")]
    pub gap_minutes: u32,
    /// Your commits, in the local clones of `[[repos]]` (`git log`, your `user.email`).
    #[serde(default = "yes")]
    pub commits: bool,
    /// Your merge requests opened, merged, approved and commented, from the forge's events.
    #[serde(default = "yes")]
    pub forge: bool,
    /// Your tickets changing status, as Prism sees them change.
    #[serde(default = "yes")]
    pub tickets: bool,
    /// Your meetings, once over (declined, canceled and free ones left out).
    #[serde(default = "yes")]
    pub meetings: bool,
}

fn yes() -> bool {
    true
}

fn default_poll() -> u64 {
    300
}

fn default_keep() -> u32 {
    30
}

fn default_gap() -> u32 {
    60
}

/// What the window shows: the journal, newest last.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TrailState {
    pub events: Vec<TrailEvent>,
    /// `[trail] gap_minutes`.
    pub gap_minutes: u32,
    /// The sources read (`[trail] commits`, `forge`, `tickets`, `meetings`).
    pub sources: Vec<String>,
    /// When they were last read.
    pub read_at: Option<DateTime<Utc>>,
}

/// One trace of your work.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(optional_fields))]
pub struct TrailEvent {
    /// Unique: a commit's hash, the forge's event id, a ticket and its new status…
    pub id: String,
    pub at: DateTime<Utc>,
    pub kind: Kind,
    /// A commit's subject, an MR's or a ticket's title, a meeting's name.
    pub title: String,
    /// The ticket it is about (`ABC-1234`), when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    /// The repository (`[[repos]] name`), and the MR's number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mr: Option<u64>,
    /// The MR is someone else's: approving or commenting it is a review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub others: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A ticket's former and new status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// A commit's branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// A meeting's length.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "TrailKind"))]
pub enum Kind {
    Commit,
    MrOpened,
    MrMerged,
    MrApproved,
    MrCommented,
    TicketMoved,
    Meeting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let read = |text: &str| settings(&toml::from_str(text).unwrap()).unwrap();
        let none = read("");
        assert_eq!((none.enabled, none.poll_seconds, none.keep_days, none.gap_minutes), (true, 300, 30, 60));
        assert!(none.commits && none.forge && none.tickets && none.meetings);
        assert!(!read("[trail]\nforge = false\n").forge);
    }
}
