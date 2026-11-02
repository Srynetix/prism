//! Mail and calendar.

use chrono::{DateTime, Utc};
use serde::Serialize;


/// The Microsoft 365 or Gmail inbox (Mail block): the unread messages, and the flagged (starred)
/// ones. No body, no recipient.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MailState {
    /// The inbox read: Microsoft 365 or Gmail.
    pub provider: crate::config::MailProvider,
    /// A refresh token is in the keychain: signed out, nothing is read.
    pub signed_in: bool,
    /// Unread messages in the inbox, beyond those listed.
    pub unread: u32,
    /// Newest first, at most `[mail] max`.
    pub messages: Vec<MailMessage>,
    /// When Mail was last read.
    pub read_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MailMessage {
    /// Its Graph id.
    pub id: String,
    pub subject: String,
    /// The sender's name, or address without one.
    pub from: String,
    pub address: String,
    pub received: Option<DateTime<Utc>>,
    pub unread: bool,
    pub flagged: bool,
    /// Outlook or Gmail on the web, where it opens (Graph's `webLink`).
    pub link: String,
}

/// Today's agenda, from the macOS Calendar or Google Calendar.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Agenda {
    /// Where it is read: the macOS Calendar or Google Calendar.
    pub provider: crate::config::CalendarProvider,
    /// `granted`, `denied` (refused in Settings), `restricted`, `unsupported` (macOS Calendar
    /// outside macOS), `signed_out` (Google Calendar).
    pub access: String,
    /// Today's events, by start time.
    pub events: Vec<CalendarEvent>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub all_day: bool,
    pub location: Option<String>,
    pub calendar: String,
    /// My response: `accepted`, `tentative`, `declined`, `pending`; absent without attendees.
    #[cfg_attr(feature = "ts", ts(type = "'accepted' | 'tentative' | 'declined' | 'pending' | null"))]
    pub my_status: Option<String>,
    pub canceled: bool,
    /// Marked "free": does not block the slot.
    pub free: bool,
    /// Teams, Zoom or Meet link found in the URL, the location or the notes.
    pub join_url: Option<String>,
    pub attendees: u32,
}
