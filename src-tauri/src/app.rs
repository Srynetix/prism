use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

pub use prism_commands::AppState;

use crate::state::Hub;

pub const TRAY_ID: &str = "prism-tray";

/// What only the window's app does for the commands: relaunch itself, serve the phone's page
/// from its bundle.
pub struct Desk(pub AppHandle);

impl prism_commands::Shell for Desk {
    fn restart(&self) -> ! {
        restart(&self.0)
    }

    fn asset(&self, path: &str) -> Option<(Vec<u8>, String)> {
        if let Some(a) = self.0.asset_resolver().get(path.to_string()) {
            return Some((a.bytes().to_vec(), a.mime_type().to_string()));
        }
        // `just dev` serves the window from Vite and bundles nothing: the last build's files.
        #[cfg(debug_assertions)]
        {
            let dist = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
            let file = dist.join(path);
            if file.starts_with(&dist) && !path.contains("..")
                && let Ok(bytes) = std::fs::read(&file)
            {
                return Some((bytes, prism_commands::serve::mime(path).into()));
            }
        }
        None
    }

    /// A toast in the window and, for a chain's `notify` step, a notification of the system.
    fn notice(&self, n: &crate::agents::Notice) {
        if n.system {
            use tauri_plugin_notification::NotificationExt;
            if let Err(e) = self.0.notification().builder().title("Prism").body(&n.text).show() {
                tracing::warn!("notification: {e}");
            }
        }
        if let Err(e) = self.0.emit_to("main", "notice", serde_json::json!({ "text": n.text, "ms": n.ms })) {
            tracing::warn!("notice: {e}");
        }
    }
}

/// Relaunches Prism (Restart Prism, the console's `restart`, `prism restart`).
/// Prism is relaunching (its `RunEvent::Exit` runs on the way, from another thread than the main).
static RESTARTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn restarting() -> bool {
    RESTARTING.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn restart(app: &tauri::AppHandle) -> ! {
    RESTARTING.store(true, std::sync::atomic::Ordering::Relaxed);
    // The new Prism must find the profile free, and the app's place, not this one about to exit:
    // the profiles' Prisms it started go on, the new one stops them when it quits.
    crate::profile::release();
    crate::profile::release_host();
    prism_dictation::whisper::unload(std::time::Duration::ZERO);
    // An update downloaded on Windows: its installer runs and starts the new Prism.
    if let Some(setup) = prism_ops::updates::take_installer() {
        match prism_ops::updates::run_installer(&setup) {
            Ok(()) => {
                app.cleanup_before_exit();
                std::process::exit(0);
            }
            Err(e) => tracing::warn!("updates: {e:#}"),
        }
    }
    app.restart()
}

/// Pushes the snapshot to the window on every change, coalescing bursts
/// (several collectors often reply a few milliseconds apart): as its changes to the last one sent
/// (`snapshot-changes`: `base`, the revision they apply to, and `rev`, the one they make), not whole
/// each time (155–490 KB through IPC and `JSON.parse`, about every second with the System block).
/// A window that has not got `base` asks the whole one again (`snapshot_sync`).
///
/// With `PRISM_DUMP_SNAPSHOT=<file>`, the snapshot is also written to that file: it is
/// the source of the frontend's preview mode in a browser (development only).
pub fn broadcast(app: AppHandle, hub: Hub) {
    let dump = std::env::var_os("PRISM_DUMP_SNAPSHOT").map(std::path::PathBuf::from);
    tauri::async_runtime::spawn(async move {
        let mut rx = hub.subscribe();
        while rx.changed().await.is_ok() {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let snap = rx.borrow_and_update().clone();
            let now = serde_json::to_value(&snap).unwrap_or_default();
            prism_commands::app::send_changes(now, |changes| {
                // The main window's: a machine's has its own (`machines`).
                if let Err(e) = app.emit_to("main", "snapshot-changes", changes) {
                    tracing::warn!("emit snapshot: {e}");
                }
            });
            if let Some(path) = &dump
                && let Err(e) = serde_json::to_vec(&snap).map_err(std::io::Error::other).and_then(|b| std::fs::write(path, b))
            {
                tracing::warn!("dump snapshot: {e}");
            }
        }
    });
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        app.state::<std::sync::Arc<AppState>>().hub.set_visible(true);
    }
}

/// Menu bar (tray) icon: a click reopens the window, the menu allows quitting. Linux has no
/// click on the icon, only its menu: "Show Prism" is there for that.
pub fn tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    // macOS: a template glyph that the menu bar tints; elsewhere, the app's icon, readable on
    // a light or dark panel alike.
    #[cfg(target_os = "macos")]
    let icon = tauri::include_image!("icons/tray.png");
    #[cfg(not(target_os = "macos"))]
    let icon = tauri::include_image!("icons/32x32.png");
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("Prism")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// The tray's menu, in the menus' language (`menu::tr`).
pub fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    use crate::menu::tr;
    let show = MenuItem::with_id(app, "show", tr("Show Prism"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", tr("Quit Prism"), true, Some("CmdOrCtrl+Q"))?;
    Menu::with_items(app, &[&show, &quit])
}

/// `prismfile://localhost/<agent>/<path>`: an agent's file, for its pages in the Web view; only
/// inside its folder, no hidden file. Tauri counts the scheme as local (IPC allowed): its CSP's
/// `sandbox` gives each page an opaque origin, wherever it opens (the Web view's iframe, or
/// the window itself), so its calls to the app are refused.
pub fn serve_agent_file(ctx: tauri::UriSchemeContext<'_, tauri::Wry>, request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let agents = ctx.app_handle().state::<std::sync::Arc<AppState>>().agents.clone();
    let path = request.uri().path().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let response = match agents.serve(&path) {
            Ok((body, mime)) => tauri::http::Response::builder()
                .header("Content-Type", mime)
                // Its scripts and styles from a CDN still load; what it reads of the folder is not sent out.
                .header("Content-Security-Policy", prism_remote::page_csp("prismfile:", "prismfile:"))
                .header("Cache-Control", "no-store")
                .body(body),
            Err((code, msg)) => tauri::http::Response::builder().status(code).header("Content-Type", "text/plain; charset=utf-8").body(msg.into_bytes()),
        };
        responder.respond(response.unwrap_or_else(|_| tauri::http::Response::new(Vec::new())));
    });
}

/// A module's web file for the `prism-module:` scheme (`prism-module://localhost/<id>/<path>`;
/// `http://prism-module.localhost/…` on Windows): its web folder only. Never cached, so a module
/// replaced shows on the window's next load.
pub fn serve_module_file(_ctx: tauri::UriSchemeContext<'_, tauri::Wry>, request: tauri::http::Request<Vec<u8>>, responder: tauri::UriSchemeResponder) {
    let path = request.uri().path().trim_start_matches('/').to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let found = path.split_once('/').ok_or_else(|| "no such file".to_string()).and_then(|(id, file)| prism_core::modules::web_file(id, file));
        let response = match found {
            Ok((body, mime)) => tauri::http::Response::builder()
                .header("Content-Type", mime)
                // The window (`tauri://localhost`) imports it as an ES module: another origin.
                .header("Access-Control-Allow-Origin", "*")
                .header("Cache-Control", "no-store")
                .body(body),
            Err(e) => tauri::http::Response::builder().status(404).header("Content-Type", "text/plain; charset=utf-8").body(e.into_bytes()),
        };
        responder.respond(response.unwrap_or_else(|_| tauri::http::Response::new(Vec::new())));
    });
}

/// SIGUSR1, caught from launch on (`catch_wake`), heard once the windows are up.
#[cfg(unix)]
static WAKE: std::sync::Mutex<Option<tokio::signal::unix::Signal>> = std::sync::Mutex::new(None);

/// Catches SIGUSR1 at once, before the app's place is claimed: a launch asking for a window
/// meanwhile must not end this Prism (the signal's default); its request waits in `host-open`.
#[cfg(unix)]
pub fn catch_wake() {
    let _guard = tauri::async_runtime::handle().inner().enter();
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()) {
        Ok(w) => *WAKE.lock().unwrap() = Some(w),
        Err(e) => tracing::warn!("SIGUSR1: {e}"),
    }
}

/// No files: what the engine leaves in the context it does not run (it keeps the real ones).
pub struct NoAssets;

impl tauri::Assets<tauri::Wry> for NoAssets {
    fn get(&self, _key: &tauri::utils::assets::AssetKey) -> Option<std::borrow::Cow<'_, [u8]>> {
        None
    }
    fn iter(&self) -> Box<tauri::utils::assets::AssetsIter<'_>> {
        Box::new(std::iter::empty())
    }
    fn csp_hashes(&self, _html_path: &tauri::utils::assets::AssetKey) -> Box<dyn Iterator<Item = tauri::utils::assets::CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

/// Windows: what SIGUSR1 / SIGUSR2 are on Unix, through this Prism's pipe
/// (`prism_sys::windows::signal_pipe`): a launch of the same profile brings the window up
/// (`profile::WAKE`), `prism restart` restarts it (`profile::RESTART`), `profile::stop` quits it
/// (`profile::STOP`). An instance per message, the next one made before the connected one is read.
#[cfg(windows)]
pub fn listen_signals(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(prism_sys::windows::listen_signals(move |byte| match byte {
        crate::profile::WAKE => {
            // Windows are the main thread's.
            let app = handle.clone();
            let _ = handle.run_on_main_thread(move || crate::profile_windows::asked(&app));
        }
        crate::profile::RESTART => {
            tracing::info!("prism restart: restarting");
            self::restart(&handle);
        }
        // Unix's SIGTERM (`profile::stop`): quits as the menu's Quit does.
        crate::profile::STOP => {
            tracing::info!("asked to stop: quitting");
            handle.exit(0);
        }
        _ => {}
    }));
}

/// The signals another launch sends: SIGUSR1 shows what it asked (`profile::ask_host`: a
/// profile's window, `prism --profile perso`; otherwise the main window), SIGUSR2 restarts
/// (`prism restart`).
#[cfg(unix)]
pub fn listen_signals(app: &AppHandle) {
    let handle = app.clone();
    let caught = WAKE.lock().unwrap().take();
    tauri::async_runtime::spawn(async move {
        let mut wake = match caught {
            Some(w) => w,
            None => match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()) {
                Ok(w) => w,
                Err(_) => return,
            },
        };
        while wake.recv().await.is_some() {
            // Windows are the main thread's.
            let app = handle.clone();
            let _ = handle.run_on_main_thread(move || crate::profile_windows::asked(&app));
        }
    });
    // `prism restart` asks this Prism to restart.
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(mut restart) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined2()) else { return };
        if restart.recv().await.is_some() {
            tracing::info!("prism restart: restarting");
            self::restart(&handle);
        }
    });
}
