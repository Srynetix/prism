//! Agents stopped by an API error (status `Failed`, Claude Code's `StopFailure`): resumed by a
//! prompt (`[agents] resume_prompt`) at once, at a time (a usage limit's reset), or on their own
//! for a passing error (`[agents] retry_on_error`: overloaded or server error, 3 times at most,
//! after 1, 5 then 15 minutes). A plan is dropped as soon as the agent is no longer in error.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use crate::state::{Agent, AgentState, SessionStatus};

/// The errors that pass on their own: resumed automatically when asked.
pub const PASSING: [&str; 2] = ["overloaded", "server_error"];
/// The automatic resumes' delays, after the error (the first) or the previous resume.
pub const DELAYS_MIN: [i64; 3] = [1, 5, 15];

/// A resume planned for an agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub at: DateTime<Utc>,
    pub auto: bool,
}

/// The plans, and the automatic resumes made in a row by agent (forgotten once a turn ends well).
#[derive(Debug, Default)]
pub struct Book {
    plans: HashMap<String, Plan>,
    attempts: HashMap<String, u32>,
}

impl Book {
    pub fn plan(&mut self, id: &str, at: DateTime<Utc>) {
        self.plans.insert(id.to_string(), Plan { at, auto: false });
    }

    /// Cancelled: no automatic resume either for this error.
    pub fn cancel(&mut self, id: &str) {
        self.plans.remove(id);
        self.attempts.insert(id.to_string(), DELAYS_MIN.len() as u32);
    }

    /// A resume sent by hand: the automatic ones start over if it fails again.
    pub fn resumed(&mut self, id: &str) {
        self.plans.remove(id);
        self.attempts.remove(id);
    }

    /// Goes over the agents: plans dropped for those no longer in error, automatic ones planned,
    /// the plan shown on each failure. Returns the agents whose resume is due, now taken out.
    pub fn tick(&mut self, list: &mut [Agent], retry: bool, now: DateTime<Utc>) -> Vec<String> {
        self.plans.retain(|id, _| list.iter().any(|a| &a.id == id && failed(a)));
        // A turn over without an error (or the agent gone): its automatic resumes start over.
        self.attempts.retain(|id, _| list.iter().any(|a| &a.id == id && (failed(a) || a.status == SessionStatus::Running)));
        let mut due = Vec::new();
        for a in list.iter_mut().filter(|a| failed(a)) {
            let Some(f) = a.failure.as_mut() else { continue };
            let attempts = self.attempts.get(&a.id).copied().unwrap_or(0);
            if retry && PASSING.contains(&f.kind.as_str()) && !self.plans.contains_key(&a.id) && (attempts as usize) < DELAYS_MIN.len() {
                self.plans.insert(a.id.clone(), Plan { at: f.at + Duration::minutes(DELAYS_MIN[attempts as usize]), auto: true });
            }
            f.attempts = attempts;
            match self.plans.get(&a.id) {
                Some(p) if p.at <= now => {
                    let auto = p.auto;
                    self.plans.remove(&a.id);
                    if auto {
                        self.attempts.insert(a.id.clone(), attempts + 1);
                    }
                    due.push(a.id.clone());
                }
                Some(p) => (f.resume_at, f.auto) = (Some(p.at), p.auto),
                None => {}
            }
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

    /// Resume, at once (`at` none or past) or at `at` (a usage limit's reset): Resume, Resume at HH:MM.
    pub async fn resume_failed(&self, id: &str, at: Option<DateTime<Utc>>) -> anyhow::Result<()> {
        match at.filter(|t| *t > Utc::now()) {
            Some(t) => self.inner.resumes.lock().unwrap().plan(id, t),
            None => {
                self.inner.resumes.lock().unwrap().resumed(id);
                self.send_resume(id).await?;
            }
        }
        self.inner.changed.notify_one();
        Ok(())
    }

    /// The resume planned is cancelled, the automatic ones too for this error.
    pub fn resume_cancel(&self, id: &str) {
        self.inner.resumes.lock().unwrap().cancel(id);
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
        list = vec![agent("overloaded", t1)];
        book.tick(&mut list, true, t1);
        assert_eq!(book.tick(&mut list, true, t1 + Duration::minutes(15)).len(), 1);
        list = vec![agent("overloaded", t1)];
        assert!(book.tick(&mut list, true, t1 + Duration::hours(2)).is_empty());
        assert_eq!(list[0].failure.as_ref().unwrap().attempts, 3);
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
        book.cancel("a1");
        assert!(book.tick(&mut list, true, t0 + Duration::hours(1)).is_empty());
    }
}
