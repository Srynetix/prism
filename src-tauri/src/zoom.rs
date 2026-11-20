//! The whole window's zoom (`[appearance] zoom`): the webview's own, as a browser's, so text,
//! spaces, icons, pictures and terminals all scale, and the page sees a narrower or wider window.
//! Each window asks for its profile's (`zoom.svelte.ts`); a machine's window shows at the main
//! window's, kept here, told again (`ui-zoom`) when it changes.

use std::sync::Mutex;

use prism_commands::commands::Cmd;
use tauri::{AppHandle, Emitter, Manager};

/// The main window's zoom, for the machines' windows.
pub struct Zoom(Mutex<f64>);

impl Default for Zoom {
    fn default() -> Self {
        Self(Mutex::new(1.0))
    }
}

/// From 50 % to 200 %; anything else (not a number) is 100 %.
pub fn clamp(zoom: f64) -> f64 {
    if zoom.is_finite() && zoom > 0.0 { zoom.clamp(0.5, 2.0) } else { 1.0 }
}

/// The calling window shown at `zoom`; the main window's also goes to the machines' windows.
#[tauri::command]
pub fn ui_zoom(app: AppHandle, webview: tauri::Webview, zoom: f64) -> Cmd {
    let zoom = clamp(zoom);
    webview.set_zoom(zoom)?;
    if webview.label() == "main" {
        *app.state::<Zoom>().0.lock().unwrap() = zoom;
        for (label, w) in app.webview_windows() {
            if label.starts_with(crate::machines::PREFIX) {
                let _ = w.set_zoom(zoom);
            }
        }
        let _ = app.emit("ui-zoom", zoom);
    }
    Ok(())
}

/// The main window's zoom: what a machine's window shows at.
#[tauri::command]
pub fn ui_zoom_main(app: AppHandle) -> f64 {
    *app.state::<Zoom>().0.lock().unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_the_zoom_between_half_and_double() {
        assert_eq!(super::clamp(1.25), 1.25);
        assert_eq!(super::clamp(0.1), 0.5);
        assert_eq!(super::clamp(5.0), 2.0);
        assert_eq!(super::clamp(f64::NAN), 1.0);
        assert_eq!(super::clamp(0.0), 1.0);
    }
}
