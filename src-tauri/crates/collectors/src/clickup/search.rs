//! A search of the tickets by their id or name (the palette's): ClickUp's API has none, so the
//! tickets of the followed spaces, closed ones included, are read once and kept a while.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures::StreamExt;
use prism_base::i18n::tr;

use super::{Api, TasksResp, Workspace};
use crate::state::Ticket;

/// Read again after this long: a ticket created meanwhile is found within it.
const KEEP: Duration = Duration::from_secs(600);
/// Pages of 100 tickets read at most, so many at once: ClickUp allows 100 requests a minute per
/// token, which the tracker's own reads share (40 pages, for one key typed, could run them out).
const MAX_PAGES: usize = 20;
const AT_ONCE: usize = 4;

/// The tickets read and when, behind a lock that also makes a second search wait for the first read.
#[derive(Default)]
pub(super) struct Index(tokio::sync::Mutex<Option<(Instant, Vec<Ticket>)>>);

pub(super) async fn search(team: &Workspace, query: &str) -> Result<Vec<Ticket>> {
    if query.split_whitespace().next().is_none() {
        return Ok(Vec::new());
    }
    let mut index = team.index.0.lock().await;
    if index.as_ref().is_none_or(|(at, _)| at.elapsed() > KEEP) {
        let id = team.get().context(tr("ClickUp workspace not resolved yet"))?;
        let spaces = team.spaces.read().unwrap().clone();
        *index = Some((Instant::now(), read(&id, &spaces).await?));
    }
    Ok(crate::tracker::found(index.as_ref().unwrap().1.iter().cloned(), query))
}

/// The workspace's tickets, subtasks and closed ones included, in its followed spaces (all of
/// them when none is known), the latest changed first.
async fn read(team: &str, spaces: &[String]) -> Result<Vec<Ticket>> {
    let api = Api::new().await?;
    let filter: String = spaces.iter().map(|id| format!("&space_ids[]={id}")).collect();
    let mut tickets = Vec::new();
    for batch in (0..MAX_PAGES).collect::<Vec<_>>().chunks(AT_ONCE) {
        let pages: Vec<Result<TasksResp>> = futures::stream::iter(batch.iter().copied())
            .map(|page| {
                let (api, filter) = (&api, &filter);
                async move { api.get(&format!("/team/{team}/task?page={page}&subtasks=true&include_closed=true&order_by=updated{filter}")).await }
            })
            .buffered(AT_ONCE)
            .collect()
            .await;
        let mut last = false;
        for page in pages {
            let page = page?;
            last |= page.last_page.unwrap_or(true) || page.tasks.is_empty();
            tickets.extend(page.tasks.into_iter().map(Ticket::from));
        }
        if last {
            return Ok(tickets);
        }
    }
    tracing::warn!("clickup: more than {} tickets in the followed spaces: the search keeps the latest changed", MAX_PAGES * 100);
    Ok(tickets)
}
