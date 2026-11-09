//! The Sound block (`[sound]`): the output and the microphone, their device, volume and mute,
//! changed from the block; and an alert when the microphone's input volume (its gain) is too low,
//! notified when an application records from it (muted is not one). Read from the audio devices'
//! settings — CoreAudio on macOS, `pactl` on Linux, Core Audio (WASAPI) on Windows: nothing is
//! ever listened to, no permission asked. Its window side: `web/src/blocks/sound/`.

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{sdk::RemoteClass, Block, BlockCtx};

mod collector;
#[cfg(target_os = "macos")]
mod coreaudio;
mod demo;
/// Linux's side. Its parsing compiles on every OS, tested from any of them.
#[cfg(any(target_os = "linux", test))]
mod pulse;
#[cfg(windows)]
mod wasapi;

pub struct Sound;

pub static BLOCK: Sound = Sound;

impl Block for Sound {
    fn id(&self) -> &'static str {
        "sound"
    }

    fn remote(&self, _command: &str, _args: &Value) -> RemoteClass {
        RemoteClass::Never
    }

    fn reloads(&self) -> bool {
        true
    }

    fn start(&self, config: &toml::Table, ctx: BlockCtx) -> Result<()> {
        let cfg: SoundConfig = super::section(config, "sound")?;
        if cfg.enabled {
            // The block knows it is on before the first read answers.
            ctx.publish(&SoundState { limits: cfg.limits(), ..Default::default() });
            prism_base::rt::spawn(collector::run(cfg, ctx));
        }
        Ok(())
    }

    fn settings(&self, config: &toml::Table) -> Vec<(&'static str, Value)> {
        super::section::<SoundConfig>(config, "sound").ok().and_then(|c| serde_json::to_value(c).ok()).map(|v| vec![("sound", v)]).unwrap_or_default()
    }

    /// The microphone too low.
    fn alerts(&self, state: &Value) -> Vec<prism_core::alerts::Alert> {
        crate::alerts::sound(state)
    }

    fn call(&self, command: String, args: Value) -> BoxFuture<'static, Result<Value>> {
        Box::pin(async move {
            let scope = match args.get("side").and_then(Value::as_str) {
                Some("input") => Scope::Input,
                Some("output") => Scope::Output,
                other => bail!("no side {other:?}: input or output"),
            };
            match command.as_str() {
                // Its volume, 0 to 100.
                "volume" => {
                    let Some(v) = args.get("value").and_then(Value::as_u64) else { bail!("volume: a value from 0 to 100") };
                    collector::set_volume(scope, v.min(100) as u8).await?
                }
                "mute" => collector::set_mute(scope, args.get("muted").and_then(Value::as_bool).unwrap_or(true)).await?,
                // The device the system defaults it to, by its id (`Device::id`).
                "device" => {
                    let Some(id) = args.get("id").and_then(Value::as_u64) else { bail!("device: an id") };
                    collector::set_device(scope, id as u32).await?
                }
                other => bail!("the Sound block has no command {other}"),
            }
            Ok(Value::Null)
        })
    }

    fn demo(&self, now: DateTime<Utc>) -> Option<Value> {
        serde_json::to_value(demo::state(now)).ok()
    }
}

/// `[sound]`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct SoundConfig {
    /// On by default: the audio devices' settings (CoreAudio, `pactl`, Windows' Core Audio), read locally every `poll_seconds`.
    pub enabled: bool,
    /// The microphone's input volume under this % is an alert (orange), notified when an
    /// application records from it. 0: never.
    pub min_input_gain: u8,
    /// An input volume under this % is a microphone turned off (a headset's boom folded up
    /// sets it to 0–1 %), like a muted one: no alert. 0: every volume counts.
    pub muted_under: u8,
    /// What "Raise" puts the input volume back to, in %.
    pub restore_gain: u8,
    /// The status bar's item always there, with the output volume and a panel to set it; off:
    /// only while the microphone is too low.
    pub status_volume: bool,
    /// Read again this often; 1 at least.
    pub poll_seconds: u64,
}

impl Default for SoundConfig {
    fn default() -> Self {
        Self { enabled: true, min_input_gain: 40, muted_under: 3, restore_gain: 75, status_volume: true, poll_seconds: 2 }
    }
}

impl SoundConfig {
    fn limits(&self) -> Limits {
        Limits { min_input_gain: self.min_input_gain.min(100), muted_under: self.muted_under.min(100), restore_gain: self.restore_gain.clamp(1, 100), status_volume: self.status_volume }
    }
}

/// What the window needs of `[sound]` for its alerts, its "Raise" button and its status bar item.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SoundLimits"))]
pub struct Limits {
    pub min_input_gain: u8,
    pub muted_under: u8,
    pub restore_gain: u8,
    pub status_volume: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Input,
    Output,
}

impl Scope {
    #[cfg_attr(not(any(target_os = "macos", target_os = "linux", windows)), allow(dead_code))]
    fn word(self) -> &'static str {
        match self {
            Scope::Input => "input",
            Scope::Output => "output",
        }
    }
}

/// What the block shows, every `poll_seconds`.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SoundState {
    /// The default output and input devices; absent: none (or not read yet).
    pub output: Option<Side>,
    pub input: Option<Side>,
    /// Every device, inputs and outputs, in CoreAudio's order.
    pub devices: Vec<Device>,
    /// The applications recording from a microphone now (macOS 14 and later, Linux, Windows).
    pub recording: Vec<Listener>,
    pub limits: Limits,
    pub updated_at: Option<DateTime<Utc>>,
}

/// A default device and its settings.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SoundSide"))]
pub struct Side {
    /// CoreAudio's id, for the session only.
    pub device: u32,
    pub name: String,
    /// `builtin`, `bluetooth`, `usb`, `display`, `airplay`, `virtual`, `continuity`, `thunderbolt`, `other`.
    #[cfg_attr(feature = "ts", ts(type = "'builtin' | 'bluetooth' | 'usb' | 'display' | 'airplay' | 'virtual' | 'continuity' | 'thunderbolt' | 'other'"))]
    pub transport: String,
    /// 0 to 100; absent: the device has no volume (a digital output).
    pub volume: Option<u8>,
    pub volume_settable: bool,
    /// Absent: it cannot be muted at the device.
    pub muted: Option<bool>,
    pub mute_settable: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SoundDevice"))]
pub struct Device {
    pub id: u32,
    pub name: String,
    pub input: bool,
    pub output: bool,
    #[cfg_attr(feature = "ts", ts(type = "'builtin' | 'bluetooth' | 'usb' | 'display' | 'airplay' | 'virtual' | 'continuity' | 'thunderbolt' | 'other'"))]
    pub transport: String,
}

/// An application recording from a microphone.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "SoundRecorder"))]
pub struct Listener {
    /// Its name (`Google Chrome`), else its bundle id.
    pub app: String,
    pub bundle: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_limits() {
        let cfg: SoundConfig = toml::from_str("").unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.limits(), Limits { min_input_gain: 40, muted_under: 3, restore_gain: 75, status_volume: true });
        let cfg: SoundConfig = toml::from_str("min_input_gain = 250\nrestore_gain = 0").unwrap();
        assert_eq!((cfg.limits().min_input_gain, cfg.limits().restore_gain), (100, 1));
    }
}
