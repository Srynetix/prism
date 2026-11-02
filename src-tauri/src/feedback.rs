//! Feedback to a pack's or Prism's project (`prism_ops::feedback`), and what only the window can
//! give it: a capture of itself.

use anyhow::Result;
#[cfg(target_os = "macos")]
use anyhow::{ensure, Context};
#[cfg(not(target_os = "macos"))]
use anyhow::bail;
#[cfg(not(target_os = "macos"))]
use prism_base::i18n::tr;

pub use prism_ops::feedback::*;


/// A capture of Prism's window, written to `path` and put on the clipboard too (GitHub's API takes
/// no attachment: it is pasted there); a small JPEG of it, for the composer to show. macOS only:
/// `screencapture` of the window's rectangle, `sips` for the thumbnail.
#[cfg(target_os = "macos")]
pub async fn capture(window: &tauri::WebviewWindow, path: &std::path::Path) -> Result<Vec<u8>> {
    let scale = window.scale_factor()?;
    let pos = window.outer_position()?.to_logical::<f64>(scale);
    let size = window.outer_size()?.to_logical::<f64>(scale);
    let rect = format!("{},{},{},{}", pos.x.round(), pos.y.round(), size.width.round(), size.height.round());
    let file = path.to_string_lossy();
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
    Ok(bytes)
}

#[cfg(not(target_os = "macos"))]
pub async fn capture(_window: &tauri::WebviewWindow, _path: &std::path::Path) -> Result<Vec<u8>> {
    bail!(tr("a capture of the window is made on macOS only"))
}
