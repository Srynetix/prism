use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// An agent launched by Prism, as remembered between two launches: tmux knows whether it is running,
/// this file knows what it is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    /// Prism's identifier, which names the tmux session and the hook log; also Claude Code's
    /// session id (`--session-id`).
    pub id: String,
    /// The CLI it runs; absent: Claude Code, as before the choice existed.
    #[serde(default)]
    pub provider: super::provider::Provider,
    pub title: String,
    pub cwd: PathBuf,
    /// Initial prompt, if there was one.
    #[serde(default)]
    pub prompt: Option<String>,
    /// Worktree created for the agent, where it resumes if relaunched.
    #[serde(default)]
    pub worktree: Option<PathBuf>,
    /// Its kind (`[[agents.kinds]]`): guessed from its command at launch, or chosen in the Agents
    /// view; it groups them.
    #[serde(default)]
    pub category: Option<String>,
    /// Free tags set in the Agents view (`[agents] tags` only suggests some), several per agent.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Ticket linked by hand in the Agents view, over the one found in its title or branch.
    #[serde(default)]
    pub ticket: Option<String>,
    /// MR linked by hand in the Agents view, over its branch's.
    #[serde(default)]
    pub mr: Option<crate::state::MrLink>,
    /// What it was launched with, as it was then (absent: launched before Prism kept it).
    #[serde(default)]
    pub launch: Option<crate::state::LaunchSpec>,
    /// Declared, not started yet: its `launch` runs when I start it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prepared: bool,
    /// The worktrees made for it in other repositories (`also`), given to its CLI (`--add-dir`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<PathBuf>,
    /// The CLIs it ran before `provider` (Continue with…), oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legs: Vec<crate::state::AgentLeg>,
    /// The session of an earlier stretch its CLI resumed (back to a CLI it ran before): what its
    /// conversation held then is that stretch's, not counted again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumed: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// `agents.json` in the app's data folder.
pub struct Registry {
    path: PathBuf,
    pub records: Vec<Record>,
}

impl Registry {
    /// An unreadable file does not block Prism: the agents start again from an empty list.
    pub fn load(path: PathBuf) -> Self {
        let records = match std::fs::read(&path) {
            Ok(raw) => serde_json::from_slice(&raw).unwrap_or_else(|e| {
                tracing::warn!("agents: {} is not valid JSON: {e}", path.display());
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Self { path, records }
    }

    /// Atomic write: temporary file then rename.
    pub fn save(&self) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&self.records)?)?;
        std::fs::rename(&tmp, &self.path).with_context(|| format!("writing {}", self.path.display()))
    }

    pub fn get(&self, id: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_survives_garbage() {
        let dir = std::env::temp_dir().join(format!("prism-agents-{}", std::process::id()));
        let path = dir.join("agents.json");
        let mut reg = Registry::load(path.clone());
        assert!(reg.records.is_empty());
        reg.records.push(Record { id: "a".into(), provider: super::super::provider::Provider::CommandCode, title: "T".into(), cwd: "/tmp".into(), prompt: None, worktree: None, category: None, tags: vec!["api".into()], ticket: Some("ABC-1".into()), mr: None, launch: None, prepared: false, extra: Vec::new(), legs: Vec::new(), resumed: None, created_at: Utc::now() });
        reg.save().unwrap();
        assert_eq!(Registry::load(path.clone()).records, reg.records);
        std::fs::write(&path, "{").unwrap();
        assert!(Registry::load(path).records.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
