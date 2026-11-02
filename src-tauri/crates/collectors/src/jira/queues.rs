//! The slow queues, read after the sprint and every few minutes: To pick and Sync (the
//! `[[jira.queues]]` used so, and the next sprint), Refinement, and the board's issues in progress
//! outside the active sprint. Each queue is a JQL query, read through the search API.

use std::collections::{BTreeMap, HashSet};

use anyhow::Result;
use serde::Deserialize;

use super::{encode, Actions, Api, RawIssue, RawSprint, LIST_FIELDS, PAGE};
use crate::{
    clickup::refinement_tree,
    state::{ListRef, OutsideSprint, PickBoard, Pickup, QuarterBoard, Refinement, Sprint, StatusOption, Ticket},
};

/// At most this many pages of a query (of [`PAGE`] issues).
const MAX_PAGES: usize = 10;

pub(super) struct Queues {
    pub refinement: Refinement,
    pub pickup: Pickup,
    pub outside: OutsideSprint,
    pub next_sprint: Option<Sprint>,
}

/// What the sprint's pass learnt of the board.
pub(super) struct BoardRead<'a> {
    pub scrum: bool,
    pub name: &'a str,
    pub statuses: &'a [StatusOption],
    /// The active sprint, if any.
    pub sprint: Option<&'a Sprint>,
}

impl Actions {
    pub(super) async fn queues(&self, api: &Api, board: &BoardRead<'_>) -> Result<Queues> {
        let pick = crate::conventions::get().pick_statuses.iter().map(|s| s.to_lowercase()).collect::<Vec<_>>();
        let (pickup, refinement, outside, next_sprint) = tokio::try_join!(self.pickup(api, board, &pick), self.refinement(api, board), self.outside(api, board), self.next_sprint(api, board))?;
        let mut pickup = pickup;
        if let Some(next) = &next_sprint {
            let board = PickBoard {
                name: next.name.clone(),
                kind: "next_sprint".into(),
                tone: "info".into(),
                pick: true,
                sync: false,
                url: next.url.clone(),
                tickets: next.tickets.iter().filter(|t| !t.closed && pick.contains(&t.status)).cloned().collect(),
                statuses: next.statuses.clone(),
            };
            let at = pickup.boards.iter().take_while(|b| b.tone == "ko" || b.tone == "warn").count();
            pickup.boards.insert(at, board);
        }
        Ok(Queues { refinement, pickup, outside, next_sprint })
    }

    /// The `pick` and `sync` queues: a `sync` one whole, a `pick` one its issues ready to be picked.
    async fn pickup(&self, api: &Api, board: &BoardRead<'_>, pick: &[String]) -> Result<Pickup> {
        let used = self.shared.cfg.queues.iter().filter(|q| q.uses("pick") || q.uses("sync"));
        let boards = futures::future::try_join_all(used.map(|q| async move {
            let sync = q.uses("sync");
            let tickets = self.search(api, &q.jql).await?;
            anyhow::Ok(PickBoard {
                name: q.name.clone(),
                kind: "board".into(),
                tone: if q.tone.is_empty() { "info".into() } else { q.tone.clone() },
                pick: q.uses("pick"),
                sync,
                url: self.query_url(&q.jql),
                tickets: tickets.into_iter().filter(|t| !t.closed && (sync || pick.contains(&t.status))).collect(),
                statuses: board.statuses.to_vec(),
            })
        }))
        .await?;
        Ok(Pickup { statuses: pick.to_vec(), boards })
    }

    /// The `refinement` queues, arranged by epic as ClickUp's are: the issues in the Refinement
    /// statuses, the open epics and their children.
    async fn refinement(&self, api: &Api, board: &BoardRead<'_>) -> Result<Refinement> {
        let cfg = &self.shared.cfg;
        let read = futures::future::try_join_all(cfg.queues.iter().filter(|q| q.uses("refinement")).map(|q| async move { anyhow::Ok((q, self.search(api, &q.jql).await?)) })).await?;
        let statuses: Vec<String> = if cfg.refinement_statuses.is_empty() {
            // Every open status found, in the board's order, then the others as met.
            let found: Vec<&String> = read.iter().flat_map(|(_, ts)| ts.iter().filter(|t| !t.closed).map(|t| &t.status)).collect();
            let mut out: Vec<String> = board.statuses.iter().map(|s| &s.status).filter(|s| found.contains(s)).cloned().collect();
            for s in found {
                if !out.contains(s) {
                    out.push(s.clone());
                }
            }
            out
        } else {
            cfg.refinement_statuses.iter().map(|s| s.to_lowercase()).collect()
        };
        let boards = read
            .into_iter()
            .map(|(q, tickets)| QuarterBoard { folder: q.name.clone(), quarter: String::new(), url: self.query_url(&q.jql), tickets: refinement_tree(tickets, &statuses), statuses: board.statuses.to_vec() })
            .collect();
        Ok(Refinement { statuses, boards })
    }

    /// The board's issues in progress (or further) that are not in the active sprint. None without
    /// one: a Kanban board's are all in the Sprint block already.
    async fn outside(&self, api: &Api, board: &BoardRead<'_>) -> Result<OutsideSprint> {
        let cfg = &self.shared.cfg;
        let mut statuses: Vec<String> = cfg.ongoing_statuses.iter().map(|s| s.to_lowercase()).collect();
        let Some(sprint) = board.sprint else { return Ok(OutsideSprint { statuses, ..Default::default() }) };
        let jql = if statuses.is_empty() { "statusCategory = \"In Progress\"".to_string() } else { format!("status in ({})", statuses.iter().map(|s| format!("\"{}\"", s.replace('"', "\\\""))).collect::<Vec<_>>().join(", ")) };
        let in_sprint: HashSet<&str> = sprint.tickets.iter().map(|t| t.task_id.as_str()).collect();
        let list = ListRef { id: cfg.board.to_string(), name: board.name.to_string() };
        let tickets: Vec<Ticket> = self
            .issues(api, &format!("/rest/agile/1.0/board/{}/issue?jql={}", cfg.board, encode(&jql)))
            .await?
            .into_iter()
            .filter(|t| !in_sprint.contains(t.task_id.as_str()))
            .map(|t| Ticket { list: Some(list.clone()), ..t })
            .collect();
        if statuses.is_empty() {
            statuses = board.statuses.iter().map(|s| &s.status).filter(|s| tickets.iter().any(|t| &t.status == *s)).cloned().collect();
        }
        Ok(OutsideSprint { statuses, tickets, list_statuses: BTreeMap::from([(list.id, board.statuses.to_vec())]) })
    }

    /// The board's next sprint, all its issues: the first of its future sprints.
    async fn next_sprint(&self, api: &Api, board: &BoardRead<'_>) -> Result<Option<Sprint>> {
        if !board.scrum {
            return Ok(None);
        }
        let id = self.shared.cfg.board;
        let future: super::Page<RawSprint> = api.get(&format!("/rest/agile/1.0/board/{id}/sprint?state=future")).await?;
        let Some(s) = future.values.into_iter().next() else { return Ok(None) };
        let tickets = self.issues(api, &format!("/rest/agile/1.0/sprint/{}/issue", s.id)).await?;
        Ok(Some(Sprint {
            name: s.name,
            list_id: s.id.to_string(),
            url: format!("{}/secure/RapidBoard.jspa?rapidView={id}&view=planning", self.shared.cfg.base_url()),
            start: s.start_date.as_deref().and_then(super::date),
            end: s.end_date.as_deref().and_then(super::date),
            tickets,
            statuses: board.statuses.to_vec(),
            boards: Vec::new(),
        }))
    }

    /// Every issue a JQL query finds (up to [`MAX_PAGES`] pages), through the search API.
    pub(super) async fn search(&self, api: &Api, jql: &str) -> Result<Vec<Ticket>> {
        let base = self.shared.cfg.base_url();
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let next = token.as_deref().map(|t| format!("&nextPageToken={}", encode(t))).unwrap_or_default();
            let page: Found = api.get(&format!("/rest/api/3/search/jql?jql={}&fields={LIST_FIELDS}&maxResults={PAGE}{next}", encode(jql))).await?;
            out.extend(page.issues.into_iter().map(|i| i.ticket(&base)));
            match page.next_page_token {
                Some(t) if !page.is_last.unwrap_or(false) => token = Some(t),
                _ => return Ok(out),
            }
        }
        tracing::warn!("jira: \"{jql}\" finds more than {} issues: the first ones kept", MAX_PAGES * PAGE);
        Ok(out)
    }

    /// The query's results in Jira.
    fn query_url(&self, jql: &str) -> String {
        format!("{}/issues/?jql={}", self.shared.cfg.base_url(), encode(jql))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Found {
    #[serde(default)]
    pub(super) issues: Vec<RawIssue>,
    next_page_token: Option<String>,
    is_last: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::Found;

    #[test]
    fn a_search_page_says_whether_more_follow() {
        let page: Found = serde_json::from_value(serde_json::json!({
            "issues": [{ "key": "ABC-1", "fields": { "summary": "x", "status": { "name": "To Do", "statusCategory": { "key": "new" } } } }],
            "nextPageToken": "abc",
            "isLast": false
        }))
        .unwrap();
        assert_eq!((page.issues.len(), page.next_page_token.as_deref(), page.is_last), (1, Some("abc"), Some(false)));
        let last: Found = serde_json::from_value(serde_json::json!({ "issues": [], "isLast": true })).unwrap();
        assert_eq!((last.next_page_token, last.is_last), (None, Some(true)));
    }

    #[test]
    fn queues_are_read_from_the_config() {
        let cfg = crate::config::parsed("[tracker]\nprovider = \"jira\"\n[jira]\nsite = \"team.atlassian.net\"\nboard = 7\n[[jira.queues]]\nname = \"Bugs\"\njql = \"type = Bug\"\nuses = [\"pick\", \"sync\"]\ntone = \"ko\"\n");
        let q = &cfg.jira.queues[0];
        assert!(q.uses("pick") && q.uses("sync") && !q.uses("refinement"));
        assert_eq!((q.name.as_str(), q.tone.as_str(), cfg.jira.board), ("Bugs", "ko", 7));
    }
}
