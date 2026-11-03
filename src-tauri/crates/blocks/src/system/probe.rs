//! The network's side: a timed TCP handshake to each host every 15 s (no ICMP, which needs
//! privileges), the default gateway's included, and the Wi-Fi link on macOS.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{Host, Wifi};

/// How often the hosts are measured, and how long a handshake may take before it counts as lost.
pub const EVERY: Duration = Duration::from_secs(15);
const TIMEOUT: Duration = Duration::from_secs(2);
/// Loss and jitter are over this window.
const WINDOW: Duration = Duration::from_secs(5 * 60);
/// The handshakes a host shows.
const RECENT: usize = 8;
/// The gateway's line in `hosts`.
pub const GATEWAY: &str = "gateway";

/// The handshakes of each host, in the window, oldest first; shared with the collector.
#[derive(Default)]
pub struct Probes {
    samples: HashMap<String, VecDeque<(Instant, Option<f32>)>>,
    order: Vec<String>,
}

pub type Shared = Arc<Mutex<Probes>>;

impl Probes {
    fn record(&mut self, host: &str, ms: Option<f32>, now: Instant) {
        if !self.order.iter().any(|h| h == host) {
            self.order.push(host.to_string());
        }
        let list = self.samples.entry(host.to_string()).or_default();
        list.push_back((now, ms));
        while list.front().is_some_and(|(t, _)| now.duration_since(*t) > WINDOW) {
            list.pop_front();
        }
    }

    /// The hosts, the gateway apart: their last handshake, the last ones, their loss.
    pub fn hosts(&self) -> Vec<Host> {
        self.order
            .iter()
            .filter(|h| *h != GATEWAY)
            .filter_map(|h| {
                let list = self.samples.get(h)?;
                let recent: Vec<Option<f32>> = list.iter().rev().take(RECENT).rev().map(|(_, ms)| *ms).collect();
                Some(Host { name: h.clone(), ms: list.back().and_then(|(_, ms)| *ms), recent, loss: loss(list.iter().map(|(_, ms)| *ms)) })
            })
            .collect()
    }

    pub fn gateway(&self) -> Option<f32> {
        self.samples.get(GATEWAY)?.back()?.1
    }

    /// The median of each host's last answer; lost over the window, every host; the mean gap
    /// between a host's consecutive answers.
    pub fn summary(&self) -> (Option<f32>, f32, Option<f32>) {
        let hosts: Vec<&VecDeque<(Instant, Option<f32>)>> = self.order.iter().filter(|h| *h != GATEWAY).filter_map(|h| self.samples.get(h)).collect();
        let mut last: Vec<f32> = hosts.iter().filter_map(|l| l.back().and_then(|(_, ms)| *ms)).collect();
        last.sort_by(f32::total_cmp);
        let median = (!last.is_empty()).then(|| last[last.len() / 2]);
        let all = hosts.iter().flat_map(|l| l.iter().map(|(_, ms)| *ms));
        let gaps: Vec<f32> = hosts
            .iter()
            .flat_map(|l| {
                let answered: Vec<f32> = l.iter().filter_map(|(_, ms)| *ms).collect();
                answered.windows(2).map(|w| (w[1] - w[0]).abs()).collect::<Vec<_>>()
            })
            .collect();
        let jitter = (!gaps.is_empty()).then(|| gaps.iter().sum::<f32>() / gaps.len() as f32);
        (median, loss(all), jitter)
    }
}

/// Lost, in % of the handshakes.
fn loss(samples: impl Iterator<Item = Option<f32>>) -> f32 {
    let (mut n, mut lost) = (0u32, 0u32);
    for s in samples {
        n += 1;
        lost += u32::from(s.is_none());
    }
    if n == 0 { 0.0 } else { 100.0 * lost as f32 / n as f32 }
}

/// `host`, `host:port`, or a URL: what to dial (443 by default).
pub fn target(raw: &str) -> Option<(String, u16)> {
    let s = raw.trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.split(['/', '?', '#']).next().unwrap_or_default();
    if s.is_empty() {
        return None;
    }
    match s.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && !host.contains(':') => Some((host.to_string(), port.parse().ok()?)),
        _ => Some((s.to_string(), 443)),
    }
}

/// Measures every host (and the gateway) every `EVERY`, while the block runs. `hosts` is asked
/// each round: the forge and tracker become known once their sources have started.
pub async fn run(shared: Shared, hosts: impl Fn() -> Vec<String> + Send + 'static) {
    loop {
        let list = hosts();
        let gateway = gateway().await;
        let mut rounds: Vec<(String, String, u16)> = list.iter().filter_map(|h| target(h).map(|(host, port)| (h.clone(), host, port))).collect();
        if let Some(ip) = gateway {
            // Port 80: a router answers, or refuses, quickly; either is an answer.
            rounds.push((GATEWAY.to_string(), ip, 80));
        }
        let results = futures::future::join_all(rounds.iter().map(|(_, host, port)| dial(host, *port))).await;
        let now = Instant::now();
        {
            let mut probes = shared.lock().unwrap();
            for ((name, _, _), ms) in rounds.iter().zip(results) {
                probes.record(name, ms, now);
            }
            // A host no longer asked for leaves.
            probes.order.retain(|h| h == GATEWAY || list.contains(h));
        }
        tokio::time::sleep(EVERY).await;
    }
}

/// A TCP handshake's time, the name resolved first; refused counts as answered (the host is
/// there); a timeout or no route, as lost.
async fn dial(host: &str, port: u16) -> Option<f32> {
    let addr = tokio::time::timeout(TIMEOUT, tokio::net::lookup_host((host, port))).await.ok()?.ok()?.next()?;
    let start = Instant::now();
    match tokio::time::timeout(TIMEOUT, tokio::net::TcpStream::connect(addr)).await {
        Ok(Ok(_)) => Some(start.elapsed().as_secs_f32() * 1000.0),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => Some(start.elapsed().as_secs_f32() * 1000.0),
        _ => None,
    }
}

/// The default gateway's address: `route -n get default` (macOS), `ip route` (Linux).
async fn gateway() -> Option<String> {
    if cfg!(target_os = "macos") {
        let out = crate::platform::command("route").args(["-n", "get", "default"]).output().await.ok()?;
        String::from_utf8_lossy(&out.stdout).lines().find_map(|l| l.trim().strip_prefix("gateway:").map(|g| g.trim().to_string()))
    } else if cfg!(target_os = "linux") {
        let out = crate::platform::command("ip").args(["route", "show", "default"]).output().await.ok()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut words = text.split_whitespace();
        words.find(|w| *w == "via")?;
        words.next().map(String::from)
    } else {
        None
    }
}

/// The Wi-Fi link, through CoreWLAN: signal and rate, never the network's name (it needs Location).
#[cfg(target_os = "macos")]
pub fn wifi() -> Option<Wifi> {
    prism_macos::corewlan::link().map(|(rssi, rate)| Wifi { rssi, rate: Some(rate) })
}

/// Linux: the level of `/proc/net/wireless` (dBm) and `iw`'s tx bitrate — the interface with a
/// level, the first line of the file. No `iw`, the level alone. The network's name, macOS keeps
/// it: here too, nothing of it.
#[cfg(target_os = "linux")]
pub fn wifi() -> Option<Wifi> {
    let proc = std::fs::read_to_string("/proc/net/wireless").ok()?;
    let line = proc.lines().find(|l| !l.contains('|'))?;
    let mut words = line.split_whitespace();
    let iface = words.next()?.trim_end_matches(':');
    // status, link, level (dBm, printed with a trailing dot).
    let level = words.nth(2)?.trim_end_matches('.').parse::<i64>().ok()?;
    let rate = std::process::Command::new("iw").args(["dev", iface, "link"]).output().ok().and_then(|out| bitrate(&String::from_utf8_lossy(&out.stdout)));
    Some(Wifi { rssi: level, rate })
}

/// `tx bitrate: 866.7 MBit/s` of an `iw dev … link`, its number.
#[cfg(any(target_os = "linux", test))]
fn bitrate(out: &str) -> Option<f64> {
    let after = out.split("tx bitrate:").nth(1)?;
    let value: String = after.trim_start().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    value.parse().ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn wifi() -> Option<Wifi> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", test))]
    #[test]
    fn the_tx_bitrate_of_iw() {
        let out = "Connected to 3a:07:16:0b:86:30 (on wlp7s0)\n\tSSID: Freebox-A45EE5-5\n\tfreq: 5220.0\n\tsignal: -59 dBm\n\ttx bitrate: 866.7 MBit/s VHT-MCS 9 80MHz\n";
        assert_eq!(bitrate(out), Some(866.7));
        assert_eq!(bitrate("Connected to aa:bb (on wlan0)\n"), None);
    }

    #[test]
    fn targets_from_hosts_ports_and_urls() {
        assert_eq!(target("gitlab.com"), Some(("gitlab.com".into(), 443)));
        assert_eq!(target(" git.example.com:8443 "), Some(("git.example.com".into(), 8443)));
        assert_eq!(target("https://team.atlassian.net/browse/"), Some(("team.atlassian.net".into(), 443)));
        assert_eq!(target(""), None);
    }

    #[test]
    fn loss_jitter_and_median_over_the_window() {
        let mut p = Probes::default();
        let t = Instant::now();
        for (i, ms) in [Some(30.0), Some(34.0), None, Some(32.0)].into_iter().enumerate() {
            p.record("gitlab.com", ms, t + Duration::from_secs(15 * i as u64));
        }
        p.record("api.clickup.com", Some(90.0), t);
        p.record(GATEWAY, Some(3.0), t);
        let (median, loss, jitter) = p.summary();
        assert_eq!(median, Some(90.0), "the median of each host's last answer");
        assert_eq!(loss, 20.0, "one lost of five, the gateway apart");
        assert_eq!(jitter, Some(3.0), "|34-30| and |32-34|");
        assert_eq!(p.gateway(), Some(3.0));
        let hosts = p.hosts();
        assert_eq!(hosts.len(), 2);
        assert_eq!((hosts[0].ms, hosts[0].recent.len(), hosts[0].loss), (Some(32.0), 4, 25.0));
    }
}
