//! The macOS Calendar through EventKit: the store, its events and their attendees, read only.
//! EventKit is not thread-safe: a `Store` stays on the thread that made it (it is not `Send`).

use std::{sync::mpsc, time::Duration};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_event_kit::{
    EKAuthorizationStatus, EKEntityType, EKEvent, EKEventAvailability, EKEventStatus, EKEventStore, EKParticipant, EKParticipantRole, EKParticipantStatus,
};
use objc2_foundation::{NSDate, NSError};

/// Prism's access to the calendars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Granted,
    Denied,
    Restricted,
    /// Never asked: `Store::request_access` asks.
    Unknown,
}

/// The access macOS gives Prism now.
pub fn access() -> Access {
    // SAFETY: a class method with no argument.
    match unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) } {
        EKAuthorizationStatus::FullAccess => Access::Granted,
        EKAuthorizationStatus::Denied | EKAuthorizationStatus::WriteOnly => Access::Denied,
        EKAuthorizationStatus::Restricted => Access::Restricted,
        _ => Access::Unknown,
    }
}

pub struct Store(Retained<EKEventStore>);

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}

impl Store {
    pub fn new() -> Self {
        // SAFETY: EKEventStore's designated initializer.
        Self(unsafe { EKEventStore::new() })
    }

    /// Asks macOS for full access (it asks the user once) and waits for the answer, up to `wait`.
    pub fn request_access(&self, wait: Duration) -> bool {
        let (tx, rx) = mpsc::channel();
        let done = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            let _ = tx.send(granted.as_bool());
        });
        // SAFETY: the completion block has the signature EventKit calls it with; EventKit copies
        // it, so it may outlive this frame.
        unsafe { self.0.requestFullAccessToEventsWithCompletion(RcBlock::as_ptr(&done)) };
        rx.recv_timeout(wait).unwrap_or(false)
    }

    /// Reloads the sources (a permission just granted).
    pub fn reset(&self) {
        // SAFETY: a method with no argument, on the store's thread.
        unsafe { self.0.reset() };
    }

    /// The events overlapping `from..to` (seconds since 1970), every calendar's.
    pub fn events(&self, from: f64, to: f64) -> Vec<Event> {
        let (from, to) = (NSDate::dateWithTimeIntervalSince1970(from), NSDate::dateWithTimeIntervalSince1970(to));
        // SAFETY: a predicate of two dates built here, no calendar list (all of them).
        let events = unsafe { self.0.eventsMatchingPredicate(&self.0.predicateForEventsWithStartDate_endDate_calendars(&from, &to, None)) };
        events.iter().map(Event).collect()
    }

    /// The names of the event calendars.
    pub fn calendar_names(&self) -> Vec<String> {
        // SAFETY: a list of calendars and their title getter.
        unsafe { self.0.calendarsForEntityType(EKEntityType::Event) }.iter().map(|c| unsafe { c.title() }.to_string()).collect()
    }
}

/// An event (one occurrence of a recurring one).
pub struct Event(Retained<EKEvent>);

/// `EKEvent`'s getters below: on an object EventKit made, read on the store's thread.
impl Event {
    /// The series' id: an occurrence of a recurring event shares it.
    pub fn id(&self) -> Option<String> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.eventIdentifier() }.map(|i| i.to_string())
    }

    /// Start, in seconds since 1970.
    pub fn start(&self) -> f64 {
        // SAFETY: see `impl Event`.
        unsafe { self.0.startDate() }.timeIntervalSince1970()
    }

    pub fn end(&self) -> f64 {
        // SAFETY: see `impl Event`.
        unsafe { self.0.endDate() }.timeIntervalSince1970()
    }

    pub fn title(&self) -> String {
        // SAFETY: see `impl Event`.
        unsafe { self.0.title() }.to_string()
    }

    pub fn location(&self) -> Option<String> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.location() }.map(|l| l.to_string())
    }

    pub fn notes(&self) -> Option<String> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.notes() }.map(|n| n.to_string())
    }

    pub fn url(&self) -> Option<String> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.URL() }.and_then(|u| u.absoluteString()).map(|u| u.to_string())
    }

    pub fn all_day(&self) -> bool {
        // SAFETY: see `impl Event`.
        unsafe { self.0.isAllDay() }
    }

    /// Its calendar's title.
    pub fn calendar(&self) -> Option<String> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.calendar() }.map(|c| unsafe { c.title() }.to_string())
    }

    pub fn canceled(&self) -> bool {
        // SAFETY: see `impl Event`.
        unsafe { self.0.status() == EKEventStatus::Canceled }
    }

    /// Shown as free time.
    pub fn free(&self) -> bool {
        // SAFETY: see `impl Event`.
        unsafe { self.0.availability() == EKEventAvailability::Free }
    }

    /// None for an event without attendees.
    pub fn attendees(&self) -> Option<Vec<Participant>> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.attendees() }.map(|list| list.iter().map(Participant).collect())
    }

    pub fn organizer(&self) -> Option<Participant> {
        // SAFETY: see `impl Event`.
        unsafe { self.0.organizer() }.map(Participant)
    }
}

/// An attendee's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Accepted,
    Declined,
    Tentative,
    Pending,
}

impl Answer {
    pub fn word(self) -> &'static str {
        match self {
            Answer::Accepted => "accepted",
            Answer::Declined => "declined",
            Answer::Tentative => "tentative",
            Answer::Pending => "pending",
        }
    }
}

/// An attendee or organizer.
pub struct Participant(Retained<EKParticipant>);

/// `EKParticipant`'s getters below: on an object EventKit made, read on the store's thread.
impl Participant {
    /// Its address (`mailto:…` for a person).
    pub fn url(&self) -> String {
        // SAFETY: see `impl Participant`.
        unsafe { self.0.URL() }.absoluteString().map(|u| u.to_string()).unwrap_or_default()
    }

    pub fn name(&self) -> Option<String> {
        // SAFETY: see `impl Participant`.
        unsafe { self.0.name() }.map(|n| n.to_string())
    }

    pub fn answer(&self) -> Answer {
        // SAFETY: see `impl Participant`.
        match unsafe { self.0.participantStatus() } {
            EKParticipantStatus::Accepted => Answer::Accepted,
            EKParticipantStatus::Declined => Answer::Declined,
            EKParticipantStatus::Tentative => Answer::Tentative,
            _ => Answer::Pending,
        }
    }

    pub fn optional(&self) -> bool {
        // SAFETY: see `impl Participant`.
        unsafe { self.0.participantRole() == EKParticipantRole::Optional }
    }

    /// The user of this Mac.
    pub fn me(&self) -> bool {
        // SAFETY: see `impl Participant`.
        unsafe { self.0.isCurrentUser() }
    }
}
