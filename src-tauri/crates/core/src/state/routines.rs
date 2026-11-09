//! The routines as the window and the phones see them (`prism_agents::routines`): each one, its
//! next times and its trust; the runs of the last days, the "To review" pile among them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Routines {
    /// In the config's order.
    pub routines: Vec<RoutineView>,
    /// The last days' runs, most recent first.
    pub runs: Vec<RoutineRun>,
    /// `[remote] keep_running`: Prism started again when it is closed, so the routines keep their times.
    pub keep_running: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RoutineView {
    pub name: String,
    pub enabled: bool,
    /// Its next times (five at most), none while it cannot run (disabled, waiting for trust, in error).
    pub next: Vec<DateTime<Utc>>,
    /// The repositories it runs on (none: a builtin, in the working folder).
    pub repos: Vec<String>,
    /// Why it does not run: its declaration does not hold.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// It waits for trust before it runs.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<RoutinePending>,
    /// Its quick action, in a few words (its prompt, mode, worktree), for Settings.
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<RoutineAction>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RoutineAction {
    pub prompt: String,
    #[cfg_attr(feature = "ts", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    pub worktree: bool,
}

impl RoutineAction {
    /// What a routine's view says of what it launches.
    pub fn of(qa: &crate::config::QuickAction) -> Self {
        Self { prompt: qa.prompt.clone(), mode: qa.mode.clone(), worktree: qa.worktree }
    }
}

/// Why a routine waits for trust.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RoutinePendingReason {
    /// Never trusted: added outside Settings.
    New,
    /// The routine changed outside Settings since it was trusted.
    Modified,
    /// Its quick action did.
    ActionModified,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RoutinePending {
    pub reason: RoutinePendingReason,
    /// What trusting it trusts: the version shown.
    pub hash: String,
    /// When this version was first seen.
    pub since: DateTime<Utc>,
    /// The Prism agents running then in a folder holding the config: a hint, not a proof.
    pub agents: Vec<String>,
    /// From the version trusted to this one, line by line (`+`, `-` or ` ` first).
    pub diff: Vec<String>,
}

/// Where a run is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum RoutineRunState {
    /// Its agent works.
    Running,
    /// Its agent waits for a permission or an answer.
    Blocked,
    /// Its agent finished its turn, files changed.
    Ready,
    /// Finished, no file changed: archived at once, history only.
    Nothing,
    /// Its CLI exited on an error, or it could not start.
    Failed,
    /// Stopped after `max_minutes`.
    TooLong,
    /// Its time came and it did not run (the one before still ran, the computer slept).
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RoutineRun {
    pub id: String,
    pub routine: String,
    /// Its repository; none: a builtin, in the working folder.
    #[serde(default)]
    pub repo: Option<String>,
    /// Its agent, once launched.
    #[serde(default)]
    pub agent: Option<String>,
    /// The time it was due (or asked, with Run now).
    pub due: DateTime<Utc>,
    #[serde(default)]
    pub started: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ended: Option<DateTime<Utc>>,
    pub state: RoutineRunState,
    /// `review` or `digest`, as the routine said when it ran.
    pub deliver: String,
    /// Run on waking, its time passed while the computer slept.
    #[serde(default)]
    pub caught_up: bool,
    /// Run now (Settings, Rerun) rather than at its time.
    #[serde(default)]
    pub manual: bool,
    /// Why it failed or was skipped, in a few words.
    #[serde(default)]
    pub note: Option<String>,
    /// The end of its agent's last reply.
    #[serde(default)]
    pub said: Option<String>,
    /// The files it changed (none read yet: 0).
    #[serde(default)]
    pub files: u32,
    #[serde(default)]
    pub branch: Option<String>,
    /// Out of "To review" (Archive); its agent stays.
    #[serde(default)]
    pub archived: bool,
    /// What its agent cost, in dollars (API prices), when its CLI says.
    #[serde(default)]
    pub cost: Option<f64>,
}

impl RoutineRun {
    /// A run of `routine` on `repo`, due at `due`: under way, or `skipped` there and then.
    pub fn new(routine: &str, repo: Option<String>, deliver: &str, due: DateTime<Utc>, now: DateTime<Utc>, skipped: bool) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            routine: routine.to_string(),
            repo,
            agent: None,
            due,
            started: (!skipped).then_some(now),
            ended: skipped.then_some(now),
            state: if skipped { RoutineRunState::Skipped } else { RoutineRunState::Running },
            deliver: deliver.to_string(),
            caught_up: false,
            manual: false,
            note: None,
            said: None,
            files: 0,
            branch: None,
            archived: false,
            cost: None,
        }
    }

    /// Waits in "To review": blocked or failed whatever its delivery, ready when delivered there.
    pub fn to_review(&self) -> bool {
        !self.archived
            && match self.state {
                RoutineRunState::Blocked | RoutineRunState::Failed | RoutineRunState::TooLong => true,
                RoutineRunState::Ready => self.deliver == "review",
                _ => false,
            }
    }

    /// Its agent still works on it (or waits): the next time of its routine on its repository waits.
    pub fn under_way(&self) -> bool {
        matches!(self.state, RoutineRunState::Running | RoutineRunState::Blocked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(state: RoutineRunState, deliver: &str) -> RoutineRun {
        let now = Utc::now();
        RoutineRun { state, ..RoutineRun::new("nightly", None, deliver, now, now, false) }
    }

    /// A run new: under way, started now; skipped there and then: ended, never started.
    #[test]
    fn a_run_starts_or_is_skipped() {
        let now = Utc::now();
        let r = RoutineRun::new("nightly", Some("app".into()), "review", now, now, false);
        assert_eq!((r.state, r.started, r.ended), (RoutineRunState::Running, Some(now), None));
        assert!(r.under_way() && !r.to_review());
        let s = RoutineRun::new("nightly", None, "review", now, now, true);
        assert_eq!((s.state, s.started, s.ended), (RoutineRunState::Skipped, None, Some(now)));
        assert!(!s.under_way() && !s.to_review());
        assert_ne!(r.id, s.id);
    }

    /// "To review": blocked or failed whatever its delivery, ready only when delivered there; archived, never.
    #[test]
    fn what_waits_in_to_review() {
        for state in [RoutineRunState::Blocked, RoutineRunState::Failed, RoutineRunState::TooLong] {
            assert!(run(state, "branch").to_review(), "{state:?}");
        }
        assert!(run(RoutineRunState::Ready, "review").to_review());
        assert!(!run(RoutineRunState::Ready, "branch").to_review());
        assert!(!run(RoutineRunState::Nothing, "review").to_review());
        assert!(!RoutineRun { archived: true, ..run(RoutineRunState::Failed, "review") }.to_review());
        assert!(run(RoutineRunState::Blocked, "review").under_way() && !run(RoutineRunState::Ready, "review").under_way());
    }
}
