//! « In passing » (`[agents.aside]`): what an agent flags out of its task while it works — dead
//! code, a stale doc, a missing test — with `suggest_task` (Prism's MCP server) or, in a Claude
//! Desktop session, `spawn_task`. Kept in a list (`asides.json`, `prism_agents::asides`) until it
//! is launched, chained after its agent, made a ticket or dismissed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A task flagged in passing: one per subject, however many agents saw it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Aside {
    /// Its `task_id`, as the agent knows it.
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub id: u64,
    /// Its repository (`[[repos]] name`), from the folder of the agent that saw it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub repo: Option<String>,
    /// Imperative, under 60 characters.
    pub title: String,
    /// What and why, in a sentence or two.
    pub tldr: String,
    /// A prompt that stands on its own: what a new agent is given.
    pub prompt: String,
    /// The paths it is about (relative to the repository), for the dedup and the overlaps.
    #[serde(default)]
    pub files: Vec<String>,
    /// One of the `[[agents.kinds]]`, as the agent guessed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub kind: Option<String>,
    /// It depends on its agent's changes: its worktree starts from that agent's branch.
    #[serde(default)]
    pub needs_current_branch: bool,
    /// The agents (or Desktop sessions) that saw it, the first first.
    pub seen: Vec<AsideSeen>,
    pub state: AsideState,
    /// When it was first seen.
    pub at: DateTime<Utc>,
    /// When its state last changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub changed_at: Option<DateTime<Utc>>,
    /// Launched: the new agent's id and title; in its session (`session`, `done`): the agent doing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent_title: Option<String>,
    /// Chained after its agent (`then`): from its branch (taken at its end) or the base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub from_branch: Option<bool>,
    /// Made a ticket: its key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub ticket: Option<String>,
    /// Dismissed: by whom (`user`, `agent`), and why; done: what the agent said of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub dismissed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub why: Option<String>,
    /// Its files changed since by a commit of another branch than its agents'.
    #[serde(default)]
    pub files_changed: bool,
}

/// An agent (or a Desktop session) that saw it, in its own words.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AsideSeen {
    /// A Prism agent's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub agent: Option<String>,
    /// A Claude Desktop session's id, and its call's (`spawn_task`'s `task_id` once answered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub call: Option<String>,
    /// Its title then (the agent's, the session's).
    pub name: String,
    /// The CLI (`claude`, `codex`, `opencode`, `desktop`).
    pub cli: String,
    /// What it worked on: its branch, and the folder it was in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cwd: Option<String>,
    /// Its model and effort then, given again to a launch from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub effort: Option<String>,
    pub tldr: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum AsideState {
    Open,
    /// Kept for later: open, without a badge.
    Later,
    /// Chained after its agent: launched when it is done.
    Then,
    Launched,
    Ticket,
    Dismissed,
    /// Sent back to the session of the agent that saw it (or noted by the user from its Chat):
    /// done there once its current turn is over.
    Session,
    /// Done in that session, as its agent said (`done_task`) or the user marked it.
    Done,
}

impl AsideState {
    /// Still to deal with: shown on the agent's chip and counted.
    pub fn open(self) -> bool {
        matches!(self, Self::Open | Self::Later)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Still to deal with: open, or set for later; launched, made a ticket, chained or dismissed, no longer.
    #[test]
    fn what_is_still_open() {
        assert!(AsideState::Open.open() && AsideState::Later.open());
        for done in [AsideState::Launched, AsideState::Ticket, AsideState::Dismissed, AsideState::Then, AsideState::Session, AsideState::Done] {
            assert!(!done.open(), "{done:?}");
        }
    }
}
