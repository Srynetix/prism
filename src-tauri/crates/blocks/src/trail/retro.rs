//! My sprint: the current sprint seen from your side, for the retro. Your tickets (delivered,
//! carried and why, estimate against time), where your time went and how long you could focus,
//! from the Day trail's traces; your reviews, agents and notes tagged #retro. Compared with your
//! own past sprints only (`RetroSummary`, kept in the shared store `retro`), never with anyone
//! else: the team shows as one line. Facts Prism saw, not judgments. The window shows it
//! (`MySprint.svelte`, through `retro_get`); the routine `builtin:retro-draft` reads its Markdown.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Utc};
use prism_base::i18n::{lang, Lang};
use prism_base::{tr, i18n::tr};
use prism_core::state::{MergeRequest, MrState, PipelineStatus, Snapshot, Ticket};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Kind, TrailState};

const MIN: i64 = 60_000;
const DAY: i64 = 86_400_000;
/// A day with more ticket switches than this is flagged.
const SWITCHES: u32 = 8;
/// Time beyond this share of the estimate is flagged.
const OVERRUN: f64 = 1.5;
/// Meetings beyond this share of the tracked time are flagged.
const MEETINGS: f64 = 0.3;
/// Sprints kept to compare with.
const KEEP: usize = 8;

/// One of your tickets of the sprint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MyTicket {
    pub t: Ticket,
    pub done: bool,
    /// Minutes: the tracker's time estimate; logged; estimated from the traces.
    pub estimate: Option<i64>,
    pub logged: i64,
    pub traced: i64,
    /// From its first move into an in-progress status to done, as the trail saw them.
    pub cycle_days: Option<f64>,
    /// Carried: why, in a few words.
    pub why: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RetroDay {
    /// `YYYY-MM-DD`, local.
    pub day: String,
    /// Minutes: the longest stretch without a meeting between the day's first and last trace.
    pub focus: i64,
    pub switches: u32,
    pub meetings: i64,
    pub traced: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Observation {
    pub good: bool,
    pub text: String,
}

/// One sprint's figures, kept to compare the next ones with (the store `retro`, its `history`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RetroSummary {
    pub name: String,
    pub end: Option<DateTime<Utc>>,
    pub delivered: i64,
    pub committed: i64,
    /// Minutes traced, and in meetings.
    pub tracked: i64,
    pub meetings: i64,
    pub focus_median: i64,
    pub switches_median: i64,
    pub reviews: i64,
    pub agents: i64,
}

/// Minutes traced: on sprint tickets, on other tickets, on no ticket; in meetings.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RetroTime {
    pub sprint: i64,
    pub other: i64,
    pub none: i64,
    pub meetings: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RetroTeam {
    pub done: i64,
    pub total: i64,
}

/// A note tagged #retro, the tag left out.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RetroNote {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Retro {
    pub name: String,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    /// Day of the sprint, and how many it has.
    pub day: i64,
    pub days: i64,
    pub mine: Vec<MyTicket>,
    pub delivered: i64,
    pub time: RetroTime,
    pub daily: Vec<RetroDay>,
    /// Your approvals and comments on others' MRs.
    pub reviews: i64,
    /// Your MRs merged in the sprint, and those an agent worked on (its branch).
    pub merged: i64,
    pub by_agents: i64,
    pub agents: i64,
    pub cost: f64,
    pub notes: Vec<RetroNote>,
    pub observations: Vec<Observation>,
    pub team: RetroTeam,
    /// The trail is on: time, focus and reviews are known.
    pub traced: bool,
    pub summary: RetroSummary,
    /// The last sprint kept before this one: what the figures compare with.
    pub previous: Option<RetroSummary>,
    /// This sprint's figures are kept as they are now (`keep`).
    pub kept: bool,
    /// The retro as Markdown, with the points written for it (`markdown`).
    pub markdown: String,
}

/* ---------- Figures ---------- */

/// `45 min`, `3 h`, `3 h 20`.
pub fn hours(min: i64) -> String {
    if min < 60 {
        return format!("{min} min");
    }
    if min % 60 == 0 { format!("{} h", min / 60) } else { format!("{} h {:02}", min / 60, min % 60) }
}

pub fn median(xs: &[i64]) -> i64 {
    if xs.is_empty() {
        return 0;
    }
    let mut s = xs.to_vec();
    s.sort_unstable();
    let m = s.len() / 2;
    if s.len() % 2 == 1 { s[m] } else { ((s[m - 1] + s[m]) as f64 / 2.0).round() as i64 }
}

/// The longest stretch of `[from, to]` no meeting covers, in minutes.
pub fn longest_free(from: i64, to: i64, meetings: &[(i64, i64)]) -> i64 {
    let mut busy: Vec<(i64, i64)> = meetings.iter().copied().filter(|&(a, b)| b > from && a < to).collect();
    busy.sort_by_key(|&(a, _)| a);
    let (mut best, mut cursor) = (0, from);
    for (a, b) in busy {
        best = best.max(a - cursor);
        cursor = cursor.max(b);
    }
    best = best.max(to - cursor);
    (best.max(0) as f64 / MIN as f64).round() as i64
}

/// A trace as the retro reads it: the journal's, an agent launched, time logged.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    /// Milliseconds since the epoch.
    pub t: i64,
    pub kind: TraceKind,
    pub ticket: Option<String>,
    pub repo: Option<String>,
    pub mr: Option<u64>,
    pub others: Option<bool>,
    pub minutes: Option<i64>,
    /// When it ends: a meeting's, an agent's last activity, a time entry's.
    pub end: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    Trail(Kind),
    Agent,
    Time,
}

/// `YYYY-MM-DD` of a moment, local.
fn day_key(ms: i64) -> String {
    Local.timestamp_millis_opt(ms).single().map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// The traces of each of `days` (`YYYY-MM-DD`, local), oldest first: the journal's, the agents',
/// the time entries' (as the Day trail block shows them, `web/src/blocks/trail/trail.ts`).
pub fn traces_by_day(state: Option<&TrailState>, s: &Snapshot, days: &[String]) -> HashMap<String, Vec<Trace>> {
    let mut out: HashMap<String, Vec<Trace>> = days.iter().map(|d| (d.clone(), Vec::new())).collect();
    let mut add = |trace: Trace| {
        if let Some(list) = out.get_mut(&day_key(trace.t)) {
            list.push(trace);
        }
    };
    for e in state.map(|s| s.events.as_slice()).unwrap_or_default() {
        let t = e.at.timestamp_millis();
        let minutes = e.minutes.filter(|m| *m > 0).map(i64::from);
        add(Trace { t, kind: TraceKind::Trail(e.kind), ticket: e.ticket.clone(), repo: e.repo.clone(), mr: e.mr, others: e.others, minutes, end: minutes.map(|m| t + m * MIN) });
    }
    for a in &s.agents {
        let t = a.created_at.timestamp_millis();
        let last = a.last_activity.map_or(t, |l| l.timestamp_millis());
        add(Trace { t, kind: TraceKind::Agent, ticket: a.ticket.clone(), repo: None, mr: None, others: None, minutes: None, end: Some(t.max(last)) });
    }
    for e in &s.time.entries {
        let t = e.start.timestamp_millis();
        add(Trace { t, kind: TraceKind::Time, ticket: e.ticket.clone(), repo: None, mr: None, others: None, minutes: Some((e.duration_ms as f64 / MIN as f64).round() as i64), end: Some(t + e.duration_ms) });
    }
    for list in out.values_mut() {
        list.sort_by_key(|tr| tr.t);
    }
    out
}

const MEETING: TraceKind = TraceKind::Trail(Kind::Meeting);
/// The time a trace claims before it: since the previous one, at most this.
const CLAIM_MAX: i64 = 45;
/// The day's first trace, a lone one: this much.
const CLAIM_FIRST: i64 = 15;

/// Minutes by ticket (`""`: on no ticket), as the Day trail estimates them: each trace claims the
/// time since the previous one (at most 45 minutes, meetings excluded), an agent its own run;
/// logged time apart. And the meetings' minutes.
pub fn by_ticket(list: &[Trace]) -> (BTreeMap<String, i64>, i64) {
    let mut traced: BTreeMap<String, i64> = BTreeMap::new();
    let mut meetings = 0;
    let mut prev: Option<i64> = None;
    for tr in list {
        if tr.kind == MEETING {
            meetings += tr.minutes.unwrap_or(0);
            prev = Some(prev.unwrap_or(0).max(tr.end.unwrap_or(tr.t)));
            continue;
        }
        let g = traced.entry(tr.ticket.clone().unwrap_or_default()).or_default();
        if tr.kind == TraceKind::Time {
            continue;
        }
        // An agent is two traces: its launch, and its last activity (it may run without you).
        let ats = match tr.end {
            Some(end) if tr.kind == TraceKind::Agent && end - tr.t > 5 * MIN => vec![tr.t, end],
            _ => vec![tr.t],
        };
        for at in ats {
            *g += match prev {
                None => CLAIM_FIRST,
                Some(p) => ((at - p) as f64 / MIN as f64).round().clamp(5.0, CLAIM_MAX as f64) as i64,
            };
            prev = Some(prev.unwrap_or(0).max(at));
        }
    }
    (traced, meetings)
}

/// What a trace is about, to count switches: its ticket, else its MR, else nothing.
fn subject(tr: &Trace) -> Option<String> {
    tr.ticket.clone().or_else(|| tr.mr.map(|mr| format!("{}!{mr}", tr.repo.as_deref().unwrap_or_default())))
}

/// A day's focus, switches, meetings and time traced; none without any trace of work.
pub fn day_figures(list: &[Trace], day: &str) -> Option<RetroDay> {
    let work: Vec<&Trace> = list.iter().filter(|tr| tr.kind != MEETING && tr.kind != TraceKind::Time).collect();
    if work.is_empty() {
        return None;
    }
    let meetings: Vec<(i64, i64)> = list.iter().filter(|tr| tr.kind == MEETING).map(|tr| (tr.t, tr.end.unwrap_or(tr.t + tr.minutes.unwrap_or(0) * MIN))).collect();
    let from = work.iter().map(|tr| tr.t).min().unwrap_or_default();
    let to = work.iter().map(|tr| if tr.kind == TraceKind::Agent { tr.t } else { tr.end.unwrap_or(tr.t) }).max().unwrap_or_default();
    let mut switches = 0;
    let mut last: Option<String> = None;
    for tr in &work {
        let Some(s) = subject(tr) else { continue };
        if last.as_ref().is_some_and(|l| *l != s) {
            switches += 1;
        }
        last = Some(s);
    }
    let (traced, met) = by_ticket(list);
    Some(RetroDay { day: day.to_string(), focus: longest_free(from, to, &meetings), switches, meetings: met, traced: traced.values().sum() })
}

/* ---------- The retro ---------- */

fn lower(xs: &[String]) -> HashSet<String> {
    xs.iter().map(|x| x.to_lowercase()).collect()
}

/// Finished ticket: closed, or in an end status (`[tracker] done_statuses`).
fn is_done(t: &Ticket, s: &Snapshot) -> bool {
    t.closed || s.conventions.done_statuses.contains(&t.status)
}

/// The MR names that ticket, among others maybe.
fn names_ticket(m: &MergeRequest, id: &str) -> bool {
    if m.tickets.is_empty() { m.ticket.as_deref() == Some(id) } else { m.tickets.iter().any(|t| t == id) }
}

/// Why a ticket was carried: blocked, never started, red CI, otherwise its status.
fn carried_why(t: &Ticket, s: &Snapshot) -> String {
    let waiting: Vec<&str> = t.waiting_on.iter().filter(|d| !d.closed).map(|d| if d.id.is_empty() { d.name.as_str() } else { d.id.as_str() }).collect();
    if !waiting.is_empty() {
        return tr!("blocked by {list}", list = waiting.join(", "));
    }
    if lower(&s.conventions.before_dev_statuses).contains(&t.status.to_lowercase()) {
        return tr("never started").to_string();
    }
    match s.merge_requests.iter().find(|m| names_ticket(m, &t.id) && m.state == MrState::Opened) {
        Some(m) if m.pipeline == Some(PipelineStatus::Failed) => tr("red CI").to_string(),
        Some(_) => tr("MR in review").to_string(),
        None => t.status.clone(),
    }
}

/// The tags of a note's text (`#infra`, `#perso/admin`), lower case: a letter first.
fn tags_of(text: &str) -> Vec<String> {
    static TAG: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?:^|\s)#(\p{L}[\p{L}\p{N}_/-]*)").expect("regex"));
    TAG.captures_iter(text).map(|c| c[1].trim_end_matches(['/', '-']).to_lowercase()).collect()
}

/// A note's text without its #retro tag.
fn without_retro(text: &str) -> String {
    static RETRO: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)\s*#retro\b").expect("regex"));
    RETRO.replace_all(text, "").trim().to_string()
}

/// The notes tagged #retro since `start`, from the store `notes` (the window's shape).
fn retro_notes(notes: &[Value], start: DateTime<Utc>) -> Vec<RetroNote> {
    notes
        .iter()
        .filter_map(|n| {
            let (id, text) = (n.get("id")?.as_str()?, n.get("text")?.as_str()?);
            let at = n.get("created_at").and_then(Value::as_str).and_then(|a| DateTime::parse_from_rfc3339(a).ok())?;
            (tags_of(text).iter().any(|t| t == "retro") && at >= start).then(|| RetroNote { id: id.to_string(), text: without_retro(text) })
        })
        .collect()
}

/// The current sprint's retro, `now`; none without a sprint that has a start date. `markdown` and
/// `kept` are left to `markdown` and the store (`kept_in`).
pub fn retro(s: &Snapshot, trail: Option<&TrailState>, notes: &[Value], history: &[RetroSummary], now: DateTime<Utc>) -> Option<Retro> {
    let sprint = s.sprint.as_ref()?;
    let start_at = sprint.start?;
    let (start, now_ms) = (start_at.timestamp_millis(), now.timestamp_millis());
    let end = sprint.end.map_or(now_ms, |e| e.timestamp_millis());
    let until = now_ms.min(end);
    let mine_t: Vec<&Ticket> = match &s.tracker_me {
        Some(me) => sprint.tickets.iter().filter(|t| t.assignees.iter().any(|a| a.id == me.id)).collect(),
        None => Vec::new(),
    };
    let sprint_ids: HashSet<&str> = sprint.tickets.iter().map(|t| t.id.as_str()).collect();

    // The trail, day by day over the sprint so far.
    let mut time = RetroTime::default();
    let mut traced_by: HashMap<String, i64> = HashMap::new();
    let mut daily = Vec::new();
    let mut reviews = 0;
    let mut days: Vec<String> = Vec::new();
    let mut d = start;
    while d <= until {
        let key = day_key(d);
        if !days.contains(&key) {
            days.push(key);
        }
        d += DAY;
    }
    let by_day = traces_by_day(trail, s, &days);
    for day in &days {
        let list: Vec<Trace> = by_day.get(day).map(|l| l.iter().filter(|tr| tr.t >= start && tr.t <= until).cloned().collect()).unwrap_or_default();
        if list.is_empty() {
            continue;
        }
        let (tickets, meetings) = by_ticket(&list);
        time.meetings += meetings;
        for (ticket, traced) in tickets {
            if ticket.is_empty() {
                time.none += traced;
                continue;
            }
            if sprint_ids.contains(ticket.as_str()) { time.sprint += traced } else { time.other += traced }
            *traced_by.entry(ticket).or_default() += traced;
        }
        reviews += list.iter().filter(|tr| matches!(tr.kind, TraceKind::Trail(Kind::MrApproved | Kind::MrCommented)) && tr.others == Some(true)).count() as i64;
        // Focus and switches need the trail's own traces: agents alone would tell a wrong day.
        if trail.is_some() {
            daily.extend(day_figures(&list, day));
        }
    }

    // A ticket's cycle: its first move into progress, then into done, as the trail recorded them.
    let progress = lower(&s.conventions.in_progress_statuses);
    let done_st = lower(&s.conventions.done_statuses);
    let cycle = |id: &str| -> Option<f64> {
        let mut moves: Vec<_> = trail.map(|t| t.events.iter().filter(|e| e.kind == Kind::TicketMoved && e.ticket.as_deref() == Some(id)).collect()).unwrap_or_default();
        moves.sort_by_key(|e| e.at);
        let to_in = |e: &&&super::TrailEvent, set: &HashSet<String>| e.to.as_ref().is_some_and(|to| set.contains(&to.to_lowercase()));
        let from = moves.iter().find(|e| to_in(e, &progress))?;
        let to = moves.iter().rev().find(|e| to_in(e, &done_st))?;
        (to.at > from.at).then(|| (((to.at - from.at).num_milliseconds() as f64 / DAY as f64) * 10.0).round() / 10.0)
    };
    let logged = |id: &str| -> i64 {
        let ms: i64 = s.time.entries.iter().filter(|e| e.ticket.as_deref() == Some(id) && e.start.timestamp_millis() >= start).map(|e| e.duration_ms).sum();
        (ms as f64 / MIN as f64).round() as i64
    };
    let mut mine: Vec<MyTicket> = mine_t
        .into_iter()
        .map(|t| {
            let done = is_done(t, s);
            MyTicket {
                t: t.clone(),
                done,
                estimate: t.time_estimate.filter(|e| *e > 0).map(|e| (e as f64 / MIN as f64).round() as i64),
                logged: logged(&t.id),
                traced: traced_by.get(&t.id).copied().unwrap_or(0),
                cycle_days: if done { cycle(&t.id) } else { None },
                why: if done { None } else { Some(carried_why(t, s)) },
            }
        })
        .collect();
    mine.sort_by(|a, b| b.done.cmp(&a.done).then_with(|| a.t.id.cmp(&b.t.id)));
    let delivered = mine.iter().filter(|m| m.done).count() as i64;

    let me_on = |m: &MergeRequest| if m.forge.is_some() { s.me_elsewhere.as_deref() } else { s.me.as_deref() };
    let merged: Vec<&MergeRequest> = s.merge_requests.iter().filter(|m| m.state == MrState::Merged && me_on(m) == Some(m.author.as_str()) && m.merged_at.is_some_and(|at| at.timestamp_millis() >= start)).collect();
    let agents: Vec<_> = s.agents.iter().filter(|a| a.created_at.timestamp_millis() >= start).collect();
    let branches: HashSet<&str> = agents.iter().filter_map(|a| a.branch.as_deref()).filter(|b| !b.is_empty()).collect();
    let by_agents = merged.iter().filter(|m| branches.contains(m.source_branch.as_str())).count() as i64;
    let cost: f64 = agents.iter().filter_map(|a| a.usage.as_ref().and_then(|u| u.cost)).sum();
    let team_done = sprint.tickets.iter().filter(|t| is_done(t, s)).count() as i64;

    let tracked = time.sprint + time.other + time.none;
    let focus: Vec<i64> = daily.iter().map(|d: &RetroDay| d.focus).collect();
    let switches: Vec<i64> = daily.iter().map(|d| i64::from(d.switches)).collect();
    let summary = RetroSummary {
        name: sprint.name.clone(),
        end: sprint.end,
        delivered,
        committed: mine.len() as i64,
        tracked,
        meetings: time.meetings,
        focus_median: median(&focus),
        switches_median: median(&switches),
        reviews,
        agents: agents.len() as i64,
    };
    let previous = history.iter().rfind(|h| h.name != sprint.name).cloned();
    let observations = observe(&Seen { mine: &mine, daily: &daily, time: &time, tracked, merged: merged.len() as i64, by_agents, summary: &summary, previous: previous.as_ref() });
    let days = ((end - start) as f64 / DAY as f64).round().max(1.0) as i64;
    Some(Retro {
        name: sprint.name.clone(),
        start: start_at,
        end: sprint.end,
        day: days.min(((now_ms - start).div_euclid(DAY) + 1).max(1)),
        days,
        mine,
        delivered,
        time,
        daily,
        reviews,
        merged: merged.len() as i64,
        by_agents,
        agents: agents.len() as i64,
        cost,
        notes: retro_notes(notes, start_at),
        observations,
        team: RetroTeam { done: team_done, total: sprint.tickets.len() as i64 },
        traced: trail.is_some(),
        kept: history.contains(&summary),
        summary,
        previous,
        markdown: String::new(),
    })
}

/// `Monday 6 Oct`, `lundi 6 oct.`: a day (`YYYY-MM-DD`) in the messages' language.
fn weekday(day: &str) -> String {
    let Ok(d) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else { return day.to_string() };
    let (w, m) = (d.weekday().num_days_from_monday() as usize, d.month0() as usize);
    match lang() {
        Lang::Fr => {
            const DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
            const MONTHS: [&str; 12] = ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
            format!("{} {} {}", DAYS[w], d.day(), MONTHS[m])
        }
        Lang::En => {
            const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            format!("{} {} {}", DAYS[w], d.day(), MONTHS[m])
        }
    }
}

/// `n` takes the singular in the messages' language: 1 in English, 0 and 1 in French.
fn single(n: i64) -> bool {
    if lang() == Lang::Fr { n.abs() < 2 } else { n == 1 }
}

struct Seen<'a> {
    mine: &'a [MyTicket],
    daily: &'a [RetroDay],
    time: &'a RetroTime,
    tracked: i64,
    merged: i64,
    by_agents: i64,
    summary: &'a RetroSummary,
    previous: Option<&'a RetroSummary>,
}

fn observe(x: &Seen) -> Vec<Observation> {
    let mut out = Vec::new();
    let mut push = |good: bool, text: String| out.push(Observation { good, text });
    let now = x.summary;
    if let Some(p) = x.previous {
        let d = now.delivered - p.delivered;
        let (sprint, n, before) = (&p.name, now.delivered, p.delivered);
        if d > 0 {
            push(true, if single(d) { tr!("{n} ticket more delivered than in {sprint} ({now} against {before}).", n = d, sprint = sprint, now = n, before = before) } else { tr!("{n} tickets more delivered than in {sprint} ({now} against {before}).", n = d, sprint = sprint, now = n, before = before) });
        } else if d < 0 {
            push(false, if single(-d) { tr!("{n} ticket fewer delivered than in {sprint} ({now} against {before}).", n = -d, sprint = sprint, now = n, before = before) } else { tr!("{n} tickets fewer delivered than in {sprint} ({now} against {before}).", n = -d, sprint = sprint, now = n, before = before) });
        }
        let f = now.focus_median - p.focus_median;
        if f.abs() >= 20 {
            let (by, median) = (hours(f.abs()), hours(now.focus_median));
            push(f > 0, if f > 0 { tr!("Your median longest focus grew by {by} ({median} a day).", by = by, median = median) } else { tr!("Your median longest focus shrank by {by} ({median} a day).", by = by, median = median) });
        }
        if (now.reviews - p.reviews).abs() >= 3 {
            let n = now.reviews;
            push(n > p.reviews, if single(n) { tr!("{n} review given, against {before} in {sprint}.", n = n, before = p.reviews, sprint = p.name) } else { tr!("{n} reviews given, against {before} in {sprint}.", n = n, before = p.reviews, sprint = p.name) });
        }
    }
    for m in x.mine {
        let spent = m.logged.max(m.traced);
        if let Some(estimate) = m.estimate.filter(|e| *e > 0 && spent as f64 > *e as f64 * OVERRUN) {
            push(false, tr!("{id} took {spent} for {estimate} estimated.", id = m.t.id, spent = hours(spent), estimate = hours(estimate)));
        }
    }
    for m in x.mine.iter().filter(|m| !m.done).take(3) {
        push(false, tr!("{id} carried: {why}.", id = m.t.id, why = m.why.as_deref().unwrap_or_default()));
    }
    // The first day of the most switches, of the longest focus.
    let busiest = x.daily.iter().fold(None::<&RetroDay>, |b, d| if b.is_none_or(|b| d.switches > b.switches) { Some(d) } else { b });
    if let Some(b) = busiest.filter(|b| b.switches >= SWITCHES) {
        push(false, tr!("{day}: {n} switches between tickets and MRs.", day = weekday(&b.day), n = b.switches));
    }
    let best = x.daily.iter().fold(None::<&RetroDay>, |b, d| if b.is_none_or(|b| d.focus > b.focus) { Some(d) } else { b });
    if let Some(b) = best.filter(|b| b.focus >= 120) {
        push(true, tr!("Your longest focus: {time} on {day}.", time = hours(b.focus), day = weekday(&b.day)));
    }
    let all = x.tracked + x.time.meetings;
    if all > 0 && x.time.meetings as f64 / all as f64 > MEETINGS {
        push(false, tr!("Meetings took {n} % of your tracked time.", n = (100.0 * x.time.meetings as f64 / all as f64).round()));
    }
    if x.merged > 0 && x.by_agents > 0 {
        push(true, tr!("Agents worked on {n} of your {merged} merged MRs.", n = x.by_agents, merged = x.merged));
    }
    out
}

/* ---------- Points and Markdown ---------- */

/// A point for the retro, written in My sprint (`web/src/retro-points.svelte.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetroPoint {
    pub id: String,
    pub text: String,
    pub kind: PointKind,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    Keep,
    Change,
    Idea,
}

/// `### My points` in Markdown, by kind; nothing without points.
pub fn points_markdown(points: &[RetroPoint]) -> Vec<String> {
    if points.is_empty() {
        return Vec::new();
    }
    let mut out = vec![String::new(), tr("### My points").to_string()];
    for (kind, label) in [(PointKind::Keep, tr("To keep")), (PointKind::Change, tr("To change")), (PointKind::Idea, tr("Idea"))] {
        let list: Vec<&RetroPoint> = points.iter().filter(|p| p.kind == kind).collect();
        if !list.is_empty() {
            out.push(format!("**{label}**"));
            out.extend(list.iter().map(|p| format!("- {}", p.text)));
        }
    }
    out
}

/// The retro as Markdown, with the points written for it: to paste in the team's retro, or to hand to an agent.
pub fn markdown(r: &Retro, points: &[RetroPoint]) -> String {
    let mut lines = vec![tr!("## My sprint · {name}", name = r.name), String::new(), tr!("- Delivered {n} of my {total} tickets (team: {done}/{all}).", n = r.delivered, total = r.mine.len(), done = r.team.done, all = r.team.total)];
    if r.traced {
        let t = &r.time;
        if t.sprint + t.other + t.none + t.meetings > 0 {
            lines.push(tr!("- Time traced: {sprint} on sprint tickets, {other} on others, {none} on no ticket, {meetings} in meetings.", sprint = hours(t.sprint), other = hours(t.other), none = hours(t.none), meetings = hours(t.meetings)));
        }
        if !r.daily.is_empty() {
            let focus: Vec<i64> = r.daily.iter().map(|d| d.focus).collect();
            let switches: Vec<i64> = r.daily.iter().map(|d| i64::from(d.switches)).collect();
            lines.push(tr!("- Longest focus, median: {focus} a day; switches, median: {switches} a day.", focus = hours(median(&focus)), switches = median(&switches)));
        }
        lines.push(tr!("- Reviews given: {n}.", n = r.reviews));
    }
    if r.agents > 0 {
        lines.push(tr!("- Agents: {n} launched, ${cost}; {by} of my {merged} merged MRs.", n = r.agents, cost = format!("{:.2}", r.cost), by = r.by_agents, merged = r.merged));
    }
    let carried: Vec<&MyTicket> = r.mine.iter().filter(|m| !m.done).collect();
    if !carried.is_empty() {
        lines.extend([String::new(), tr("### Carried").to_string()]);
        lines.extend(carried.iter().map(|m| format!("- {} {}: {}", m.t.id, m.t.name, m.why.as_deref().unwrap_or_default())));
    }
    if !r.observations.is_empty() {
        lines.extend([String::new(), tr("### Seen by Prism").to_string()]);
        lines.extend(r.observations.iter().map(|o| format!("- {} {}", if o.good { '▲' } else { '▼' }, o.text)));
    }
    lines.extend(points_markdown(points));
    if !r.notes.is_empty() {
        lines.extend([String::new(), tr("### My notes").to_string()]);
        lines.extend(r.notes.iter().map(|n| format!("- {}", n.text)));
    }
    lines.join("\n")
}

/* ---------- The store `retro` ---------- */

/// The points of `sprint` in the store `retro` (`{ sprints: { <name>: [point…] }, history }`), in
/// the order written; a kind unknown counts as an idea, a point without its text is left out.
pub fn points_of(store: Option<&Value>, sprint: &str) -> Vec<RetroPoint> {
    let list = store.and_then(|s| s.get("sprints")).and_then(|s| s.get(sprint)).and_then(Value::as_array);
    list.into_iter()
        .flatten()
        .filter_map(|p| {
            let kind = serde_json::from_value(p.get("kind").cloned().unwrap_or_default()).unwrap_or(PointKind::Idea);
            let text = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
            Some(RetroPoint { id: text("id")?, text: text("text")?, kind, created_at: text("created_at").unwrap_or_default() })
        })
        .collect()
}

/// The sprints kept in the store `retro`, oldest first.
pub fn history_of(store: Option<&Value>) -> Vec<RetroSummary> {
    let list = store.and_then(|s| s.get("history")).and_then(Value::as_array);
    list.into_iter().flatten().filter_map(|h| serde_json::from_value(h.clone()).ok()).collect()
}

/// The store `retro` with `summary` kept in its history (the last `KEEP` sprints, a sprint's last
/// figures replacing its former ones), and `migrated` as its history when it has none yet (the
/// window's former pref `retro.history`); none when nothing changes.
pub fn kept_in(store: Option<Value>, summary: Option<&RetroSummary>, migrated: Option<Vec<RetroSummary>>) -> Option<Value> {
    let mut store = match store {
        Some(Value::Object(o)) => o,
        _ => serde_json::Map::new(),
    };
    let had = store.contains_key("history");
    let mut history = if had { history_of(Some(&Value::Object(store.clone()))) } else { migrated.clone().unwrap_or_default() };
    let mut changed = !had && migrated.is_some_and(|m| !m.is_empty());
    if let Some(s) = summary.filter(|s| !history.contains(s)) {
        history.retain(|h| h.name != s.name);
        history.push(s.clone());
        let drop = history.len().saturating_sub(KEEP);
        history.drain(..drop);
        changed = true;
    }
    if !changed {
        return None;
    }
    store.insert("history".into(), serde_json::to_value(history).ok()?);
    Some(Value::Object(store))
}

/// A write of the store `retro` that does not say its history (a points' write, the window's or a
/// phone's): the history kept as stored.
pub fn with_history(stored: Option<&Value>, mut value: Value) -> Value {
    if let (Some(o), Some(h)) = (value.as_object_mut(), stored.and_then(|s| s.get("history"))) {
        o.entry("history").or_insert_with(|| h.clone());
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::state::{Dependency, Person, Sprint};

    fn at(day: &str, hm: &str) -> i64 {
        let d = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        let (h, m) = hm.split_once(':').unwrap();
        Local.from_local_datetime(&d.and_hms_opt(h.parse().unwrap(), m.parse().unwrap(), 0).unwrap()).single().unwrap().timestamp_millis()
    }

    fn t(hm: &str) -> i64 {
        at("2026-10-06", hm)
    }

    fn trace(hm: &str, kind: TraceKind, ticket: Option<&str>, minutes: Option<i64>) -> Trace {
        let t = t(hm);
        Trace { t, kind, ticket: ticket.map(Into::into), repo: None, mr: None, others: None, minutes, end: minutes.map(|m| t + m * MIN) }
    }

    const COMMIT: TraceKind = TraceKind::Trail(Kind::Commit);

    #[test]
    fn formats_and_averages() {
        assert_eq!(hours(45), "45 min");
        assert_eq!(hours(120), "2 h");
        assert_eq!(hours(200), "3 h 20");
        assert_eq!(hours(65), "1 h 05");
        assert_eq!(median(&[5, 1, 3]), 3);
        assert_eq!(median(&[1, 2]), 2);
        assert_eq!(median(&[]), 0);
    }

    #[test]
    fn finds_the_longest_stretch_without_a_meeting() {
        assert_eq!(longest_free(t("09:00"), t("17:00"), &[(t("10:00"), t("11:00")), (t("14:00"), t("14:30"))]), 180);
        assert_eq!(longest_free(t("09:00"), t("10:00"), &[]), 60);
    }

    #[test]
    fn counts_switches_between_subjects_not_traces() {
        let list = [trace("09:00", COMMIT, Some("A"), None), trace("09:30", COMMIT, Some("A"), None), trace("10:00", MEETING, None, Some(60)), trace("11:30", COMMIT, Some("B"), None), trace("12:00", COMMIT, Some("A"), None)];
        let d = day_figures(&list, "2026-10-06").unwrap();
        assert_eq!((d.switches, d.meetings, d.focus), (2, 60, 60));
        assert!(day_figures(&[trace("10:00", MEETING, None, Some(30))], "2026-10-06").is_none(), "no work, no day");
    }

    #[test]
    fn claims_time_by_ticket() {
        let list = [trace("09:00", COMMIT, Some("A"), None), trace("09:30", COMMIT, Some("A"), None), trace("12:00", COMMIT, None, None), trace("12:01", TraceKind::Time, Some("A"), Some(90))];
        let (traced, meetings) = by_ticket(&list);
        assert_eq!((traced["A"], traced[""], meetings), (15 + 30, 45, 0));
    }

    fn ticket(id: &str, status: &str, me: &Person) -> Ticket {
        Ticket {
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
            name: format!("{id} name"),
            status: status.into(),
            closed: false,
            assignees: vec![me.clone()],
            url: String::new(),
            created_at: None,
            updated_at: None,
            fields: Vec::new(),
            waiting_on: Vec::new(),
            blocking: Vec::new(),
        }
    }

    fn snapshot() -> Snapshot {
        let me = Person { id: 1, name: "Me".into(), initials: "M".into(), color: None, avatar: None };
        let mut b = ticket("B", "in progress", &me);
        b.waiting_on = vec![Dependency { id: "X-1".into(), ..Dependency::unknown("x1") }];
        let mut c = ticket("C", "to do", &me);
        c.assignees.clear();
        let mut s = Snapshot { sprint: Some(Sprint {
            name: "S42".into(),
            list_id: String::new(),
            url: String::new(),
            start: Some("2026-09-29T00:00:00Z".parse().unwrap()),
            end: Some("2026-10-10T00:00:00Z".parse().unwrap()),
            tickets: vec![ticket("A", "done", &me), b, c],
            statuses: Vec::new(),
            boards: Vec::new(),
        }), tracker_me: Some(me), me: Some("me".into()), ..Default::default() };
        s.conventions.done_statuses = vec!["done".into()];
        s.conventions.in_progress_statuses = vec!["in progress".into()];
        s.conventions.before_dev_statuses = vec!["to do".into()];
        s
    }

    #[test]
    fn splits_my_tickets_into_delivered_and_carried_with_why() {
        let s = snapshot();
        let now = "2026-10-08T12:00:00Z".parse().unwrap();
        let notes = [serde_json::json!({ "id": "n1", "text": "Too many meetings #retro", "created_at": "2026-10-01T09:00:00Z" }), serde_json::json!({ "id": "n2", "text": "#retro old", "created_at": "2026-09-01T09:00:00Z" }), serde_json::json!({ "id": "n3", "text": "no tag", "created_at": "2026-10-01T09:00:00Z" })];
        let r = retro(&s, Some(&TrailState::default()), &notes, &[], now).unwrap();
        let mine: Vec<(&str, bool, Option<&str>)> = r.mine.iter().map(|m| (m.t.id.as_str(), m.done, m.why.as_deref())).collect();
        assert_eq!(mine, [("A", true, None), ("B", false, Some("blocked by X-1"))]);
        assert_eq!((r.team.done, r.team.total, r.delivered), (1, 3, 1));
        assert!(r.observations.iter().any(|o| o.text.contains("B carried: blocked by X-1")));
        assert_eq!((r.day, r.days), (10, 11));
        assert_eq!(r.notes, [RetroNote { id: "n1".into(), text: "Too many meetings".into() }]);
        assert!(r.traced && r.previous.is_none() && !r.kept);
        assert!(retro(&Snapshot::default(), None, &[], &[], now).is_none(), "no sprint");
    }

    #[test]
    fn compares_with_the_last_sprint_kept() {
        let s = snapshot();
        let now = "2026-10-08T12:00:00Z".parse().unwrap();
        let before = RetroSummary { name: "S41".into(), delivered: 3, reviews: 6, ..Default::default() };
        let r = retro(&s, None, &[], &[before.clone(), RetroSummary { name: "S42".into(), ..Default::default() }], now).unwrap();
        assert_eq!(r.previous.as_ref(), Some(&before));
        assert!(!r.traced && r.daily.is_empty());
        assert!(r.observations.iter().any(|o| !o.good && o.text == "2 tickets fewer delivered than in S41 (1 against 3)."), "{:?}", r.observations);
        assert!(r.observations.iter().any(|o| !o.good && o.text == "0 reviews given, against 6 in S41."));
        let kept = kept_in(None, Some(&r.summary), None).unwrap();
        assert_eq!(history_of(Some(&kept)), std::slice::from_ref(&r.summary));
        assert!(kept_in(Some(kept.clone()), Some(&r.summary), None).is_none(), "the same figures: nothing to write");
    }

    #[test]
    fn writes_markdown() {
        let s = snapshot();
        let mut r = retro(&s, None, &[], &[], "2026-10-08T12:00:00Z".parse().unwrap()).unwrap();
        r.notes = vec![RetroNote { id: "n".into(), text: "Pair more".into() }];
        let md = markdown(&r, &[]);
        assert!(md.starts_with("## My sprint · S42\n\n- Delivered 1 of my 2 tickets (team: 1/3)."), "{md}");
        assert!(md.contains("### Carried\n- B B name: blocked by X-1") && md.contains("### My notes\n- Pair more") && !md.contains("Reviews given"));
    }

    #[test]
    fn groups_the_points_by_kind_and_says_nothing_without_any() {
        let p = |text: &str, kind: PointKind| RetroPoint { id: text.into(), text: text.into(), kind, created_at: "2026-10-15T10:00:00Z".into() };
        assert!(points_markdown(&[]).is_empty());
        let points = [p("Pairing on reviews", PointKind::Keep), p("Too many meetings", PointKind::Change), p("A demo every Friday", PointKind::Idea), p("Async standup", PointKind::Keep)];
        assert_eq!(points_markdown(&points), ["", "### My points", "**To keep**", "- Pairing on reviews", "- Async standup", "**To change**", "- Too many meetings", "**Idea**", "- A demo every Friday"]);
    }

    #[test]
    fn reads_and_keeps_the_store() {
        let store = serde_json::json!({ "sprints": { "S1": [{ "id": "a", "text": "x", "kind": "odd", "created_at": "" }, { "id": "b" }] } });
        assert_eq!(points_of(Some(&store), "S1").iter().map(|p| (p.id.as_str(), p.kind)).collect::<Vec<_>>(), [("a", PointKind::Idea)]);
        let old = |name: &str| RetroSummary { name: name.into(), ..Default::default() };
        // The window's former history moves in once; the store's own wins after.
        let migrated = kept_in(Some(store.clone()), None, Some(vec![old("S0")])).unwrap();
        assert_eq!(history_of(Some(&migrated)), [old("S0")]);
        assert!(migrated["sprints"]["S1"].is_array(), "the points stay");
        assert!(kept_in(Some(migrated.clone()), None, Some(vec![old("S9")])).is_none());
        let mut many = migrated;
        for i in 1..=10 {
            many = kept_in(Some(many), Some(&old(&format!("S{i}"))), None).unwrap();
        }
        assert_eq!(history_of(Some(&many)).iter().map(|h| h.name.clone()).collect::<Vec<_>>(), (3..=10).map(|i| format!("S{i}")).collect::<Vec<_>>());
        // A points' write keeps the history.
        let written = with_history(Some(&many), serde_json::json!({ "sprints": {} }));
        assert_eq!(history_of(Some(&written)).len(), 8);
        // The pref's shape, from the window.
        let pref: RetroSummary = serde_json::from_value(serde_json::json!({ "name": "S", "end": null, "delivered": 2, "committed": 3, "tracked": 10, "meetings": 1, "focusMedian": 90, "switchesMedian": 4, "reviews": 5, "agents": 1 })).unwrap();
        assert_eq!((pref.focus_median, pref.switches_median), (90, 4));
    }
}
