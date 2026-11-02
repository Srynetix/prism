//! A ticket opened in the window, whatever the tracker: its content, read on demand when it is
//! opened (never collected in the background nor kept in the snapshot, it never leaves the front
//! end's memory), and the snapshot's copy of a ticket updated after an action on it.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::state::{Hub, Person, Snapshot, StatusOption, Ticket};

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TaskView {
    pub task_id: String,
    /// `ABC-1234`, falling back to the internal id.
    pub id: String,
    pub name: String,
    /// ClickUp custom task type (`EPIC`…); absent for an ordinary task.
    pub task_type: Option<String>,
    /// The Font Awesome solid icon the tracker draws the type with (`bug`): ClickUp's.
    pub task_type_icon: Option<String>,
    pub status: String,
    /// Status color in ClickUp (`#7c4dff`).
    pub status_color: Option<String>,
    /// Statuses of the ticket's list, in board order: the choices of the status picker.
    pub statuses: Vec<StatusOption>,
    pub closed: bool,
    pub priority: Option<crate::state::Priority>,
    pub assignees: Vec<Person>,
    pub creator: Option<Person>,
    pub tags: Vec<String>,
    pub list: Option<String>,
    pub parent: Option<String>,
    pub url: String,
    /// Description in ClickUp's Markdown format, falling back to plain text.
    pub description: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    /// Estimate, in milliseconds.
    pub time_estimate: Option<u64>,
    /// The time everyone logged on it, as the tracker sums it, in milliseconds; absent: none, or not told.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub time_spent: Option<u64>,
    /// The custom fields of `[clickup] fields` that have a value.
    pub fields: Vec<crate::state::TicketField>,
    pub checklists: Vec<Checklist>,
    pub subtasks: Vec<Subtask>,
    /// Oldest to newest.
    pub comments: Vec<Comment>,
    pub attachments: Vec<Attachment>,
}

/// A ticket attachment. Images are displayed via [`image`], the rest opens in the
/// browser (the user's ClickUp session).
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Attachment {
    pub title: String,
    pub url: String,
    pub extension: Option<String>,
    pub image: bool,
    /// Bytes, when ClickUp provides it.
    pub size: Option<u64>,
    /// Thumbnail, for images (lighter than the original).
    pub thumbnail: Option<String>,
    pub date: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Checklist {
    pub name: String,
    pub items: Vec<ChecklistItem>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChecklistItem {
    pub name: String,
    pub resolved: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Subtask {
    pub task_id: String,
    pub id: String,
    pub name: String,
    pub status: String,
    pub closed: bool,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Comment {
    /// ClickUp id of the comment, to edit it.
    pub id: String,
    pub author: Option<Person>,
    pub text: String,
    pub date: Option<DateTime<Utc>>,
    pub replies: u32,
}

pub fn all_tickets(s: &Snapshot) -> impl Iterator<Item = &Ticket> {
    let boards = [s.sprint.as_ref().map(|b| &b.tickets), s.next_sprint.as_ref().map(|b| &b.tickets), s.time_tasks.as_ref().map(|b| &b.tickets)];
    let queue = s.refinement.boards.iter().map(|b| &b.tickets).chain(s.pickup.boards.iter().map(|b| &b.tickets)).chain(std::iter::once(&s.outside_sprint.tickets));
    boards.into_iter().flatten().chain(queue).flatten()
}

/// Applies a change to every copy of a ticket in the snapshot (a ticket can be
/// both in the sprint and in a board), without waiting for the next collection.
pub fn update_ticket(hub: &Hub, task_id: &str, mut apply: impl FnMut(&mut Ticket) -> bool) {
    hub.update(|s| {
        let boards = [s.sprint.as_mut().map(|b| &mut b.tickets), s.next_sprint.as_mut().map(|b| &mut b.tickets), s.time_tasks.as_mut().map(|b| &mut b.tickets)];
        let queue = s
            .refinement
            .boards
            .iter_mut()
            .map(|b| &mut b.tickets)
            .chain(s.pickup.boards.iter_mut().map(|b| &mut b.tickets))
            .chain(std::iter::once(&mut s.outside_sprint.tickets));
        let mut changed = false;
        for t in boards.into_iter().flatten().chain(queue).flat_map(|l| l.iter_mut()).filter(|t| t.task_id == task_id) {
            changed |= apply(t);
        }
        changed
    });
}
