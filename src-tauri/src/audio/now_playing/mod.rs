//! "Now Playing" provider: what the system shows in its widget (Spotify in the browser,
//! YouTube, Music…), and its commands. macOS: MediaRemote through osascript (`macos.rs`, a JXA
//! script — MediaRemote no longer answers third-party apps since macOS 15.4); Linux: MPRIS
//! through `playerctl` (`linux.rs`). Either writes a line on each change; the loop below
//! relaunches it, and the window is told through the `now-playing` event.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "External"))]
pub struct NowPlaying {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Identifier of the playing app (`com.brave.Browser`, `com.spotify.client`…).
    pub app: Option<String>,
    pub app_name: Option<String>,
    pub playing: bool,
    /// Seconds; absent for a live stream.
    pub duration: Option<f64>,
    pub elapsed: Option<f64>,
}

/// Last state read, for a front end that opens after a change.
#[derive(Clone, Default)]
pub struct Latest(Arc<Mutex<Option<NowPlaying>>>);

impl Latest {
    pub fn get(&self) -> Option<NowPlaying> {
        self.0.lock().unwrap().clone()
    }
}

/// Reads "Now Playing" continuously and emits `now-playing` on each change. The watcher is
/// relaunched if it stops; after several failures in a row (the system may have closed it), the
/// attempts are spaced out.
pub fn run(app: AppHandle, latest: Latest) {
    tauri::async_runtime::spawn(async move {
        let mut failures = 0u32;
        loop {
            let started = std::time::Instant::now();
            if let Err(e) = watch(&app, &latest).await {
                tracing::warn!("now playing: {e:#}");
            }
            failures = if started.elapsed() > Duration::from_secs(60) { 0 } else { failures + 1 };
            *latest.0.lock().unwrap() = None;
            let _ = app.emit("now-playing", None::<NowPlaying>);
            tokio::time::sleep(Duration::from_secs(if failures > 3 { 300 } else { 5 })).await;
        }
    });
}

/// One watching process's round: its lines until it stops.
async fn watch(app: &AppHandle, latest: &Latest) -> Result<()> {
    #[cfg(target_os = "macos")]
    return macos::watch(app, latest).await;
    #[cfg(target_os = "linux")]
    return linux::watch(app, latest).await;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    return Err(anyhow::anyhow!("Now Playing reads macOS and Linux's players only"));
}

/// Command to the active player of "Now Playing".
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub async fn command(cmd: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    return macos::command(cmd).await;
    #[cfg(target_os = "linux")]
    return linux::command(cmd).await;
}

/// The running watcher, to stop when Prism quits: the task that owns it is not cleaned up on
/// exit, `kill_on_drop` is not enough.
static WATCHER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// On Prism's exit: stops the "Now Playing" watcher.
pub fn stop() {
    let pid = WATCHER.swap(0, std::sync::atomic::Ordering::SeqCst);
    if pid != 0 {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    }
}
