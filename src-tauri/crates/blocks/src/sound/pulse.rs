//! Linux's audio devices through `pactl` — PulseAudio's command, which a PipeWire server also
//! answers — the way a terminal would: the sinks' and sources' volume and mute, the default
//! device, and the applications recording now (the server's source outputs). Nothing is listened
//! to, no permission asked. The parsing is the same everywhere (tested from any OS); the `pactl`
//! calls are Linux's, in the C locale: its text is translated otherwise (`Destination #`,
//! `Sourdine : oui`).

use std::sync::LazyLock;

#[cfg(target_os = "linux")]
use std::process::Command;

use regex::Regex;

#[cfg(target_os = "linux")]
use anyhow::{Context, Result};

#[cfg(target_os = "linux")]
use prism_base::tr;

use super::{Device, Listener, Side};
#[cfg(target_os = "linux")]
use super::Scope;

/// Where a card-less device's id goes (a server-made sink, `auto_null`): past every card's
/// number, the sources' past the sinks', so the two numberings never meet.
const NO_CARD: u32 = 1_000_000_000;

/// Where a device's data comes from: a sink (an output) or a source (an input).
#[derive(Default, Clone, Copy, PartialEq)]
enum Kind {
    #[default]
    Sink,
    Source,
}

/// One sink or source of a `pactl list`.
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
    /// The card the sink or source belongs to, from its properties (`pactl` prints no card
    /// line): PipeWire's `device.id`, else PulseAudio's `alsa.card`. None: a server-made one,
    /// standing alone.
    card: Option<u32>,
    device_id: Option<u32>,
    alsa_card: Option<u32>,
    bus: Option<String>,
    api: Option<String>,
    is_virtual: bool,
    transport: &'static str,
}

impl Entry {
    /// A sink played back into the server, listed with the sources: not a microphone.
    fn is_monitor(&self) -> bool {
        self.name.ends_with(".monitor")
    }

    fn label(&self) -> &str {
        if self.description.is_empty() { &self.name } else { &self.description }
    }
}

/// The block's device id of an entry: its card's, or (no card: a server-made one standing alone)
/// its own index, pushed past every card's.
fn device_id(e: &Entry) -> u32 {
    e.card.unwrap_or(match e.kind {
        Kind::Sink => NO_CARD,
        Kind::Source => 2 * NO_CARD,
    } + e.index)
}

/// What a device is plugged through, its properties' words onto the block's transports: the
/// bluetooth stack (`api = "bluez5"`, `device.bus = "bluetooth"`), an HDMI output, a server-made
/// one (`node.virtual`, `auto_null`), then the bus, else.
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
fn volume_percent(text: &str) -> Option<u8> {
    let at: Vec<f32> = PERCENT.captures_iter(text).filter_map(|c| c[1].parse().ok()).collect();
    (!at.is_empty()).then(|| (at.iter().sum::<f32>() / at.len() as f32).round().clamp(0.0, 100.0) as u8)
}

/// A property line's key and value (`\t\tdevice.bus = "pci"`).
fn property(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.strip_prefix("\t\t")?.split_once(" = ")?;
    Some((k, v.trim_matches('"')))
}

/// Every block of a `pactl list sinks` or `list sources`: a `Sink #41` or `Source #41` line, its
/// fields one tab in (`Name:`, `Description:`, `Mute:`), the volume's channels one tab and spaces
/// in, the properties (and the ports) two tabs in.
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
            } else if line.starts_with("\tVolume:") || line.starts_with("\t ") {
                e.volume_text.push_str(line);
            } else if let Some((k, v)) = property(line) {
                match k {
                    "device.id" => e.device_id = v.parse().ok(),
                    "alsa.card" => e.alsa_card = v.parse().ok(),
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
        // One numbering per server: PipeWire gives every device's node its `device.id`,
        // PulseAudio its ALSA ones their `alsa.card`.
        e.card = e.device_id.or(e.alsa_card);
    }
    out
}

/// The block's devices of entries (monitors apart): a card's sink and source are one device (the
/// card's number, the first one's description); a card-less one stands alone.
fn to_devices(all: &[Entry]) -> Vec<Device> {
    let mut out: Vec<Device> = Vec::new();
    for e in all.iter().filter(|e| !e.is_monitor()) {
        let id = device_id(e);
        match out.iter_mut().find(|d| d.id == id) {
            Some(d) => {
                d.input |= e.kind == Kind::Source;
                d.output |= e.kind == Kind::Sink;
            }
            None => out.push(Device { id, name: e.label().into(), input: e.kind == Kind::Source, output: e.kind == Kind::Sink, transport: e.transport.into() }),
        }
    }
    out
}

/// The default sink's and source's names of a `pactl info`; none when the server has none.
fn defaults(info: &str) -> (Option<String>, Option<String>) {
    let at = |key: &str| {
        info.lines().find_map(|l| l.strip_prefix(key)).map(str::trim).filter(|n| !n.is_empty() && !n.starts_with('@')).map(String::from)
    };
    (at("Default Sink:"), at("Default Source:"))
}

/// The side of the default device `name` of `kind`; none: the server has no default for it (a
/// machine with no microphone).
fn side(all: &[Entry], kind: Kind, name: Option<&str>) -> Option<Side> {
    let e = all.iter().find(|e| e.kind == kind && Some(e.name.as_str()) == name)?;
    Some(Side {
        device: device_id(e),
        name: e.label().into(),
        transport: e.transport.into(),
        volume: e.volume,
        volume_settable: e.volume.is_some(),
        muted: e.muted,
        mute_settable: true,
    })
}

/// The source outputs of a `pactl list source-outputs` output, one per application recording;
/// from a monitor (`monitors`: the sources' indices) or the server itself apart.
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

/// What the block shows, of one read of the server.
#[derive(Default)]
pub struct State {
    pub output: Option<Side>,
    pub input: Option<Side>,
    pub devices: Vec<Device>,
    pub recording: Vec<Listener>,
}

/// The state of `pactl list sinks`, `list sources`, `info` and `list source-outputs`.
fn state(sinks: &str, sources: &str, info: &str, outputs: &str) -> State {
    let mut all = entries(Kind::Sink, sinks);
    all.extend(entries(Kind::Source, sources));
    let (sink, source) = defaults(info);
    let monitors: Vec<u32> = all.iter().filter(|e| e.kind == Kind::Source && e.is_monitor()).map(|e| e.index).collect();
    State {
        output: side(&all, Kind::Sink, sink.as_deref()),
        input: side(&all, Kind::Source, source.as_deref()),
        devices: to_devices(&all),
        recording: source_outputs(outputs, &monitors),
    }
}

/// Runs `pactl` with `args`, its standard output, in the C locale (the words parsed are
/// English). Blocking: the collector calls it off the async threads. Not there, or the server
/// not answering: the block's error, the panel's.
#[cfg(target_os = "linux")]
fn pactl(args: &[&str]) -> Result<String> {
    let out = Command::new("pactl").env("LC_ALL", "C").args(args).output().with_context(|| tr!("{program} not found", program = "pactl"))?;
    if !out.status.success() {
        anyhow::bail!("{}: {}", tr!("{program} did not answer", program = "pactl"), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One read of the server: four `pactl` calls. The recordings are best effort: a server that
/// does not list them, no one.
#[cfg(target_os = "linux")]
pub fn read() -> Result<State> {
    let sinks = pactl(&["list", "sinks"])?;
    let sources = pactl(&["list", "sources"])?;
    let info = pactl(&["info"])?;
    let outputs = pactl(&["list", "source-outputs"]).unwrap_or_default();
    Ok(state(&sinks, &sources, &info, &outputs))
}

/// The server's name for the default device of `s`.
#[cfg(target_os = "linux")]
fn default_of(s: Scope) -> &'static str {
    match s {
        Scope::Output => "@DEFAULT_SINK@",
        Scope::Input => "@DEFAULT_SOURCE@",
    }
}

/// The default device of `s` to `percent` % of its base volume.
#[cfg(target_os = "linux")]
pub fn set_volume(s: Scope, percent: u8) -> Result<()> {
    let arg = match s {
        Scope::Output => "set-sink-volume",
        Scope::Input => "set-source-volume",
    };
    pactl(&[arg, default_of(s), &format!("{percent}%")])?;
    Ok(())
}

/// The default device of `s`, muted or not.
#[cfg(target_os = "linux")]
pub fn set_mute(s: Scope, muted: bool) -> Result<()> {
    let arg = match s {
        Scope::Output => "set-sink-mute",
        Scope::Input => "set-source-mute",
    };
    pactl(&[arg, default_of(s), if muted { "1" } else { "0" }])?;
    Ok(())
}

/// The device `id`'s sink or source the server defaults `s` to: a card with several (a dock's
/// outputs), its first.
#[cfg(target_os = "linux")]
pub fn set_default(s: Scope, id: u32) -> Result<()> {
    let (kind, list, arg) = match s {
        Scope::Output => (Kind::Sink, "sinks", "set-default-sink"),
        Scope::Input => (Kind::Source, "sources", "set-default-source"),
    };
    let all = entries(kind, &pactl(&["list", list])?);
    let e = all.iter().filter(|e| !e.is_monitor()).find(|e| device_id(e) == id).with_context(|| format!("no {} device {id}", s.word()))?;
    pactl(&[arg, &e.name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PipeWire server (`LC_ALL=C pactl list sinks`) with a built-in card (sink and source), an
    /// HDMI output, a bluetooth headset (sink and source) and the server's Dummy Output, no
    /// device. No card line: the card is the `device.id` property.
    const SINKS: &str = concat!(
        "Sink #41\n",
        "\tState: IDLE\n",
        "\tName: alsa_output.pci-0000_00_1f.3.analog-stereo\n",
        "\tDescription: Built-in Audio Analog Stereo\n",
        "\tDriver: PipeWire\n",
        "\tSample Specification: s16le 2ch 44100Hz\n",
        "\tChannel Map: front-left,front-right\n",
        "\tOwner Module: 4294967295\n",
        "\tMute: no\n",
        "\tVolume: front-left: 30149 /  46% / -20.00 dB,   front-right: 30149 /  46% / -20.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tMonitor Source: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n",
        "\tLatency: 0 usec, configured 0 usec\n",
        "\tFlags: HARDWARE HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\talsa.card = \"0\"\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\t\tdevice.id = \"52\"\n",
        "\t\tnode.name = \"alsa_output.pci-0000_00_1f.3.analog-stereo\"\n",
        "\tPorts:\n",
        "\t\tanalog-output-speaker: Speakers (type: Speaker, priority: 10000, availability unknown)\n",
        "\tActive Port: analog-output-speaker\n",
        "\tFormats:\n",
        "\t\tpcm\n",
        "\n",
        "Sink #45\n",
        "\tState: RUNNING\n",
        "\tName: alsa_output.pci-0000_00_01.0.hdmi-stereo\n",
        "\tDescription: HDMI Audio Output\n",
        "\tDriver: PipeWire\n",
        "\tMute: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: HARDWARE DECIBEL_VOLUME \n",
        "\tProperties:\n",
        "\t\talsa.card = \"1\"\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\t\tdevice.id = \"53\"\n",
        "\n",
        "Sink #51\n",
        "\tState: RUNNING\n",
        "\tName: bluez_output.00_11_22_33_44_55.1\n",
        "\tDescription: WH-1000XM5\n",
        "\tDriver: PipeWire\n",
        "\tMute: yes\n",
        "\tVolume: front-left: 45875 /  70% / -9.29 dB,   front-right: 45875 /  70% / -9.29 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME \n",
        "\tProperties:\n",
        "\t\tapi.bluez5.address = \"00:11:22:33:44:55\"\n",
        "\t\tdevice.api = \"bluez5\"\n",
        "\t\tdevice.bus = \"bluetooth\"\n",
        "\t\tdevice.id = \"60\"\n",
        "\n",
        "Sink #53\n",
        "\tState: SUSPENDED\n",
        "\tName: auto_null\n",
        "\tDescription: Dummy Output\n",
        "\tDriver: PipeWire\n",
        "\tMute: no\n",
        "\tVolume: front-left: 45875 /  70% / -9.29 dB,   front-right: 45875 /  70% / -9.29 dB,   front-center: 39322 /  60% / -10.00 dB\n",
        "\t        lfe: 39322 /  60% / -10.00 dB,   rear-left: 32768 /  50% / -13.00 dB,   rear-right: 32768 /  50% / -13.00 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\tnode.name = \"auto_null\"\n",
        "\t\tnode.virtual = \"true\"\n",
    );

    /// The same server's sources: the built-in microphone, its sink's monitor, the headset's.
    const SOURCES: &str = concat!(
        "Source #42\n",
        "\tState: IDLE\n",
        "\tName: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n",
        "\tDescription: Monitor of Built-in Audio Analog Stereo\n",
        "\tMute: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n",
        "\tProperties:\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.class = \"monitor\"\n",
        "\t\tdevice.id = \"52\"\n",
        "\n",
        "Source #44\n",
        "\tState: IDLE\n",
        "\tName: alsa_input.pci-0000_00_1f.3.analog-stereo\n",
        "\tDescription: Built-in Audio Analog Stereo\n",
        "\tDriver: PipeWire\n",
        "\tMute: no\n",
        "\tVolume: front-left: 13107 /  20% / -35.75 dB,   front-right: 13107 /  20% / -35.75 dB\n",
        "\t        balance 0.00\n",
        "\tBase Volume: 65536 / 100% / 0.00 dB\n",
        "\tFlags: HARDWARE HW_MUTE_CTRL HW_VOLUME_CTRL DECIBEL_VOLUME LATENCY \n",
        "\tProperties:\n",
        "\t\talsa.card = \"0\"\n",
        "\t\tdevice.api = \"alsa\"\n",
        "\t\tdevice.bus = \"pci\"\n",
        "\t\tdevice.id = \"52\"\n",
        "\tPorts:\n",
        "\t\tanalog-input-mic: Microphone (type: Mic, priority: 8700, availability unknown)\n",
        "\n",
        "Source #57\n",
        "\tState: RUNNING\n",
        "\tName: bluez_input.00_11_22_33_44_55.0\n",
        "\tDescription: WH-1000XM5\n",
        "\tMute: no\n",
        "\tVolume: mono: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tProperties:\n",
        "\t\tdevice.api = \"bluez5\"\n",
        "\t\tdevice.bus = \"bluetooth\"\n",
        "\t\tdevice.id = \"60\"\n",
    );

    const INFO: &str = concat!(
        "Server String: /run/user/1000/pulse/native\n",
        "Server Name: PulseAudio (on PipeWire 1.0.5)\n",
        "Default Sink: bluez_output.00_11_22_33_44_55.1\n",
        "Default Source: alsa_input.pci-0000_00_1f.3.analog-stereo\n",
        "Cookie: 6f0a:1d2e\n",
    );

    /// Four applications on the server: a browser on the microphone, a recorder on the monitor, a
    /// loopback of the server's, and another browser (the same application, once).
    const RECORDINGS: &str = concat!(
        "Source Output #123\n",
        "\tDriver: PipeWire\n",
        "\tOwner Module: n/a\n",
        "\tSource: 44\n",
        "\tSample Specification: s16le 2ch 48000Hz\n",
        "\tCorked: no\n",
        "\tVolume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB\n",
        "\t        balance 0.00\n",
        "\tProperties:\n",
        "\t\tapplication.name = \"Google Chrome\"\n",
        "\t\tapplication.process.binary = \"chrome\"\n",
        "Source Output #124\n",
        "\tSource: 42\n",
        "\tProperties:\n",
        "\t\tapplication.name = \"Recorder\"\n",
        "\t\tapplication.process.binary = \"gnome-screen-recorder\"\n",
        "Source Output #125\n",
        "\tSource: 44\n",
        "\tProperties:\n",
        "\t\tapplication.process.binary = \"pipewire\"\n",
        "Source Output #126\n",
        "\tSource: 44\n",
        "\tProperties:\n",
        "\t\tapplication.process.binary = \"obs\"\n",
        "\t\tapplication.name = \"OBS Studio\"\n",
        "Source Output #127\n",
        "\tSource: 44\n",
        "\tProperties:\n",
        "\t\tapplication.name = \"Google Chrome\"\n",
        "\t\tapplication.process.binary = \"chrome\"\n",
    );

    #[test]
    fn cards_merge_into_devices() {
        let s = state(SINKS, SOURCES, INFO, "");
        let ds = &s.devices;
        let at = |id: u32| ds.iter().find(|d| d.id == id).unwrap();
        // The built-in card: its sink and its source, one device.
        let built = at(52);
        assert!(built.input && built.output);
        assert_eq!(built.name, "Built-in Audio Analog Stereo");
        assert_eq!(built.transport, "builtin");
        // The HDMI output alone, the bluetooth headset both ways, the Dummy Output alone and
        // pushed past every card's id.
        let hdmi = at(53);
        assert!(hdmi.output && !hdmi.input);
        assert_eq!(hdmi.transport, "display");
        let bt = at(60);
        assert!(bt.input && bt.output);
        assert_eq!(bt.transport, "bluetooth");
        let dummy = at(NO_CARD + 53);
        assert!(dummy.output && !dummy.input);
        assert_eq!(dummy.transport, "virtual");
        // A monitor is a sink played back, not a device of its own.
        assert_eq!(ds.len(), 4);
        assert!(!ds.iter().any(|d| d.name.contains("Monitor")));
    }

    /// PulseAudio has no `device.id`: its ALSA card's number joins the sink and the source.
    #[test]
    fn pulseaudio_cards_by_alsa_card() {
        let sinks = "Sink #0\n\tName: alsa_output.usb-dock.analog-stereo\n\tDescription: Dock\n\tProperties:\n\t\talsa.card = \"2\"\n\t\tdevice.bus = \"usb\"\n";
        let sources = "Source #1\n\tName: alsa_input.usb-dock.mono\n\tDescription: Dock\n\tProperties:\n\t\talsa.card = \"2\"\n\t\tdevice.bus = \"usb\"\n";
        let s = state(sinks, sources, "", "");
        assert_eq!(s.devices.len(), 1);
        assert!(s.devices[0].input && s.devices[0].output);
        assert_eq!((s.devices[0].id, s.devices[0].transport.as_str()), (2, "usb"));
    }

    #[test]
    fn the_default_sides() {
        let s = state(SINKS, SOURCES, INFO, "");
        let out = s.output.unwrap();
        assert_eq!((out.device, out.name.as_str(), out.transport.as_str()), (60, "WH-1000XM5", "bluetooth"));
        assert_eq!((out.volume, out.muted, out.volume_settable), (Some(70), Some(true), true));
        let input = s.input.unwrap();
        assert_eq!((input.device, input.volume, input.muted), (52, Some(20), Some(false)));
        // No default, or one still to come (`@DEFAULT_SOURCE@`): no side.
        let s = state(SINKS, SOURCES, "Default Sink: \nDefault Source: @DEFAULT_SOURCE@\n", "");
        assert!(s.output.is_none() && s.input.is_none());
    }

    #[test]
    fn volumes_and_mutes() {
        let sinks = entries(Kind::Sink, SINKS);
        let built = sinks.iter().find(|e| e.index == 41).unwrap();
        assert_eq!(built.volume, Some(46));
        assert_eq!(built.muted, Some(false));
        // 70, 70, 60, 60, 50, 50 over the two lines.
        let dummy = sinks.iter().find(|e| e.name == "auto_null").unwrap();
        assert_eq!(dummy.volume, Some(60));
        assert_eq!(dummy.card, None);
        assert_eq!(volume_percent(""), None);
        // The ports' lines (two tabs in, no ` = `) are no property.
        assert_eq!(built.card, Some(52));
    }

    #[test]
    fn recordings_are_the_microphone_ones() {
        let out = state(SINKS, SOURCES, INFO, RECORDINGS).recording;
        let apps: Vec<&str> = out.iter().map(|l| l.app.as_str()).collect();
        // The monitor's recorder and the server's own loopback are no one; Chrome, once.
        assert_eq!(apps, ["Google Chrome", "OBS Studio"]);
        assert_eq!(out[0].bundle, "chrome");
    }
}
