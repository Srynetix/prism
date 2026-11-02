//! Profiles: list, create, switch.


use std::sync::Arc;

use prism_commands::commands::Cmd;
use tauri::{Manager, State};

use crate::app::AppState;

/// The profiles (rail menu, Settings › General › Profiles), the default one first.
#[tauri::command]
pub async fn list_profiles() -> Vec<crate::profile::Profile> {
    crate::profile::list().await
}

/// A new profile, empty or from a pack (`pack`: a file, `glab:group/project:path@ref` or `gh:owner/repo:path@ref`).
#[tauri::command]
pub async fn create_profile(app: tauri::AppHandle, name: String, color: String, pack: Option<String>, demo: Option<bool>) -> Cmd<crate::profile::Profile> {
    let p = crate::profile::create(&name, &color, pack.as_deref().filter(|p| !p.trim().is_empty()), demo.unwrap_or(false)).await?;
    let _ = crate::menu::set_profiles(&app);
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

/// Switches to another profile: Prism relaunches on it (`true`). The agents keep running in their
/// own tmux. Already open in another Prism: that one comes to the front, this one stays (`false`).
#[tauri::command]
pub fn switch_profile(app: tauri::AppHandle, id: String) -> Cmd<bool> {
    if id == crate::profile::active() {
        return Ok(false);
    }
    if let Some(pid) = crate::profile::holder(&id) {
        crate::profile::wake(pid);
        return Ok(false);
    }
    crate::profile::choose(&id)?;
    crate::profile::relaunch(&id)?;
    app.exit(0);
    Ok(true)
}

/// Opens a profile in another Prism, next to this one; already open, that Prism comes to the front.
#[tauri::command]
pub fn open_profile(app: tauri::AppHandle, id: String) -> Cmd {
    if id == crate::profile::active() {
        crate::app::show_main(&app);
    } else if let Some(pid) = crate::profile::holder(&id) {
        crate::profile::wake(pid);
    } else {
        crate::profile::launch(&id)?;
    }
    Ok(())
}
