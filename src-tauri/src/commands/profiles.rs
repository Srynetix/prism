//! Profiles: list, create, open in a window of this app (`profile_windows`).


use std::sync::Arc;

use prism_commands::commands::Cmd;
use tauri::{Manager, State};

use crate::app::AppState;

/// The profiles (rail menu, Settings › General › Profiles), the default one first, each with its
/// state here (a window, in the background, closed).
#[tauri::command]
pub async fn list_profiles(app: tauri::AppHandle) -> Vec<crate::profile::Profile> {
    let mut list = crate::profile::list().await;
    for p in &mut list {
        p.state = crate::profile_windows::state_of(&app, p).to_string();
    }
    list
}

/// A new profile, empty or from a pack (`pack`: a file, `glab:group/project:path@ref` or `gh:owner/repo:path@ref`).
#[tauri::command]
pub async fn create_profile(app: tauri::AppHandle, name: String, color: String, pack: Option<String>, demo: Option<bool>) -> Cmd<crate::profile::Profile> {
    let p = crate::profile::create(&name, &color, pack.as_deref().filter(|p| !p.trim().is_empty()), demo.unwrap_or(false)).await?;
    let _ = crate::menu::set_profiles(&app);
    crate::profile_windows::retitle(&app);
    Ok(p)
}

/// First launch: the active profile's config.toml, empty, from a pack or the demo's, then Prism
/// relaunches on it.
#[tauri::command]
pub async fn setup_profile(app: tauri::AppHandle, name: String, color: String, pack: Option<String>, demo: Option<bool>) -> Cmd {
    crate::profile::setup(&name, &color, pack.as_deref().filter(|p| !p.trim().is_empty()), demo.unwrap_or(false)).await?;
    crate::app::restart(&app)
}

/// Renames a profile, or changes its color; the active one's shows at the next launch.
#[tauri::command]
pub fn rename_profile(app: tauri::AppHandle, state: State<'_, Arc<AppState>>, id: String, name: String, color: String) -> Cmd {
    crate::profile::rename(&id, &name, &color)?;
    let _ = crate::menu::set_profiles(&app);
    crate::profile_windows::retitle(&app);
    if id == crate::profile::active() {
        state.hub.update(|s| {
            s.profile.name = name.trim().to_string();
            s.profile.color = color.trim().to_string();
            true
        });
    }
    Ok(())
}

/// Removes a profile that is neither the default, nor the active one, nor running agents.
#[tauri::command]
pub async fn remove_profile(app: tauri::AppHandle, id: String) -> Cmd {
    let base = app.path().app_data_dir()?;
    crate::profile::remove(&id, &base).await?;
    let _ = crate::menu::set_profiles(&app);
    Ok(())
}

/// Switches to another profile: its window comes to the front (opened, its Prism started, if
/// need be); nothing relaunches (`false`, kept for the pages that ask).
#[tauri::command]
pub async fn switch_profile(app: tauri::AppHandle, id: String) -> Cmd<bool> {
    open_profile(app, id).await?;
    Ok(false)
}

/// A profile in front: the main window for the app's own, otherwise its window.
#[tauri::command]
pub async fn open_profile(app: tauri::AppHandle, id: String) -> Cmd {
    // Windows are the main thread's; its Prism started meanwhile.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(crate::profile_windows::open(&handle, &id));
    })?;
    Ok(rx.await.map_err(|_| "the app is closing")??)
}

/// A profile's window closed and its Prism stopped: its agents go on in their tmux.
#[tauri::command]
pub async fn stop_profile(app: tauri::AppHandle, id: String) -> Cmd {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(crate::profile_windows::stop(&handle, &id));
    })?;
    Ok(rx.await.map_err(|_| "the app is closing")??)
}
