//! What the app sets up once Tauri runs: Prism itself (`prism_commands`: the collectors, the agents,
//! the remote, the state the commands share), then what the window adds: its title, its notices,
//! Now Playing, the snapshot pushed to it, the tray and the menu.

use std::sync::Arc;

use tauri::{App, Manager};

use crate::{app, audio, collectors, profile, state};
#[cfg(target_os = "macos")]
use crate::menu;

pub fn run(app: &mut App, hub: state::Hub, time: collectors::time::TimeService, team_id: collectors::clickup::TeamId) -> Result<(), Box<dyn std::error::Error>> {
    // The active profile's data: agents, tmux, notes, layout.
    let data_dir = profile::data_dir(&app.path().app_data_dir()?);
    let state = app::AppState::start(data_dir, hub.clone(), time, team_id, Arc::new(app::Desk(app.handle().clone())), env!("PRISM_BUILD").to_string())?;
    // With several profiles, the window says which one is open.
    let shared = hub.subscribe().borrow().profile.shared;
    if shared && let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&crate::profile_windows::title(profile::active()));
    }
    // Elsewhere (Windows), nothing playing: the command answers so.
    let now_playing = audio::now_playing::Latest::default();
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    audio::now_playing::run(app.handle().clone(), now_playing.clone());
    app.manage(now_playing);
    app.manage(state);
    // Dictation: its recordings, its models, the Fn key.
    crate::dictation::setup(app)?;

    // The other computers' Prism, for the status bar and the notifications.
    crate::machines::watch(hub.clone());
    app::broadcast(app.handle().clone(), hub);
    #[cfg(unix)]
    app::listen_signals(app.handle());
    app::tray(app.handle())?;
    #[cfg(target_os = "macos")]
    menu::install(app.handle())?;
    // The profiles open in windows last time, back; then what launches asked meanwhile.
    crate::profile_windows::reopen(app.handle());
    crate::profile_windows::asked_quietly(app.handle());
    Ok(())
}
