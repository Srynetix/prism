//! macOS's audio devices through CoreAudio's property API (`AudioObjectGetPropertyData` and the
//! like): no permission, nothing listened to. Each property is read by its four-letter code from
//! `AudioHardware.h`; a device that lacks one (a volume on a digital output) answers `None`.

use anyhow::{bail, Result};
use prism_base::{i18n::tr, tr};
use prism_macos::coreaudio::{self as ca, get, has, ids, settable, string, Address, ObjectId, SYSTEM};

use super::{Device, Listener, Scope, Side};

const fn code(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const GLOBAL: u32 = code(b"glob");
const INPUT: u32 = code(b"inpt");
const OUTPUT: u32 = code(b"outp");
const MAIN: u32 = 0;

const DEVICES: u32 = code(b"dev#");
const DEFAULT_INPUT: u32 = code(b"dIn ");
const DEFAULT_OUTPUT: u32 = code(b"dOut");
const PROCESSES: u32 = code(b"prs#");
const NAME: u32 = code(b"lnam");
const STREAMS: u32 = code(b"stm#");
const TRANSPORT: u32 = code(b"tran");
const VOLUME: u32 = code(b"volm");
const MUTE: u32 = code(b"mute");
/// `kAudioHardwareServiceDeviceProperty_VirtualMainVolume`: the one volume macOS's menu moves,
/// over every channel.
const VIRTUAL_VOLUME: u32 = code(b"vmvc");
const PID: u32 = code(b"ppid");
const BUNDLE: u32 = code(b"pbid");
const RUNNING_INPUT: u32 = code(b"piri");

/// The system's own listeners, never "an app using the microphone": Siri's wake word, dictation.
const SYSTEM_LISTENERS: &[&str] = &["com.apple.corespeechd", "com.apple.CoreSpeech", "com.apple.assistantd", "com.apple.SpeechRecognitionCore", "com.apple.siri"];

const fn scope(s: Scope) -> u32 {
    match s {
        Scope::Input => INPUT,
        Scope::Output => OUTPUT,
    }
}

fn addr(selector: u32, scope: u32, element: u32) -> Address {
    Address { selector, scope, element }
}

fn set<T: ca::Value>(object: ObjectId, a: &Address, value: T) -> Result<()> {
    if let Err(status) = ca::set(object, a, value) {
        bail!(tr!("CoreAudio refused the change (status {status})", status = status));
    }
    Ok(())
}

fn has_streams(id: ObjectId, s: u32) -> bool {
    ca::size(id, &addr(STREAMS, s, MAIN)).is_some_and(|n| n > 0)
}

fn transport(id: ObjectId) -> &'static str {
    match get::<u32>(id, &addr(TRANSPORT, GLOBAL, MAIN)).map(u32::to_be_bytes).as_ref() {
        Some(b"bltn") => "builtin",
        Some(b"blue") | Some(b"blea") => "bluetooth",
        Some(b"usb ") => "usb",
        Some(b"hdmi") | Some(b"dprt") => "display",
        Some(b"airp") => "airplay",
        Some(b"virt") | Some(b"grup") | Some(b"fgrp") => "virtual",
        Some(b"ccwd") | Some(b"ccwl") => "continuity",
        Some(b"thun") => "thunderbolt",
        _ => "other",
    }
}

pub fn devices() -> Vec<Device> {
    ca::notify_on_own_thread();
    ids(SYSTEM, &addr(DEVICES, GLOBAL, MAIN))
        .into_iter()
        .map(|id| Device { id, name: string(id, &addr(NAME, GLOBAL, MAIN)).unwrap_or_else(|| format!("Device {id}")), input: has_streams(id, INPUT), output: has_streams(id, OUTPUT), transport: transport(id).into() })
        .filter(|d| d.input || d.output)
        .collect()
}

pub fn default_device(s: Scope) -> Option<ObjectId> {
    ca::notify_on_own_thread();
    get::<u32>(SYSTEM, &addr(if s == Scope::Input { DEFAULT_INPUT } else { DEFAULT_OUTPUT }, GLOBAL, MAIN)).filter(|id| *id != 0)
}

pub fn set_default(s: Scope, id: ObjectId) -> Result<()> {
    ca::notify_on_own_thread();
    set::<u32>(SYSTEM, &addr(if s == Scope::Input { DEFAULT_INPUT } else { DEFAULT_OUTPUT }, GLOBAL, MAIN), id)
}

/// Where a device's volume is: macOS's virtual main volume, else the main element's, else
/// channels 1 and 2 (a built-in microphone often has only those).
fn volume_addresses(id: ObjectId, s: Scope) -> Vec<Address> {
    let sc = scope(s);
    let virt = addr(VIRTUAL_VOLUME, sc, MAIN);
    if has(id, &virt) {
        return vec![virt];
    }
    let main = addr(VOLUME, sc, MAIN);
    if has(id, &main) {
        return vec![main];
    }
    (1..=2).map(|ch| addr(VOLUME, sc, ch)).filter(|a| has(id, a)).collect()
}

/// The default device of `s`: its name, its volume (0–100) and mute, and whether they can change.
pub fn side(s: Scope) -> Option<Side> {
    let id = default_device(s)?;
    let at = volume_addresses(id, s);
    let values: Vec<f32> = at.iter().filter_map(|a| get::<f32>(id, a)).collect();
    let mute = addr(MUTE, scope(s), MAIN);
    Some(Side {
        device: id,
        name: string(id, &addr(NAME, GLOBAL, MAIN)).unwrap_or_default(),
        transport: transport(id).into(),
        volume: (!values.is_empty()).then(|| (100.0 * values.iter().sum::<f32>() / values.len() as f32).round().clamp(0.0, 100.0) as u8),
        volume_settable: at.iter().any(|a| settable(id, a)),
        muted: get::<u32>(id, &mute).map(|m| m != 0),
        mute_settable: settable(id, &mute),
    })
}

pub fn set_volume(s: Scope, percent: u8) -> Result<()> {
    let Some(id) = default_device(s) else { bail!("no default {} device", s.word()) };
    let at: Vec<Address> = volume_addresses(id, s).into_iter().filter(|a| settable(id, a)).collect();
    if at.is_empty() {
        bail!(tr("this device's volume cannot be changed from the Mac"));
    }
    let v = f32::from(percent.min(100)) / 100.0;
    for a in &at {
        set::<f32>(id, a, v)?;
    }
    Ok(())
}

pub fn set_mute(s: Scope, muted: bool) -> Result<()> {
    let Some(id) = default_device(s) else { bail!("no default {} device", s.word()) };
    let a = addr(MUTE, scope(s), MAIN);
    if !settable(id, &a) {
        bail!(tr("this device cannot be muted from the Mac"));
    }
    set::<u32>(id, &a, u32::from(muted))
}

/// The applications recording from a microphone now (macOS 14 and later: the HAL's process
/// objects), the system's own listeners apart. `name` resolves a process (pid, bundle id) to its
/// application's name.
pub fn listeners(name: impl Fn(i32, &str) -> Option<String>) -> Vec<Listener> {
    ca::notify_on_own_thread();
    let mut out: Vec<Listener> = Vec::new();
    for p in ids(SYSTEM, &addr(PROCESSES, GLOBAL, MAIN)) {
        if get::<u32>(p, &addr(RUNNING_INPUT, GLOBAL, MAIN)) != Some(1) {
            continue;
        }
        let bundle = string(p, &addr(BUNDLE, GLOBAL, MAIN)).unwrap_or_default();
        if bundle.is_empty() || SYSTEM_LISTENERS.iter().any(|s| bundle.starts_with(s)) {
            continue;
        }
        let pid = get::<i32>(p, &addr(PID, GLOBAL, MAIN)).unwrap_or(0);
        // A browser records from a helper process: its application's name, once.
        let app = name(pid, &bundle).unwrap_or_else(|| bundle.clone());
        if !out.iter().any(|l| l.app == app) {
            out.push(Listener { app, bundle });
        }
    }
    out
}
