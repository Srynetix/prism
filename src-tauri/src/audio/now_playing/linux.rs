//! Linux's "Now Playing": MPRIS2 through `playerctl`, which follows the most recently active
//! player (a new player takes it over, like macOS's widget) and prints a line on each change.
//! The fields are one unit separator apart; the times come as `[H:]MM:SS`, `(--)` for a live
//! stream's.

use anyhow::{bail, Context, Result};
use prism_base::tr;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{Latest, NowPlaying};

/// One state: the player, its status, the track, and the two times.
const FORMAT: &str = "{{playerName}}\u{1f}{{status}}\u{1f}{{title}}\u{1f}{{artist}}\u{1f}{{album}}\u{1f}{{duration}}\u{1f}{{position}}";

pub(super) async fn watch(app: &AppHandle, latest: &Latest) -> Result<()> {
    let mut child = Command::new("playerctl")
        .args(["--follow", "metadata", "--format", FORMAT])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| tr!("{program} not found", program = "playerctl"))?;
    super::WATCHER.store(child.id().unwrap_or(0), std::sync::atomic::Ordering::SeqCst);
    let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
    let mut last: Option<NowPlaying> = None;
    while let Some(line) = lines.next_line().await? {
        let now = parse(&line);
        // Some players re-emit their state: the window is told of changes only.
        if now == last {
            continue;
        }
        last = now.clone();
        *latest.0.lock().unwrap() = now.clone();
        let _ = app.emit("now-playing", now);
    }
    bail!("playerctl stopped ({})", child.wait().await?)
}

/// One line of `playerctl --follow metadata`: player, status, title, artist, album, duration,
/// position. No player of ours (the WebView publishes too); no title, no track.
fn parse(line: &str) -> Option<NowPlaying> {
    let mut at = line.split('\u{1f}');
    let player = at.next()?.trim();
    if player.is_empty() || player.to_lowercase().contains("prism") {
        return None;
    }
    let status = at.next()?.trim();
    let title = at.next()?.trim();
    if title.is_empty() {
        return None;
    }
    Some(NowPlaying {
        title: title.to_string(),
        artist: maybe(at.next()),
        album: maybe(at.next()),
        app: Some(player.to_string()),
        app_name: Some(player.to_string()),
        playing: status == "Playing",
        duration: at.next().and_then(time),
        elapsed: at.next().and_then(time),
    })
}

fn maybe(at: Option<&str>) -> Option<String> {
    at.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// A time of `playerctl`'s format (`4:35`, `1:05:30`, a plain number), as seconds. A live stream
/// has none (`(--):(--)`, empty).
fn time(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s.contains('(') {
        return None;
    }
    s.split(':').try_fold(0.0f64, |acc, part| Some(acc * 60.0 + part.parse::<f64>().ok()?))
}

/// Command to the active player of "Now Playing".
pub(super) async fn command(cmd: &str) -> Result<()> {
    let arg = match cmd {
        "play" => "play",
        "pause" => "pause",
        "toggle" => "play-pause",
        "next" => "next",
        "previous" => "previous",
        _ => bail!("unknown command {cmd}"),
    };
    let out = Command::new("playerctl").arg(arg).output().await.with_context(|| tr!("{program} not found", program = "playerctl"))?;
    if !out.status.success() {
        bail!("command {cmd} refused: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: char = '\u{1f}';

    #[test]
    fn reads_a_playing_track() {
        let line = format!("spotify{AT}Playing{AT}Song Title{AT}The Artist{AT}The Album{AT}4:35{AT}1:05");
        let n = parse(&line).unwrap();
        assert_eq!(n.title, "Song Title");
        assert_eq!(n.artist.as_deref(), Some("The Artist"));
        assert_eq!(n.album.as_deref(), Some("The Album"));
        assert_eq!(n.app.as_deref(), Some("spotify"));
        assert_eq!(n.app_name.as_deref(), Some("spotify"));
        assert!(n.playing);
        assert_eq!(n.duration, Some(275.0));
        assert_eq!(n.elapsed, Some(65.0));
    }

    #[test]
    fn pauses_empties_and_skips() {
        let n = parse(&format!("spotify{AT}Paused{AT}Song{AT}{AT}{AT}(--):(--){AT}")).unwrap();
        assert!(!n.playing);
        assert_eq!(n.artist, None);
        // A live stream: no duration, no position.
        assert_eq!(n.duration, None);
        assert_eq!(n.elapsed, None);
        // No title (a stopped player), one of ours, not a line of ours: no state.
        assert_eq!(parse(&format!("spotify{AT}Stopped{AT}{AT}{AT}{AT}{AT}")), None);
        assert_eq!(parse(&format!("prism{AT}Playing{AT}X{AT}{AT}{AT}{AT}{AT}")), None);
        assert_eq!(parse("no fields"), None);
        // Hours, and an unknown time.
        assert_eq!(time("1:05:30"), Some(3930.0));
        assert_eq!(time(""), None);
    }
}
