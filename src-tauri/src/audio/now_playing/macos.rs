//! macOS's "Now Playing": MediaRemote through osascript (an Apple-signed binary: it still
//! answers it, which it refuses to third-party apps since macOS 15.4). The script (`now_playing.js`)
//! writes a JSON line on standard output on each change, polling every second; `cmd <n>` sends a
//! command to the active player (0 play, 1 pause, 2 toggle, 4 next, 5 previous), `once` reads once.

use anyhow::{bail, Result};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{Latest, NowPlaying};

/// Our own player also publishes to "Now Playing": we do not show it twice.
const OWN_APP: &str = "app.prism.desktop";

const SCRIPT: &str = include_str!("now_playing.js");

pub(super) async fn watch(app: &AppHandle, latest: &Latest) -> Result<()> {
    let mut child = Command::new("osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT, "watch"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    super::WATCHER.store(child.id().unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
    let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    while let Some(line) = lines.next_line().await? {
        let now = parse(&line);
        *latest.0.lock().unwrap() = now.clone();
        let _ = app.emit("now-playing", now);
    }
    bail!("osascript stopped ({})", child.wait().await?)
}

fn parse(line: &str) -> Option<NowPlaying> {
    match serde_json::from_str::<Option<NowPlaying>>(line) {
        Ok(now) => now.filter(|n| n.app.as_deref() != Some(OWN_APP)),
        Err(e) => {
            tracing::debug!("now playing: unreadable line ({e}): {line}");
            None
        }
    }
}

/// Command to the active player of "Now Playing".
pub(super) async fn command(cmd: &str) -> Result<()> {
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
