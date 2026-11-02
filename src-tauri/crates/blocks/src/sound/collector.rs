//! Reads CoreAudio every `[sound] poll_seconds`, and at once after a change from the window.
//! Elsewhere than macOS the block says it cannot read.

use std::time::Duration;

use anyhow::Result;
use tokio::sync::Notify;

use super::{Scope, SoundConfig, SoundState};
use crate::BlockCtx;

/// A change from the window: the next read is now.
static CHANGED: Notify = Notify::const_new();

pub async fn run(cfg: SoundConfig, ctx: BlockCtx) {
    let every = Duration::from_secs(cfg.poll_seconds.max(1));
    let limits = cfg.limits();
    let mut last: Option<SoundState> = None;
    loop {
        match tokio::task::spawn_blocking(read).await.map_err(anyhow::Error::from).and_then(|r| r) {
            Ok(mut state) => {
                state.limits = limits.clone();
                // Nothing moved: the read's time stays the former one, so the snapshot does not
                // change (a window update every poll, for nothing).
                if let Some(prev) = &last {
                    let read_at = std::mem::replace(&mut state.updated_at, prev.updated_at);
                    if state != *prev {
                        state.updated_at = read_at;
                    }
                }
                ctx.publish(&state);
                last = Some(state);
                ctx.set_error(None);
            }
            Err(e) => ctx.set_error(Some(format!("{e:#}"))),
        }
        tokio::select! {
            _ = ctx.pause(every) => {}
            _ = CHANGED.notified() => {}
        }
    }
}

#[cfg(target_os = "macos")]
fn read() -> Result<SoundState> {
    use super::coreaudio as ca;
    use chrono::Utc;
    Ok(SoundState { output: ca::side(Scope::Output), input: ca::side(Scope::Input), devices: ca::devices(), recording: ca::listeners(app_name), updated_at: Some(Utc::now()), ..Default::default() })
}

#[cfg(not(target_os = "macos"))]
fn read() -> Result<SoundState> {
    anyhow::bail!(prism_base::i18n::tr("the Sound block reads macOS's audio devices only"))
}

/// Runs a change off the async threads (CoreAudio blocks briefly), then reads again.
async fn change(f: impl FnOnce() -> Result<()> + Send + 'static) -> Result<()> {
    tokio::task::spawn_blocking(f).await??;
    CHANGED.notify_one();
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn set_volume(s: Scope, percent: u8) -> Result<()> {
    change(move || super::coreaudio::set_volume(s, percent)).await
}

#[cfg(target_os = "macos")]
pub async fn set_mute(s: Scope, muted: bool) -> Result<()> {
    change(move || super::coreaudio::set_mute(s, muted)).await
}

#[cfg(target_os = "macos")]
pub async fn set_device(s: Scope, id: u32) -> Result<()> {
    change(move || super::coreaudio::set_default(s, id)).await
}

#[cfg(not(target_os = "macos"))]
pub async fn set_volume(_: Scope, _: u8) -> Result<()> {
    change(|| anyhow::bail!(prism_base::i18n::tr("macOS only"))).await
}

#[cfg(not(target_os = "macos"))]
pub async fn set_mute(_: Scope, _: bool) -> Result<()> {
    change(|| anyhow::bail!(prism_base::i18n::tr("macOS only"))).await
}

#[cfg(not(target_os = "macos"))]
pub async fn set_device(_: Scope, _: u32) -> Result<()> {
    change(|| anyhow::bail!(prism_base::i18n::tr("macOS only"))).await
}

/// The application a recording process belongs to: a browser's helper (`com.google.Chrome.helper`)
/// counts as its browser.
#[cfg(target_os = "macos")]
fn app_name(pid: i32, bundle: &str) -> Option<String> {
    let base = bundle.find(".helper").or_else(|| bundle.find(".Helper")).map_or(bundle, |i| &bundle[..i]);
    prism_macos::appkit::running_app_name(base, pid)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    /// This Mac's devices, read, and the output volume set to what it already is:
    /// `cargo test -p prism-blocks sound -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the machine's audio devices"]
    fn reads_this_mac() {
        let s = super::read().unwrap();
        println!("{}", serde_json::to_string_pretty(&s).unwrap());
        assert!(s.output.is_some() || s.devices.is_empty());
        if let Some(v) = s.output.as_ref().filter(|o| o.volume_settable).and_then(|o| o.volume) {
            super::super::coreaudio::set_volume(super::Scope::Output, v).unwrap();
            assert_eq!(super::read().unwrap().output.and_then(|o| o.volume), Some(v));
        }
    }
}
