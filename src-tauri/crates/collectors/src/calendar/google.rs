//! The agenda from Google Calendar (`[calendar] provider = "google"`), on any system: the events
//! of the day through the Calendar API, read-only (`calendar.calendarlist.readonly`,
//! `calendar.events.readonly`), signed in with the profile's Google account (`google`). The
//! calendars read are those listed in `[calendar] calendars`, else those checked in Google
//! Calendar. An event's id is its calendar's and its own, a space between them (neither has one).

use prism_base::i18n::tr;
use std::{sync::LazyLock, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use serde::Deserialize;

use super::{join_url, Attendee, EventDetails};
use crate::google::{self, Google};
use crate::{
    config::{CalendarConfig, CalendarProvider},
    state::{Agenda, CalendarEvent, Hub},
};

const SOURCE: &str = "calendar";
const API: &str = "https://www.googleapis.com/calendar/v3";
const POLL: Duration = Duration::from_secs(60);

static WAKE: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);

/// Reads the agenda now (a sign-in or out just happened).
pub fn wake() {
    WAKE.notify_one();
}

/// Reads today's events now, then every minute or when woken.
pub async fn run(cfg: CalendarConfig, hub: Hub) {
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = hub.pause(POLL) => {}
                _ = WAKE.notified() => {}
            }
        }
        hub.publish(SOURCE, read(&cfg).await, |s, agenda| crate::state::replace(&mut s.agenda, Some(agenda)));
    }
}

async fn read(cfg: &CalendarConfig) -> Result<Agenda> {
    let g = google::get()?;
    let Some(token) = g.access_token().await? else {
        return Ok(Agenda { provider: CalendarProvider::Google, access: "signed_out".into(), events: Vec::new() });
    };
    let events = events_of(g, &token, cfg, Local::now().date_naive()).await?;
    Ok(Agenda { provider: CalendarProvider::Google, access: "granted".into(), events })
}

/// The events of `day`, signed in.
pub async fn day(day: NaiveDate, cfg: &CalendarConfig) -> Result<Vec<CalendarEvent>> {
    let (g, token) = signed_in().await?;
    events_of(g, &token, cfg, day).await
}

/// The names of your calendars, sorted, for Settings to pick from; none signed out.
pub async fn names() -> Result<Vec<String>> {
    let Ok(g) = google::get() else { return Ok(Vec::new()) };
    let Some(token) = g.access_token().await? else { return Ok(Vec::new()) };
    let mut names: Vec<String> = calendars(g, &token).await?.into_iter().map(|c| c.name()).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    Ok(names)
}

pub async fn details(id: &str) -> Result<EventDetails> {
    let (g, token) = signed_in().await?;
    let (calendar, e) = event(g, &token, id).await?;
    let base = e.clone().into_event(&calendar).context("unreadable event")?;
    let people: Vec<Attendee> = e.attendees.iter().map(RawAttendee::attendee).collect();
    let organizer = e.organizer.as_ref().map(|o| {
        // The organizer as an attendee, with their response, when they are one.
        people.iter().find(|p| p.email.is_some() && p.email == o.email).cloned().unwrap_or(Attendee {
            name: o.display_name.clone().or_else(|| o.email.clone()).unwrap_or_else(|| "?".into()),
            email: o.email.clone(),
            status: "accepted".into(),
            optional: false,
            me: o.is_self,
        })
    });
    Ok(EventDetails {
        title: base.title,
        start: base.start,
        end: base.end,
        all_day: base.all_day,
        location: base.location,
        calendar: base.calendar,
        organizer,
        attendees: people,
        notes: e.description.as_deref().map(text).filter(|n| !n.trim().is_empty()),
        url: None,
        join_url: base.join_url,
        my_status: base.my_status,
        canceled: base.canceled,
    })
}

/// Opens the event in Google Calendar on the web, where the invitation can be answered.
pub async fn open(id: &str) -> Result<()> {
    let (g, token) = signed_in().await?;
    let (_, e) = event(g, &token, id).await?;
    let link = e.html_link.context("Google gave no link to this event")?;
    anyhow::ensure!(["https://www.google.com/calendar/", "https://calendar.google.com/"].iter().any(|p| link.starts_with(p)), "not a Google Calendar link");
    crate::platform::open_url(&link)
}

async fn signed_in() -> Result<(&'static Google, String)> {
    let g = google::get()?;
    let token = g.access_token().await?.context(tr("not signed in to Google: Settings › Work › Calendar › Google Calendar"))?;
    Ok((g, token))
}

/// Event `id` and the name of its calendar.
async fn event(g: &Google, token: &str, id: &str) -> Result<(String, RawEvent)> {
    let (calendar, event) = id.split_once(' ').context("not a Google Calendar event id")?;
    let e: RawEvent = g.get(token, &format!("{API}/calendars/{}/events/{}", segment(calendar), segment(event))).await?;
    let name = calendars(g, token).await.ok().and_then(|list| list.into_iter().find(|c| c.id == calendar)).map(|c| c.name()).unwrap_or_default();
    Ok((name, e))
}

async fn calendars(g: &Google, token: &str) -> Result<Vec<RawCalendar>> {
    let list: CalendarList = g.get(token, &format!("{API}/users/me/calendarList?maxResults=250")).await?;
    Ok(list.items)
}

async fn events_of(g: &Google, token: &str, cfg: &CalendarConfig, day: NaiveDate) -> Result<Vec<CalendarEvent>> {
    let start = Local.from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap()).earliest().context("no midnight that day")?;
    let end = start + chrono::Duration::days(1);
    let shown: Vec<RawCalendar> = calendars(g, token).await?.into_iter().filter(|c| if cfg.calendars.is_empty() { c.selected } else { cfg.shows(&c.name()) }).collect();
    let reads = shown.iter().map(|c| {
        let url = format!(
            "{API}/calendars/{}/events?singleEvents=true&orderBy=startTime&maxResults=250&timeMin={}&timeMax={}",
            segment(&c.id),
            segment(&start.to_rfc3339()),
            segment(&end.to_rfc3339())
        );
        async move { g.get::<Events>(token, &url).await.map(|e| (c, e.items)) }
    });
    let mut out: Vec<CalendarEvent> = Vec::new();
    for (c, items) in futures::future::try_join_all(reads).await? {
        let name = c.name();
        out.extend(items.into_iter().filter_map(|e| e.into_event_of(&c.id, &name)));
    }
    out.sort_by_key(|a| (!a.all_day, a.start));
    Ok(out)
}

/// A path segment, percent-encoded (a calendar id holds `@` and `#`).
fn segment(s: &str) -> String {
    crate::http::encode(s)
}

/// An event's description as plain text: Google writes it in HTML.
fn text(html: &str) -> String {
    static BREAK: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)<br\s*/?>|</(p|div|li|h\d)>").unwrap());
    static TAG: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"<[^>]*>").unwrap());
    let broken = BREAK.replace_all(html, "\n");
    let bare = TAG.replace_all(&broken, "");
    bare.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&").trim().to_string()
}

#[derive(Deserialize)]
struct CalendarList {
    #[serde(default)]
    items: Vec<RawCalendar>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCalendar {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    summary_override: Option<String>,
    /// Checked in Google Calendar's list.
    #[serde(default)]
    selected: bool,
}

impl RawCalendar {
    /// Its name as you see it: your own, else its owner's.
    fn name(&self) -> String {
        self.summary_override.clone().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| self.summary.clone())
    }
}

#[derive(Deserialize)]
struct Events {
    #[serde(default)]
    items: Vec<RawEvent>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    html_link: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    start: Option<When>,
    end: Option<When>,
    #[serde(default)]
    transparency: String,
    #[serde(default)]
    hangout_link: Option<String>,
    #[serde(default)]
    conference_data: Option<Conference>,
    #[serde(default)]
    attendees: Vec<RawAttendee>,
    #[serde(default)]
    organizer: Option<Person>,
    /// The calendar it was read from (`into_event_of`), for its id.
    #[serde(skip)]
    calendar_id: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct When {
    date_time: Option<DateTime<Utc>>,
    /// An all-day event's day.
    date: Option<NaiveDate>,
}

impl When {
    fn at(&self) -> Option<DateTime<Utc>> {
        self.date_time.or_else(|| Some(Local.from_local_datetime(&self.date?.and_hms_opt(0, 0, 0)?).earliest()?.with_timezone(&Utc)))
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Conference {
    #[serde(default)]
    entry_points: Vec<EntryPoint>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryPoint {
    #[serde(default)]
    entry_point_type: String,
    #[serde(default)]
    uri: String,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAttendee {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    response_status: String,
    #[serde(default)]
    optional: bool,
    #[serde(default, rename = "self")]
    is_self: bool,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Person {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default, rename = "self")]
    is_self: bool,
}

/// `accepted`, `tentative`, `declined`, `pending`, as EventKit's are.
fn status(s: &str) -> &'static str {
    match s {
        "accepted" => "accepted",
        "declined" => "declined",
        "tentative" => "tentative",
        _ => "pending",
    }
}

impl RawAttendee {
    fn attendee(&self) -> Attendee {
        Attendee {
            name: self.display_name.clone().or_else(|| self.email.clone()).unwrap_or_else(|| "?".into()),
            email: self.email.clone(),
            status: status(&self.response_status).into(),
            optional: self.optional,
            me: self.is_self,
        }
    }
}

impl RawEvent {
    fn into_event_of(mut self, calendar_id: &str, calendar: &str) -> Option<CalendarEvent> {
        self.calendar_id = calendar_id.to_string();
        self.into_event(calendar)
    }

    fn into_event(self, calendar: &str) -> Option<CalendarEvent> {
        let start = self.start.as_ref()?.at()?;
        let end = self.end.as_ref().and_then(When::at).unwrap_or(start);
        let all_day = self.start.as_ref().is_some_and(|s| s.date_time.is_none());
        let location = self.location.filter(|l| !l.trim().is_empty());
        let video = self.conference_data.as_ref().and_then(|c| c.entry_points.iter().find(|p| p.entry_point_type == "video")).map(|p| p.uri.clone());
        // The event's own links too: https only (an organizer writes them; a `obsidian://` or
        // `file:` one would open as the meeting).
        let https = |u: String| u.starts_with("https://").then_some(u);
        let join = self.hangout_link.and_then(https).or(video.and_then(https)).or_else(|| join_url([location.as_deref(), self.description.as_deref()].into_iter().flatten()));
        Some(CalendarEvent {
            id: format!("{} {}", self.calendar_id, self.id),
            title: self.summary.unwrap_or_default(),
            start,
            end,
            all_day,
            location,
            calendar: calendar.to_string(),
            my_status: self.attendees.iter().find(|a| a.is_self).map(|a| status(&a.response_status).into()),
            canceled: self.status == "cancelled",
            free: self.transparency == "transparent",
            join_url: join,
            attendees: self.attendees.len() as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: &str = r#"{"items":[
        {"id":"ev1","status":"confirmed","summary":"Standup","start":{"dateTime":"2026-10-13T09:30:00+02:00"},"end":{"dateTime":"2026-10-13T09:45:00+02:00"},
         "hangoutLink":"https://meet.google.com/abc-defg-hij","attendees":[{"email":"me@example.org","self":true,"responseStatus":"needsAction"},{"email":"ann@example.org","responseStatus":"accepted"}]},
        {"id":"ev2","status":"confirmed","summary":"Holiday","start":{"date":"2026-10-13"},"end":{"date":"2026-10-14"},"transparency":"transparent"},
        {"id":"ev3","status":"cancelled","summary":"Call","location":"https://acme.zoom.us/j/123456","start":{"dateTime":"2026-10-13T14:00:00Z"},"end":{"dateTime":"2026-10-13T15:00:00Z"}},
        {"id":"ev4","status":"confirmed","summary":"Trap","start":{"dateTime":"2026-10-13T16:00:00Z"},"end":{"dateTime":"2026-10-13T17:00:00Z"},
         "conferenceData":{"entryPoints":[{"entryPointType":"video","uri":"obsidian://new?file=x&overwrite=true"}]}}
    ]}"#;

    #[test]
    fn reads_google_events() {
        let items: Events = serde_json::from_str(EVENTS).unwrap();
        let e: Vec<CalendarEvent> = items.items.into_iter().filter_map(|e| e.into_event_of("me@example.org", "Perso")).collect();
        assert_eq!(e[0].id, "me@example.org ev1");
        assert_eq!(e[0].start.to_rfc3339(), "2026-10-13T07:30:00+00:00");
        assert_eq!((e[0].my_status.as_deref(), e[0].attendees, e[0].join_url.as_deref()), (Some("pending"), 2, Some("https://meet.google.com/abc-defg-hij")));
        assert!(e[1].all_day && e[1].free && !e[1].canceled && e[1].end - e[1].start == chrono::Duration::days(1));
        assert!(e[2].canceled);
        assert_eq!(e[2].join_url.as_deref(), Some("https://acme.zoom.us/j/123456"));
        assert_eq!(e[3].join_url, None, "a video link that is not https");
    }

    #[test]
    fn descriptions_read_as_text() {
        assert_eq!(text("Agenda:<br>1. <b>Q4</b> &amp; plans<br/>2. Next"), "Agenda:\n1. Q4 & plans\n2. Next");
    }

    #[test]
    fn calendar_ids_are_encoded() {
        assert_eq!(segment("en.french#holiday@group.v.calendar.google.com"), "en.french%23holiday%40group.v.calendar.google.com");
    }
}
