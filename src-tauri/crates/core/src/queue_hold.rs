//! MRs held out of the merge queue until the MR they depend on is merged (the « Hold » fix of a
//! merge queue alert, `crate::mr_deps`): Prism took the queue's label off, and puts it back once
//! that MR is merged. Held while a Prism runs; one that was not running then leaves the MR out
//! of the queue, as when the MR it waits for was closed instead, or the held one merged or closed.
//!
//! Kept per profile in its data folder (`queue-held.json`), mirrored in `Snapshot::queue_held`
//! for the window and the phones. `due` says what is to be done; the forge's collector does it.

use std::{
    path::PathBuf,
    sync::{LazyLock, Mutex},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::state::{Hub, MrState, Snapshot};

/// An MR held out of the queue (`repo`, `iid`) until another is merged (`on_repo`, `on_iid`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Held {
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub repo: Option<String>,
    pub iid: u64,
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub on_repo: Option<String>,
    pub on_iid: u64,
    pub at: DateTime<Utc>,
}

impl Held {
    fn is(&self, repo: Option<&str>, iid: u64) -> bool {
        self.repo.as_deref() == repo && self.iid == iid
    }
}

/// What becomes of a held MR, now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The MR it waited for is merged: back into the queue.
    Requeue,
    /// Nothing more to wait for, and not to put back: it was merged or closed, or the MR it
    /// waited for was closed or is no longer read.
    Drop,
}

static HELD: LazyLock<Mutex<Option<Vec<Held>>>> = LazyLock::new(|| Mutex::new(None));
/// Tests read and write their own file.
static PATH: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

fn path() -> PathBuf {
    PATH.lock().unwrap().clone().unwrap_or_else(|| crate::profile::data_dir(&crate::profile::data_base()).join("queue-held.json"))
}

/// The held MRs, read from their file the first time; `f` says whether it changed them, which
/// writes them and mirrors them in the snapshot.
fn with<R>(hub: Option<&Hub>, f: impl FnOnce(&mut Vec<Held>) -> (R, bool)) -> R {
    let mut held = HELD.lock().unwrap();
    let list = held.get_or_insert_with(|| std::fs::read(path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default());
    let (out, changed) = f(list);
    if changed {
        let bytes = serde_json::to_vec_pretty(&*list).unwrap_or_default();
        if let Err(e) = path().parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| prism_base::files::write_private(&path(), bytes)) {
            tracing::warn!("queue held: {e}");
        }
    }
    if let Some(hub) = hub {
        let now = list.clone();
        hub.update(|s| {
            if s.queue_held == now {
                return false;
            }
            s.queue_held = now;
            true
        });
    }
    out
}

/// The held MRs into the snapshot, at launch.
pub fn load(hub: &Hub) {
    with(Some(hub), |_| ((), false));
}

/// Holds an MR until another is merged (it replaces an earlier hold of that MR).
pub fn hold(hub: &Hub, repo: Option<String>, iid: u64, on_repo: Option<String>, on_iid: u64) {
    with(Some(hub), |list| {
        list.retain(|h| !h.is(repo.as_deref(), iid));
        list.push(Held { repo, iid, on_repo, on_iid, at: Utc::now() });
        ((), true)
    });
}

/// No longer held; its hold if it had one.
pub fn release(hub: &Hub, repo: Option<&str>, iid: u64) -> Option<Held> {
    with(Some(hub), |list| {
        let i = list.iter().position(|h| h.is(repo, iid));
        (i.map(|i| list.remove(i)), i.is_some())
    })
}

/// What is to be done with each held MR, as the snapshot stands; none before the forge's MRs
/// were read, nor for an MR not read (yet).
pub fn due(s: &Snapshot) -> Vec<(Held, Outcome)> {
    if s.merge_requests.is_empty() {
        return Vec::new();
    }
    let find = |repo: &Option<String>, iid: u64| s.merge_requests.iter().find(|m| &m.repo == repo && m.iid == iid);
    s.queue_held
        .iter()
        .filter_map(|h| {
            let mr = find(&h.repo, h.iid)?;
            let outcome = if mr.state != MrState::Opened {
                Outcome::Drop
            } else {
                match find(&h.on_repo, h.on_iid).map(|o| o.state) {
                    Some(MrState::Opened) => return None,
                    Some(MrState::Merged) => Outcome::Requeue,
                    _ => Outcome::Drop,
                }
            };
            Some((h.clone(), outcome))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MergeRequest;

    fn mr(iid: u64, state: MrState) -> MergeRequest {
        MergeRequest { iid, state, ..Default::default() }
    }

    fn held(iid: u64, on: u64) -> Held {
        Held { repo: None, iid, on_repo: None, on_iid: on, at: Utc::now() }
    }

    #[test]
    fn back_in_the_queue_once_merged_dropped_when_it_cannot_be() {
        let mut s = Snapshot { queue_held: vec![held(1, 2), held(3, 4), held(5, 6), held(7, 8), held(9, 10), held(11, 99)], ..Default::default() };
        assert!(due(&s).is_empty(), "nothing before the MRs are read");
        use MrState::*;
        s.merge_requests = vec![mr(1, Opened), mr(2, Opened), mr(3, Opened), mr(4, Merged), mr(5, Opened), mr(6, Closed), mr(7, Merged), mr(8, Opened), mr(11, Opened)];
        let got: Vec<(u64, Outcome)> = due(&s).into_iter().map(|(h, o)| (h.iid, o)).collect();
        assert_eq!(got, [(3, Outcome::Requeue), (5, Outcome::Drop), (7, Outcome::Drop), (11, Outcome::Drop)]);
    }

    #[test]
    fn hold_and_release_are_kept() {
        let dir = std::env::temp_dir().join(format!("prism-held-{}", std::process::id()));
        *PATH.lock().unwrap() = Some(dir.join("queue-held.json"));
        let hub = Hub::new();
        hold(&hub, None, 1, None, 2);
        hold(&hub, None, 1, None, 3);
        assert_eq!(hub.subscribe().borrow().queue_held.iter().map(|h| h.on_iid).collect::<Vec<_>>(), [3]);
        *HELD.lock().unwrap() = None;
        assert_eq!(release(&hub, None, 1).map(|h| h.on_iid), Some(3), "read back from its file");
        assert!(hub.subscribe().borrow().queue_held.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
