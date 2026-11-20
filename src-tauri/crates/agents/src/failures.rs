//! Agents stopped by an API error (status `Failed`, Claude Code's `StopFailure`): resumed by a
//! prompt (`[agents] resume_prompt`) at once, at a time (a usage limit's reset), or on their own
//! for a passing error (`[agents] retry_on_error`: overloaded or server error, 3 times at most,
//! after 1, 5 then 15 minutes). A plan is dropped as soon as the agent is no longer in error.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::state::{Agent, AgentState, SessionStatus};

/// The errors that pass on their own: resumed automatically when asked.
pub const PASSING: [&str; 2] = ["overloaded", "server_error"];
/// The automatic resumes' delays: from the error (or, an error seen late, from when Prism saw it).
pub const DELAYS_MIN: [i64; 3] = [1, 5, 15];
/// A compaction asked before the resume (Compact then resume) is given this long.
const COMPACTING_MIN: i64 = 15;

/// A resume planned for an agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub at: DateTime<Utc>,
    pub auto: bool,
}

/// What lasts across a restart: the plans (Resume at HH:MM), the automatic resumes made in a row,
/// the errors dealt with by hand.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Kept {
    plans: HashMap<String, Plan>,
    attempts: HashMap<String, u32>,
    /// The error (its time) already resumed or cancelled: no automatic resume for it — its
    /// resume sent, the next refresh may come before its hook says it works again.
    handled: HashMap<String, DateTime<Utc>>,
}

/// The resumes of the agents in error (`resumes.json` in the agents' folder).
#[derive(Debug, Default)]
pub struct Book {
    kept: Kept,
    /// When Prism first saw each error (its time, then when): the automatic delays count from
    /// the later of the two, so an old error is not resumed at once, its countdown unseen.
    seen: HashMap<String, (DateTime<Utc>, DateTime<Utc>)>,
    /// Compacting before its resume (Compact then resume): sent once back at its prompt.
    compacting: HashMap<String, DateTime<Utc>>,
    path: Option<PathBuf>,
}

impl Book {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("resumes.json");
        let kept = std::fs::read(&path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
        Self { kept, path: Some(path), ..Default::default() }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let tmp = path.with_extension("json.tmp");
        let done = serde_json::to_vec_pretty(&self.kept).map_err(anyhow::Error::from).and_then(|raw| Ok(std::fs::write(&tmp, raw).and_then(|()| std::fs::rename(&tmp, path))?));
        if let Err(e) = done {
            tracing::warn!("agents: writing {}: {e:#}", path.display());
        }
    }

    /// Resume at `at` (a usage limit's reset).
    pub fn plan(&mut self, id: &str, at: DateTime<Utc>) {
        self.kept.plans.insert(id.to_string(), Plan { at, auto: false });
        self.save();
    }

    /// The error at `error` cancelled: its plan dropped, no automatic resume for it (a new error is
    /// resumed again).
    pub fn cancel(&mut self, id: &str, error: Option<DateTime<Utc>>) {
        self.kept.plans.remove(id);
        if let Some(at) = error {
            self.kept.handled.insert(id.to_string(), at);
        }
        self.save();
    }

    /// The error at `error` resumed by hand: no automatic resume for it, and those of the next
    /// errors start over.
    pub fn resumed(&mut self, id: &str, error: Option<DateTime<Utc>>) {
        self.kept.plans.remove(id);
        self.kept.attempts.remove(id);
        if let Some(at) = error {
            self.kept.handled.insert(id.to_string(), at);
        }
        self.save();
    }

    /// Compacting (Compact then resume): its resume goes once it is back at its prompt.
    pub fn compacting(&mut self, id: &str, now: DateTime<Utc>) {
        self.compacting.insert(id.to_string(), now);
    }

    /// Goes over the agents: plans dropped for those no longer in error, automatic ones planned,
    /// the plan shown on each failure. Returns the agents whose resume is due, now taken out.
    pub fn tick(&mut self, list: &mut [Agent], retry: bool, now: DateTime<Utc>) -> Vec<String> {
        let before = serde_json::to_vec(&self.kept).unwrap_or_default();
        let mut due = Vec::new();
        let error_of = |id: &str| list.iter().find(|a| a.id == id && failed(a)).and_then(|a| a.failure.as_ref()).map(|f| f.at);
        // Compacted: back at its prompt (no longer in error), its resume goes; too long, forgotten.
        self.compacting.retain(|id, since| match list.iter().find(|a| &a.id == id) {
            Some(a) if a.state == AgentState::Running && matches!(a.status, SessionStatus::Idle | SessionStatus::NeedsYou) => {
                due.push(id.clone());
                false
            }
            Some(a) if a.state == AgentState::Running => now - *since < Duration::minutes(COMPACTING_MIN),
            _ => false,
        });
        self.kept.plans.retain(|id, _| error_of(id).is_some());
        // A turn over without an error (or the agent gone): its automatic resumes start over.
        self.kept.attempts.retain(|id, _| list.iter().any(|a| &a.id == id && (failed(a) || a.status == SessionStatus::Running)));
        // Another error than the one dealt with: that one is over.
        self.kept.handled.retain(|id, at| error_of(id) == Some(*at));
        self.seen.retain(|id, (at, _)| error_of(id) == Some(*at));
        for a in list.iter_mut().filter(|a| failed(a)) {
            let Some(f) = a.failure.as_mut() else { continue };
            let since = self.seen.entry(a.id.clone()).or_insert((f.at, now)).1.max(f.at);
            let mut attempts = self.kept.attempts.get(&a.id).copied().unwrap_or(0);
            let handled = self.kept.handled.get(&a.id) == Some(&f.at);
            if retry && PASSING.contains(&f.kind.as_str()) && !handled && !self.kept.plans.contains_key(&a.id) && (attempts as usize) < DELAYS_MIN.len() {
                self.kept.plans.insert(a.id.clone(), Plan { at: since + Duration::minutes(DELAYS_MIN[attempts as usize]), auto: true });
            }
            match self.kept.plans.get(&a.id) {
                Some(p) if p.at <= now => {
                    if p.auto {
                        attempts += 1;
                        self.kept.attempts.insert(a.id.clone(), attempts);
                    }
                    self.kept.plans.remove(&a.id);
                    self.kept.handled.insert(a.id.clone(), f.at);
                    due.push(a.id.clone());
                }
                Some(p) => (f.resume_at, f.auto) = (Some(p.at), p.auto),
                None => {}
            }
            f.attempts = attempts;
        }
        if serde_json::to_vec(&self.kept).unwrap_or_default() != before {
            self.save();
        }
        due
    }
}

impl crate::Agents {
    /// The resume prompt (`[agents] resume_prompt`) sent to agent `id`, now.
    pub(crate) async fn send_resume(&self, id: &str) -> anyhow::Result<crate::chains::Sent> {
        let text = self.inner.settings.lock().unwrap().resume_text();
        self.send(id, &text).await
    }

    /// The time of the error agent `id` is stopped by.
    fn error_at(&self, id: &str) -> Option<DateTime<Utc>> {
        self.agent(id).filter(failed).and_then(|a| a.failure.map(|f| f.at))
    }

    /// Resume, at once (`at` none or past) or at `at` (a usage limit's reset): Resume, Resume at HH:MM.
    pub async fn resume_failed(&self, id: &str, at: Option<DateTime<Utc>>) -> anyhow::Result<()> {
        match at.filter(|t| *t > Utc::now()) {
            Some(t) => self.inner.resumes.lock().unwrap().plan(id, t),
            None => {
                let error = self.error_at(id);
                self.inner.resumes.lock().unwrap().resumed(id, error);
                self.send_resume(id).await?;
            }
        }
        self.inner.changed.notify_one();
        Ok(())
    }

    /// Compact then resume: `/compact` now, its resume prompt once it is back at its prompt.
    pub async fn compact_then_resume(&self, id: &str) -> anyhow::Result<()> {
        let error = self.error_at(id);
        {
            let mut book = self.inner.resumes.lock().unwrap();
            book.resumed(id, error);
            book.compacting(id, Utc::now());
        }
        self.send(id, "/compact").await?;
        self.inner.changed.notify_one();
        Ok(())
    }

    /// The resume planned is cancelled, the automatic ones too for this error.
    pub fn resume_cancel(&self, id: &str) {
        let error = self.error_at(id);
        self.inner.resumes.lock().unwrap().cancel(id, error);
        self.inner.changed.notify_one();
    }
}

fn failed(a: &Agent) -> bool {
    a.state == AgentState::Running && a.status == SessionStatus::Failed && a.failure.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentFailure;

    fn agent(kind: &str, at: DateTime<Utc>) -> Agent {
        Agent {
            id: "a1".into(),
            provider: Default::default(),
            account: None,
            title: "A".into(),
            category: None,
            tags: Vec::new(),
            cwd: "/w/a1".into(),
            worktree: None,
            worktree_note: None,
            adopted: Vec::new(),
            also: Vec::new(),
            branch: None,
            ticket: None,
            ticket_linked: false,
            mr: None,
            prompt: None,
            launch: None,
            created_at: at,
            state: AgentState::Running,
            exit_code: None,
            status: SessionStatus::Failed,
            needs_action: Some(kind.into()),
            own_folder: false,
            read: false,
            suggestion: None,
            gate: None,
            last_activity: Some(at),
            moved_at: None,
            context: None,
            looping: None,
            failure: Some(AgentFailure { kind: kind.into(), text: None, at, resume_at: None, auto: false, attempts: 0 }),
            planned: false,
            usage: None,
            model: None,
            legs: Vec::new(),
        }
    }

    #[test]
    fn a_passing_error_is_resumed_three_times_then_left() {
        let t0 = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut book = Book::default();
        let mut list = vec![agent("overloaded", t0)];
        // Off: nothing planned.
        assert!(book.tick(&mut list, false, t0).is_empty());
        assert_eq!(list[0].failure.as_ref().unwrap().resume_at, None);
        // On: in a minute, shown.
        assert!(book.tick(&mut list, true, t0).is_empty());
        assert_eq!(list[0].failure.as_ref().unwrap().resume_at, Some(t0 + Duration::minutes(1)));
        assert_eq!(book.tick(&mut list, true, t0 + Duration::minutes(1)), vec!["a1".to_string()]);
        // It failed again: 5 minutes after the new error, then 15, then no more.
        let t1 = t0 + Duration::minutes(2);
        list = vec![agent("overloaded", t1)];
        book.tick(&mut list, true, t1);
        assert_eq!(list[0].failure.as_ref().unwrap().resume_at, Some(t1 + Duration::minutes(5)));
        assert_eq!(book.tick(&mut list, true, t1 + Duration::minutes(5)).len(), 1);
        let t2 = t1 + Duration::minutes(6);
        list = vec![agent("overloaded", t2)];
        book.tick(&mut list, true, t2);
        assert_eq!(book.tick(&mut list, true, t2 + Duration::minutes(15)).len(), 1);
        assert_eq!(list[0].failure.as_ref().unwrap().attempts, 3, "counted as it goes");
        let t3 = t2 + Duration::minutes(16);
        list = vec![agent("overloaded", t3)];
        assert!(book.tick(&mut list, true, t3 + Duration::hours(2)).is_empty());
        assert_eq!(list[0].failure.as_ref().unwrap().attempts, 3);
    }

    /// Resumed by hand: the next refresh, before its hook says it works, plans nothing for that
    /// error (no second resume); an error seen late counts its delay from when it was seen.
    #[test]
    fn a_hand_resume_or_a_late_error_is_not_resumed_at_once() {
        let t0 = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut book = Book::default();
        let mut list = vec![agent("overloaded", t0)];
        book.resumed("a1", Some(t0));
        assert!(book.tick(&mut list, true, t0 + Duration::minutes(5)).is_empty());
        assert_eq!(list[0].failure.as_ref().unwrap().resume_at, None);
        // Seen two hours after it happened (retry turned on, Prism started): a minute from now.
        let mut book = Book::default();
        let late = t0 + Duration::hours(2);
        assert!(book.tick(&mut list, true, late).is_empty());
        assert_eq!(list[0].failure.as_ref().unwrap().resume_at, Some(late + Duration::minutes(1)));
        // Cancelled: not for this error, but for the next.
        book.cancel("a1", Some(t0));
        assert!(book.tick(&mut list, true, late + Duration::hours(1)).is_empty());
        let t1 = late + Duration::hours(2);
        let mut next = vec![agent("overloaded", t1)];
        book.tick(&mut next, true, t1);
        assert_eq!(next[0].failure.as_ref().unwrap().resume_at, Some(t1 + Duration::minutes(1)));
    }

    /// Compact then resume: the resume waits until it is back at its prompt.
    #[test]
    fn a_compaction_resumes_once_back_at_its_prompt() {
        let t0 = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut book = Book::default();
        let mut list = vec![agent("max_output_tokens", t0)];
        book.compacting("a1", t0);
        assert!(book.tick(&mut list, false, t0).is_empty(), "still compacting");
        list[0].status = SessionStatus::Idle;
        list[0].failure = None;
        assert_eq!(book.tick(&mut list, false, t0 + Duration::minutes(1)), vec!["a1".to_string()]);
        assert!(book.tick(&mut list, false, t0 + Duration::minutes(2)).is_empty(), "once");
    }

    #[test]
    fn a_usage_limit_waits_for_its_plan_and_a_cancel_holds() {
        let t0 = DateTime::from_timestamp(1_800_000_000, 0).unwrap();
        let mut book = Book::default();
        let mut list = vec![agent("rate_limit", t0)];
        assert!(book.tick(&mut list, true, t0 + Duration::hours(1)).is_empty());
        book.plan("a1", t0 + Duration::hours(2));
        book.tick(&mut list, true, t0);
        assert_eq!(list[0].failure.as_ref().unwrap().resume_at, Some(t0 + Duration::hours(2)));
        assert_eq!(list[0].failure.as_ref().unwrap().auto, false);
        // Back at work by hand: the plan goes.
        list[0].status = SessionStatus::Running;
        book.tick(&mut list, true, t0);
        list = vec![agent("overloaded", t0)];
        book.cancel("a1", Some(t0));
        assert!(book.tick(&mut list, true, t0 + Duration::hours(1)).is_empty());
    }
}
