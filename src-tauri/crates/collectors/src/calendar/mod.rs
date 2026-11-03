//! Today's agenda, read from the macOS Calendar (EventKit): the Exchange / Microsoft
//! 365 accounts added to it are included. No token nor network call: macOS asks once
//! for permission to access the calendars. Only the displayed fields are kept (not the
//! notes, from which only the video call link is drawn). With `[calendar] provider =
//! "google"`, Google Calendar's instead, on any system (`gcal`).

use prism_base::{i18n::tr, tr};
use std::{
    sync::LazyLock,
    time::Duration,
};

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use regex::Regex;
use crate::{
    config::{CalendarConfig, CalendarProvider},
    state::{CalendarEvent, Hub},
};

mod google;

/// The agenda changes little: re-read every minute, and on opening.
const POLL: Duration = Duration::from_secs(60);

/// Details of an event, read on demand when it is opened: notes and attendees are
/// never kept in the snapshot.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EventDetails {
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub location: Option<String>,
    pub calendar: String,
    pub organizer: Option<Attendee>,
    pub attendees: Vec<Attendee>,
    /// Invitation text (often the body of the Outlook email).
    pub notes: Option<String>,
    pub url: Option<String>,
    pub join_url: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "'accepted' | 'tentative' | 'declined' | 'pending' | null"))]
    pub my_status: Option<String>,
    pub canceled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Attendee {
    pub name: String,
    pub email: Option<String>,
    /// `accepted`, `tentative`, `declined`, `pending`.
    #[cfg_attr(feature = "ts", ts(type = "'accepted' | 'tentative' | 'declined' | 'pending'"))]
    pub status: String,
    pub optional: bool,
    pub me: bool,
}

/// Events of a day other than the snapshot's (the dashboard's date picker).
pub async fn day(day: NaiveDate) -> anyhow::Result<Vec<CalendarEvent>> {
    let cfg = CONFIG.get().cloned().unwrap_or_default();
    anyhow::ensure!(cfg.enabled, "{}", tr("the agenda is disabled in the config"));
    if cfg.provider == CalendarProvider::Google {
        return google::day(day, &cfg).await;
    }
    tokio::task::spawn_blocking(move || platform::day_events(day, &cfg)).await?
}

/// The names of the calendars (macOS Calendar's, or Google's), sorted, for Settings to pick from; none
/// without access.
pub async fn names() -> anyhow::Result<Vec<String>> {
    if provider() == CalendarProvider::Google {
        return google::names().await;
    }
    tokio::task::spawn_blocking(platform::names).await?
}

/// Details of event `id` (as in the snapshot: EventKit id `@` start, or Google's).
pub async fn details(id: String) -> anyhow::Result<EventDetails> {
    if provider() == CalendarProvider::Google {
        return google::details(&id).await;
    }
    tokio::task::spawn_blocking(move || platform::details(&id)).await?
}

/// Opens the event in the Calendar app (Google Calendar on the web), where the invitation can be answered.
pub async fn open_in_calendar(id: String) -> anyhow::Result<()> {
    if provider() == CalendarProvider::Google {
        return google::open(&id).await;
    }
    let ident = id.rsplit_once('@').map(|(i, _)| i).unwrap_or(&id);
    if ident.is_empty() || !ident.chars().all(|c| c.is_ascii_alphanumeric() || "-:_.%+=/".contains(c)) {
        anyhow::bail!("not a calendar event id");
    }
    anyhow::ensure!(cfg!(target_os = "macos"), "{}", tr("the agenda reads the macOS Calendar"));
    crate::platform::open_url(&format!("ical://ekevent/{ident}?method=show&options=more")).map_err(|e| anyhow::anyhow!(tr!("Calendar could not open the event: {err}", err = format!("{e:#}"))))
}

static JOIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https://(?:teams\.microsoft\.com/(?:l/meetup-join|meet)/|[\w-]+\.zoom\.us/[jw]/|meet\.google\.com/)[^\s<>"')\]]+"#).unwrap()
});

/// First video call link in these texts.
pub fn join_url<'a>(texts: impl IntoIterator<Item = &'a str>) -> Option<String> {
    texts.into_iter().find_map(|t| JOIN.find(t)).map(|m| m.as_str().to_string())
}

/// The config of the collector running, for on-demand reads (the calendars read).
static CONFIG: prism_base::rt::Swap<CalendarConfig> = prism_base::rt::Swap::new();

fn provider() -> CalendarProvider {
    CONFIG.get().map(|c| c.provider).unwrap_or_default()
}

/// Reads the agenda now, when Google's (a sign-in or out just happened).
pub fn wake() {
    google::wake();
}

pub fn run(cfg: CalendarConfig, hub: Hub) {
    CONFIG.set(cfg.clone());
    if !cfg.enabled {
        return;
    }
    if cfg.provider == CalendarProvider::Google {
        prism_base::rt::spawn(google::run(cfg, hub));
        return;
    }
    // EventKit is not thread-safe: a dedicated thread holds its event store, until its collectors
    // stop (a change of the config).
    let generation = prism_base::rt::Generation::current();
    let alive = move || !generation.as_ref().is_some_and(prism_base::rt::Generation::cancelled);
    std::thread::Builder::new()
        .name("calendar".into())
        .spawn(move || platform::run(cfg, hub, POLL, alive))
        .expect("calendar thread");
}

#[cfg(target_os = "macos")]
mod platform {
    use prism_base::i18n::tr;
    use std::time::Duration;

    use anyhow::Context;
    use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
    use prism_macos::eventkit::{self, Access, Event, Participant, Store};

    use super::{join_url, Attendee, EventDetails};
    use crate::{
        config::CalendarConfig,
        state::{Agenda, CalendarEvent, Hub},
    };

    pub fn run(cfg: CalendarConfig, hub: Hub, every: Duration, alive: impl Fn() -> bool) {
        let store = Store::new();
        while alive() {
            let agenda = match access(&store) {
                access if access == "granted" => {
                    // A permission just granted: the store must reload its sources.
                    store.reset();
                    Agenda { access, events: events_of(&store, &cfg, Local::now().date_naive()), ..Default::default() }
                }
                access => Agenda { access, ..Default::default() },
            };
            if !alive() {
                return;
            }
            hub.update(|s| {
                let changed = s.agenda.as_ref() != Some(&agenda);
                s.agenda = Some(agenda);
                changed
            });
            let until = std::time::Instant::now() + every;
            while alive() && std::time::Instant::now() < until {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }

    /// Permission state, requested only once (macOS does not ask again after a refusal).
    fn access(store: &Store) -> String {
        match eventkit::access() {
            Access::Granted => "granted".into(),
            Access::Denied => "denied".into(),
            Access::Restricted => "restricted".into(),
            Access::Unknown => {
                // The answer arrives when the user has clicked: we wait for it, without a useful limit.
                let granted = store.request_access(Duration::from_secs(600));
                tracing::info!("calendar: access {}", if granted { "granted" } else { "refused" });
                if granted { "granted" } else { "denied" }.into()
            }
        }
    }

    pub fn details(id: &str) -> anyhow::Result<EventDetails> {
        let (ident, ts) = id.rsplit_once('@').context("not a calendar event id")?;
        let ts: i64 = ts.parse().context("not a calendar event id")?;
        if eventkit::access() != Access::Granted {
            anyhow::bail!("{}", tr("no access to Calendar"));
        }
        let store = Store::new();
        // The exact occurrence of a recurring event: the one starting at that time.
        let e = store
            .events(ts as f64 - 60.0, ts as f64 + 86_400.0)
            .into_iter()
            .find(|e| e.id().is_some_and(|i| i == ident) && e.start() as i64 == ts)
            .context(tr("event not found (moved or deleted?)"))?;
        let base = event(&e).context("unreadable event")?;
        Ok(EventDetails {
            title: base.title,
            start: base.start,
            end: base.end,
            all_day: base.all_day,
            location: base.location,
            calendar: base.calendar,
            organizer: e.organizer().map(|p| attendee(&p)),
            attendees: e.attendees().map(|list| list.iter().map(attendee).collect()).unwrap_or_default(),
            notes: e.notes().filter(|n| !n.trim().is_empty()),
            url: e.url(),
            join_url: base.join_url,
            my_status: base.my_status,
            canceled: base.canceled,
        })
    }

    fn attendee(p: &Participant) -> Attendee {
        let url = p.url();
        let email = url.strip_prefix("mailto:").map(String::from);
        Attendee {
            name: p.name().or_else(|| email.clone()).unwrap_or_else(|| "?".into()),
            email,
            status: p.answer().word().into(),
            optional: p.optional(),
            me: p.me(),
        }
    }

    pub fn names() -> anyhow::Result<Vec<String>> {
        if eventkit::access() != Access::Granted {
            return Ok(Vec::new());
        }
        let mut names = Store::new().calendar_names();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        Ok(names)
    }

    pub fn day_events(day: NaiveDate, cfg: &CalendarConfig) -> anyhow::Result<Vec<CalendarEvent>> {
        if eventkit::access() != Access::Granted {
            anyhow::bail!("{}", tr("no access to Calendar"));
        }
        Ok(events_of(&Store::new(), cfg, day))
    }

    fn events_of(store: &Store, cfg: &CalendarConfig, day: NaiveDate) -> Vec<CalendarEvent> {
        let now = Local::now();
        let midnight = day.and_hms_opt(0, 0, 0).unwrap();
        let start = Local.from_local_datetime(&midnight).earliest().unwrap_or(now);
        let end = start + chrono::Duration::days(1);
        let mut out: Vec<CalendarEvent> = store
            .events(start.timestamp() as f64, end.timestamp() as f64)
            .iter()
            .filter_map(event)
            .filter(|e| cfg.shows(&e.calendar))
            .collect();
        out.sort_by_key(|a| (!a.all_day, a.start));
        out
    }

    fn event(e: &Event) -> Option<CalendarEvent> {
        let utc = |secs: f64| DateTime::<Utc>::from_timestamp(secs as i64, 0);
        let start = utc(e.start())?;
        let end = utc(e.end())?;
        let location = e.location().filter(|l| !l.trim().is_empty());
        let notes = e.notes();
        let url = e.url();
        let attendees = e.attendees();
        let my_status = attendees.as_ref().and_then(|list| list.iter().find(|p| p.me()).map(|p| p.answer().word()));
        Some(CalendarEvent {
            // An occurrence of a recurring event shares the series' id.
            id: format!("{}@{}", e.id().unwrap_or_default(), start.timestamp()),
            join_url: join_url([url.as_deref(), location.as_deref(), notes.as_deref()].into_iter().flatten()),
            title: e.title(),
            start,
            end,
            all_day: e.all_day(),
            location,
            calendar: e.calendar().unwrap_or_default(),
            my_status: my_status.map(String::from),
            canceled: e.canceled(),
            free: e.free(),
            attendees: attendees.map(|a| a.len() as u32).unwrap_or(0),
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use prism_base::i18n::tr;
    use std::time::Duration;

    use crate::{
        config::CalendarConfig,
        state::{Agenda, Hub},
    };

    pub fn day_events(_day: chrono::NaiveDate, _cfg: &CalendarConfig) -> anyhow::Result<Vec<crate::state::CalendarEvent>> {
        anyhow::bail!("{}", tr("the agenda reads the macOS Calendar"))
    }

    pub fn details(_id: &str) -> anyhow::Result<super::EventDetails> {
        anyhow::bail!("{}", tr("the agenda reads the macOS Calendar"))
    }

    pub fn names() -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    pub fn run(_cfg: CalendarConfig, hub: Hub, _every: Duration, _alive: impl Fn() -> bool) {
        hub.update(|s| {
            s.agenda = Some(Agenda { access: "unsupported".into(), ..Default::default() });
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_meeting_links() {
        let notes = "Réunion Microsoft Teams\nRejoindre : https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0?context=%7b%7d\nMerci";
        assert_eq!(
            join_url([notes]).as_deref(),
            Some("https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0?context=%7b%7d")
        );
        assert_eq!(join_url(["https://acme.zoom.us/j/123456"]).as_deref(), Some("https://acme.zoom.us/j/123456"));
        assert_eq!(join_url(["Salle 2", "https://example.com/doc"]), None);
    }

    /// The calendars listed only, case aside; none listed: all of them.
    #[test]
    fn shows_the_listed_calendars() {
        let all = CalendarConfig::default();
        assert!(all.shows("Birthdays") && all.shows("Work"));
        let work = CalendarConfig { calendars: vec!["Calendar".into(), " work ".into()], ..all };
        assert!(work.shows("calendar") && work.shows("Work"));
        assert!(!work.shows("Birthdays"));
    }
}
