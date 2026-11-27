//! The dependencies between MRs: which MR waits for which, and why.
//!
//! - **Stacked**: its target branch is another open MR's source branch, in the same repository.
//! - **Declared**: on the forge (`MergeRequest::blocked_by`): GitLab's MR dependencies, the
//!   « Depends on #12 » / « Blocked by #12 » lines of a GitHub PR's description (`declared`).
//! - **Ticket**: one of its tickets waits for a ticket (the tracker's blockers) another MR names.
//!   Inferred, hence shown apart and never the cause of a merge queue alert.
//!
//! Computed from the snapshot with the alerts (`Hub::refresh_alerts`) into `Snapshot::mr_deps`,
//! which the window and the phones read. A pair of MRs counts once, by its surest source.

use std::{
    collections::{HashMap, HashSet},
    sync::LazyLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::state::{MergeRequest, MrBlocker, MrState, Snapshot, Ticket};

/// Why an MR waits for another, the surest first.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum DepVia {
    Stacked,
    Declared,
    Ticket,
}

/// An open MR (`repo`, `iid`) waiting for another (`on_repo`, `on_iid`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MrDep {
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub repo: Option<String>,
    pub iid: u64,
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub on_repo: Option<String>,
    pub on_iid: u64,
    pub via: DepVia,
    /// The MR it waits for is merged: nothing left to wait for, shown while the other is open.
    pub done: bool,
}

impl MrDep {
    /// Whether it may hold an MR back in the merge queue: not inferred, not satisfied.
    pub fn blocks(&self) -> bool {
        !self.done && self.via != DepVia::Ticket
    }
}

/// `owner/repo#12`, `#12`, or a PR's address.
static REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https://github\.com/([\w.-]+/[\w.-]+)/pull/(\d+)|(?:\b([\w.-]+/[\w.-]+))?#(\d+)\b").unwrap());
/// A line of a description declaring what it waits for.
static LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?im)^[\s>*-]*(?:depends\s+on|blocked\s+by)\b\s*:?\s*(.+)$").unwrap());

/// The MRs a description says it waits for: its « Depends on » and « Blocked by » lines, each
/// naming `#12`, `owner/repo#12` or the PR's address, several separated by commas. The project
/// is empty for the description's own one.
pub fn declared(text: &str) -> Vec<MrBlocker> {
    let mut out: Vec<MrBlocker> = Vec::new();
    for line in LINE.captures_iter(text) {
        for r in REF.captures_iter(&line[1]) {
            let (project, iid) = match (r.get(1), r.get(2), r.get(3), r.get(4)) {
                (Some(p), Some(n), _, _) => (p.as_str(), n.as_str()),
                (_, _, p, Some(n)) => (p.map_or("", |p| p.as_str()), n.as_str()),
                _ => continue,
            };
            let Ok(iid) = iid.parse() else { continue };
            let link = MrBlocker { project: project.to_string(), iid };
            if !out.contains(&link) {
                out.push(link);
            }
        }
    }
    out
}

/// The line « Declare a dependency » adds to a GitHub description: `#12`, or `owner/repo#12`
/// for another repository's PR.
pub fn declare_line(link: &MrBlocker) -> String {
    if link.project.is_empty() { format!("Depends on #{}", link.iid) } else { format!("Depends on {}#{}", link.project, link.iid) }
}

type Key = (Option<String>, u64);

fn key(m: &MergeRequest) -> Key {
    (m.repo.clone(), m.iid)
}

/// Every dependency of the open MRs, by MR then by the MR waited for.
pub fn compute(s: &Snapshot) -> Vec<MrDep> {
    let primary = s.repositories.iter().find(|r| r.primary).and_then(|r| r.project.clone());
    // The project an MR is in: its own, or the primary repository's.
    let project_of = |m: &MergeRequest| m.project.clone().or_else(|| if m.repo.is_none() { primary.clone() } else { None }).unwrap_or_default().to_lowercase();
    let mrs: Vec<&MergeRequest> = s.merge_requests.iter().filter(|m| m.state != MrState::Closed).collect();
    let by_project: HashMap<(String, u64), &MergeRequest> = mrs.iter().map(|m| ((project_of(m), m.iid), *m)).collect();
    let tickets = tickets(s);

    let mut out: Vec<MrDep> = Vec::new();
    let mut seen: HashSet<(Key, Key)> = HashSet::new();
    let mut add = |m: &MergeRequest, o: &MergeRequest, via: DepVia, out: &mut Vec<MrDep>| {
        if key(m) == key(o) || !seen.insert((key(m), key(o))) {
            return;
        }
        out.push(MrDep { repo: m.repo.clone(), iid: m.iid, on_repo: o.repo.clone(), on_iid: o.iid, via, done: o.state == MrState::Merged });
    };
    let open: Vec<&MergeRequest> = mrs.iter().copied().filter(|m| m.state == MrState::Opened).collect();
    // The surest first: a pair already found keeps its first source.
    for m in &open {
        for o in &open {
            if o.repo == m.repo && o.source_branch == m.target_branch {
                add(m, o, DepVia::Stacked, &mut out);
            }
        }
    }
    for m in &open {
        let own = project_of(m);
        for link in &m.blocked_by {
            let project = if link.project.is_empty() { own.clone() } else { link.project.to_lowercase() };
            if let Some(o) = by_project.get(&(project, link.iid)) {
                add(m, o, DepVia::Declared, &mut out);
            }
        }
    }
    for m in &open {
        for id in &m.tickets {
            let Some(t) = tickets.get(&id.to_lowercase()) else { continue };
            for d in t.waiting_on.iter().filter(|d| !d.id.is_empty()) {
                let wanted = d.id.to_lowercase();
                for o in mrs.iter().filter(|o| o.tickets.iter().any(|x| x.to_lowercase() == wanted) && !o.tickets.iter().any(|x| x.eq_ignore_ascii_case(id))) {
                    add(m, o, DepVia::Ticket, &mut out);
                }
            }
        }
    }
    out
}

/// The tickets Prism knows, by their id in lower case.
fn tickets(s: &Snapshot) -> HashMap<String, &Ticket> {
    let mut out = HashMap::new();
    let all = s.sprint.iter().flat_map(|sp| &sp.tickets).chain(s.pickup.boards.iter().flat_map(|b| &b.tickets)).chain(&s.outside_sprint.tickets);
    for t in all {
        out.entry(t.id.to_lowercase()).or_insert(t);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Dependency;

    fn mr(repo: Option<&str>, iid: u64, source: &str, target: &str) -> MergeRequest {
        MergeRequest { repo: repo.map(Into::into), iid, source_branch: source.into(), target_branch: target.into(), state: MrState::Opened, ..Default::default() }
    }

    fn pairs(deps: &[MrDep]) -> Vec<(u64, u64, DepVia, bool)> {
        deps.iter().map(|d| (d.iid, d.on_iid, d.via, d.done)).collect()
    }

    #[test]
    fn reads_the_declared_lines() {
        let text = "Adds the export.\n\nDepends on #12, owner/web#7\n- Blocked by: https://github.com/o/r/pull/3\nNot this #99\n> depends on #12";
        let links = declared(text);
        let want = [("", 12), ("owner/web", 7), ("o/r", 3)].map(|(p, n)| MrBlocker { project: p.into(), iid: n });
        assert_eq!(links, want);
        assert!(declared("Fixes #4").is_empty());
        assert_eq!(declare_line(&MrBlocker { project: String::new(), iid: 5 }), "Depends on #5");
        assert_eq!(declare_line(&MrBlocker { project: "o/r".into(), iid: 5 }), "Depends on o/r#5");
    }

    #[test]
    fn stacked_then_declared_then_tickets_each_pair_once() {
        let mut s = Snapshot::default();
        let base = mr(None, 410, "feat/schema", "main");
        let api = MergeRequest { tickets: vec!["ABC-2".into()], blocked_by: vec![MrBlocker { project: String::new(), iid: 410 }], ..mr(None, 412, "feat/api", "feat/schema") };
        let web = MergeRequest { project: Some("g/web".into()), blocked_by: vec![MrBlocker { project: "G/API".into(), iid: 412 }, MrBlocker { project: "g/api".into(), iid: 999 }], ..mr(Some("web"), 88, "feat/screen", "main") };
        let auth = MergeRequest { state: MrState::Merged, tickets: vec!["ABC-1".into()], ..mr(None, 405, "feat/auth", "main") };
        let logout = MergeRequest { tickets: vec!["ABC-3".into()], ..mr(Some("web"), 91, "feat/logout", "main") };
        let closed = MergeRequest { state: MrState::Closed, ..mr(None, 400, "feat/old", "main") };
        s.merge_requests = vec![base, api, web, auth, logout, closed];
        s.repositories = vec![crate::config::RepoInfo { name: "api".into(), project: Some("g/api".into()), primary: true, ..Default::default() }];
        let ticket = |id: &str, on: &str| Ticket {
            id: id.into(),
            task_id: id.into(),
            parent: None,
            priority: None,
            task_type: None,
            task_type_icon: None,
            list: None,
            repo: None,
            description: None,
            time_estimate: None,
            time_spent: None,
            tags: Vec::new(),
            name: id.into(),
            status: "ready".into(),
            closed: false,
            assignees: Vec::new(),
            url: String::new(),
            created_at: None,
            updated_at: None,
            fields: Vec::new(),
            waiting_on: vec![Dependency { id: on.into(), ..Dependency::unknown("t") }],
            blocking: Vec::new(),
        };
        s.outside_sprint.tickets = vec![ticket("ABC-3", "ABC-1"), ticket("ABC-2", "ABC-9")];
        assert_eq!(pairs(&compute(&s)), [(412, 410, DepVia::Stacked, false), (88, 412, DepVia::Declared, false), (91, 405, DepVia::Ticket, true)]);
        assert!(compute(&s)[0].blocks() && !compute(&s)[2].blocks());
    }
}
