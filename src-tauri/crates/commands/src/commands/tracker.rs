//! Tickets and logged time, whatever the tracker.


use super::State;

use crate::collectors::time::NewEntry;

use crate::app::AppState;

use super::Cmd;

/// ClickUp time entry. Only the app's webview can call it: no more open HTTP port.
#[prism_macros::command]
pub async fn log_time(state: State<'_, AppState>, entry: NewEntry) -> Cmd {
    state.time.log(entry).await.map_err(Into::into)
}

/// Content of a ticket (description, subtasks, comments), read when its preview opens.
#[prism_macros::command]
pub async fn get_ticket(state: State<'_, AppState>, id: String) -> Cmd<crate::collectors::task_view::TaskView> {
    if crate::demo::active() {
        return crate::demo::details::ticket(&state.hub.subscribe().borrow(), &id).map_err(Into::into);
    }
    tracing::debug!("front: ticket {id}");
    state.tracker().ticket(&id).await.map_err(Into::into)
}

/// The tickets whose id or name has every word of `query`, beyond the ones the window shows (the palette).
#[prism_macros::command]
pub async fn search_tickets(state: State<'_, AppState>, query: String) -> Cmd<Vec<crate::state::Ticket>> {
    if crate::demo::active() {
        return Ok(crate::demo::details::search_tickets(&state.hub.subscribe().borrow(), &query));
    }
    state.tracker().search(&query).await.inspect_err(|e| tracing::warn!("tracker: search: {e:#}")).map_err(Into::into)
}

/// A ticket's commits in the local clones (the ticket panel): named in their message, or on its branches.
#[prism_macros::command]
pub async fn ticket_commits(state: State<'_, AppState>, id: String) -> Cmd<Vec<crate::collectors::ticket_commits::TicketCommit>> {
    if crate::demo::active() {
        return Ok(crate::demo::details::ticket_commits(&id));
    }
    let repos = state.hub.subscribe().borrow().repos.clone();
    Ok(crate::collectors::ticket_commits::read(&repos, &id).await)
}

/// Image of a ticket (attachment, description image), as a `data:` URL.
#[prism_macros::command]
pub async fn ticket_image(state: State<'_, AppState>, url: String) -> Cmd<String> {
    state.tracker().image(&url).await.map_err(Into::into)
}

/// Status change of a ticket, from its preview.
#[prism_macros::command]
pub async fn set_ticket_status(state: State<'_, AppState>, task_id: String, status: String) -> Cmd {
    // Written down too: the window's message about it goes by.
    state.tracker().set_status(&state.hub, &task_id, &status).await.inspect_err(|e| tracing::warn!("tracker: {task_id} → {status}: {e:#}")).map_err(Into::into)
}

/// Assign a ticket to me, or remove me from it, from a board.
#[prism_macros::command]
pub async fn set_ticket_mine(state: State<'_, AppState>, task_id: String, assign: bool) -> Cmd {
    state.tracker().set_mine(&state.hub, &task_id, assign).await.inspect_err(|e| tracing::warn!("tracker: {task_id} {}: {e:#}", if assign { "assign me" } else { "unassign me" })).map_err(Into::into)
}

/// Priority of a ticket (`urgent`, `high`, `normal`, `low`, or none).
#[prism_macros::command]
pub async fn set_ticket_priority(state: State<'_, AppState>, task_id: String, priority: Option<crate::state::Priority>) -> Cmd {
    state.tracker().set_priority(&state.hub, &task_id, priority).await.map_err(Into::into)
}

/// A ticket's estimate in hours (`null`: cleared): the tracker's own (`field` `null`) or a custom field.
#[prism_macros::command]
pub async fn set_ticket_estimate(state: State<'_, AppState>, task_id: String, field: Option<String>, hours: Option<f64>) -> Cmd {
    state.tracker().set_estimate(&state.hub, &task_id, field.as_deref(), hours).await.map_err(Into::into)
}

/// Comment on a ticket, from its preview.
#[prism_macros::command]
pub async fn add_ticket_comment(state: State<'_, AppState>, task_id: String, text: String) -> Cmd {
    state.tracker().add_comment(&state.hub, &task_id, &text).await.map_err(Into::into)
}

/// A new ticket on one of the sprint's boards (Obsidian); returns its key, to open it.
#[prism_macros::command]
pub async fn create_ticket(state: State<'_, AppState>, board: String, status: String, title: String) -> Cmd<String> {
    state.tracker().create(&state.hub, &board, &status, &title).await.map_err(Into::into)
}

/// Edit of one of my comments on a ticket.
#[prism_macros::command]
pub async fn edit_ticket_comment(state: State<'_, AppState>, task_id: String, comment: String, text: String) -> Cmd {
    state.tracker().edit_comment(&state.hub, &task_id, &comment, &text).await.map_err(Into::into)
}

/// Immediate reread of the tracker (lists, sprint, slow queues), replying when it is done.
#[prism_macros::command]
pub async fn refresh_tickets(state: State<'_, AppState>) -> Cmd {
    if crate::demo::active() {
        return Ok(());
    }
    state.tracker().refresh().await.map_err(Into::into)
}

/// Adds a ticket to the current sprint, in addition to its list, from "To pick".
#[prism_macros::command]
pub async fn add_to_sprint(state: State<'_, AppState>, task_id: String) -> Cmd {
    state.tracker().add_to_sprint(&state.hub, &task_id).await.map_err(Into::into)
}

/// Edit of a ClickUp time entry displayed by Prism (ticket, start, duration, description).
#[prism_macros::command]
pub async fn update_time(state: State<'_, AppState>, id: String, entry: NewEntry) -> Cmd {
    state.time.update(&id, entry).await.map_err(Into::into)
}

/// Deletes a time entry displayed by Prism.
#[prism_macros::command]
pub async fn delete_time(state: State<'_, AppState>, id: String) -> Cmd {
    state.time.delete(&id).await.map_err(Into::into)
}
