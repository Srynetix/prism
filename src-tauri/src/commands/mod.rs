//! The window's own commands (`desk` in `prism_commands`' table): its window, menu, tray,
//! clipboard, Dock icon, audio, profiles, capture and terminals, which only Tauri gives. The
//! others are `prism_commands`', registered by `handler()` with these.

pub mod profiles;

use std::sync::Arc;

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
        profiles::stop_profile,
        set_help_guides,
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
        crate::machines::machine_list,
        crate::machines::machine_token,
        crate::machines::machine_pair,
        crate::machines::machine_open,
        crate::machines::machine_main,
        crate::machines::machine_connect,
        crate::machines::machine_call,
        crate::machines::machine_attach,
        crate::machines::machine_write,
        crate::machines::machine_resize,
        crate::machines::machine_detach,
        crate::machines::machine_export,
        crate::dictation::dictation_start,
        crate::dictation::dictation_stop,
        crate::dictation::dictation_cancel,
        crate::dictation::dictation_status,
        crate::dictation::dictation_download,
        crate::dictation::dictation_cancel_download,
        crate::dictation::dictation_remove,
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

/// How long the system clipboard may take to answer before it is reported unavailable.
const CLIPBOARD_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Runs `f` on the system clipboard off the main thread, `wait` at most. On Linux, the main
/// thread blocked on the clipboard cannot answer for the window when the text was copied from it
/// (its webview): the app froze (the plugin warns of it). Past the wait, an error rather than a
/// promise that never returns; the call left behind ends on its own, or never.
async fn on_clipboard<T: Send + 'static, E: std::fmt::Display + Send + 'static>(wait: std::time::Duration, f: impl FnOnce() -> Result<T, E> + Send + 'static) -> Cmd<T> {
    match tokio::time::timeout(wait, tokio::task::spawn_blocking(f)).await {
        Ok(done) => done?.map_err(|e| e.to_string().into()),
        Err(_) => Err(tr!("The clipboard did not answer within {seconds} s", seconds = wait.as_secs().max(1)).into()),
    }
}

/// Copies text to the system clipboard: the selection of an agent's terminal, which tmux
/// sends outside of any user gesture (where the webview would refuse).
#[tauri::command]
pub async fn copy_text(app: AppHandle, text: String) -> Cmd {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    on_clipboard(CLIPBOARD_WAIT, move || app.clipboard().write_text(text)).await
}

/// Reads the system clipboard: Ctrl+Shift+V in an agent's terminal (Linux, Windows), Paste in
/// its menu or a field's, a gesture of mine.
#[tauri::command]
pub async fn paste_text(app: AppHandle) -> Cmd<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    on_clipboard(CLIPBOARD_WAIT, move || app.clipboard().read_text()).await
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

/// The system's "Now Playing" (macOS, Linux): the current state (then pushed by the
/// `now-playing` event). No managed state (Windows): no player read.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[tauri::command]
pub fn now_playing(latest: State<'_, crate::audio::now_playing::Latest>) -> Option<serde_json::Value> {
    latest.get().and_then(|n| serde_json::to_value(n).ok())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[tauri::command]
pub fn now_playing() -> Option<serde_json::Value> {
    None
}

/// Command to the "Now Playing" player: `play`, `pause`, `toggle`, `next`, `previous`.
#[tauri::command]
pub async fn now_playing_command(cmd: String) -> Cmd {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    return crate::audio::now_playing::command(&cmd).await.map_err(Into::into);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(tr!("{cmd}: macOS and Linux only", cmd = cmd).into());
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

/// A capture of the window, as a data URI for the composer to show; written to the store too, the
/// file the forge's upload takes, and on the clipboard (to paste where the upload is refused).
#[tauri::command]
pub async fn feedback_capture(state: State<'_, Arc<AppState>>, window: tauri::WebviewWindow) -> Cmd<String> {
    Ok(crate::feedback::capture(&window, &crate::feedback::capture_path(&state.store)).await?)
}

#[cfg(test)]
mod tests {
    /// A clipboard that does not answer: an error past the wait, the caller never held.
    #[tokio::test]
    async fn a_clipboard_that_does_not_answer_is_an_error() {
        use std::time::Duration;
        let slow = super::on_clipboard(Duration::from_millis(50), || {
            std::thread::sleep(Duration::from_millis(500));
            Ok::<_, String>("late".to_string())
        });
        let started = std::time::Instant::now();
        let err = format!("{:?}", slow.await.unwrap_err());
        assert!(started.elapsed() < Duration::from_millis(400), "returned at the wait");
        assert!(err.contains("did not answer") || err.contains("pas répondu"), "{err}");
        assert_eq!(super::on_clipboard(Duration::from_secs(1), || Ok::<_, String>("text".to_string())).await.unwrap(), "text");
        assert!(format!("{:?}", super::on_clipboard(Duration::from_secs(1), || Err::<String, _>("no text".to_string())).await.unwrap_err()).contains("no text"));
    }

    /// The window's own commands, as `prism_commands`' table lists them (`desk`), are this
    /// module's, registered by `handler()`.
    #[test]
    fn the_desk_commands_are_here() {
        let sources = [include_str!("mod.rs"), include_str!("profiles.rs"), include_str!("../machines.rs"), include_str!("../dictation.rs")].concat();
        let handler = &sources[sources.find("generate_handler![").unwrap()..];
        let handler = &handler[..handler.find("];").unwrap()];
        for name in prism_commands::commands::DESK {
            assert!(sources.contains(&format!("fn {name}(")), "{name}: in the table's desk, not here");
            assert!(handler.contains(&format!("{name},")), "{name}: not registered by handler()");
        }
    }
}
