//! The tracker: tickets, sprint, queues, logged time.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Person {
    pub id: u64,
    pub name: String,
    pub initials: String,
    /// The tracker's color for them (ClickUp's `#7b68ee`), their avatar's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Their profile picture's address (ClickUp's `profilePicture`), loaded by `ticket_image`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// A ticket's priority, whichever the tracker: ClickUp's four, Jira's read as them (`Highest` is
/// urgent…), Obsidian's `#p0` to `#p3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Urgent,
    High,
    Normal,
    Low,
}

impl Priority {
    /// From the most urgent.
    pub const ALL: [Priority; 4] = [Self::Urgent, Self::High, Self::Normal, Self::Low];

    pub fn name(self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    /// Its rank, 0 the most urgent.
    pub fn rank(self) -> usize {
        self as usize
    }

    /// Prism's own name (`high`), whatever its case.
    pub fn of(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Ticket {
    /// Displayed identifier (`ABC-1234`), or the raw ClickUp ID failing that.
    pub id: String,
    /// Internal ClickUp identifier, needed for time entries.
    pub task_id: String,
    /// Internal identifier of the parent task, for a subtask.
    pub parent: Option<String>,
    /// None set (ClickUp, Obsidian), or one of Jira's of its own.
    pub priority: Option<Priority>,
    /// ClickUp custom task type (`EPIC`, `Milestone`…); absent for an ordinary task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// The Font Awesome solid icon the tracker draws the type with (`bug`): ClickUp's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type_icon: Option<String>,
    /// ClickUp list of the ticket, when it comes from a read of the whole space (outside the sprint);
    /// the Obsidian board it is a card of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<ListRef>,
    /// The repository (`[[repos]] name`) its id resolves in, when the tracker's ids are only
    /// unique per repository (Obsidian boards); absent: unique across the tracker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Start of the description, only for the lists where it is used (cross-cutting tasks).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// ClickUp estimate, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_estimate: Option<u64>,
    /// The time everyone logged on it, as the tracker sums it (ClickUp's `time_spent`, Jira's
    /// `timespent`), in milliseconds; absent: none, or not told.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub time_spent: Option<u64>,
    /// Its tags (ClickUp) or labels (Jira): a ticket's type may be read from them (`[tracker] ticket_type`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub tags: Vec<String>,
    pub name: String,
    pub status: String,
    pub closed: bool,
    pub assignees: Vec<Person>,
    pub url: String,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    /// The custom fields of `[clickup] fields` that have a value, in that order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<TicketField>,
    /// The tickets it waits on (ClickUp's dependencies): its blockers, done or not.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub waiting_on: Vec<Dependency>,
    /// The tickets waiting on it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocking: Vec<Dependency>,
}

/// The other end of a dependency: what a ticket row shows of it. Its id, name and status come
/// from the tickets read, or from ClickUp for one outside them (another list); until then, empty.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Dependency {
    /// ClickUp's internal id.
    pub task_id: String,
    /// Displayed identifier (`ABC-1234`); empty while unknown.
    pub id: String,
    pub name: String,
    pub status: String,
    pub closed: bool,
    pub url: String,
}

impl Dependency {
    pub fn unknown(task_id: &str) -> Self {
        Dependency { task_id: task_id.into(), id: String::new(), name: String::new(), status: String::new(), closed: false, url: String::new() }
    }

    pub fn of(t: &Ticket) -> Self {
        Dependency { task_id: t.task_id.clone(), id: t.id.clone(), name: t.name.clone(), status: t.status.clone(), closed: t.closed, url: t.url.clone() }
    }
}

/// A custom field of a ticket, its value put in words (several for labels).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TicketField {
    pub name: String,
    pub values: Vec<FieldValue>,
    /// Also a filter and a grouping of the ticket lists (`[clickup] filter_fields`); otherwise only shown.
    pub filter: bool,
    /// An estimate in hours entered from Prism (`[clickup] estimate_fields`): kept even without a value.
    pub estimate: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FieldValue {
    pub label: String,
    /// The option's color in ClickUp (`#e50000`), for a dropdown or a label.
    pub color: Option<String>,
    /// The option's place in ClickUp's list (`orderindex`): the order of the groups by this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Sprint {
    pub name: String,
    /// Identifier of the sprint's ClickUp list: the destination of "Add to sprint".
    pub list_id: String,
    /// Link to the ClickUp board.
    pub url: String,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub tickets: Vec<Ticket>,
    /// Board statuses, in order: the choices of the status picker.
    pub statuses: Vec<StatusOption>,
    /// When its tickets come from several boards (Obsidian), those boards and their statuses: a
    /// ticket's picker offers its own board's (by its list id), a new card goes to one of them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub boards: Vec<SprintBoard>,
}

/// A board the sprint's tickets come from.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SprintBoard {
    /// Its tickets' list id.
    pub id: String,
    pub name: String,
    pub statuses: Vec<StatusOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TimeEntry {
    pub id: String,
    pub ticket: Option<String>,
    pub task_name: Option<String>,
    pub task_url: Option<String>,
    pub start: DateTime<Utc>,
    pub duration_ms: i64,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TimeTracking {
    /// Entries since `history_start` (`[clickup] time_history_days`, at least last month), from most recent to oldest.
    pub entries: Vec<TimeEntry>,
    /// Tickets logged over that history, from most recent to oldest.
    pub recent_tickets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Board {
    /// Its name in Prism (`[[clickup.boards]] name`).
    pub name: String,
    pub url: String,
    pub tickets: Vec<Ticket>,
    /// Board statuses, in order: the choices of the status picker.
    pub statuses: Vec<StatusOption>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Refinement {
    /// Tracked statuses, in flow order (`to specify`, `to refine`, `to estimate`).
    pub statuses: Vec<String>,
    pub boards: Vec<QuarterBoard>,
}

/// Quarterly list of a delivery folder (`Product` · `Q4 2026`).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct QuarterBoard {
    pub folder: String,
    /// List name: `Q4 2026`.
    pub quarter: String,
    pub url: String,
    /// Only the tickets in the tracked statuses.
    pub tickets: Vec<Ticket>,
    /// All the list's statuses: the choices of the status picker.
    pub statuses: Vec<StatusOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ListRef {
    pub id: String,
    pub name: String,
}

/// Tickets in progress (or further) that are not in the current sprint, across all
/// lists: the work done outside the sprint.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OutsideSprint {
    /// Tracked statuses (`doing`, `review`…).
    pub statuses: Vec<String>,
    pub tickets: Vec<Ticket>,
    /// Statuses of each ticket's list, by list id: the picker's choices.
    pub list_statuses: BTreeMap<String, Vec<StatusOption>>,
}

/// Tickets ready to be picked up: next sprint, bugs and hotfixes, in the "to pick" statuses
/// (Architecture comes from its own board, already collected).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Pickup {
    /// "To pick" statuses (`ready to dev`, `todo`…).
    pub statuses: Vec<String>,
    pub boards: Vec<PickBoard>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PickBoard {
    /// The board's name (`[[clickup.boards]]`), or the next sprint's.
    pub name: String,
    /// `board` or `next_sprint`.
    #[cfg_attr(feature = "ts", ts(type = "'board' | 'next_sprint'"))]
    pub kind: String,
    /// To pick's colour: `ko`, `warn`, `info`, `violet`, `ok`.
    pub tone: String,
    /// Shown in To pick (a `pick` board, or the next sprint).
    pub pick: bool,
    /// Read whole, its tickets checked against their MRs (a `sync` board); otherwise only the
    /// tickets in the "to pick" statuses.
    pub sync: bool,
    pub url: String,
    pub tickets: Vec<Ticket>,
    /// All the list's statuses: the choices of the status picker.
    pub statuses: Vec<StatusOption>,
}

/// Status offered by a ClickUp list.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct StatusOption {
    pub status: String,
    /// Status color in ClickUp (`#7c4dff`).
    pub color: Option<String>,
    pub closed: bool,
}
