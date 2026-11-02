//! The Day trail's collector: every `[trail] poll_seconds`, your new traces are added to the
//! journal (`trail.json` in the profile's data folder), the oldest beyond `keep_days` dropped.
//!
//! - commits: `git log --all` of each local clone, by your `user.email` (worktrees share it);
//! - the forge: your own events (GitLab's `events`, GitHub's `users/<me>/events`): MRs opened,
//!   merged, approved, commented; and your MRs the snapshot shows merged (by you or a queue);
//! - tickets: yours whose status changed since the last read, as the tracker shows them;
//! - meetings: today's agenda, once over.
//!
//! An event's id names what it is (`mr_merged:shop:1288`), so two sources of the same fact make one.

use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use anyhow::{Context, Result};
use prism_base::tr;
use chrono::{DateTime, Duration as Span, Utc};
use serde::{Deserialize, Serialize};

use super::{Kind, TrailConfig, TrailEvent, TrailState};
use crate::{BlockCtx, git::run as git, state::Snapshot};

static REFRESH: OnceLock<tokio::sync::Notify> = OnceLock::new();

/// Reads the sources again now (the window's ↻).
pub fn refresh() {
    if let Some(n) = REFRESH.get() {
        n.notify_one();
    }
}

/// What the collector keeps between two reads, and between two launches.
#[derive(Default, Serialize, Deserialize)]
struct Journal {
    events: Vec<TrailEvent>,
    /// The last status seen of each of your tickets, by its tracker id.
    #[serde(default)]
    statuses: BTreeMap<String, String>,
    /// GitLab projects by id: their path and web address.
    #[serde(default)]
    projects: BTreeMap<u64, Project>,
    /// When the forge's events were last read.
    #[serde(default)]
    forge_read: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Project {
    path: String,
    url: String,
}

pub async fn run(cfg: TrailConfig, ctx: BlockCtx) {
    let notify = REFRESH.get_or_init(tokio::sync::Notify::new);
    let path = ctx.data_dir().join("trail.json");
    let mut journal = load(&path);
    // The block knows it is on, with what it kept, before the first read.
    publish(&ctx, &cfg, &journal, None);
    let every = Duration::from_secs(cfg.poll_seconds.max(60));
    // The first read waits for the other sources (the forge's user, the clones, the tickets).
    tokio::time::sleep(Duration::from_secs(20)).await;
    loop {
        let view = ctx.snapshot(View::of);
        let now = Utc::now();
        let mut fresh = Vec::new();
        let mut errors = Vec::new();
        if cfg.commits {
            match commits(&view, cfg.keep_days).await {
                Ok(e) => fresh.extend(e),
                Err(e) => errors.push(format!("commits: {e:#}")),
            }
        }
        if cfg.forge && view.code {
            fresh.extend(merged_mrs(&view));
            match forge_events(&view, &mut journal, cfg.keep_days, now).await {
                Ok(e) => fresh.extend(e),
                Err(e) => errors.push(format!("forge events: {e:#}")),
            }
        }
        if cfg.tickets {
            fresh.extend(ticket_moves(&view, &mut journal.statuses, now));
        }
        if cfg.meetings {
            fresh.extend(meetings(&view, now));
        }
        merge(&mut journal.events, fresh, now - Span::days(cfg.keep_days.into()));
        if let Err(e) = save(&path, &journal) {
            tracing::warn!("trail: {e:#}");
        }
        publish(&ctx, &cfg, &journal, Some(now));
        ctx.set_error((!errors.is_empty()).then(|| errors.join(" · ")));
        tokio::select! {
            _ = ctx.pause(every) => {}
            _ = notify.notified() => {}
        }
    }
}

fn publish(ctx: &BlockCtx, cfg: &TrailConfig, journal: &Journal, read_at: Option<DateTime<Utc>>) {
    let sources = [("commits", cfg.commits), ("forge", cfg.forge), ("tickets", cfg.tickets), ("meetings", cfg.meetings)];
    ctx.publish(&TrailState {
        events: journal.events.clone(),
        gap_minutes: cfg.gap_minutes,
        sources: sources.iter().filter(|(_, on)| *on).map(|(s, _)| s.to_string()).collect(),
        read_at,
    });
}

/// Adds the events not known yet, drops those before `since`, oldest first.
fn merge(events: &mut Vec<TrailEvent>, fresh: Vec<TrailEvent>, since: DateTime<Utc>) {
    let mut known: HashSet<String> = events.iter().map(|e| e.id.clone()).collect();
    for e in fresh {
        if known.insert(e.id.clone()) {
            events.push(e);
        }
    }
    events.retain(|e| e.at >= since);
    events.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
}

fn load(path: &Path) -> Journal {
    std::fs::read(path).ok().and_then(|raw| serde_json::from_slice(&raw).map_err(|e| tracing::warn!("trail: {} unreadable: {e}", path.display())).ok()).unwrap_or_default()
}

/// Atomic: a temporary file then a rename.
fn save(path: &PathBuf, journal: &Journal) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(journal)?).with_context(|| tr!("writing {path}", path = tmp.display()))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// What a read needs of the snapshot.
struct View {
    /// The forge's user.
    me: Option<String>,
    /// The forge is read, and which one.
    code: bool,
    forge: crate::forge::Provider,
    /// Local clones: their repository and folder.
    clones: Vec<(String, String)>,
    /// Every repository's name and project (`group/project`).
    projects: Vec<(String, String)>,
    mrs: Vec<Mr>,
    /// Your tickets, from every board Prism reads.
    tickets: Vec<MyTicket>,
    meetings: Vec<crate::state::CalendarEvent>,
}

struct Mr {
    repo: String,
    iid: u64,
    /// By you, on its forge (your login there).
    mine: bool,
    title: String,
    ticket: Option<String>,
    url: String,
    merged_at: Option<DateTime<Utc>>,
}

struct MyTicket {
    task_id: String,
    id: String,
    name: String,
    status: String,
    url: String,
    updated_at: Option<DateTime<Utc>>,
}

impl View {
    fn of(s: &Snapshot) -> Self {
        let primary = s.repositories.iter().find(|r| r.primary).map(|r| r.name.clone()).unwrap_or_default();
        let mine = s.tracker_me.as_ref().map(|p| p.id);
        let mut seen = HashSet::new();
        let tickets = s
            .sprint
            .iter()
            .flat_map(|sp| sp.tickets.iter())
            .chain(s.next_sprint.iter().flat_map(|sp| sp.tickets.iter()))
            .chain(s.outside_sprint.tickets.iter())
            .filter(|t| mine.is_some_and(|me| t.assignees.iter().any(|a| a.id == me)))
            .filter(|t| seen.insert(t.task_id.clone()))
            .map(|t| MyTicket { task_id: t.task_id.clone(), id: t.id.clone(), name: t.name.clone(), status: t.status.clone(), url: t.url.clone(), updated_at: t.updated_at })
            .collect();
        View {
            me: s.me.clone(),
            code: s.gitlab,
            forge: s.forge,
            clones: s.repos.iter().filter(|r| r.error.is_none() && !r.path.is_empty()).map(|r| (r.name.clone(), r.path.clone())).collect(),
            projects: s.repositories.iter().filter_map(|r| Some((r.name.clone(), r.project.clone()?))).collect(),
            mrs: s
                .merge_requests
                .iter()
                .map(|m| Mr {
                    repo: m.repo.clone().unwrap_or_else(|| primary.clone()),
                    iid: m.iid,
                    mine: if m.forge.is_some() { &s.me_elsewhere } else { &s.me }.as_deref() == Some(m.author.as_str()),
                    title: m.title.clone(),
                    ticket: m.ticket.clone(),
                    url: m.url.clone(),
                    merged_at: m.merged_at,
                })
                .collect(),
            tickets,
            meetings: s.agenda.as_ref().map(|a| a.events.clone()).unwrap_or_default(),
        }
    }

    /// A project's repository name (`[[repos]] name`), or its path's last part.
    fn repo_of(&self, project: &str) -> String {
        self.projects.iter().find(|(_, p)| p.eq_ignore_ascii_case(project)).map(|(n, _)| n.clone()).unwrap_or_else(|| project.rsplit('/').next().unwrap_or(project).to_string())
    }

    fn mr(&self, repo: &str, iid: u64) -> Option<&Mr> {
        self.mrs.iter().find(|m| m.repo == repo && m.iid == iid)
    }

    /// An event on an MR: its repository, ticket, link and whether it is someone else's.
    #[allow(clippy::too_many_arguments)]
    fn mr_event(&self, kind: Kind, id: String, at: DateTime<Utc>, repo: String, iid: u64, title: &str, url: Option<String>) -> TrailEvent {
        let known = self.mr(&repo, iid);
        let others = match known {
            Some(m) => Some(!m.mine),
            // An MR one approves is someone else's.
            None => (kind == Kind::MrApproved).then_some(true),
        };
        TrailEvent {
            id,
            at,
            kind,
            title: known.map(|m| m.title.clone()).unwrap_or_else(|| title.to_string()),
            ticket: known.and_then(|m| m.ticket.clone()).or_else(|| crate::ticket::find([title])),
            url: known.map(|m| m.url.clone()).or(url),
            repo: Some(repo),
            mr: Some(iid),
            others,
            from: None,
            to: None,
            branch: None,
            minutes: None,
        }
    }
}

/* ---------- Commits ---------- */


/// Your commits of the last `days` days in every local clone, by its `user.email`.
async fn commits(view: &View, days: u32) -> Result<Vec<TrailEvent>> {
    let since = format!("--since={}", (Utc::now() - Span::days(days.into())).to_rfc3339());
    let mut out = Vec::new();
    let mut hashes = HashSet::new();
    for (repo, dir) in &view.clones {
        let email = git(dir, &["config", "user.email"]).await.unwrap_or_default().trim().to_string();
        if email.is_empty() {
            continue;
        }
        let author = format!("--author={email}");
        let log = git(dir, &["log", "--all", "--no-merges", "--source", &since, &author, "--format=%H%x1f%aI%x1f%s%x1f%S"]).await?;
        for line in log.lines() {
            let Some(e) = commit(repo, line) else { continue };
            if hashes.insert(e.id.clone()) {
                out.push(e);
            }
        }
    }
    Ok(out)
}

/// One line of `git log --format=%H%x1f%aI%x1f%s%x1f%S --source`.
fn commit(repo: &str, line: &str) -> Option<TrailEvent> {
    let mut parts = line.split('\x1f');
    let (hash, date, subject, source) = (parts.next()?, parts.next()?, parts.next()?, parts.next().unwrap_or(""));
    let at = DateTime::parse_from_rfc3339(date).ok()?.with_timezone(&Utc);
    let branch = source.trim_start_matches("refs/heads/").trim_start_matches("refs/remotes/").trim_start_matches("origin/").to_string();
    Some(TrailEvent {
        id: format!("commit:{hash}"),
        at,
        kind: Kind::Commit,
        title: subject.to_string(),
        ticket: crate::ticket::find([subject, branch.as_str()]),
        repo: Some(repo.to_string()),
        mr: None,
        others: None,
        url: None,
        from: None,
        to: None,
        branch: (!branch.is_empty()).then_some(branch),
        minutes: None,
    })
}

/* ---------- The forge ---------- */

/// Your MRs the snapshot shows merged: by you, a merge queue or someone else.
fn merged_mrs(view: &View) -> Vec<TrailEvent> {
    view.mrs
        .iter()
        .filter(|m| m.mine)
        .filter_map(|m| {
            let at = m.merged_at?;
            Some(view.mr_event(Kind::MrMerged, format!("mr_merged:{}:{}", m.repo, m.iid), at, m.repo.clone(), m.iid, &m.title, Some(m.url.clone())))
        })
        .collect()
}

async fn forge_events(view: &View, journal: &mut Journal, days: u32, now: DateTime<Utc>) -> Result<Vec<TrailEvent>> {
    let since = journal.forge_read.map(|t| t - Span::days(2)).unwrap_or(now - Span::days(days.into()));
    let events = match view.forge {
        crate::forge::Provider::Github => github(view, since).await?,
        _ => gitlab(view, journal, since).await?,
    };
    journal.forge_read = Some(now);
    Ok(events)
}

#[derive(Deserialize)]
struct GlEvent {
    id: u64,
    project_id: Option<u64>,
    action_name: String,
    target_type: Option<String>,
    target_iid: Option<u64>,
    target_title: Option<String>,
    created_at: DateTime<Utc>,
    note: Option<GlNote>,
}

#[derive(Deserialize)]
struct GlNote {
    noteable_type: Option<String>,
    noteable_iid: Option<u64>,
}

#[derive(Deserialize)]
struct GlProject {
    path_with_namespace: String,
    web_url: String,
}

/// Your events since `since` (`events?after=`, a day before it: the API counts whole days).
async fn gitlab(view: &View, journal: &mut Journal, since: DateTime<Utc>) -> Result<Vec<TrailEvent>> {
    let after = (since - Span::days(1)).format("%Y-%m-%d");
    let mut raw: Vec<GlEvent> = Vec::new();
    for page in 1..=10 {
        let batch: Vec<GlEvent> = crate::collectors::gitlab::glab(&["api".into(), format!("events?after={after}&per_page=100&page={page}")]).await?;
        let full = batch.len() == 100;
        raw.extend(batch);
        if !full {
            break;
        }
    }
    let mut out = Vec::new();
    for e in raw.into_iter().filter(|e| e.created_at >= since) {
        let on_note = e.note.as_ref().filter(|n| n.noteable_type.as_deref() == Some("MergeRequest"));
        let (kind, iid) = match (e.target_type.as_deref(), e.action_name.as_str()) {
            (Some("MergeRequest"), "opened") => (Kind::MrOpened, e.target_iid),
            (Some("MergeRequest"), "accepted") => (Kind::MrMerged, e.target_iid),
            (Some("MergeRequest"), "approved") => (Kind::MrApproved, e.target_iid),
            (_, "commented on") if on_note.is_some() => (Kind::MrCommented, on_note.and_then(|n| n.noteable_iid)),
            _ => continue,
        };
        let (Some(iid), Some(project)) = (iid, e.project_id) else { continue };
        let Some(p) = project_of(journal, project).await else { continue };
        let repo = view.repo_of(&p.path);
        let id = match kind {
            Kind::MrOpened => format!("mr_opened:{repo}:{iid}"),
            Kind::MrMerged => format!("mr_merged:{repo}:{iid}"),
            Kind::MrApproved => format!("mr_approved:{repo}:{iid}"),
            _ => format!("gitlab:{}", e.id),
        };
        let url = format!("{}/-/merge_requests/{iid}", p.url);
        out.push(view.mr_event(kind, id, e.created_at, repo, iid, e.target_title.as_deref().unwrap_or_default(), Some(url)));
    }
    Ok(out)
}

/// A GitLab project by id, read once then kept in the journal.
async fn project_of(journal: &mut Journal, id: u64) -> Option<Project> {
    if let Some(p) = journal.projects.get(&id) {
        return Some(p.clone());
    }
    let read: GlProject = crate::collectors::gitlab::glab(&["api".into(), format!("projects/{id}")]).await.map_err(|e| tracing::debug!("trail: project {id}: {e:#}")).ok()?;
    let p = Project { path: read.path_with_namespace, url: read.web_url };
    journal.projects.insert(id, p.clone());
    Some(p)
}

#[derive(Deserialize)]
struct GhEvent {
    id: String,
    r#type: String,
    created_at: DateTime<Utc>,
    repo: GhRepo,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Deserialize)]
struct GhRepo {
    name: String,
}

/// Your public and private events (`users/<me>/events`, the last 90 days at most).
async fn github(view: &View, since: DateTime<Utc>) -> Result<Vec<TrailEvent>> {
    let Some(me) = view.me.as_deref() else { return Ok(Vec::new()) };
    let mut raw: Vec<GhEvent> = Vec::new();
    for page in 1..=3 {
        let batch: Vec<GhEvent> = crate::collectors::github::api(&format!("users/{me}/events?per_page=100&page={page}")).await?;
        let more = batch.len() == 100 && batch.last().is_some_and(|e| e.created_at >= since);
        raw.extend(batch);
        if !more {
            break;
        }
    }
    let mut out = Vec::new();
    for e in raw.into_iter().filter(|e| e.created_at >= since) {
        let p = &e.payload;
        let pr = p.get("pull_request").or_else(|| p.get("issue").filter(|i| i.get("pull_request").is_some()));
        let Some(pr) = pr else { continue };
        let kind = match (e.r#type.as_str(), p.get("action").and_then(|a| a.as_str())) {
            ("PullRequestEvent", Some("opened")) => Kind::MrOpened,
            ("PullRequestEvent", Some("closed")) if pr.get("merged").and_then(|m| m.as_bool()) == Some(true) => Kind::MrMerged,
            ("PullRequestReviewEvent", _) if p.pointer("/review/state").and_then(|s| s.as_str()) == Some("approved") => Kind::MrApproved,
            ("PullRequestReviewEvent" | "PullRequestReviewCommentEvent" | "IssueCommentEvent", _) => Kind::MrCommented,
            _ => continue,
        };
        let Some(number) = pr.get("number").and_then(|n| n.as_u64()) else { continue };
        let repo = view.repo_of(&e.repo.name);
        let id = match kind {
            Kind::MrOpened => format!("mr_opened:{repo}:{number}"),
            Kind::MrMerged => format!("mr_merged:{repo}:{number}"),
            Kind::MrApproved => format!("mr_approved:{repo}:{number}"),
            _ => format!("github:{}", e.id),
        };
        let title = pr.get("title").and_then(|t| t.as_str()).unwrap_or_default();
        let url = pr.get("html_url").and_then(|u| u.as_str()).map(String::from);
        out.push(view.mr_event(kind, id, e.created_at, repo, number, title, url));
    }
    Ok(out)
}

/* ---------- Tickets and meetings ---------- */

/// Your tickets whose status changed since the last read; the first sight of one is only noted.
fn ticket_moves(view: &View, statuses: &mut BTreeMap<String, String>, now: DateTime<Utc>) -> Vec<TrailEvent> {
    let mut out = Vec::new();
    for t in &view.tickets {
        let Some(old) = statuses.insert(t.task_id.clone(), t.status.clone()) else { continue };
        if old.eq_ignore_ascii_case(&t.status) {
            continue;
        }
        // When the tracker says it changed, if that is recent; otherwise now.
        let at = t.updated_at.filter(|u| *u <= now && now - *u < Span::hours(12)).unwrap_or(now);
        out.push(TrailEvent {
            id: format!("move:{}:{}:{}", t.task_id, t.status, at.timestamp()),
            at,
            kind: Kind::TicketMoved,
            title: t.name.clone(),
            ticket: Some(t.id.clone()),
            repo: None,
            mr: None,
            others: None,
            url: Some(t.url.clone()),
            from: Some(old),
            to: Some(t.status.clone()),
            branch: None,
            minutes: None,
        });
    }
    out
}

/// Today's meetings already over; declined, canceled, free and all-day ones left out.
fn meetings(view: &View, now: DateTime<Utc>) -> Vec<TrailEvent> {
    view.meetings
        .iter()
        .filter(|e| !e.all_day && !e.canceled && !e.free && e.my_status.as_deref() != Some("declined") && e.end <= now)
        .map(|e| TrailEvent {
            id: format!("meeting:{}:{}", e.id, e.start.timestamp()),
            at: e.start,
            kind: Kind::Meeting,
            title: e.title.clone(),
            ticket: None,
            repo: None,
            mr: None,
            others: None,
            url: None,
            from: None,
            to: None,
            branch: None,
            minutes: Some(((e.end - e.start).num_minutes().max(0)) as u32),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, minutes_ago: i64) -> TrailEvent {
        let mut e = commit("shop", &format!("{id}\x1f2026-10-13T09:00:00+02:00\x1fwip\x1frefs/heads/main")).unwrap();
        e.id = id.into();
        e.at = Utc::now() - Span::minutes(minutes_ago);
        e
    }

    #[test]
    fn a_commit_line_is_read() {
        let e = commit("shop", "abc\x1f2026-10-13T09:15:00+02:00\x1ffix: round each line\x1frefs/remotes/origin/fix/x-433-vat").unwrap();
        assert_eq!(e.id, "commit:abc");
        assert_eq!(e.at.to_rfc3339(), "2026-10-13T07:15:00+00:00");
        assert_eq!(e.branch.as_deref(), Some("fix/x-433-vat"));
        assert_eq!(e.repo.as_deref(), Some("shop"));
        assert!(commit("shop", "garbage").is_none());
    }

    #[test]
    fn merging_keeps_one_per_id_and_drops_the_old() {
        let mut events = vec![event("a", 10), event("old", 60 * 24 * 40)];
        merge(&mut events, vec![event("a", 5), event("b", 1)], Utc::now() - Span::days(30));
        assert_eq!(events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    #[test]
    fn a_ticket_is_noted_then_its_moves_recorded() {
        let ticket = |status: &str| MyTicket { task_id: "t1".into(), id: "ABC-1".into(), name: "x".into(), status: status.into(), url: String::new(), updated_at: None };
        let view = |status: &str| View { me: None, code: false, forge: Default::default(), clones: vec![], projects: vec![], mrs: vec![], tickets: vec![ticket(status)], meetings: vec![] };
        let mut statuses = BTreeMap::new();
        let now = Utc::now();
        assert!(ticket_moves(&view("todo"), &mut statuses, now).is_empty());
        assert!(ticket_moves(&view("todo"), &mut statuses, now).is_empty());
        let moved = ticket_moves(&view("doing"), &mut statuses, now);
        assert_eq!((moved[0].from.as_deref(), moved[0].to.as_deref()), (Some("todo"), Some("doing")));
    }
}
