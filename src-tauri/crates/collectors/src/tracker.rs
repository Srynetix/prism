//! The active profile's tracker, whichever its provider (`[tracker] provider`): what the window
//! asks of it goes to the provider's own actions.

use prism_base::{i18n::tr, tr};
use anyhow::{bail, Result};

pub use prism_core::tracker::*;

use crate::{
    {
        jira, obsidian,
        task_view::{Mention, TaskView},
    },
    state::Hub,
};

/// What Obsidian has no notion of.
const NOT_ON_OBSIDIAN: &str = "Obsidian boards have no assignees, comment authors nor sprint lists";

#[derive(Clone)]
pub enum Tracker {
    Clickup(crate::clickup::Actions),
    Obsidian(obsidian::Actions),
    Jira(jira::Actions),
}

impl Tracker {
    /// A ticket's content, read when its preview opens. `id`: the displayed id, or the provider's own.
    pub async fn ticket(&self, id: &str) -> Result<TaskView> {
        match self {
            Self::Clickup(c) => c.ticket(id).await,
            Self::Obsidian(o) => o.ticket(id).await,
            Self::Jira(j) => j.ticket(id).await,
        }
    }

    /// The tickets whose id or name has every word of `query` (the palette's search), beyond the
    /// ones the window shows: closed ones included where the provider reads them.
    pub async fn search(&self, query: &str) -> Result<Vec<crate::state::Ticket>> {
        match self {
            Self::Clickup(c) => c.search(query).await,
            Self::Obsidian(o) => Ok(o.search(query)),
            Self::Jira(j) => j.search_text(query).await,
        }
    }

    /// An image of a ticket (attachment, description image), as a `data:` URL.
    pub async fn image(&self, url: &str) -> Result<String> {
        match self {
            Self::Clickup(c) => c.image(url).await,
            Self::Obsidian(_) => bail!("Obsidian cards have no remote images"),
            Self::Jira(_) => bail!("Prism shows no Jira attachment"),
        }
    }

    pub async fn set_status(&self, hub: &Hub, task_id: &str, status: &str) -> Result<()> {
        match self {
            Self::Clickup(c) => c.set_status(hub, task_id, status).await,
            Self::Obsidian(o) => o.set_status(hub, task_id, status).await,
            Self::Jira(j) => j.set_status(hub, task_id, status).await,
        }
    }

    pub async fn set_priority(&self, hub: &Hub, task_id: &str, priority: Option<crate::state::Priority>) -> Result<()> {
        match self {
            Self::Clickup(c) => c.set_priority(hub, task_id, priority).await,
            Self::Obsidian(o) => o.set_priority(hub, task_id, priority).await,
            Self::Jira(j) => j.set_priority(hub, task_id, priority).await,
        }
    }

    /// A ticket's estimate in hours (`None`: cleared): the tracker's own (`field` `None`, when
    /// `[tracker] estimate_builtin`), or one of `[clickup] estimate_fields`. Anything else is refused.
    pub async fn set_estimate(&self, hub: &Hub, task_id: &str, field: Option<&str>, hours: Option<f64>) -> Result<()> {
        if let Some(h) = hours
            && (!h.is_finite() || !(0.0..=10_000.0).contains(&h))
        {
            bail!("{}", tr!("{h} is not an estimate in hours", h = h));
        }
        let c = crate::conventions::get();
        match field {
            None if !c.estimate_builtin => bail!("{}", tr("the tracker's own estimate is not entered from Prism ([tracker] estimate_builtin)")),
            Some(f) if !c.estimate_fields.iter().any(|e| e.eq_ignore_ascii_case(f.trim())) => bail!("{}", tr!("\"{f}\" is not one of [clickup] estimate_fields", f = f)),
            _ => {}
        }
        match self {
            Self::Clickup(c) => c.set_estimate(hub, task_id, field, hours).await,
            Self::Obsidian(_) => bail!("Obsidian cards have no estimate"),
            Self::Jira(_) if field.is_some() => bail!("Prism enters no Jira custom field"),
            Self::Jira(j) => j.set_estimate(hub, task_id, hours).await,
        }
    }

    pub async fn set_mine(&self, hub: &Hub, task_id: &str, assign: bool) -> Result<()> {
        match self {
            Self::Clickup(c) => c.set_mine(hub, task_id, assign).await,
            Self::Obsidian(_) => bail!("{}", tr(NOT_ON_OBSIDIAN)),
            Self::Jira(j) => j.set_mine(hub, task_id, assign).await,
        }
    }

    pub async fn add_to_sprint(&self, hub: &Hub, task_id: &str) -> Result<()> {
        match self {
            Self::Clickup(c) => c.add_to_sprint(hub, task_id).await,
            Self::Obsidian(_) => bail!("{}", tr(NOT_ON_OBSIDIAN)),
            Self::Jira(j) => j.add_to_sprint(hub, task_id).await,
        }
    }

    /// `mentions`: the people to notify, by their token in `text` (the providers that offer it).
    pub async fn add_comment(&self, hub: &Hub, task_id: &str, text: &str, mentions: &[Mention]) -> Result<()> {
        match self {
            Self::Clickup(c) => c.add_comment(task_id, text, mentions).await,
            Self::Obsidian(o) => o.add_comment(hub, task_id, text).await,
            Self::Jira(j) => j.add_comment(task_id, text).await,
        }
    }

    pub async fn edit_comment(&self, hub: &Hub, task_id: &str, comment: &str, text: &str, mentions: &[Mention]) -> Result<()> {
        match self {
            Self::Clickup(c) => c.edit_comment(hub, task_id, comment, text, mentions).await,
            Self::Obsidian(_) => bail!("{}", tr(NOT_ON_OBSIDIAN)),
            Self::Jira(j) => j.edit_comment(task_id, comment, text).await,
        }
    }

    /// Who can be mentioned on the tickets of a list (`Features::mentions`).
    pub async fn members(&self, list: &str) -> Result<Vec<crate::state::Person>> {
        match self {
            Self::Clickup(c) => c.members(list).await,
            Self::Obsidian(_) | Self::Jira(_) => bail!("{}", tr("mentions are not offered on this tracker")),
        }
    }

    /// Creates a ticket titled `title` on `board` (a sprint board's id), in `status`; returns its key.
    pub async fn create(&self, hub: &Hub, board: &str, status: &str, title: &str) -> Result<String> {
        match self {
            Self::Clickup(_) => bail!("{}", tr("creating ClickUp tickets is not offered yet")),
            Self::Obsidian(o) => o.create(hub, board, status, title).await,
            Self::Jira(j) => j.create(hub, board, status, title).await,
        }
    }

    /// Immediate, full re-read, without waiting for the interval; returns when it is done.
    pub async fn refresh(&self) -> Result<()> {
        match self {
            Self::Clickup(c) => c.refresh().await,
            Self::Obsidian(o) => o.refresh().await,
            Self::Jira(j) => j.refresh().await,
        }
    }
}
