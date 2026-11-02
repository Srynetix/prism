//! What each agent cost, kept past its end (`agent-costs.json`, beside `agents.json`): its ticket,
//! branch, kind, CLI and model, its active time (working, not waiting for you), its tokens and cost,
//! as last seen. An agent closed leaves the list but not this book: a ticket's actual time and cost
//! count it. Written as the figures change, at most every `SAVE_EVERY`, and when an agent closes.

use std::{
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use chrono::Utc;

use super::{state, Agents};
use state::{Agent, AgentCost, AgentState};

/// An agent's line is kept this long after its last activity: the sprints compared go back a quarter or so.
const KEEP_DAYS: i64 = 400;
/// The figures of a working agent change on every tick: written at most this often.
const SAVE_EVERY: Duration = Duration::from_secs(20);

/// The book, and whether it holds figures not written yet.
#[derive(Debug, Default)]
pub(crate) struct Book {
    pub list: Vec<AgentCost>,
    dirty: bool,
    saved: Option<Instant>,
}

/// Most recent first; an unreadable file is an empty book (the next write replaces it).
pub(crate) fn load(path: &Path) -> Vec<AgentCost> {
    let mut list: Vec<AgentCost> = std::fs::read(path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
    let oldest = Utc::now() - chrono::Duration::days(KEEP_DAYS);
    list.retain(|c| c.last >= oldest);
    list
}

fn save(path: &Path, list: &[AgentCost]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(list)?)?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

/// An agent's line as the list tells it now, `active_ms` from its hook log; none for one not
/// started, or that has done nothing yet.
pub(crate) fn of(a: &Agent, active_ms: u64) -> Option<AgentCost> {
    // Every CLI it ran: its earlier stretches, then the current one.
    let mut total: Option<crate::usage::AgentUsage> = None;
    for u in a.legs.iter().filter_map(|l| l.usage.as_ref()).chain(a.usage.as_ref()) {
        total.get_or_insert_with(Default::default).add(u);
    }
    let usage = total.as_ref();
    let tokens = usage.map_or(0, |u| u.tokens());
    if a.state == AgentState::Prepared || (active_ms == 0 && tokens == 0) {
        return None;
    }
    Some(AgentCost {
        id: a.id.clone(),
        ticket: a.ticket.clone(),
        branch: a.branch.clone(),
        title: a.title.clone(),
        kind: a.category.clone(),
        provider: a.provider,
        model: a.model.clone().or_else(|| a.usage.as_ref().and_then(|u| u.model.clone())),
        started: a.created_at,
        last: a.last_activity.unwrap_or(a.created_at).max(a.created_at),
        active_ms,
        tokens,
        cost: usage.and_then(|u| u.cost),
    })
}

impl Book {
    pub(crate) fn new(list: Vec<AgentCost>) -> Self {
        Self { list, dirty: false, saved: None }
    }

    /// An agent's figures merged into its line: never lower than kept (a log or a transcript read
    /// again from its start counts again), its ticket and branch kept once known. Returns whether
    /// the line changed.
    pub(crate) fn note(&mut self, mut c: AgentCost) -> bool {
        match self.list.iter_mut().find(|k| k.id == c.id) {
            Some(k) => {
                c.active_ms = c.active_ms.max(k.active_ms);
                c.tokens = c.tokens.max(k.tokens);
                c.cost = match (c.cost, k.cost) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                c.ticket = c.ticket.or_else(|| k.ticket.clone());
                c.branch = c.branch.or_else(|| k.branch.clone());
                c.model = c.model.or_else(|| k.model.clone());
                c.last = c.last.max(k.last);
                let changed = *k != c;
                *k = c;
                self.dirty |= changed;
                changed
            }
            None => {
                self.list.insert(0, c);
                self.dirty = true;
                true
            }
        }
    }

    /// Written if it holds new figures, and the last write is far enough (`now`: whatever the last).
    fn flush(&mut self, path: &Path, now: bool) {
        if !self.dirty || (!now && self.saved.is_some_and(|t| t.elapsed() < SAVE_EVERY)) {
            return;
        }
        self.list.sort_by_key(|c| std::cmp::Reverse(c.started));
        match save(path, &self.list) {
            Ok(()) => {
                self.dirty = false;
                self.saved = Some(Instant::now());
            }
            Err(e) => tracing::warn!("agents: {e:#}"),
        }
    }
}

impl Agents {
    pub(crate) fn costs_path(&self) -> std::path::PathBuf {
        self.inner.work.with_file_name("agent-costs.json")
    }

    /// The figures of the agents listed, each with its active time, into the book and the snapshot.
    pub(crate) fn note_costs(&self, list: &[Agent], active: &std::collections::HashMap<String, u64>) {
        let path = self.costs_path();
        let changed = {
            let mut book = self.inner.costs.lock().unwrap();
            let mut changed = false;
            for c in list.iter().filter_map(|a| of(a, active.get(&a.id).copied().unwrap_or(0))) {
                changed |= book.note(c);
            }
            book.flush(&path, false);
            changed
        };
        if changed {
            self.publish_costs();
        }
    }

    /// The book written now, whatever the last write: an agent closes.
    pub(crate) fn flush_costs(&self) {
        let path = self.costs_path();
        self.inner.costs.lock().unwrap().flush(&path, true);
    }

    /// The book, most recent first (`agent_costs`).
    pub fn costs(&self) -> Vec<AgentCost> {
        let mut list = self.inner.costs.lock().unwrap().list.clone();
        list.sort_by_key(|c| std::cmp::Reverse(c.started));
        list
    }

    /// The book changed: those showing it read it again.
    pub(crate) fn publish_costs(&self) {
        self.inner.hub.update(|s| {
            s.agent_costs_rev = s.agent_costs_rev.wrapping_add(1);
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;

    fn line(id: &str, active_ms: u64, tokens: u64, cost: Option<f64>) -> AgentCost {
        let at = chrono::DateTime::from_timestamp(1_600_000_000, 0).unwrap();
        AgentCost { id: id.into(), ticket: None, branch: None, title: "x".into(), kind: None, provider: Provider::Claude, model: None, started: at, last: at, active_ms, tokens, cost }
    }

    /// Figures never go down (a log read again from its start), a ticket once known stays.
    #[test]
    fn a_line_keeps_its_highest_figures() {
        let mut book = Book::default();
        assert!(book.note(AgentCost { ticket: Some("ABC-1".into()), ..line("a", 60_000, 1_000, Some(0.5)) }));
        assert!(!book.note(line("a", 0, 10, None)), "nothing new");
        assert!(book.note(line("a", 90_000, 2_000, Some(0.8))));
        assert!(book.note(line("b", 1, 1, None)));
        let a = book.list.iter().find(|c| c.id == "a").unwrap();
        assert_eq!((a.active_ms, a.tokens, a.cost, a.ticket.as_deref()), (90_000, 2_000, Some(0.8), Some("ABC-1")));
        assert_eq!(book.list.len(), 2);
    }

    #[test]
    fn written_then_read_back_without_the_old_lines() {
        let dir = std::env::temp_dir().join(format!("prism-costs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("agent-costs.json");
        let recent = AgentCost { last: Utc::now(), ..line("new", 1, 1, None) };
        save(&path, &[recent.clone(), line("old", 1, 1, None)]).unwrap();
        assert_eq!(load(&path), vec![recent]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
