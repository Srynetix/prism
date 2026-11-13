//! Sessions marked read: an agent (or a Claude Desktop session) whose turn is over, which I saw
//! and leave there. It stops waiting for me — out of « Waiting for you », the alerts and the
//! counts, on the computer and the phone — and shows as stopped, though its CLI still runs. The
//! mark falls by itself once the session moves again (a prompt, a tool, a permission): for an
//! agent, its hooks' last move (`Agent::moved_at`), not a mere notification; for a Desktop
//! session, its transcript. A session asking something (a question, a plan to approve: its
//! `needs_action`) or a permission is never marked: it waits for an answer.
//!
//! Applied where the sessions enter the snapshot (`apply`), so every reader follows. Kept per
//! profile in its data folder (`sessions-read.json`): its id → its moment when marked. Written
//! by `flush`, never under the hub's lock.

use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        LazyLock, Mutex,
    },
};

use chrono::{DateTime, Utc};

use crate::state::{Hub, SessionStatus};

/// Each session marked read → its moment then.
type Marks = BTreeMap<String, DateTime<Utc>>;

static MARKS: LazyLock<Mutex<Option<Marks>>> = LazyLock::new(|| Mutex::new(None));
/// Changed since written.
static DIRTY: AtomicBool = AtomicBool::new(false);
/// Tests read and write their own file.
static PATH: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

fn path() -> PathBuf {
    PATH.lock().unwrap().clone().unwrap_or_else(|| crate::profile::data_dir(&crate::profile::data_base()).join("sessions-read.json"))
}

/// The marks, read from their file the first time; `f` says whether it changed them.
fn with<R>(f: impl FnOnce(&mut Marks) -> (R, bool)) -> R {
    let mut marks = MARKS.lock().unwrap();
    let map = marks.get_or_insert_with(|| std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    let (out, changed) = f(map);
    if changed {
        DIRTY.store(true, Ordering::Relaxed);
    }
    out
}

/// Writes the marks if they changed.
pub fn flush() {
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let bytes = with(|m| (serde_json::to_vec_pretty(&*m).unwrap_or_default(), false));
    if let Err(e) = path().parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| prism_base::files::write_private(&path(), bytes)) {
        tracing::warn!("sessions read: {e}");
    }
}

/// Its turn is over and it asks nothing: it may be marked read.
pub fn can_mark(status: SessionStatus, needs_action: Option<&str>) -> bool {
    matches!(status, SessionStatus::NeedsYou | SessionStatus::ReviewReady) && needs_action.is_none_or(|n| n.trim().is_empty())
}

/// Marked read as it is now (`at`: its moment, see the module).
pub fn mark(id: &str, at: Option<DateTime<Utc>>) {
    let at = at.unwrap_or(DateTime::<Utc>::MIN_UTC);
    with(|m| ((), m.insert(id.to_string(), at) != Some(at)));
    flush();
}

/// A session as it enters the snapshot: marked and still where it was, it no longer waits for
/// me (`Idle`, `read`); moved since, its mark is forgotten. True when it changed the session.
pub fn apply(id: &str, status: &mut SessionStatus, needs_action: &mut Option<String>, at: Option<DateTime<Utc>>, read: &mut bool) -> bool {
    with(|m| {
        let Some(marked) = m.get(id).copied() else { return (false, false) };
        let moved = at.unwrap_or(DateTime::<Utc>::MIN_UTC) > marked;
        if !moved && can_mark(*status, needs_action.as_deref()) {
            (*status, *needs_action, *read) = (SessionStatus::Idle, None, true);
            return (true, false);
        }
        // Still idle as it was (its CLI quit, a reload): kept until it moves.
        if !moved && matches!(status, SessionStatus::Idle | SessionStatus::Completed) {
            return (false, false);
        }
        m.remove(id);
        (false, true)
    })
}

/// The marks of sessions gone (an agent closed, a Desktop session deleted) forgotten: `present`,
/// every agent and session listed now.
pub fn prune(present: &HashSet<&str>) {
    with(|m| {
        let before = m.len();
        m.retain(|id, _| present.contains(id.as_str()));
        ((), m.len() != before)
    });
}

/// The marks applied to the snapshot at once, without waiting for the next read of the sessions.
pub fn apply_now(hub: &Hub) {
    hub.update(|s| {
        let mut changed = false;
        for a in &mut s.agents {
            changed |= apply(&a.id, &mut a.status, &mut a.needs_action, a.moved_at.or(a.last_activity), &mut a.read);
        }
        for c in &mut s.sessions {
            let at = s.agents.iter().find(|a| a.id == c.id).map_or(c.last_activity, |a| a.moved_at.or(a.last_activity));
            changed |= apply(&c.id, &mut c.status, &mut c.needs_action, at, &mut c.read);
        }
        // Gone ones forgotten, once the Desktop sessions are read: before, they would all look gone.
        if s.answered.iter().any(|a| *a == "sessions") {
            prune(&s.agents.iter().map(|a| a.id.as_str()).chain(s.sessions.iter().map(|c| c.id.as_str())).collect());
        }
        changed
    });
    flush();
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
        assert!(can_mark(status, None) && !can_mark(SessionStatus::WaitingTool, None) && !can_mark(status, Some("Answer its question")) && !can_mark(status, Some("Choose A or B")));
        mark("a", Some(t0));
        assert!(apply("a", &mut status, &mut needs, Some(t0), &mut read));
        assert_eq!((status, read), (SessionStatus::Idle, true));
        // Read back from its file.
        *MARKS.lock().unwrap() = None;
        let (mut status, mut read) = (SessionStatus::ReviewReady, false);
        apply("a", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::Idle, true));
        // It moves: it waits for me again, its mark forgotten.
        let later = t0 + chrono::Duration::seconds(5);
        let (mut status, mut read) = (SessionStatus::NeedsYou, false);
        assert!(!apply("a", &mut status, &mut needs, Some(later), &mut read));
        assert_eq!((status, read), (SessionStatus::NeedsYou, false));
        apply("a", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::NeedsYou, false), "forgotten");
        // Something asked meanwhile, at the same moment: it waits for an answer.
        mark("b", Some(t0));
        let (mut status, mut needs, mut read) = (SessionStatus::NeedsYou, Some("Answer its question".to_string()), false);
        apply("b", &mut status, &mut needs, Some(t0), &mut read);
        assert_eq!((status, read), (SessionStatus::NeedsYou, false));
        // Gone: forgotten, and written.
        mark("c", None);
        prune(&HashSet::from(["a"]));
        flush();
        assert_eq!(std::fs::read_to_string(dir.join("sessions-read.json")).unwrap().trim(), "{}");
        *PATH.lock().unwrap() = None;
        *MARKS.lock().unwrap() = None;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
