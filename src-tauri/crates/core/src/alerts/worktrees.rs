//! Worktrees: whether each one is stale and can be removed without losing anything, or its folder
//! is gone, as the Worktrees view says it (`describeWorktrees`, web/src/rules.ts), and whether it
//! is archived (`worktrees.archived`, or every agent and session working in it archived).

use std::collections::HashMap;

use prism_base::tn;

use super::rules::{iso, waits_for_you, Ctx};
use super::{Alert, Severity};
use crate::state::*;

/// A session still at work, or waiting: its worktree is in use.
const LIVE: [SessionStatus; 5] = [SessionStatus::Running, SessionStatus::Background, SessionStatus::NeedsYou, SessionStatus::WaitingTool, SessionStatus::ReviewReady];

use crate::agents::DRAFTER;

#[derive(Debug, PartialEq)]
pub(super) enum State {
    InUse,
    Stale,
    Idle,
    Gone,
}

/// What the alerts need of a worktree.
pub(super) struct Info {
    pub state: State,
    pub removable: bool,
    pub hidden: bool,
}

/// The worktree containing `dir`: the deepest.
fn owner_of<'a>(dir: &str, worktrees: &'a [Worktree]) -> Option<&'a str> {
    worktrees.iter().filter(|w| dir == w.path || dir.strip_prefix(w.path.as_str()).is_some_and(|rest| rest.starts_with('/'))).max_by_key(|w| w.path.len()).map(|w| w.path.as_str())
}

impl Ctx<'_> {
    /// Archived (`agents.archived`), and not waiting for you since; a routine's agent until opened
    /// or until it waits for a permission or an answer.
    fn agent_archived(&self, id: &str, status: SessionStatus, last_activity: Option<chrono::DateTime<chrono::Utc>>, needs_action: Option<&str>, drafter: bool) -> bool {
        if drafter {
            return true;
        }
        match self.prefs.agents_archived.get(id) {
            Some(at) => !(waits_for_you(status) && iso(last_activity).as_str() > at.as_str()),
            None => {
                if !self.s.routines.runs.iter().any(|r| r.agent.as_deref() == Some(id)) || self.prefs.routines_opened.contains(id) {
                    return false;
                }
                !(status == SessionStatus::WaitingTool || needs_action == Some("Answer its question"))
            }
        }
    }

    /// Each worktree's state, whether it may be removed safely, and whether it is hidden. Each
    /// folder's owner looked up once, not once a worktree (quadratic with many worktrees and agents).
    pub(super) fn describe_worktrees(&self) -> Vec<Info> {
        let s = self.s;
        let all = &s.worktrees;
        fn cached<'w>(owners: &mut HashMap<&'w str, Option<&'w str>>, dir: Option<&'w str>, all: &'w [Worktree]) -> Option<&'w str> {
            let dir = dir.filter(|d| !d.is_empty())?;
            *owners.entry(dir).or_insert_with(|| owner_of(dir, all))
        }
        let mut owners = HashMap::new();
        let mut owner = |dir| cached(&mut owners, dir, all);
        // Desktop sessions, agents (their folder, and those they adopted or work in too), archived sessions, by worktree.
        let mut sessions: HashMap<&str, Vec<&ClaudeSession>> = HashMap::new();
        for c in s.sessions.iter().filter(|c| c.origin == SessionOrigin::Desktop) {
            if let Some(w) = owner(c.worktree.as_deref()) {
                sessions.entry(w).or_default().push(c);
            }
        }
        let mut agents: HashMap<&str, Vec<&Agent>> = HashMap::new();
        for a in &s.agents {
            let own = owner(Some(a.worktree.as_deref().unwrap_or(&a.cwd)));
            let mut at: Vec<&str> = own.into_iter().chain(a.adopted.iter().chain(&a.also).map(String::as_str)).collect();
            at.sort_unstable();
            at.dedup();
            for p in at {
                agents.entry(p).or_default().push(a);
            }
        }
        let mut archived: HashMap<&str, Vec<&ArchivedSession>> = HashMap::new();
        for c in &s.archived_sessions {
            if let Some(w) = owner(Some(&c.worktree)) {
                archived.entry(w).or_default().push(c);
            }
        }
        let mut by_branch: HashMap<(String, &str), Vec<&MergeRequest>> = HashMap::new();
        for m in &s.merge_requests {
            by_branch.entry((self.repo_name(m.repo.as_deref()), m.source_branch.as_str())).or_default().push(m);
        }
        let index = self.ticket_index();
        let in_use_minutes = f64::from(self.cfg.worktree_in_use_hours) * 60.0;
        all.iter()
            .map(|w| {
                let path = w.path.as_str();
                let sessions = sessions.get(path).map_or(&[][..], Vec::as_slice);
                // Agents in the order of the snapshot, as the window lists them.
                let agents = agents.get(path).map_or(&[][..], Vec::as_slice);
                let archived = archived.get(path).map_or(&[][..], Vec::as_slice);
                let by: &[&MergeRequest] = w.branch.as_deref().and_then(|b| by_branch.get(&(w.repo.clone(), b))).map_or(&[], Vec::as_slice);
                let opened = by.iter().find(|m| m.state == MrState::Opened);
                let merged = by.iter().filter(|m| m.state == MrState::Merged).max_by(|a, b| a.merged_at.cmp(&b.merged_at).then(std::cmp::Ordering::Greater));
                let ticket = w.ticket.as_deref().filter(|t| !t.is_empty()).and_then(|t| index.get(&self.ticket_ref(t, Some(&w.repo)))).map(|e| e.t);
                let activity = std::iter::once(w.last_commit).chain(sessions.iter().map(|c| c.last_activity)).chain(agents.iter().map(|a| a.last_activity.or(Some(a.created_at)))).chain(archived.iter().map(|c| c.last_activity)).flatten().max();

                let running = agents.iter().any(|a| a.state == AgentState::Running);
                let busy = running || !sessions.is_empty();
                let live = running || sessions.iter().any(|c| LIVE.contains(&c.status)) || (activity.is_some() && self.minutes_since(activity) < in_use_minutes);

                let mut stale = false;
                if !w.is_main && !w.prunable {
                    let idle = activity.map(|a| (self.minutes_since(Some(a)) / 1440.0).floor());
                    stale = (opened.is_none() && merged.is_some())
                        || w.upstream_gone
                        || ticket.is_some_and(|t| self.is_done(t))
                        || (!archived.is_empty() && sessions.is_empty() && agents.is_empty())
                        || idle.is_some_and(|d| d >= f64::from(s.worktree_stale_days));
                }
                // Commits from before the merge of a squashed MR are shipped, even if absent from the remote.
                let delivered = opened.is_none() && merged.is_some_and(|m| w.last_commit.is_some() && m.merged_at.is_some() && w.last_commit <= m.merged_at);
                let keep = w.changes > 0 || w.untracked.is_some_and(|n| n > 0) || (w.unpushed > 0 && !delivered);

                let state = if w.prunable {
                    State::Gone
                } else if live {
                    State::InUse
                } else if stale {
                    State::Stale
                } else {
                    State::Idle
                };
                // Out of the list: archived itself, or every agent and session working in it archived.
                let archived_itself = self.prefs.worktrees_archived.get(path).is_some_and(|at| *at == format!("{}|{}", w.head.as_deref().unwrap_or_default(), w.changes));
                let workers = agents.len() + sessions.len();
                let all_archived = agents.iter().all(|a| self.agent_archived(&a.id, a.status, a.last_activity, a.needs_action.as_deref(), a.launch.as_ref().and_then(|l| l.from.as_deref()) == Some(DRAFTER)))
                    && sessions.iter().all(|c| self.agent_archived(&c.id, c.status, c.last_activity, c.needs_action.as_deref(), false));
                let hidden = archived_itself || (!w.prunable && workers > 0 && all_archived);
                Info { state, removable: !w.is_main && !w.prunable && !w.locked && !busy && !keep, hidden }
            })
            .collect()
    }

    /// Worktrees stale and safe to remove, and those whose folder is gone. Archived ones are left
    /// alone; a folder gone still says so.
    pub fn worktree_alerts(&self) -> Vec<Alert> {
        let infos: Vec<Info> = self.describe_worktrees().into_iter().filter(|i| i.state == State::Gone || !i.hidden).collect();
        let safe = infos.iter().filter(|i| i.state == State::Stale && i.removable).count();
        let gone = infos.iter().filter(|i| i.state == State::Gone).count();
        let mut out = Vec::new();
        if safe > 0 {
            out.push(Alert::new("Worktrees", Severity::Info, tn!(safe, "{n} stale worktree can be removed safely", "{n} stale worktrees can be removed safely")).notify(false));
        }
        if gone > 0 {
            out.push(Alert::new("Worktrees", Severity::Info, tn!(gone, "{n} worktree whose folder is gone (prune)", "{n} worktrees whose folder is gone (prune)")).notify(false));
        }
        out
    }
}
