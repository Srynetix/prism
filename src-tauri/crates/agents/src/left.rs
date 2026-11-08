//! The last agent closed in each repository (`agents-left.json`): its record, its CLI's session
//! and hook log kept aside (`agents/left/<id>.jsonl`), what it said last. The Agents view by project
//! shows where a project was left; Reopen puts the agent back in the list and resumes it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use prism_base::{i18n::tr, tr};
use serde::{Deserialize, Serialize};

use super::{conversation, registry::Record, state, Agents};

/// What is kept of a closed agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct Left {
    pub repo: String,
    pub record: Record,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub said: Option<String>,
    pub closed_at: DateTime<Utc>,
}

/// One per repository, most recent first.
pub(crate) fn load(path: &Path) -> Vec<Left> {
    std::fs::read(path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default()
}

fn save(path: &Path, list: &[Left]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// The end of the last reply of a conversation: its last paragraph or so, at most `MAX` characters.
pub(crate) fn last_words(c: &conversation::Conversation) -> Option<String> {
    const MAX: usize = 400;
    let text = c.turns.iter().rev().flat_map(|t| t.entries.iter().rev()).find_map(|e| match e {
        conversation::Entry::Text { text } if !text.trim().is_empty() => Some(text.trim()),
        _ => None,
    })?;
    if text.chars().count() <= MAX {
        return Some(text.to_string());
    }
    let tail: String = text.chars().rev().take(MAX).collect::<Vec<_>>().into_iter().rev().collect();
    // From a line's start, when there is one in it: not a word cut in half.
    let tail = tail.split_once('\n').map(|(_, rest)| rest).filter(|r| r.len() > MAX / 3).unwrap_or(&tail);
    Some(format!("…{}", tail.trim_start()))
}

/// The snapshot's view of them.
fn shown(list: &[Left]) -> Vec<state::LeftSession> {
    list.iter()
        .map(|l| {
            let dir = l.record.worktree.as_deref().filter(|w| w.is_dir()).unwrap_or(&l.record.cwd);
            state::LeftSession {
                repo: l.repo.clone(),
                id: l.record.id.clone(),
                provider: l.record.provider,
                title: l.record.title.clone(),
                branch: l.branch.clone(),
                worktree: l.record.worktree.as_deref().filter(|w| w.is_dir()).map(|w| w.display().to_string()),
                cwd: l.record.cwd.display().to_string(),
                closed_at: l.closed_at,
                said: l.said.clone(),
                resumable: dir.is_dir(),
            }
        })
        .collect()
}

impl Agents {
    pub(crate) fn left_path(&self) -> PathBuf {
        self.inner.work.with_file_name("agents-left.json")
    }

    fn kept_log(&self, id: &str) -> PathBuf {
        self.inner.work.join("left").join(format!("{id}.jsonl"))
    }

    /// The closed agents into the snapshot: at start, and on each change.
    pub(crate) fn publish_left(&self) {
        let list = shown(&load(&self.left_path()));
        self.inner.hub.update(|s| {
            let changed = s.left != list;
            s.left = list;
            changed
        });
    }

    /// What to keep of an agent being closed, while it is still known: its repository (none:
    /// nothing kept, nor for one never started or drafting a quick action), its branch, its last words.
    pub(crate) async fn left_of(&self, record: &Record) -> Option<Left> {
        // Nor an agent drafting a quick action's prompt: not where its repository was left.
        if record.prepared || record.launch.as_ref().and_then(|l| l.from.as_deref()) == Some(super::session_source::DRAFTER) {
            return None;
        }
        let (repo, branch) = {
            let snap = self.inner.hub.subscribe();
            let snap = snap.borrow();
            let agent = snap.agents.iter().find(|a| a.id == record.id);
            let dir = agent.and_then(|a| a.worktree.clone()).or_else(|| record.worktree.as_ref().map(|w| w.display().to_string())).unwrap_or_else(|| record.cwd.display().to_string());
            let inside = |root: &str| dir == root || dir.starts_with(&format!("{root}/")) || dir.starts_with(&format!("{root}.worktrees/"));
            let repo = snap.worktrees.iter().find(|w| w.path == dir).map(|w| w.repo.clone()).or_else(|| {
                let mut repos: Vec<_> = snap.repos.iter().collect();
                repos.sort_by_key(|r| std::cmp::Reverse(r.path.len()));
                repos.into_iter().find(|r| inside(&r.path)).map(|r| r.name.clone())
            });
            (repo?, agent.and_then(|a| a.branch.clone()))
        };
        let said = match self.transcript(&record.id) {
            Ok(Some(transcript)) => {
                let cwd = record.worktree.clone().unwrap_or_else(|| record.cwd.clone());
                tokio::task::spawn_blocking(move || last_words(&conversation::read(&transcript, Some(&cwd), 1))).await.ok().flatten()
            }
            _ => None,
        };
        Some(Left { repo, record: record.clone(), branch, said, closed_at: Utc::now() })
    }

    /// The agent being closed, remembered as its repository's last (`repo`), its hook log kept
    /// aside: the one it replaces goes, with its log.
    pub(crate) fn remember_left(&self, left: Left) {
        let path = self.left_path();
        let mut list = load(&path);
        let log = self.inner.events.join(format!("{}.jsonl", left.record.id));
        if log.is_file() {
            let kept = self.kept_log(&left.record.id);
            let _ = std::fs::create_dir_all(kept.parent().unwrap_or(Path::new(".")));
            if let Err(e) = std::fs::rename(&log, &kept) {
                tracing::warn!("agents: keeping the log of {}: {e}", left.record.id);
            }
        }
        for old in list.iter().filter(|l| l.repo == left.repo && l.record.id != left.record.id) {
            let _ = std::fs::remove_file(self.kept_log(&old.record.id));
        }
        list.retain(|l| l.repo != left.repo && l.record.id != left.record.id);
        list.insert(0, left);
        if let Err(e) = save(&path, &list) {
            tracing::warn!("agents: {e:#}");
        }
        self.publish_left();
    }

    /// The last agent closed in `repo` back in the list, resumed on its conversation: in its
    /// worktree if it still exists, otherwise in the folder it was launched from. Returns its id.
    pub async fn reopen(&self, repo: &str) -> Result<String> {
        let path = self.left_path();
        let mut list = load(&path);
        let i = list.iter().position(|l| l.repo == repo).context(tr("nothing was left in this repository"))?;
        let mut record = list[i].record.clone();
        if record.worktree.as_deref().is_some_and(|w| !w.is_dir()) {
            record.worktree = None;
        }
        record.extra.retain(|p| p.is_dir());
        anyhow::ensure!(record.cwd.is_dir() || record.worktree.is_some(), tr!("its folder is gone: {dir}", dir = record.cwd.display()));
        let id = record.id.clone();
        let kept = self.kept_log(&id);
        if kept.is_file() {
            std::fs::create_dir_all(&self.inner.events)?;
            std::fs::rename(&kept, self.inner.events.join(format!("{id}.jsonl")))?;
        }
        self.update_registry(|reg| {
            reg.records.retain(|r| r.id != id);
            reg.records.push(record);
        })?;
        list.remove(i);
        save(&path, &list)?;
        self.publish_left();
        self.refresh().await;
        self.resume(&id).await?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(texts: &[&str]) -> conversation::Conversation {
        let entries = texts.iter().map(|t| conversation::Entry::Text { text: t.to_string() }).collect();
        conversation::Conversation { turns: vec![conversation::Turn { entries, ..Default::default() }], ..Default::default() }
    }

    #[test]
    fn last_words_are_the_last_reply_cut_at_a_line() {
        assert_eq!(last_words(&conv(&["first", "Done: the drift works."])).as_deref(), Some("Done: the drift works."));
        assert_eq!(last_words(&conv(&[])), None);
        let long = format!("{}\nThe end, kept whole.", "x".repeat(600));
        let said = last_words(&conv(&[&long])).unwrap();
        assert!(said.starts_with('…') && said.chars().count() <= 401, "{said}");
    }
}
