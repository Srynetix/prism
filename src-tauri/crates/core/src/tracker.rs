//! The tracker: where the tickets live, their statuses and the sprint. A profile speaks to one
//! (`[tracker] provider`): ClickUp, Obsidian Kanban boards, or Jira Cloud. The window only sees `Tracker` and the snapshot's types, never a provider's
//! own: a provider maps its API onto them, and says through [`Features`] what it offers. The
//! `Tracker` itself, which reaches the provider, is in `prism-collectors` (`collectors::tracker`).

use serde::{Deserialize, Serialize};

/// The provider of the active profile's tracker, in the snapshot (absent: no tracker read).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "TrackerProvider"))]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Clickup,
    Obsidian,
    Jira,
}

impl Provider {
    /// What this provider offers.
    pub fn features(self) -> Features {
        match self {
            Self::Clickup => Features {
                sprint: true,
                next_sprint: true,
                refinement: true,
                pickup: true,
                outside_sprint: true,
                time: true,
                status: true,
                assignees: true,
                priority: true,
                comment: true,
                comment_edit: true,
                mentions: true,
                add_to_sprint: true,
                create: false,
            },
            // The open cards of its boards, as the sprint and To pick: moved between lanes,
            // prioritized, commented.
            Self::Obsidian => Features { sprint: true, pickup: true, status: true, priority: true, comment: true, create: true, ..Default::default() },
            // The board's sprints and JQL queues, and nearly every change.
            Self::Jira => Features { sprint: true, next_sprint: true, refinement: true, pickup: true, outside_sprint: true, time: true, status: true, assignees: true, priority: true, comment: true, comment_edit: true, mentions: false, add_to_sprint: true, create: true },
        }
    }
}

/// What the tracker offers: the window hides what it lacks rather than failing on it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "TrackerFeatures"))]
pub struct Features {
    /// The current sprint, its board and its statuses.
    pub sprint: bool,
    /// The sprint after the current one, in To pick.
    pub next_sprint: bool,
    /// The refinement queue.
    pub refinement: bool,
    /// The boards whose tickets are ready to pick.
    pub pickup: bool,
    /// Tickets in progress outside the sprint.
    pub outside_sprint: bool,
    /// Time entries on tickets.
    pub time: bool,
    /// Changing a ticket's status.
    pub status: bool,
    /// Assignees: "mine", assign to me.
    pub assignees: bool,
    pub priority: bool,
    /// Commenting on a ticket.
    pub comment: bool,
    /// Editing one of my comments.
    pub comment_edit: bool,
    /// Mentioning someone in a comment (`@`), who is then notified: the list's members, a rich comment.
    pub mentions: bool,
    /// Adding a ticket to the current sprint, on top of its board.
    pub add_to_sprint: bool,
    /// Creating a ticket, on one of the sprint's boards.
    pub create: bool,
}

/// At most so many tickets found by a search (the palette's).
pub const SEARCH_LIMIT: usize = 20;

/// Does a ticket answer a search: every word typed found in its id or its name, whatever the case.
pub fn matches(t: &crate::state::Ticket, query: &str) -> bool {
    let text = format!("{} {}", t.id, t.name).to_lowercase();
    let mut words = query.split_whitespace().peekable();
    words.peek().is_some() && words.all(|w| text.contains(&w.to_lowercase()))
}

/// The tickets a search finds among these: its id first, then the names that have the words as
/// typed, then the others, the latest changed first; at most [`SEARCH_LIMIT`].
pub fn found(tickets: impl IntoIterator<Item = crate::state::Ticket>, query: &str) -> Vec<crate::state::Ticket> {
    let q = query.trim().to_lowercase();
    let mut seen = std::collections::HashSet::new();
    let mut hits: Vec<_> = tickets.into_iter().filter(|t| matches(t, &q) && seen.insert(t.task_id.clone())).collect();
    let rank = |t: &crate::state::Ticket| if t.id.to_lowercase() == q { 0 } else if t.name.to_lowercase().contains(&q) { 1 } else { 2 };
    hits.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.closed.cmp(&b.closed)).then(b.updated_at.cmp(&a.updated_at)));
    hits.truncate(SEARCH_LIMIT);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Ticket;

    fn ticket(id: &str, name: &str) -> Ticket {
        Ticket { id: id.into(), task_id: id.to_lowercase(), parent: None, priority: None, task_type: None, task_type_icon: None, list: None, repo: None, description: None, time_estimate: None, time_spent: None, tags: Vec::new(), name: name.into(), status: "open".into(), closed: false, assignees: Vec::new(), url: String::new(), created_at: None, updated_at: None, fields: Vec::new(), waiting_on: Vec::new(), blocking: Vec::new() }
    }

    #[test]
    fn a_search_finds_every_word_its_id_first() {
        let all = vec![ticket("ABC-2", "Export the report as PDF"), ticket("ABC-12", "PDF report export"), ticket("ABC-3", "Login page")];
        let hits: Vec<String> = found(all.clone(), "report pdf").into_iter().map(|t| t.id).collect();
        assert_eq!(hits, ["ABC-2", "ABC-12"]);
        assert_eq!(found(all.clone(), "abc-3")[0].id, "ABC-3");
        assert_eq!(found(all.clone(), "pdf report")[0].id, "ABC-12");
        assert!(found(all, "  ").is_empty());
    }
}
