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
pub fn restart(app: &tauri::AppHandle) -> ! {
    // The new Prism must find the profile free, not this one about to exit.
    crate::profile::release();
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

/// The signals another launch sends: SIGUSR1 brings the window up (`prism --profile`, Open
/// alongside), SIGUSR2 restarts (`prism restart`).
#[cfg(unix)]
pub fn listen_signals(app: &AppHandle) {
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(mut wake) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::user_defined1()) else { return };
        while wake.recv().await.is_some() {
            show_main(&handle);
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
