//! Linux's audio devices through `pactl` — PulseAudio's command, which a PipeWire server also
//! answers — the way a terminal would: the sinks' and sources' volume and mute, the default
//! device, and the applications recording now (the server's source outputs). Nothing is listened
//! to, no permission asked. The parsing is the same everywhere (tested from any OS); the `pactl`
//! calls are Linux's.

#[cfg(any(target_os = "linux", test))]
use std::sync::LazyLock;

#[cfg(target_os = "linux")]
use std::process::Command;

#[cfg(any(target_os = "linux", test))]
use regex::Regex;

#[cfg(target_os = "linux")]
use anyhow::{bail, Context, Result};

#[cfg(target_os = "linux")]
use prism_base::tr;

#[cfg(any(target_os = "linux", test))]
use super::{Device, Listener};
#[cfg(target_os = "linux")]
use super::{Scope, Side};

/// Where a card-less device's id goes (a server-made sink, `auto_null`): past every card's
/// index, the sources' past the sinks', so the two numberings never meet.
const NO_CARD: u32 = 1_000_000_000;

/// Where a device's data comes from: a sink (an output) or a source (an input).
#[cfg(any(target_os = "linux", test))]
#[derive(Default, Clone, Copy, PartialEq)]
enum Kind {
    #[default]
    Sink,
    Source,
}

/// One sink or source of a `pactl list`.
#[cfg(any(target_os = "linux", test))]
#[derive(Default)]
struct Entry {
    kind: Kind,
    index: u32,
    name: String,
    description: String,
    /// The `Volume:` lines as they came, one `NN%` per channel, averaged at the end of the parse.
    volume_text: String,
    volume: Option<u8>,
    muted: Option<bool>,
    /// The card the sink or source belongs to; none: a server-made one, standing alone.
    card: Option<u32>,
    bus: Option<String>,
    api: Option<String>,
    is_virtual: bool,
    transport: &'static str,
}

/// The block's device id of an entry: its card's, or (no card: a server-made one standing alone)
/// its own index, pushed past every card's.
#[cfg(any(target_os = "linux", test))]
fn device_id(e: &Entry) -> u32 {
    e.card.unwrap_or(match e.kind {
        Kind::Sink => NO_CARD,
        Kind::Source => 2 * NO_CARD,
    } + e.index)
}

/// What a device is plugged through, its properties' words onto the block's transports: the
/// bluetooth stack (`api = "bluez5"`, `device.bus = "bluetooth"`), an HDMI output, a server-made
/// one (`node.virtual`, `auto_null`), then the bus, else.
#[cfg(any(target_os = "linux", test))]
fn transport(name: &str, bus: Option<&str>, api: Option<&str>, is_virtual: bool) -> &'static str {
    if api.is_some_and(|a| a.starts_with("bluez")) || bus == Some("bluetooth") {
        return "bluetooth";
    }
    if name.contains("hdmi") {
        return "display";
    }
    if is_virtual || name.starts_with("auto_null") {
        return "virtual";
    }
    match bus {
        Some("usb") => "usb",
        Some("pci") | Some("platform") | Some("acpi") => "builtin",
        _ => "other",
    }
}

static PERCENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+)%").unwrap());

/// A volume's share, its channels' `NN%` averaged (the wrapped lines together), 0–100: a boosted
/// 150 % reads 100.
#[cfg(any(target_os = "linux", test))]
fn volume_percent(text: &str) -> Option<u8> {
    let at: Vec<f32> = PERCENT.captures_iter(text).filter_map(|c| c[1].parse().ok()).collect();
    (!at.is_empty()).then(|| (at.iter().sum::<f32>() / at.len() as f32).round().clamp(0.0, 100.0) as u8)
}

/// A property line's key and value (`\t\tdevice.bus = "pci"`).
#[cfg(any(target_os = "linux", test))]
fn property(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.strip_prefix("\t\t")?.split_once(" = ")?;
    Some((k, v.trim_matches('"')))
}

/// Every block of a `pactl list sinks` or `list sources`: a `Sink #41` or `Source #41` line, its
/// fields one tab in (`Name:`, `Description:`, `Mute:`, `Card:`), the volume's channels one tab
/// and spaces in, the properties two tabs in.
#[cfg(any(target_os = "linux", test))]
fn entries(kind: Kind, text: &str) -> Vec<Entry> {
    let head = match kind {
        Kind::Sink => "Sink #",
        Kind::Source => "Source #",
    };
    let mut out: Vec<Entry> = Vec::new();
    for line in text.lines() {
        if let Some(index) = line.strip_prefix(head).and_then(|n| n.trim().parse().ok()) {
            out.push(Entry { kind, index, ..Default::default() });
        } else if let Some(e) = out.last_mut() {
            if let Some(v) = line.strip_prefix("\tName: ") {
                e.name = v.into();
            } else if let Some(v) = line.strip_prefix("\tDescription: ") {
                e.description = v.into();
            } else if let Some(v) = line.strip_prefix("\tMute: ") {
                e.muted = Some(v == "yes");
            } else if let Some(v) = line.strip_prefix("\tCard: ") {
                e.card = v.trim().parse().ok();
            } else if line.starts_with("\tVolume:") || line.starts_with("\t ") {
                e.volume_text.push_str(line);
            } else if let Some((k, v)) = property(line) {
                match k {
                    "device.bus" => e.bus = Some(v.into()),
                    "api" | "device.api" => e.api = Some(v.into()),
                    "node.virtual" => e.is_virtual = v == "true",
                    _ => {}
                }
            }
        }
    }
    for e in &mut out {
        e.volume = volume_percent(&e.volume_text);
        e.transport = transport(&e.name, e.bus.as_deref(), e.api.as_deref(), e.is_virtual);
    }
    out
}

/// The block's devices of entries: a card's sink and source are one device (the card's index, the
/// sink's description); a card-less one stands alone.
#[cfg(any(target_os = "linux", test))]
fn to_devices(all: Vec<Entry>) -> Vec<Device> {
    let mut out: Vec<Device> = Vec::new();
    for e in all {
        let id = device_id(&e);
        match out.iter_mut().find(|d| d.id == id) {
            Some(d) => {
                d.input |= e.kind == Kind::Source;
                d.output |= e.kind == Kind::Sink;
            }
            None => out.push(Device {
                id,
                name: if e.description.is_empty() { e.name.clone() } else { e.description.clone() },
                input: e.kind == Kind::Source,
                output: e.kind == Kind::Sink,
                transport: e.transport.into(),
            }),
        }
    }
    out
}

/// The source outputs of a `pactl list source-outputs` output, one per application recording;
/// from a monitor (`monitors`: the sources' indices) or the server itself apart.
#[cfg(any(target_os = "linux", test))]
fn source_outputs(text: &str, monitors: &[u32]) -> Vec<Listener> {
    let mut out: Vec<Listener> = Vec::new();
    let mut source: Option<u32> = None;
    let mut app = String::new();
    let mut bundle = String::new();
    for line in text.lines() {
        if line.starts_with("Source Output #") {
            keep(&mut out, monitors, source, &app, &bundle);
            source = None;
            app.clear();
            bundle.clear();
        } else if let Some(index) = line.strip_prefix("\tSource: ").and_then(|n| n.trim().parse().ok()) {
            source = Some(index);
        } else if let Some((k, v)) = property(line) {
            match k {
                "application.name" => app = v.into(),
                "application.process.binary" => bundle = v.into(),
                _ => {}
            }
        }
    }
    keep(&mut out, monitors, source, &app, &bundle);
    out
}

/// One application's recording, if it is one: a process recording from a microphone, named by its
/// application (its binary otherwise), once.
#[cfg(any(target_os = "linux", test))]
fn keep(out: &mut Vec<Listener>, monitors: &[u32], source: Option<u32>, app: &str, bundle: &str) {
    // The server's own loopbacks (`pipewire`) and a monitor's (a sink played back, not a
    // microphone) are no one recording.
    if bundle.is_empty() || bundle == "pipewire" || source.is_some_and(|s| monitors.contains(&s)) {
        return;
    }
    let app = if app.is_empty() { bundle.to_string() } else { app.to_string() };
    if !out.iter().any(|l| l.app == app) {
        out.push(Listener { app, bundle: bundle.into() });
    }
}

/// Runs `pactl` with `args`, its standard output. Blocking: the collector calls it off the async
/// threads. Not there, or the server not answering: the block's error, the panel's.
#[cfg(target_os = "linux")]
fn pactl(args: &[&str]) -> Result<String> {
    let out = Command::new("pactl").args(args).output().with_context(|| tr!("{program} not found", program = "pactl"))?;
    if !out.status.success() {
        bail!("{}: {}", tr!("{program} did not answer", program = "pactl"), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every sink or source, a `pactl list`'s parse.
#[cfg(target_os = "linux")]
fn list(kind: Kind) -> Result<Vec<Entry>> {
    let arg = match kind {
        Kind::Sink => "sinks",
        Kind::Source => "sources",
    };
    Ok(entries(kind, &pactl(&["list", arg])?))
}

/// Every sink and every source of the server, the sources' monitors apart: a sink played back
/// into the server is not a microphone.
#[cfg(target_os = "linux")]
fn all_entries() -> Result<Vec<Entry>> {
    let mut all = list(Kind::Sink)?;
    all.extend(list(Kind::Source)?.into_iter().filter(|e| !e.name.ends_with(".monitor")));
    Ok(all)
}

/// The default sink's or source's name; none when the server has none (its `@DEFAULT_SOURCE@` to
/// come, or an empty answer).
#[cfg(target_os = "linux")]
fn default_name(s: Scope) -> Result<Option<String>> {
    let arg = match s {
        Scope::Output => "get-default-sink",
        Scope::Input => "get-default-source",
    };
    let name = pactl(&[arg])?;
    let name = name.trim();
    Ok((!name.is_empty() && !name.starts_with('@')).then(|| name.to_string()))
}

/// Every device: the server's cards, a sink and a source of one card as one.
#[cfg(target_os = "linux")]
pub fn devices() -> Result<Vec<Device>> {
    all_entries().map(to_devices)
}

/// The default device of `s`: its name, its volume (0–100) and mute. None: the server has no
/// default for it (a machine with no microphone).
#[cfg(target_os = "linux")]
pub fn side(s: Scope) -> Result<Option<Side>> {
    let kind = match s {
        Scope::Output => Kind::Sink,
        Scope::Input => Kind::Source,
    };
    let Some(name) = default_name(s)? else { return Ok(None) };
    let e = all_entries()?.into_iter().find(|e| e.kind == kind && e.name == name);
    Ok(e.map(|e| Side {
        device: device_id(&e),
        name: if e.description.is_empty() { e.name } else { e.description },
        transport: e.transport.into(),
        volume: e.volume,
        volume_settable: e.volume.is_some(),
        muted: e.muted,
        mute_settable: true,
    }))
}

/// The default device of `s` to `percent` % of its base volume.
#[cfg(target_os = "linux")]
pub fn set_volume(s: Scope, percent: u8) -> Result<()> {
    let arg = match s {
        Scope::Output => "set-sink-volume",
        Scope::Input => "set-source-volume",
    };
    let Some(name) = default_name(s)? else { bail!("no default {} device", s.word()) };
    pactl(&[arg, &name, &format!("{percent}%")])?;
    Ok(())
}

/// The default device of `s`, muted or not.
#[cfg(target_os = "linux")]
pub fn set_mute(s: Scope, muted: bool) -> Result<()> {
    let arg = match s {
        Scope::Output => "set-sink-mute",
        Scope::Input => "set-source-mute",
    };
    let Some(name) = default_name(s)? else { bail!("no default {} device", s.word()) };
    pactl(&[arg, &name, if muted { "1" } else { "0" }])?;
    Ok(())
}

/// The card `id`'s device the server defaults `s` to.
#[cfg(target_os = "linux")]
pub fn set_default(s: Scope, id: u32) -> Result<()> {
    let (kind, arg) = match s {
        Scope::Output => (Kind::Sink, "set-default-sink"),
        Scope::Input => (Kind::Source, "set-default-source"),
    };
    let e = list(kind)?
        .into_iter()
        .filter(|e| !e.name.ends_with(".monitor"))
        .find(|e| device_id(e) == id)
        .with_context(|| format!("no {} device {id}", s.word()))?;
    pactl(&[arg, &e.name])?;
    Ok(())
}

/// The applications recording from a microphone now, the server's source outputs, one per
/// application: the block's recording chip and its red alert. Best effort: a server not
/// answering, no one.
#[cfg(target_os = "linux")]
pub fn listeners() -> Vec<Listener> {
    let Ok(text) = pactl(&["list", "source-outputs"]) else { return Vec::new() };
    let monitors: Vec<u32> = list(Kind::Source)
        .map(|all| all.iter().filter(|e| e.name.ends_with(".monitor")).map(|e| e.index).collect())
        .unwrap_or_default();
    source_outputs(&text, &monitors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server with a built-in card (sink and source), an HDMI output, a bluetooth headset
    /// (sink and source) and the server's Dummy Output, no card.
    const SINKS: &str = concat!(
        "Sink #41\n",
        "\tState: IDLE\n",
        "\tName: alsa_output.pci-0000_00_1f.3.analog-stereo\n",
        "\tDescription: Built-in Audio Analog Stereo\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 2ch 44100Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tOwner Module: n/a\n",
        "\tMute: no\n",
        "\tVolume: front-left: 30149 /  46% / -20.00 dB,   front-right: 30149 /  46% / -20.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tMonitor Source: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n",
        "\tFlags: HARDWARE HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\t\tnode.name = \"alsa_output.pci-0000_00_1f.3.analog-stereo\"\n",
        "\tCard: 0\n",
        "Sink #45\n",
        "\tState: RUNNING\n",
        "\tName: alsa_output.pci-0000_00_01.0.hdmi-stereo\n",
        "\tDescription: HDMI Audio Output\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 2ch 48000Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tOwner Module: n/a\n",
        "\tMute: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tMonitor Source: alsa_output.pci-0000_00_01.0.hdmi-stereo.monitor\n",
        "\tFlags: HARDWARE DECIBEL_VOLUME \n",
        "\tProperties:\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\tCard: 2\n",
        "Sink #51\n",
        "\tState: RUNNING\n",
        "\tName: bluez_output.00_11_22_33_44_55.1\n",
        "\tDescription: WH-1000XM5\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 2ch 48000Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tOwner Module: n/a\n",
        "\tMute: yes\n",
        "\tVolume: front-left: 45875 /  70% / -9.29 dB,   front-right: 45875 /  70% / -9.29 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tMonitor Source: bluez_output.00_11_22_33_44_55.1.monitor\n",
        "\tFlags: HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME \n",
        "\tProperties:\n",
        "\t\tapi = \"bluez5\"\n",
        "\t\tdevice.bus = \"bluetooth\"\n",
        "\t\tdevice.form_factor = \"headset\"\n",
        "\tCard: 3\n",
        "Sink #53\n",
        "\tState: SUSPENDED\n",
        "\tName: auto_null\n",
        "\tDescription: Dummy Output\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: float32le 6ch 48000Hz\n",
        "\tChannel Map: front-left,front-right,front-center,lfe,rear-left,rear-right\n",
        "\tOwner Module: 4294967295\n",
        "\tMute: no\n",
        "\tVolume: front-left: 45875 /  70% / -9.29 dB,   front-right: 45875 /  70% / -9.29 dB,   front-center: 39322 /  60% / -10.00 dB\n",
        "\t        lfe: 39322 /  60% / -10.00 dB,   rear-left: 32768 /  50% / -13.00 dB,   rear-right: 32768 /  50% / -13.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tMonitor Source: auto_null.monitor\n",
        "\tFlags: DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\tnode.name = \"auto_null\"\n",
        "\t\tnode.virtual = \"true\"\n",
    );

    /// The same server's sources: the built-in microphone (card 0), its sink's monitor, the
    /// headset's.
    const SOURCES: &str = concat!(
        "Source #41\n",
        "\tState: IDLE\n",
        "\tName: alsa_input.pci-0000_00_1f.3.analog-stereo\n",
        "\tDescription: Built-in Audio Analog Stereo\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 2ch 44100Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tOwner Module: n/a\n",
        "\tMute: no\n",
        "\tVolume: front-left: 13107 /  20% / -35.75 dB,   front-right: 13107 /  20% / -35.75 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: HARDWARE HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\tCard: 0\n",
        "Source #42\n",
        "\tState: IDLE\n",
        "\tName: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n",
        "\tDescription: Monitor of Built-in Audio Analog Stereo\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 2ch 44100Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tOwner Module: n/a\n",
        "\tMute: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\tCard: 0\n",
        "Source #57\n",
        "\tState: RUNNING\n",
        "\tName: bluez_source.00_11_22_33_44_55.1\n",
        "\tDescription: WH-1000XM5\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 1ch 16000Hz\n",
        "\tChannel Map: mono\n",
        "\tOwner Module: n/a\n",
        "\tMute: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME \n",
        "\tProperties:\n",
        "\t\tapi = \"bluez5\"\n",
        "\t\tdevice.bus = \"bluetooth\"\n",
        "\tCard: 3\n",
    );

    /// Four applications on the server: a browser on the microphone, a recorder on the monitor, a
    /// loopback of the server's, and another browser (the same application, once).
    const RECORDINGS: &str = concat!(
        "Source Output #123\n",
        "\tDriver: PipeWire\n",
        "\tOwner Module: n/a\n",
        "\tSource: 41\n",
        "\tSample Specification: s16le 2ch 48000Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tCorked: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tProperties:\n",
        "\t\tapplication.name = \"Google Chrome\"\n",
        "\t\tapplication.process.binary = \"chrome\"\n",
        "Source Output #124\n",
        "\tDriver: PipeWire\n",
        "\tOwner Module: n/a\n",
        "\tSource: 42\n",
        "\tProperties:\n",
        "\t\tapplication.name = \"Recorder\"\n",
        "\t\tapplication.process.binary = \"gnome-screen-recorder\"\n",
        "Source Output #125\n",
        "\tDriver: PipeWire\n",
        "\tOwner Module: n/a\n",
        "\tSource: 41\n",
        "\tProperties:\n",
        "\t\tapplication.process.binary = \"pipewire\"\n",
        "Source Output #126\n",
        "\tDriver: PipeWire\n",
        "\tOwner Module: n/a\n",
        "\tSource: 41\n",
        "\tProperties:\n",
        "\t\tapplication.process.binary = \"obs\"\n",
        "\t\tapplication.name = \"OBS Studio\"\n",
        "Source Output #127\n",
        "\tDriver: PipeWire\n",
        "\tOwner Module: n/a\n",
        "\tSource: 41\n",
        "\tProperties:\n",
        "\t\tapplication.name = \"Google Chrome\"\n",
        "\t\tapplication.process.binary = \"chrome\"\n",
    );

    #[test]
    fn cards_merge_into_devices() {
        let mut all = entries(Kind::Sink, SINKS);
        all.extend(entries(Kind::Source, SOURCES).into_iter().filter(|e| !e.name.ends_with(".monitor")));
        let ds = to_devices(all);
        let at = |id: u32| ds.iter().find(|d| d.id == id).unwrap();
        // The built-in card: its sink and its source, one device.
        let built = at(0);
        assert!(built.input && built.output);
        assert_eq!(built.name, "Built-in Audio Analog Stereo");
        assert_eq!(built.transport, "builtin");
        // The HDMI output alone, the bluetooth headset both ways, the Dummy Output alone and
        // pushed past every card's id.
        let hdmi = at(2);
        assert!(hdmi.output && !hdmi.input);
        assert_eq!(hdmi.transport, "display");
        let bt = at(3);
        assert!(bt.input && bt.output);
        assert_eq!(bt.transport, "bluetooth");
        let dummy = at(NO_CARD + 53);
        assert!(dummy.output && !dummy.input);
        assert_eq!(dummy.transport, "virtual");
        // A monitor is a sink played back, not a device of its own.
        assert_eq!(ds.len(), 4);
        assert!(!ds.iter().any(|d| d.name.contains("Monitor")));
    }

    #[test]
    fn volumes_and_mutes() {
        let sinks = entries(Kind::Sink, SINKS);
        let built = sinks.iter().find(|e| e.index == 41).unwrap();
        assert_eq!(built.volume, Some(46));
        assert_eq!(built.muted, Some(false));
        assert_eq!(built.card, Some(0));
        let bt = sinks.iter().find(|e| e.name.starts_with("bluez")).unwrap();
        assert_eq!(bt.muted, Some(true));
        // 70, 70, 60, 60, 50, 50 over the two lines.
        let dummy = sinks.iter().find(|e| e.name == "auto_null").unwrap();
        assert_eq!(dummy.volume, Some(60));
        assert_eq!(dummy.card, None);
        assert_eq!(volume_percent(""), None);
        let sources = entries(Kind::Source, SOURCES);
        assert_eq!(sources.iter().find(|e| e.index == 41).unwrap().volume, Some(20));
    }

    #[test]
    fn recordings_are_the_microphone_ones() {
        let out = source_outputs(RECORDINGS, &[42]);
        let apps: Vec<&str> = out.iter().map(|l| l.app.as_str()).collect();
        // The monitor's recorder and the server's own loopback are no one; Chrome, once.
        assert_eq!(apps, ["Google Chrome", "OBS Studio"]);
        assert_eq!(out[0].bundle, "chrome");
    }
}
