//! Windows' side of the Sound block: the devices read through Core Audio (`prism_sys::audio`),
//! as the block shows them.

use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use prism_base::i18n::tr;
use prism_sys::audio::{self, Endpoint};

use super::{Device, Listener, Scope, Side};

/// The devices' Windows ids seen this session: a device's `id` is its place here, plus one.
static IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn number_of(id: &str) -> u32 {
    let mut ids = IDS.lock().unwrap_or_else(|e| e.into_inner());
    let at = ids.iter().position(|i| i == id).unwrap_or_else(|| {
        ids.push(id.to_string());
        ids.len() - 1
    });
    at as u32 + 1
}

fn id_of(number: u32) -> Option<String> {
    IDS.lock().unwrap_or_else(|e| e.into_inner()).get(number.checked_sub(1)? as usize).cloned()
}

/// How a device is connected, from the bus Windows found it on (`BTHENUM`, `USB`, `HDAUDIO`…)
/// and its form factor (9: a display's audio).
fn transport(enumerator: &str, form_factor: Option<u32>) -> &'static str {
    let e = enumerator.to_ascii_uppercase();
    if e.starts_with("BTH") {
        "bluetooth"
    } else if form_factor == Some(9) {
        "display"
    } else if e == "USB" {
        "usb"
    } else if ["HDAUDIO", "INTELAUDIO", "ACPI", "PCI"].contains(&e.as_str()) {
        "builtin"
    } else if e == "ROOT" || e == "SWD" {
        "virtual"
    } else {
        "other"
    }
}

fn name(e: &Endpoint) -> String {
    if e.name.is_empty() { tr("Unknown device").into() } else { e.name.clone() }
}

pub fn devices() -> Vec<Device> {
    audio::endpoints().iter().map(|e| Device { id: number_of(&e.id), name: name(e), input: e.input, output: !e.input, transport: transport(&e.enumerator, e.form_factor).into() }).collect()
}

pub fn side(s: Scope) -> Option<Side> {
    let (e, level) = audio::default_endpoint(s == Scope::Input)?;
    let volume = level.volume.map(|f| (f.clamp(0.0, 1.0) * 100.0).round() as u8);
    Some(Side { device: number_of(&e.id), name: name(&e), transport: transport(&e.enumerator, e.form_factor).into(), volume_settable: volume.is_some(), volume, mute_settable: level.muted.is_some(), muted: level.muted })
}

pub fn set_volume(s: Scope, percent: u8) -> Result<()> {
    audio::set_volume(s == Scope::Input, f32::from(percent.min(100)) / 100.0).with_context(|| tr("setting the volume"))
}

pub fn set_mute(s: Scope, muted: bool) -> Result<()> {
    audio::set_mute(s == Scope::Input, muted).with_context(|| tr("muting"))
}

pub fn set_default(s: Scope, number: u32) -> Result<()> {
    let id = id_of(number).ok_or_else(|| anyhow!("no device {number}"))?;
    let done = audio::set_default(&id, s == Scope::Input).with_context(|| tr("the default device cannot be changed"))?;
    anyhow::ensure!(done, "not an {} device", s.word());
    Ok(())
}

/// The applications recording now, by their program (`Teams`, `chrome`).
pub fn listeners() -> Vec<Listener> {
    let mut out: Vec<Listener> = Vec::new();
    for pid in audio::recording() {
        let Some(exe) = prism_sys::windows::executable(pid) else { continue };
        let app = exe.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let bundle = exe.display().to_string();
        if !app.is_empty() && !out.iter().any(|l| l.bundle == bundle) {
            out.push(Listener { app, bundle });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_device_s_connection() {
        use super::transport;
        assert_eq!(transport("BTHENUM", Some(5)), "bluetooth");
        assert_eq!(transport("HDAUDIO", Some(9)), "display");
        assert_eq!(transport("usb", Some(3)), "usb");
        assert_eq!(transport("HDAUDIO", Some(1)), "builtin");
        assert_eq!(transport("ROOT", None), "virtual");
        assert_eq!(transport("", None), "other");
    }
}
