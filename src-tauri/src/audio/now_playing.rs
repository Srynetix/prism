//! "Now Playing" provider: what macOS shows in its widget (Spotify in the browser,
//! YouTube, Music…), and its commands. MediaRemote no longer answers third-party apps since
//! macOS 15.4; it still answers osascript, hence a JXA script (`now_playing.js`) launched once
//! that writes a JSON line on each change.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const SCRIPT: &str = include_str!("now_playing.js");

/// Our own player also publishes to "Now Playing": we do not show it twice.
const OWN_APP: &str = "app.prism.desktop";

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

/// Reads "Now Playing" continuously and emits `now-playing` on each change. The script is
/// relaunched if it stops; after several failures in a row (macOS may have closed it), the
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

/// The running script, to stop when Prism quits: the task that owns it is not
/// cleaned up on exit, `kill_on_drop` is not enough.
static WATCHER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// On Prism's exit: stops the "Now Playing" script.
pub fn stop() {
    let pid = WATCHER.swap(0, std::sync::atomic::Ordering::SeqCst);
    if pid != 0 {
        let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
    }
}

async fn watch(app: &AppHandle, latest: &Latest) -> Result<()> {
    let mut child = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT, "watch"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    WATCHER.store(child.id().unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
    let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    while let Some(line) = lines.next_line().await? {
        let now = match serde_json::from_str::<Option<NowPlaying>>(&line) {
            Ok(now) => now.filter(|n| n.app.as_deref() != Some(OWN_APP)),
            Err(e) => {
                tracing::debug!("now playing: unreadable line ({e}): {line}");
                continue;
            }
        };
        *latest.0.lock().unwrap() = now.clone();
        let _ = app.emit("now-playing", now);
    }
    bail!("osascript stopped ({})", child.wait().await?)
}

/// Command to the active player of "Now Playing".
pub async fn command(cmd: &str) -> Result<()> {
    let code = match cmd {
        "play" => 0,
        "pause" => 1,
        "toggle" => 2,
        "next" => 4,
        "previous" => 5,
        _ => bail!("unknown command {cmd}"),
    };
    let out = Command::new("osascript").args(["-l", "JavaScript", "-e", SCRIPT, "cmd", &code.to_string()]).output().await?;
    if !out.status.success() || String::from_utf8_lossy(&out.stdout).trim() != "true" {
        bail!("command {cmd} refused: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads "Now Playing" for real: `cargo test -- --ignored now_playing`.
    #[tokio::test]
    #[ignore = "needs macOS and something playing"]
    async fn reads_now_playing() {
        let out = Command::new("osascript").args(["-l", "JavaScript", "-e", SCRIPT, "once"]).output().await.unwrap();
        let line = String::from_utf8(out.stdout).unwrap();
        let now: Option<NowPlaying> = serde_json::from_str(line.trim()).unwrap();
        println!("{now:?}");
    }
}
