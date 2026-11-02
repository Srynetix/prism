//! The Pet block's settings (`[pet]`): it lives in the window (`web/src/blocks/fun/`), this side only
//! reads its switches and publishes them as its state.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Block, BlockCtx};

pub struct Pet;

pub static BLOCK: Pet = Pet;

impl Block for Pet {
    fn id(&self) -> &'static str {
        "pet"
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        ctx.publish(&settings(config)?);
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        settings(config).ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("pet", v)]).unwrap_or_default()
    }

    fn call(&self, command: String, _args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move { bail!("the Pet block has no command {command}") })
    }

    fn demo(&self, _now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(settings(&toml::Table::new()).ok()?).ok()
    }
}

fn settings(config: &toml::Table) -> Result<PetConfig> {
    super::section(config, "pet")
}

/// `[pet]`: what the Pet does beyond its needs and cares, one switch each.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PetConfig {
    /// Grown up, it takes the form of what you did most over the last two weeks.
    #[serde(default = "yes")]
    pub forms: bool,
    /// Trophies unlocked by milestones, each an object in its room.
    #[serde(default = "yes")]
    pub trophies: bool,
    /// A page of its diary on your week, written on Friday afternoon.
    #[serde(default = "yes")]
    pub diary: bool,
    /// The running agents sit in its room, the one waiting for you waving.
    #[serde(default = "yes")]
    pub friends: bool,
    /// Fetch: it brings back the next thing waiting for you.
    #[serde(default = "yes")]
    pub fetch: bool,
    /// Its window follows the hour and the season, with a few special days.
    #[serde(default = "yes")]
    pub seasons: bool,
    /// A small Pet in the status bar while its block is not on the page shown.
    #[serde(default = "yes")]
    pub status_bar: bool,
    /// A photo at each milestone, in an album.
    #[serde(default = "yes")]
    pub album: bool,
}

fn yes() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_on_by_default() {
        let read = |text: &str| settings(&toml::from_str(text).unwrap()).unwrap();
        assert_eq!(read(""), PetConfig { forms: true, trophies: true, diary: true, friends: true, fetch: true, seasons: true, status_bar: true, album: true });
        assert!(!read("[pet]\ndiary = false\n").diary);
        assert!(!read("[pet]\nstatus_bar = false\n").status_bar);
    }
}
