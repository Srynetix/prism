//! The System block in the demo profile: a busy afternoon, the memory under pressure.

use chrono::{DateTime, Utc};

use super::{Cpu, Disk, Group, History, Host, Machine, Memory, Network, Pressure, SystemState, Wifi};

const GB: u64 = 1 << 30;
const MB: u64 = 1 << 20;

pub fn state(now: DateTime<Utc>) -> SystemState {
    let wave = |base: f32, amp: f32, i: usize| base + amp * ((i as f32) / 6.0).sin() + if i % 17 == 9 { amp } else { 0.0 };
    let cpu: Vec<f32> = (0..60).map(|i| wave(38.0, 14.0, i).clamp(4.0, 100.0)).collect();
    let memory: Vec<f32> = (0..60).map(|i| 72.0 + i as f32 * 0.17).collect();
    let latency: Vec<Option<f32>> = (0..60).map(|i| if i == 41 { None } else { Some(wave(18.0, 3.0, i)) }).collect();
    let group = |kind: &str, name: &str, detail: &str, cpu: f32, memory: f64| Group { kind: kind.into(), name: name.into(), detail: detail.into(), cpu, memory: (memory * GB as f64) as u64 };
    let host = |name: &str, ms: f32, recent: [f32; 8]| Host { name: name.into(), ms: Some(ms), recent: recent.iter().map(|v| Some(*v)).collect(), loss: 0.0 };
    SystemState {
        machine: Machine { chip: "Apple M3 Pro".into(), cores: 12, memory: 36 * GB },
        cpu: Cpu { usage: 47.0, load: [6.2, 5.4, 4.8], hot_seconds: 0 },
        memory: Memory { total: 36 * GB, used: 29 * GB + 400 * MB, app: Some(18 * GB + 700 * MB), wired: Some(7 * GB), compressed: Some(3 * GB + 600 * MB), swap: GB, pressure: Pressure::Warn },
        network: Network {
            latency_ms: Some(18.0),
            loss: 0.0,
            jitter_ms: Some(4.0),
            rx: 3_100_000,
            tx: 400_000,
            gateway_ms: Some(3.0),
            wifi: Some(Wifi { rssi: -58, rate: 866.0 }),
            hosts: vec![
                host("gitlab.example.org", 31.0, [30.0, 33.0, 29.0, 64.0, 31.0, 30.0, 32.0, 31.0]),
                host("api.clickup.com", 96.0, [94.0, 97.0, 92.0, 99.0, 95.0, 96.0, 98.0, 96.0]),
                host("api.anthropic.com", 42.0, [43.0, 42.0, 45.0, 44.0, 42.0, 43.0, 46.0, 42.0]),
            ],
        },
        disks: vec![
            Disk { name: "Macintosh HD".into(), mount: "/".into(), total: 994 * GB, available: 61 * GB, removable: false },
            Disk { name: "Backup".into(), mount: "/Volumes/Backup".into(), total: 1863 * GB, available: 712 * GB, removable: false },
        ],
        groups: vec![
            group("agents", "Prism agents", "3 (claude ×2, codex)", 19.0, 2.4),
            group("docker", "Docker", "7 processes", 12.0, 8.0),
            group("shells", "Shells & dev servers", "5 processes", 6.0, 1.1),
            group("prism", "Prism", "4 processes", 2.0, 0.6),
            group("app", "Google Chrome", "41 processes", 5.0, 4.9),
            group("app", "Slack", "6 processes", 1.0, 0.9),
        ],
        history: History { cpu, memory, latency },
        alerts: true,
        disk_limits: Default::default(),
        updated_at: Some(now),
    }
}
