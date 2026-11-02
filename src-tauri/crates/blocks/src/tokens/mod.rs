//! The Tokens block (`[tokens]`): what the coding tools consumed, day by day, over weeks and months
//! — Claude Code, Codex, Command Code and OpenCode, every session of this computer, Prism's agents
//! or not. Read from their usage figures (never what was said, `sources`), kept as daily totals in
//! the profile's data folder: the tools delete their sessions after a while, the history stays.
//! Its window side: `web/src/blocks/tokens/`.

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use prism_base::tr;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{sdk::RemoteClass, Block, BlockCtx};

mod collector;
mod demo;
mod sources;

pub struct Tokens;

pub static BLOCK: Tokens = Tokens;

impl Block for Tokens {
    fn id(&self) -> &'static str {
        "tokens"
    }

    fn remote(&self, command: &str, _args: &Value) -> RemoteClass {
        if command == "refresh" || command == "days" { RemoteClass::Read } else { RemoteClass::Never }
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg: TokensConfig = super::section(config, "tokens")?;
        if cfg.enabled {
            prism_base::rt::spawn(collector::run(cfg, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        super::section::<TokensConfig>(config, "tokens").ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("tokens", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            match command.as_str() {
                // ↻ in the block: read the transcripts now.
                "refresh" => {
                    collector::refresh_now();
                    Ok(Value::Null)
                }
                // ‹ back past the snapshot's days: the history's days up to `to` (excluded).
                "days" => Ok(serde_json::to_value(collector::days_before(&date_arg(&args, "to")?))?),
                other => bail!("the Tokens block has no command {other}"),
            }
        })
    }

    fn demo_call(&self, command: &str, args: &Value) -> Option<Result<Value>> {
        // Its two months are all in its state: nothing older.
        (command == "days").then(|| date_arg(args, "to").map(|_| Value::Array(Vec::new())))
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }
}

/// `[tokens]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokensConfig {
    /// On by default: the transcripts are local, only their usage figures are read.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// The tools read: `claude`, `codex`, `command-code`, `opencode`. Empty: all of them (those
    /// not installed have nothing to read).
    #[serde(default)]
    pub sources: Vec<String>,
    /// Days of history kept (daily totals, in the profile's data folder).
    #[serde(default = "default_keep_days")]
    pub keep_days: u32,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
}

impl TokensConfig {
    /// The tools to read, as `sources` names them; unknown names are left out.
    pub fn tools(&self) -> Vec<&'static str> {
        let named: Vec<&'static str> = sources::ALL.into_iter().filter(|t| self.sources.iter().any(|s| s.trim().eq_ignore_ascii_case(t))).collect();
        if self.sources.iter().all(|s| s.trim().is_empty()) { sources::ALL.to_vec() } else { named }
    }
}

fn default_true() -> bool {
    true
}

fn default_keep_days() -> u32 {
    400
}

fn default_poll() -> u64 {
    300
}

/// A date argument of a call (`YYYY-MM-DD`).
fn date_arg(args: &Value, key: &str) -> Result<String> {
    let d = args.get(key).and_then(Value::as_str).unwrap_or_default();
    match chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
        Ok(_) => Ok(d.to_string()),
        Err(_) => bail!(tr!("{key}: not a date ({value})", key = key, value = d)),
    }
}

/// What the window shows: the days with a reply, oldest first.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TokensState {
    /// The recent ones (`collector::RECENT_DAYS`): the older, asked when the block goes back to them (`days`).
    pub days: Vec<Day>,
    /// The oldest day of the history, when it is older than `days`: ‹ goes back to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub oldest: Option<String>,
    /// The last read of the transcripts.
    pub read_at: Option<DateTime<Utc>>,
}

/// A day's consumption (local date), every session summed.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "TokenDay"))]
pub struct Day {
    /// `YYYY-MM-DD`, in local time.
    pub date: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub replies: u32,
    /// Estimated at the models' API prices, in dollars, for the priced replies.
    pub cost: f64,
    /// Tokens of replies by a model without a known price: not in `cost`.
    pub unpriced: u64,
    /// Tokens by model, then by project (the session folder's name), then by tool (`sources`
    /// names), largest first.
    pub models: Vec<(String, u64)>,
    pub projects: Vec<(String, u64)>,
    /// Absent from a history written before the other tools were read: Claude Code's.
    #[serde(default)]
    pub tools: Vec<(String, u64)>,
    /// The same day tool by tool (`sources` names), for the block's filter; each without its own
    /// `by_tool`. Absent from a history written before: a day of one tool is all of it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "ts", ts(as = "Option<BTreeMap<String, Day>>", optional))]
    pub by_tool: BTreeMap<String, Day>,
}

impl Day {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}
