//! The window's own commands (`desk` in `prism_commands`' table): its window, menu, tray,
//! clipboard, Dock icon, audio, profiles, capture and terminals, which only Tauri gives. The
//! others are `prism_commands`', registered by `handler()` with these.

pub mod profiles;

use std::sync::Arc;

#[cfg(not(target_os = "macos"))]
use prism_base::tr;
use prism_commands::commands::{refused_in_demo, Cmd, DESK};
use tauri::{AppHandle, State};

use crate::app::AppState;

/// Every command the window can invoke, registered with Tauri: the window's own, then
/// `prism_commands`'; the demo profile refuses those its table says.
pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    let commands = prism_commands::commands::tauri_handler();
    let desk: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
        profiles::list_profiles,
        profiles::create_profile,
        profiles::setup_profile,
        profiles::rename_profile,
        profiles::remove_profile,
        profiles::switch_profile,
        profiles::open_profile,
        set_help_guides,
        app_icon,
        set_dock_icon,
        set_tray,
        font_face,
        youtube_relay,
        copy_text,
        paste_text,
        quit_app,
        toggle_fullscreen,
        audio_sources,
        audio_feed,
        now_playing,
        now_playing_command,
        feedback_capture,
        agent_attach,
    ];
    move |invoke: tauri::ipc::Invoke| {
        let command = invoke.message.command();
        if refused_in_demo(command) {
            invoke.resolver.reject(crate::demo::READ_ONLY);
            return true;
        }
        if DESK.contains(&command) { desk(invoke) } else { commands(invoke) }
    }
}

/// The file of one face listed by `font_faces`, as raw bytes.
#[tauri::command]
pub async fn font_face(family: String, index: usize) -> Cmd<tauri::ipc::Response> {
    let bytes = tokio::task::spawn_blocking(move || crate::fonts::file(&family, index)).await?;
    bytes.map(tauri::ipc::Response::new).map_err(Into::into)
}

/// Copies text to the system clipboard: the selection of an agent's terminal, which tmux
/// sends outside of any user gesture (where the webview would refuse).
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Cmd {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string().into())
}

/// Reads the system clipboard: Ctrl+Shift+V in an agent's terminal (Linux, Windows), a
/// gesture of mine.
#[tauri::command]
pub fn paste_text(app: AppHandle) -> Cmd<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().map_err(|e| e.to_string().into())
}

/// "Audio" block: radios and podcasts from the config.
#[tauri::command]
pub fn audio_sources(state: State<'_, Arc<AppState>>) -> crate::audio::Sources {
    (&state.audio()).into()
}

/// Episodes of a podcast from the config, by its index: the frontend does not choose the URL played.
#[tauri::command]
pub async fn audio_feed(state: State<'_, Arc<AppState>>, index: usize) -> Cmd<Vec<crate::audio::Episode>> {
    let feed = state.audio().feeds.get(index).cloned().ok_or("unknown feed")?;
    crate::audio::episodes(&feed).await.map_err(Into::into)
}

/// macOS "Now Playing": the current state (then pushed by the `now-playing` event).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn now_playing(latest: State<'_, crate::audio::now_playing::Latest>) -> Option<serde_json::Value> {
    latest.get().and_then(|n| serde_json::to_value(n).ok())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn now_playing() -> Option<serde_json::Value> {
    None
}

/// Command to the "Now Playing" player: `play`, `pause`, `toggle`, `next`, `previous`.
#[tauri::command]
pub async fn now_playing_command(cmd: String) -> Cmd {
    #[cfg(target_os = "macos")]
    return crate::audio::now_playing::command(&cmd).await.map_err(Into::into);
    #[cfg(not(target_os = "macos"))]
    return Err(tr!("{cmd}: macOS only", cmd = cmd).into());
}

/// The window gives its language, which the menus follow (the tray's everywhere, the menu bar on
/// macOS), and its Help guides, which the macOS Help menu lists.
#[tauri::command]
pub fn set_help_guides(app: AppHandle, guides: Vec<crate::menu::Guide>, lang: Option<String>) -> Cmd {
    if crate::menu::set_language(lang.as_deref().unwrap_or("en")) {
        if let Some(tray) = app.tray_by_id(crate::app::TRAY_ID) {
            tray.set_menu(Some(crate::app::tray_menu(&app)?))?;
        }
        #[cfg(target_os = "macos")]
        crate::menu::build(&app)?;
    }
    #[cfg(target_os = "macos")]
    return crate::menu::set_help_guides(&app, guides).map_err(Into::into);
    #[cfg(not(target_os = "macos"))]
    return {
        let _ = guides;
        Ok(())
    };
}

/// Quits Prism, as the macOS menu's Quit: where there is no menu bar (Linux), from the palette
/// or Ctrl+Q, the window's close button only minimising it.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0)
}

/// The window in full screen, or back: the macOS menu's View › Enter Full Screen, elsewhere F11.
#[tauri::command]
pub fn toggle_fullscreen(window: tauri::WebviewWindow) -> Result<(), String> {
    let on = window.is_fullscreen().map_err(|e| e.to_string())?;
    window.set_fullscreen(!on).map_err(|e| e.to_string())
}

/// The app's icon (PNG, 256 px): the window draws the profile's mark on it for the Dock.
#[tauri::command]
pub fn app_icon() -> tauri::ipc::Response {
    tauri::ipc::Response::new(include_bytes!("../../icons/128x128@2x.png").to_vec())
}

/// macOS: this Prism's Dock icon, with its profile's mark (`png`, drawn by the window): two
/// Prism open on two profiles are told apart in the Dock and ⌘-Tab.
#[tauri::command]
pub fn set_dock_icon(app: AppHandle, png: Vec<u8>) -> Cmd {
    #[cfg(target_os = "macos")]
    app.run_on_main_thread(move || {
        prism_macos::appkit::set_app_icon(&png);
    })?;
    #[cfg(not(target_os = "macos"))]
    let _ = (app, png);
    Ok(())
}

/// The YouTube player's relay page (`http://127.0.0.1:<port>`): YouTube wants a referrer the
/// app's own page cannot send.
#[tauri::command]
pub async fn youtube_relay() -> Cmd<String> {
    crate::youtube_relay::origin().await.map_err(Into::into)
}

/// Counter shown next to the menu bar icon (computed by the frontend, which holds the rules).
#[tauri::command]
pub fn set_tray(app: AppHandle, count: u32, tooltip: String) -> Cmd {
    tracing::debug!("front: tray count {count}");
    let tray = app.tray_by_id(crate::app::TRAY_ID).ok_or("no tray icon")?;
    tray.set_title(if count > 0 { Some(count.to_string()) } else { None })?;
    tray.set_tooltip(Some(tooltip))?;
    Ok(())
}

/// Attaches the view's terminal to the agent: its output arrives as raw bytes via `on_data`,
/// then a `{"end":true}` when the attachment ends.
#[tauri::command]
pub fn agent_attach(state: State<'_, Arc<AppState>>, id: String, cols: u16, rows: u16, on_data: tauri::ipc::Channel<tauri::ipc::InvokeResponseBody>) -> Cmd {
    // A demo agent's terminal replays its recorded session; its shell is none.
    if crate::demo::active() && id.starts_with(crate::agents::SHELL) {
        let _ = on_data.send(tauri::ipc::InvokeResponseBody::Raw(format!("{}\r\n", crate::demo::READ_ONLY).into_bytes()));
        return Ok(());
    }
    if crate::demo::active() {
        let screen = crate::demo::details::terminal(&state.hub.subscribe().borrow(), &id)?;
        let _ = on_data.send(tauri::ipc::InvokeResponseBody::Raw(screen));
        return Ok(());
    }
    let end = on_data.clone();
    state
        .agents
        .attach(
            &id,
            cols,
            rows,
            move |bytes| {
                let _ = on_data.send(tauri::ipc::InvokeResponseBody::Raw(bytes));
            },
            move || {
                let _ = end.send(tauri::ipc::InvokeResponseBody::Json(r#"{"end":true}"#.into()));
            },
        )
        .map_err(Into::into)
}

/// A capture of the window, kept for the issue and put on the clipboard: its thumbnail, base64
/// JPEG, for the composer to show.
#[tauri::command]
pub async fn feedback_capture(state: State<'_, Arc<AppState>>, window: tauri::WebviewWindow) -> Cmd<String> {
    use base64::Engine;
    let thumb = crate::feedback::capture(&window, &crate::feedback::capture_path(&state.store)).await?;
    Ok(base64::engine::general_purpose::STANDARD.encode(thumb))
}

#[cfg(test)]
mod tests {
    /// The window's own commands, as `prism_commands`' table lists them (`desk`), are this
    /// module's, registered by `handler()`.
    #[test]
    fn the_desk_commands_are_here() {
        let sources = [include_str!("mod.rs"), include_str!("profiles.rs")].concat();
        let handler = &sources[sources.find("generate_handler![").unwrap()..];
        let handler = &handler[..handler.find("];").unwrap()];
        for name in prism_commands::commands::DESK {
            assert!(sources.contains(&format!("fn {name}(")), "{name}: in the table's desk, not here");
            assert!(handler.contains(&format!("{name},")), "{name}: not registered by handler()");
        }
    }
}
