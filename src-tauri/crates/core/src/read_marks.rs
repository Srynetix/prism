//! Sessions marked read: an agent (or a Claude Desktop session) whose turn is over, which I saw
//! and leave there. It stops waiting for me — out of « Waiting for you », the alerts and the
//! counts, on the computer and the phone — and shows as stopped, though its CLI still runs. The
//! mark falls by itself once the session moves again (a turn, a question, a permission, a
//! prompt). A permission or a question being asked is never marked: it waits for an answer.
//!
//! Applied where the sessions enter the snapshot (`apply`), so every reader follows. Kept per
//! profile in its data folder (`sessions-read.json`): its id → its last activity when marked.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use chrono::{DateTime, Utc};

use crate::state::{Hub, SessionStatus};

/// An agent's question (`needs_action`): it waits for an answer.
pub const QUESTION: &str = "Answer its question";

/// Each session marked read → its last activity then.
type Marks = BTreeMap<String, DateTime<Utc>>;

static MARKS: LazyLock<Mutex<Option<Marks>>> = LazyLock::new(|| Mutex::new(None));
/// Tests read and write their own file.
static PATH: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

fn path() -> PathBuf {
    PATH.lock().unwrap().clone().unwrap_or_else(|| crate::profile::data_dir(&crate::profile::data_base()).join("sessions-read.json"))
}

fn with<R>(f: impl FnOnce(&mut Marks) -> (R, bool)) -> R {
    let mut marks = MARKS.lock().unwrap();
    let map = marks.get_or_insert_with(|| std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    let (out, changed) = f(map);
    if changed {
        let bytes = serde_json::to_vec_pretty(&*map).unwrap_or_default();
        if let Err(e) = path().parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| prism_base::files::write_private(&path(), bytes)) {
            tracing::warn!("sessions read: {e}");
        }
    }
    out
}

/// Its turn is over and it asks nothing: it may be marked read.
pub fn can_mark(status: SessionStatus, needs_action: Option<&str>) -> bool {
    matches!(status, SessionStatus::NeedsYou | SessionStatus::ReviewReady) && needs_action != Some(QUESTION)
}

/// Marked read as it is now (`at`: its last activity).
pub fn mark(id: &str, at: Option<DateTime<Utc>>) {
    with(|m| ((), m.insert(id.to_string(), at.unwrap_or(DateTime::<Utc>::MIN_UTC)).is_none_or(|was| Some(was) != at)));
}

/// Its turn waits for me again.
pub fn unmark(id: &str) {
    with(|m| ((), m.remove(id).is_some()));
}

/// A session as it enters the snapshot: marked and still where it was, it no longer waits for
/// me (`Idle`, `read`); moved since, its mark is forgotten.
pub fn apply(id: &str, status: &mut SessionStatus, needs_action: &mut Option<String>, last: Option<DateTime<Utc>>, read: &mut bool) {
    with(|m| {
        let Some(at) = m.get(id).copied() else { return ((), false) };
        let moved = last.unwrap_or(DateTime::<Utc>::MIN_UTC) > at;
        if !moved && can_mark(*status, needs_action.as_deref()) {
            (*status, *needs_action, *read) = (SessionStatus::Idle, None, true);
            return ((), false);
        }
        // Still idle as it was (its CLI quit, a reload): kept until it moves.
        if !moved && matches!(status, SessionStatus::Idle | SessionStatus::Completed) {
            return ((), false);
        }
        m.remove(id);
        ((), true)
    });
}

/// The marks applied to the snapshot at once, without waiting for the next read of the sessions.
pub fn apply_now(hub: &Hub) {
    hub.update(|s| {
        let before = (s.agents.clone(), s.sessions.clone());
        for a in &mut s.agents {
            apply(&a.id, &mut a.status, &mut a.needs_action, a.last_activity, &mut a.read);
        }
        for c in &mut s.sessions {
            apply(&c.id, &mut c.status, &mut c.needs_action, c.last_activity, &mut c.read);
        }
        before != (s.agents.clone(), s.sessions.clone())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_read_stops_waiting_until_it_moves() {
        let dir = std::env::temp_dir().join(format!("prism-read-{}", std::process::id()));
        *PATH.lock().unwrap() = Some(dir.join("sessions-read.json"));
        *MARKS.lock().unwrap() = None;
        let t0 = Utc::now();
        let (mut status, mut needs, mut read) = (SessionStatus::NeedsYou, None, false);
        assert!(can_mark(status, None) && !can_mark(SessionStatus::WaitingTool, None) && !can_mark(status, Some(QUESTION)));
        mark("a", Some(t0));
        apply("a", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::Idle, true));
        // Read back from its file.
        *MARKS.lock().unwrap() = None;
        let (mut status, mut read) = (SessionStatus::ReviewReady, false);
        apply("a", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::Idle, true));
        // A new turn: it waits for me again, its mark forgotten.
        let later = t0 + chrono::Duration::seconds(5);
        let (mut status, mut read) = (SessionStatus::NeedsYou, false);
        apply("a", &mut status, &mut needs, Some(later), &mut read);
        assert_eq!((status, read), (SessionStatus::NeedsYou, false));
        apply("a", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::NeedsYou, false), "forgotten");
        // A question asked meanwhile, at the same moment: it waits for an answer.
        mark("b", Some(t0));
        let (mut status, mut needs, mut read) = (SessionStatus::NeedsYou, Some(QUESTION.to_string()), false);
        apply("b", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::NeedsYou, false));
        unmark("b");
        assert!(std::fs::read_to_string(dir.join("sessions-read.json")).unwrap().trim() == "{}");
        *PATH.lock().unwrap() = None;
        *MARKS.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
