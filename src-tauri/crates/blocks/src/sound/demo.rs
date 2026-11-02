//! The Sound block in the demo profile: AirPods out, the built-in microphone too low while a call
//! records from it.

use chrono::{DateTime, Utc};

use super::{Device, Limits, Listener, Side, SoundState};

pub fn state(now: DateTime<Utc>) -> SoundState {
    let device = |id: u32, name: &str, input: bool, output: bool, transport: &str| Device { id, name: name.into(), input, output, transport: transport.into() };
    SoundState {
        output: Some(Side { device: 61, name: "AirPods Pro".into(), transport: "bluetooth".into(), volume: Some(62), volume_settable: true, muted: Some(false), mute_settable: true }),
        input: Some(Side { device: 82, name: "MacBook Pro Microphone".into(), transport: "builtin".into(), volume: Some(18), volume_settable: true, muted: None, mute_settable: false }),
        devices: vec![
            device(61, "AirPods Pro", true, true, "bluetooth"),
            device(75, "MacBook Pro Speakers", false, true, "builtin"),
            device(82, "MacBook Pro Microphone", true, false, "builtin"),
            device(90, "Studio Display Speakers", false, true, "display"),
            device(93, "Shure MV7", true, false, "usb"),
        ],
        recording: vec![Listener { app: "Google Chrome".into(), bundle: "com.google.Chrome.helper".into() }],
        limits: Limits { min_input_gain: 40, muted_under: 3, restore_gain: 75, status_volume: true },
        updated_at: Some(now),
    }
}
