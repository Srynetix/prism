//! Mail and calendars, Google sign-in.





use super::Cmd;

/// Details of an agenda event, read when its preview opens.
#[prism_macros::command]
pub async fn calendar_event(id: String) -> Cmd<crate::collectors::calendar::EventDetails> {
    if crate::demo::active() {
        return crate::demo::details::event(&id).map_err(Into::into);
    }
    crate::collectors::calendar::details(id).await.map_err(Into::into)
}

/// Agenda of a day other than today (date picker), read on demand.
#[prism_macros::command]
pub async fn calendar_day(day: String) -> Cmd<Vec<crate::state::CalendarEvent>> {
    let day = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d").map_err(|e| format!("bad date {day}: {e}"))?;
    if crate::demo::active() {
        return Ok(crate::demo::details::day(day));
    }
    crate::collectors::calendar::day(day).await.map_err(Into::into)
}

/// The computer's calendars, for Settings › Work › Calendar to pick the ones read.
#[prism_macros::command]
pub async fn calendar_names() -> Cmd<Vec<String>> {
    crate::collectors::calendar::names().await.map_err(Into::into)
}

/// Opens a message of the Mail block in Outlook on the web.
#[prism_macros::command]
pub fn mail_open(id: String) -> Cmd {
    crate::collectors::mail::open(&id).map_err(Into::into)
}

/// Starts signing in to Microsoft 365 for the Mail block: the code to type at Microsoft's address.
#[prism_macros::command]
pub async fn mail_sign_in_start() -> Cmd<crate::collectors::mail::DeviceCode> {
    crate::collectors::mail::sign_in_start().await.map_err(Into::into)
}

/// Waits for the code to be typed and the permission accepted.
#[prism_macros::command]
pub async fn mail_sign_in_wait() -> Cmd {
    crate::collectors::mail::sign_in_wait().await.map_err(Into::into)
}

/// Signs out of Microsoft 365: the refresh token leaves the keychain.
#[prism_macros::command]
pub async fn mail_sign_out() -> Cmd {
    crate::collectors::mail::sign_out().await.map_err(Into::into)
}

/// Whether the profile is signed in to Google, and the permissions Prism asks for.
#[prism_macros::command]
pub async fn google_state() -> Cmd<crate::collectors::google::State> {
    if crate::demo::active() {
        return Ok(crate::collectors::google::State { signed_in: false, scopes: Vec::new() });
    }
    crate::collectors::google::state().await.map_err(Into::into)
}

/// Signs in to Google in the browser, for Gmail and Google Calendar; returns once signed in.
#[prism_macros::command]
pub async fn google_sign_in() -> Cmd {
    crate::collectors::google::sign_in().await.map_err(Into::into)
}

/// Signs out of Google: the refresh token is revoked and leaves the keychain.
#[prism_macros::command]
pub async fn google_sign_out() -> Cmd {
    crate::collectors::google::sign_out().await.map_err(Into::into)
}

/// Opens an event in Calendar (to respond to the invitation there).
#[prism_macros::command]
pub async fn open_in_calendar(id: String) -> Cmd {
    crate::collectors::calendar::open_in_calendar(id).await.map_err(Into::into)
}
