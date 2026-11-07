//! This computer's other profiles, each in a window of this app (`profile-<id>`): one app, one Dock
//! icon, ⌘` from one to the other. The app's own profile is the main window's; another runs as a
//! Prism without a window (`prism engine`, its own process: its config, secrets, tmux and data
//! stay apart), which its window reaches through the profile's local socket (`machines`, as
//! another computer's, without token nor confirmation: the same account on the same computer).
//!
//! Closing a profile's window hides it: its Prism goes on (agents, alerts, the phone). Quitting
//! the app stops the Prisms it started; their agents go on in their tmux. One started apart
//! (`[remote] keep_running`) stays, and the next app shows it again.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::{ensure, Result};
use prism_core::profile;
use tauri::{AppHandle, Manager};

pub const PREFIX: &str = "profile-";

/// The profiles' Prisms this app started: stopped when it quits. Kept in a file too: the app
/// relaunched (Restart Prism, an update) stops them in its turn.
pub struct Engines {
    started: Mutex<HashSet<String>>,
    /// When each was last started: one starting has not claimed its profile yet (`holder`).
    starting: Mutex<std::collections::HashMap<String, std::time::Instant>>,
}

impl Default for Engines {
    /// Those a former app started, still running.
    fn default() -> Self {
        let started = std::fs::read_to_string(started_path()).unwrap_or_default().lines().map(str::trim).filter(|id| !id.is_empty() && profile::holder(id).is_some()).map(String::from).collect();
        Self { started: Mutex::new(started), starting: Default::default() }
    }
}

impl Engines {
    fn save(started: &HashSet<String>) {
        let mut ids: Vec<&str> = started.iter().map(String::as_str).collect();
        ids.sort();
        let path = started_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, ids.join("\n"));
    }
}

fn started_path() -> std::path::PathBuf {
    profile::data_base().join("engines").join("started")
}

/// The window of profile `id`.
pub fn label(id: &str) -> String {
    format!("{PREFIX}{id}")
}

/// The profile a window shows, if it is a profile's.
pub fn of_label(label: &str) -> Option<&str> {
    label.strip_prefix(PREFIX)
}

/// A profile's name, as the menus show it.
fn name_of(id: &str) -> String {
    profile::names().into_iter().find(|(i, _)| i == id).map_or_else(|| id.to_string(), |(_, n)| n)
}

/// A profile's window title, `Prism — ● Perso`: its name after a dot of its color (a title bar
/// shows text only: the nearest colored circle).
pub fn title(id: &str) -> String {
    let color = prism_core::config::load(&profile::config_of(id)).map(|c| c.profile.color.trim().to_string()).unwrap_or_default();
    format!("Prism — {} {}", dot(&color), name_of(id))
}

/// The circle nearest a profile's color (`web/src/profiles.svelte.ts`); the accent's by default.
fn dot(color: &str) -> &'static str {
    match color {
        "red" | "coral" => "🔴",
        "orange" | "amber" => "🟠",
        "green" | "teal" => "🟢",
        "purple" | "pink" => "🟣",
        "brown" => "🟤",
        "slate" => "⚫",
        _ => "🔵",
    }
}

/// Every window's title again (a profile renamed, or its color changed).
pub fn retitle(app: &AppHandle) {
    let several = profile::names().len() > 1;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_title(&if several { title(profile::active()) } else { "Prism".into() });
    }
    for (label, w) in app.webview_windows() {
        if let Some(id) = of_label(&label) {
            let _ = w.set_title(&title(id));
        }
    }
}

/// Profile `id` in front: the main window for the app's own, otherwise its window, opened (its
/// Prism started if none runs) or shown again.
pub fn open(app: &AppHandle, id: &str) -> Result<()> {
    if id == profile::active() {
        crate::app::show_main(app);
        return Ok(());
    }
    ensure!(profile::exists(id), "unknown profile {id}");
    engine(app, id)?;
    let label = label(id);
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    let name = name_of(id);
    let machines = app.state::<Arc<crate::machines::Machines>>();
    machines.open.lock().unwrap().insert(label.clone(), crate::machines::Open { name: name.clone(), profile: None, machine: None, local: Some(id.to_string()) });
    // The page learns which profile it shows before any of its scripts (JSON that cannot fail to write).
    let script = format!("window.__PRISM_PROFILE_WINDOW__ = {};", serde_json::json!(id));
    let window = tauri::WebviewWindowBuilder::new(app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title(title(id))
        .inner_size(1440.0, 900.0)
        .min_inner_size(900.0, 560.0)
        .disable_drag_drop_handler()
        .initialization_script(&script)
        .build();
    if let Err(e) = window {
        machines.open.lock().unwrap().remove(&label);
        return Err(e.into());
    }
    remember(app);
    let _ = crate::menu::set_profiles(app);
    Ok(())
}

/// Profile `id`'s Prism, started if none runs (another app's, or one kept running, is used).
fn engine(app: &AppHandle, id: &str) -> Result<()> {
    if profile::holder(id).is_some() {
        return Ok(());
    }
    let engines = app.state::<Arc<Engines>>();
    {
        // Asked twice at once (a reopened profile also asked by a launch, a double click): one.
        let mut starting = engines.starting.lock().unwrap();
        if starting.get(id).is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(20)) {
            return Ok(());
        }
        starting.insert(id.to_string(), std::time::Instant::now());
    }
    let pid = profile::start_engine(id)?;
    tracing::info!("profile {id}: its Prism started without a window (pid {pid})");
    let mut started = engines.started.lock().unwrap();
    started.insert(id.to_string());
    Engines::save(&started);
    Ok(())
}

/// The profiles with a window here, remembered for the next launch.
fn remember(app: &AppHandle) {
    let mut ids: Vec<String> = app.webview_windows().keys().filter_map(|l| of_label(l)).map(String::from).collect();
    ids.sort();
    if let Err(e) = profile::remember_open(&ids) {
        tracing::warn!("open profiles: {e:#}");
    }
}

/// Profile `id`'s window closed for good and its Prism stopped (Settings › General › Profiles):
/// its agents go on in their tmux.
pub fn stop(app: &AppHandle, id: &str) -> Result<()> {
    ensure!(id != profile::active(), "the app's own profile stops with the app");
    if let Some(w) = app.get_webview_window(&label(id)) {
        let _ = w.destroy();
    }
    {
        let engines = app.state::<Arc<Engines>>();
        let mut started = engines.started.lock().unwrap();
        started.remove(id);
        Engines::save(&started);
        engines.starting.lock().unwrap().remove(id);
    }
    profile::stop(id);
    // The window is gone once its event ran: the list read without it.
    let mut ids: Vec<String> = app.webview_windows().keys().filter_map(|l| of_label(l)).filter(|i| *i != id).map(String::from).collect();
    ids.sort();
    let _ = profile::remember_open(&ids);
    let _ = crate::menu::set_profiles(app);
    Ok(())
}

/// Each profile's state here: `window` (shown in a window of this app), `background` (its Prism
/// runs, no window shown) or `closed`.
pub fn state_of(app: &AppHandle, p: &profile::Profile) -> &'static str {
    if p.active {
        return "window";
    }
    match app.get_webview_window(&label(&p.id)) {
        Some(w) if w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false) => "window",
        _ if p.open => "background",
        _ => "closed",
    }
}

/// At launch: the profiles open in windows last time, back.
pub fn reopen(app: &AppHandle) {
    for id in profile::reopen() {
        if let Err(e) = open(app, &id) {
            tracing::warn!("profile {id}: {e:#}");
        }
    }
}

/// What other launches asked (`prism --profile perso`, the Dock): each profile shown; none named,
/// the main window.
pub fn asked(app: &AppHandle) {
    let ids = profile::take_requests();
    if ids.is_empty() {
        crate::app::show_main(app);
    }
    for id in ids {
        if let Err(e) = open(app, &id) {
            tracing::warn!("profile {id}: {e:#}");
        }
    }
}

/// At launch: what launches asked before this app listened, if anything (the main window shows
/// anyway).
pub fn asked_quietly(app: &AppHandle) {
    for id in profile::take_requests() {
        if let Err(e) = open(app, &id) {
            tracing::warn!("profile {id}: {e:#}");
        }
    }
}

/// The app quits: the Prisms it started stop (their agents go on in tmux).
pub fn quit(app: &AppHandle) {
    let engines = app.state::<Arc<Engines>>();
    let mut started = engines.started.lock().unwrap();
    for id in started.drain() {
        if profile::stop(&id) {
            tracing::info!("profile {id}: its Prism stopped with the app");
        }
    }
    Engines::save(&started);
}
