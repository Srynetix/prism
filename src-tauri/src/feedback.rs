//! Feedback to a pack's or Prism's project (`prism_ops::feedback`), and what only the window can
//! give it: a capture of itself.

use anyhow::Result;
#[cfg(target_os = "macos")]
use anyhow::{ensure, Context};
#[cfg(target_os = "linux")]
use anyhow::{bail, ensure, Context};
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
use anyhow::bail;
#[cfg(not(target_os = "macos"))]
use prism_base::i18n::tr;

pub use prism_ops::feedback::*;

/// A capture of Prism's window, written to `path` (the full picture, the one the forge's upload
/// takes) and put on the clipboard too (to paste into a comment); a data URI of it, for the
/// composer to show. macOS: `screencapture` of the window's rectangle, `sips` for the thumbnail. Linux: `spectacle` (KDE, the active window), `scrot` or `import`
/// (X11, the window's rectangle), `grim` (Wayland without them, the whole screen); the clipboard
/// by `wl-copy` or `xclip`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub async fn capture(window: &tauri::WebviewWindow, path: &std::path::Path) -> Result<String> {
    let file = path.to_string_lossy().into_owned();
    #[cfg(target_os = "macos")]
    let (bytes, mime) = {
        let scale = window.scale_factor()?;
        let pos = window.outer_position()?.to_logical::<f64>(scale);
        let size = window.outer_size()?.to_logical::<f64>(scale);
        let rect = format!("{},{},{},{}", pos.x.round(), pos.y.round(), size.width.round(), size.height.round());
        let out = crate::platform::command("screencapture").args(["-x", "-t", "png", "-R", &rect, &file]).output().await.context("screencapture")?;
        ensure!(out.status.success(), "screencapture: {}", String::from_utf8_lossy(&out.stderr).trim());
        // The path goes in as an argument, never into the script's text.
        let script = "on run argv\nset the clipboard to (read (POSIX file (item 1 of argv)) as «class PNGf»)\nend run";
        let out = crate::platform::command("osascript").args(["-e", script, &file]).output().await.context("osascript")?;
        ensure!(out.status.success(), "osascript: {}", String::from_utf8_lossy(&out.stderr).trim());
        let thumb = path.with_extension("thumb.jpg");
        let out = crate::platform::command("sips").args(["-Z", "640", "-s", "format", "jpeg", &file, "--out"]).arg(&thumb).output().await.context("sips")?;
        ensure!(out.status.success(), "sips: {}", String::from_utf8_lossy(&out.stderr).trim());
        let bytes = std::fs::read(&thumb)?;
        let _ = std::fs::remove_file(&thumb);
        (bytes, "image/jpeg")
    };
    #[cfg(target_os = "linux")]
    let (bytes, mime) = {
        let found = prism_core::detect::which;
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        if found("spectacle").is_some() {
            // KDE: the active window — the composer is Prism's own, Prism has the focus.
            let out = crate::platform::command("spectacle").args(["-b", "-a", "-n", "-o", &file]).output().await.context("spectacle")?;
            ensure!(out.status.success(), "spectacle: {}", String::from_utf8_lossy(&out.stderr).trim());
        } else if wayland && found("grim").is_some() {
            // wlroots and the like: the whole screen.
            let out = crate::platform::command("grim").arg(&file).output().await.context("grim")?;
            ensure!(out.status.success(), "grim: {}", String::from_utf8_lossy(&out.stderr).trim());
        } else if !wayland {
            // X11: the window's rectangle, `scrot` or ImageMagick's `import`.
            let scale = window.scale_factor()?;
            let pos = window.outer_position()?.to_logical::<f64>(scale);
            let size = window.outer_size()?.to_logical::<f64>(scale);
            let (x, y, w, h) = (pos.x.round() as i32, pos.y.round() as i32, size.width.round() as i32, size.height.round() as i32);
            let (csv, geometry) = (format!("{x},{y},{w},{h}"), format!("{w}x{h}+{x}+{y}"));
            let (program, args): (&str, Vec<&str>) = if found("scrot").is_some() {
                ("scrot", vec!["-a", &csv, "-o", &file])
            } else if found("import").is_some() {
                ("import", vec!["-window", "root", "-crop", &geometry, &file])
            } else {
                bail!(tr("a capture of the window needs spectacle (KDE), grim (Wayland) or scrot/import (X11)"))
            };
            let out = crate::platform::command(program).args(&args).output().await.context(program)?;
            ensure!(out.status.success(), "{program}: {}", String::from_utf8_lossy(&out.stderr).trim());
        } else {
            bail!(tr("a capture of the window needs spectacle (KDE), grim (Wayland) or scrot/import (X11)"))
        };
        // The clipboard: `wl-copy` drinks its stdin, `xclip` takes a file.
        let (program, args): (&str, Vec<&str>) = if wayland { ("wl-copy", vec![]) } else { ("xclip", vec!["-selection", "clipboard", "-t", "image/png", "-i"]) };
        let png = std::fs::File::open(path)?;
        let out = crate::platform::command(program).args(&args).stdin(std::process::Stdio::from(png)).output().await.context(program)?;
        ensure!(out.status.success(), "{program}: {}", String::from_utf8_lossy(&out.stderr).trim());
        (std::fs::read(path)?, "image/png")
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let (bytes, mime) = {
        let _ = (window, path);
        bail!(tr("the capture of the window is made on macOS and Linux only"))
    };
    Ok(uri(bytes, mime))
}

/// `data:<mime>;base64,…`: the composer shows it as it comes.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn uri(bytes: Vec<u8>, mime: &str) -> String {
    use base64::Engine;
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}
