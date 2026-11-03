//! Reads the machine every `[system] poll_seconds`: CPU and memory (sysinfo; on macOS the memory
//! pressure from sysctl and its breakdown from `vm_stat`), the processes grouped by what Prism
//! knows of them, the network counters; the latency comes from `probe`.

use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use sysinfo::{Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use super::probe::{self, Probes};
use super::{Cpu, Disk, Group, History, Machine, Memory, Network, SystemConfig, SystemState};
use crate::BlockCtx;

/// A point of history every so often, for 10 minutes.
const POINT_EVERY: Duration = Duration::from_secs(10);
const POINTS: usize = 60;
/// The CPU is hot from this %.
const HOT: f32 = 90.0;
/// Applications shown beside Prism's own groups.
const OTHERS: usize = 3;
/// The disks' list and free space are read again this often.
const DISKS_EVERY: Duration = Duration::from_secs(60);

pub async fn run(cfg: SystemConfig, agent: String, ctx: BlockCtx) {
    let every = Duration::from_secs(cfg.poll_seconds.max(2));
    let probes: probe::Shared = Arc::new(Mutex::new(Probes::default()));
    let hosts_ctx = ctx.clone();
    let extra = cfg.hosts.clone();
    prism_base::rt::spawn(probe::run(probes.clone(), move || hosts(&hosts_ctx, &agent, &extra)));

    let mut sys = System::new();
    let mut nets = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
    let mut last_net = Instant::now();
    let machine = machine(&mut sys);
    let mut history = Points::default();
    let mut hot_since: Option<Instant> = None;
    let mut disks_read: Option<Instant> = None;
    // The first CPU reading, the machine's and each process's, needs one before it.
    sys.refresh_cpu_usage();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu());
    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(500))).await;
    loop {
        // The list of volumes again: one plugged in or ejected since. Once a minute: a volume
        // query takes 15–50 ms (measured), whatever is asked of it, and free space moves slowly.
        let read_disks = disks_read.is_none_or(|t| t.elapsed() >= DISKS_EVERY);
        // Off the async threads: every process read is 15 ms, the volumes 40–60 ms, which held a
        // worker, and whatever waited on it, that long.
        let refreshed = tokio::task::spawn_blocking(move || {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet));
            nets.refresh(true);
            if read_disks {
                disks.refresh(true);
            }
            (sys, nets, disks)
        })
        .await;
        let Ok(r) = refreshed else { return };
        (sys, nets, disks) = r;
        if read_disks {
            disks_read = Some(Instant::now());
        }
        let elapsed = last_net.elapsed().as_secs_f64().max(0.001);
        last_net = Instant::now();
        let (rx, tx) = nets.iter().filter(|(name, _)| !name.starts_with("lo")).fold((0u64, 0u64), |(r, t), (_, n)| (r + n.received(), t + n.transmitted()));

        let usage = sys.global_cpu_usage();
        hot_since = if usage >= HOT { hot_since.or(Some(Instant::now())) } else { None };
        let load = System::load_average();
        let memory = memory(&sys).await;
        let panes = panes().await;
        let groups = groups(&sys, &panes);
        let disks = shown(disks.list().iter().map(|d| Disk { name: d.name().to_string_lossy().into_owned(), mount: d.mount_point().display().to_string(), total: d.total_space(), available: d.available_space(), removable: d.is_removable() }).collect(), &home);

        let (latency_ms, loss, jitter_ms, gateway_ms, hosts) = {
            let p = probes.lock().unwrap();
            let (median, loss, jitter) = p.summary();
            (median, loss, jitter, p.gateway(), p.hosts())
        };
        let used_pct = if memory.total > 0 { 100.0 * memory.used as f32 / memory.total as f32 } else { 0.0 };
        history.add(usage, used_pct, latency_ms);

        ctx.publish(&SystemState {
            machine: machine.clone(),
            cpu: Cpu { usage, load: [load.one, load.five, load.fifteen], hot_seconds: hot_since.map_or(0, |t| t.elapsed().as_secs()) },
            memory,
            network: Network {
                latency_ms,
                loss,
                jitter_ms,
                rx: (rx as f64 / elapsed) as u64,
                tx: (tx as f64 / elapsed) as u64,
                gateway_ms,
                wifi: probe::wifi(),
                hosts,
            },
            disks,
            groups,
            history: history.snapshot(),
            alerts: cfg.alerts,
            disk_limits: cfg.disk.clone(),
            updated_at: Some(Utc::now()),
        });
        ctx.set_error(None);
        ctx.pause(every).await;
    }
}

/// The disks worth showing: the one the home folder is on first (the longest mount point it starts
/// with), then the external volumes (`/Volumes`, `/media`, `/mnt`, `/run/media`, or removable).
/// Not the system's own volumes (`/System/Volumes/…` on macOS, `/boot`, a simulator's), nor the
/// same volume twice.
fn shown(all: Vec<Disk>, home: &Path) -> Vec<Disk> {
    let external = |d: &Disk| d.removable || ["/Volumes/", "/media/", "/mnt/", "/run/media/"].iter().any(|p| d.mount.starts_with(p));
    let main = all.iter().filter(|d| d.total > 0 && home.starts_with(&d.mount)).max_by_key(|d| d.mount.len()).cloned();
    let mut out: Vec<Disk> = main.into_iter().collect();
    for d in all.into_iter().filter(|d| d.total > 0 && external(d)) {
        if !out.iter().any(|o| o.mount == d.mount) {
            out.push(d);
        }
    }
    out
}

fn machine(sys: &mut System) -> Machine {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    let chip = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    Machine { chip, cores: sys.cpus().len(), memory: sys.total_memory() }
}

/// The hosts measured: Prism's forge, its tracker, its agents' API, then `[system] hosts`.
fn hosts(ctx: &BlockCtx, agent: &str, extra: &[String]) -> Vec<String> {
    let env = ctx.env();
    let mut list: Vec<String> = Vec::new();
    if env.code {
        match env.forge {
            crate::forge::Provider::Gitlab => list.push(env.gitlab.host.trim().to_string()),
            crate::forge::Provider::Github => list.push("api.github.com".into()),
        }
    }
    let (tracker, base) = ctx.snapshot(|s| (s.tracker, s.ticket_base.clone()));
    match tracker {
        Some(crate::tracker::Provider::Clickup) => list.push("api.clickup.com".into()),
        Some(crate::tracker::Provider::Jira) => list.extend(base.as_deref().and_then(probe::target).map(|(h, _)| h)),
        _ => {}
    }
    match agent {
        "claude" => list.push("api.anthropic.com".into()),
        "codex" => list.push("api.openai.com".into()),
        _ => {}
    }
    list.extend(extra.iter().map(|h| h.trim().to_string()));
    let mut seen = std::collections::HashSet::new();
    list.retain(|h| !h.is_empty() && seen.insert(h.to_lowercase()));
    list
}

/// The memory: sysinfo's figures; on macOS, its pressure and what applications, the system and
/// the compressor hold.
async fn memory(sys: &System) -> Memory {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut m = Memory { total: sys.total_memory(), used: sys.used_memory(), swap: sys.used_swap(), ..Default::default() };
    #[cfg(target_os = "macos")]
    {
        m.pressure = pressure();
        if let Some((app, wired, compressed)) = vm_stat().await {
            (m.app, m.wired, m.compressed) = (Some(app), Some(wired), Some(compressed));
        }
    }
    m
}

/// `kern.memorystatus_vm_pressure_level`: 1 normal, 2 warn, 4 critical.
#[cfg(target_os = "macos")]
fn pressure() -> super::Pressure {
    use super::Pressure;
    match prism_sys::sysctl::int(c"kern.memorystatus_vm_pressure_level") {
        Some(1) => Pressure::Normal,
        Some(2) => Pressure::Warn,
        Some(4) => Pressure::Critical,
        _ => Pressure::Unknown,
    }
}

/// App (anonymous less purgeable), wired and compressed memory, in bytes, from `vm_stat`.
#[cfg(target_os = "macos")]
async fn vm_stat() -> Option<(u64, u64, u64)> {
    let out = crate::platform::command("vm_stat").output().await.ok()?;
    parse_vm_stat(&String::from_utf8_lossy(&out.stdout))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_vm_stat(text: &str) -> Option<(u64, u64, u64)> {
    let page: u64 = text.lines().next()?.split("page size of").nth(1)?.split_whitespace().next()?.parse().ok()?;
    let pages: HashMap<&str, u64> = text.lines().skip(1).filter_map(|l| {
        let (k, v) = l.split_once(':')?;
        Some((k.trim(), v.trim().trim_end_matches('.').parse().ok()?))
    }).collect();
    let get = |k: &str| pages.get(k).copied().unwrap_or(0) * page;
    let app = get("Anonymous pages").saturating_sub(get("Pages purgeable"));
    Some((app, get("Pages wired down"), get("Pages occupied by compressor")))
}

/// Prism's tmux panes, by session: an agent's (`prism-…`) or a shell's (`prismsh-…`), and the pid
/// of what runs in it.
async fn panes() -> Vec<(String, u32)> {
    let out = crate::platform::command("tmux").args(["-L", &crate::profile::tmux_socket(), "list-panes", "-a", "-F", "#{session_name} #{pane_pid}"]).output().await;
    let Ok(out) = out else { return Vec::new() };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| {
        let (name, pid) = l.rsplit_once(' ')?;
        Some((name.to_string(), pid.parse().ok()?))
    }).collect()
}

/// Which of Prism's groups a root process starts.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    Agents,
    Shells,
    Docker,
    Prism,
}

/// Docker's virtual machine and its helpers, whichever runs it.
fn is_docker(name: &str) -> bool {
    let n = name.to_lowercase();
    n.starts_with("com.docker") || n.starts_with("docker") || n.contains("orbstack") || n.starts_with("colima") || n.starts_with("limactl") || n == "com.apple.virtualization.virtualmachine" || n.starts_with("qemu-system")
}

/// The processes, each in the group of its closest root (an agent's pane, a shell's, Docker's VM,
/// Prism itself), the others by application; Prism's groups first, then the biggest others.
fn groups(sys: &System, panes: &[(String, u32)]) -> Vec<Group> {
    let procs = sys.processes();
    let mut roots: HashMap<u32, Kind> = HashMap::new();
    for (session, pid) in panes {
        roots.insert(*pid, if session.starts_with("prismsh-") { Kind::Shells } else { Kind::Agents });
    }
    roots.insert(std::process::id(), Kind::Prism);
    for (pid, p) in procs {
        if is_docker(&p.name().to_string_lossy()) {
            roots.entry(pid.as_u32()).or_insert(Kind::Docker);
        }
    }

    #[derive(Default)]
    struct Sum {
        cpu: f32,
        memory: u64,
        count: usize,
        names: HashMap<String, usize>,
    }
    let mut ours: HashMap<Kind, Sum> = HashMap::new();
    let mut apps: HashMap<String, Sum> = HashMap::new();
    let mut agent_roots = 0;
    for (pid, p) in procs {
        // Threads are not processes of their own.
        if p.thread_kind().is_some_and(|k| k == sysinfo::ThreadKind::Userland) {
            continue;
        }
        let name = p.name().to_string_lossy().into_owned();
        let kind = owner(*pid, procs, &roots);
        let sum = match kind {
            Some(k) => ours.entry(k).or_default(),
            None => apps.entry(app_of(p.exe(), &name)).or_default(),
        };
        sum.cpu += p.cpu_usage();
        sum.memory += p.memory();
        sum.count += 1;
        if kind == Some(Kind::Agents) && roots.get(&pid.as_u32()) == Some(&Kind::Agents) {
            agent_roots += 1;
            *sum.names.entry(name).or_default() += 1;
        }
    }

    let mut out = Vec::new();
    for (kind, id, label) in [(Kind::Agents, "agents", "Prism agents"), (Kind::Docker, "docker", "Docker"), (Kind::Shells, "shells", "Shells & dev servers"), (Kind::Prism, "prism", "Prism")] {
        let Some(s) = ours.get(&kind) else { continue };
        let detail = match kind {
            Kind::Agents => {
                let mut names: Vec<(&String, &usize)> = s.names.iter().collect();
                names.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                let list = names.iter().map(|(n, c)| if **c > 1 { format!("{n} ×{c}") } else { n.to_string() }).collect::<Vec<_>>().join(", ");
                format!("{agent_roots}{}", if list.is_empty() { String::new() } else { format!(" ({list})") })
            }
            _ => plural(s.count, "process", "processes"),
        };
        out.push(Group { kind: id.into(), name: label.into(), detail, cpu: s.cpu, memory: s.memory });
    }
    let mut others: Vec<(String, Sum)> = apps.into_iter().collect();
    // The heaviest: CPU first, memory as a tie-break weighted to compare (1 GB ≈ 10 %).
    let weight = |s: &Sum| s.cpu + s.memory as f32 / 1e8;
    others.sort_by(|a, b| weight(&b.1).total_cmp(&weight(&a.1)));
    for (name, s) in others.into_iter().take(OTHERS) {
        out.push(Group { kind: "app".into(), detail: plural(s.count, "process", "processes"), name, cpu: s.cpu, memory: s.memory });
    }
    out
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The group of a process: the first root met going up its parents.
fn owner(pid: Pid, procs: &HashMap<Pid, sysinfo::Process>, roots: &HashMap<u32, Kind>) -> Option<Kind> {
    let mut at = Some(pid);
    for _ in 0..64 {
        let p = at?;
        if let Some(k) = roots.get(&p.as_u32()) {
            return Some(*k);
        }
        at = procs.get(&p).and_then(|x| x.parent());
    }
    None
}

/// The application a process belongs to: the outermost `.app` of its path (Chrome's helpers under
/// Google Chrome), otherwise its name.
fn app_of(exe: Option<&Path>, name: &str) -> String {
    exe.and_then(|e| e.components().find_map(|c| c.as_os_str().to_str().and_then(|s| s.strip_suffix(".app")).map(String::from))).unwrap_or_else(|| name.to_string())
}

/// The last 10 minutes, a point every `POINT_EVERY`: each the mean of the readings since.
#[derive(Default)]
struct Points {
    cpu: VecDeque<f32>,
    memory: VecDeque<f32>,
    latency: VecDeque<Option<f32>>,
    pending: Vec<(f32, f32, Option<f32>)>,
    last: Option<Instant>,
}

impl Points {
    fn add(&mut self, cpu: f32, memory: f32, latency: Option<f32>) {
        self.pending.push((cpu, memory, latency));
        if self.last.is_some_and(|t| t.elapsed() < POINT_EVERY) {
            return;
        }
        self.last = Some(Instant::now());
        let n = self.pending.len() as f32;
        let mean = |f: fn(&(f32, f32, Option<f32>)) -> f32| self.pending.iter().map(f).sum::<f32>() / n;
        let (c, m) = (mean(|p| p.0), mean(|p| p.1));
        let l = self.pending.iter().rev().find_map(|p| p.2);
        self.pending.clear();
        for (list, v) in [(&mut self.cpu, c), (&mut self.memory, m)] {
            list.push_back(v);
            if list.len() > POINTS {
                list.pop_front();
            }
        }
        self.latency.push_back(l);
        if self.latency.len() > POINTS {
            self.latency.pop_front();
        }
    }

    fn snapshot(&self) -> History {
        History { cpu: self.cpu.iter().copied().collect(), memory: self.memory.iter().copied().collect(), latency: self.latency.iter().copied().collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_stat_gives_app_wired_and_compressed() {
        let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:                               12000.\nAnonymous pages:                        1000000.\nPages purgeable:                          50000.\nPages wired down:                        400000.\nPages occupied by compressor:            200000.\n";
        assert_eq!(parse_vm_stat(text), Some((950_000 * 16384, 400_000 * 16384, 200_000 * 16384)));
        assert_eq!(parse_vm_stat("nothing"), None);
    }

    #[test]
    fn an_app_is_its_outermost_bundle() {
        let helper = Path::new("/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)");
        assert_eq!(app_of(Some(helper), "Google Chrome Helper (Renderer)"), "Google Chrome");
        assert_eq!(app_of(Some(Path::new("/usr/bin/node")), "node"), "node");
        assert_eq!(app_of(None, "kernel_task"), "kernel_task");
    }

    /// This machine, read for real: `cargo test -p prism-blocks reads_this_machine -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn reads_this_machine() {
        let mut sys = System::new();
        let m = machine(&mut sys);
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu());
        tokio::time::sleep(Duration::from_secs(1)).await;
        sys.refresh_cpu_usage();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cpu().with_memory().with_exe(UpdateKind::OnlyIfNotSet));
        let mem = memory(&sys).await;
        let panes = panes().await;
        println!("{} · {} cores · CPU {:.0} % · memory {:?}", m.chip, m.cores, sys.global_cpu_usage(), mem);
        println!("panes: {}", panes.len());
        for g in groups(&sys, &panes) {
            println!("  {:<22} {:>6.1} % {:>6} MB  {}", g.name, g.cpu, g.memory >> 20, g.detail);
        }
        println!("wifi: {:?}", super::probe::wifi());
        assert!(m.cores > 0 && mem.total > 0);
    }

    /// This computer's disks, as shown: `cargo test -p prism-blocks this_machines_disks -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn this_machines_disks() {
        let disks = Disks::new_with_refreshed_list();
        let all: Vec<Disk> = disks.list().iter().map(|d| Disk { name: d.name().to_string_lossy().into_owned(), mount: d.mount_point().display().to_string(), total: d.total_space(), available: d.available_space(), removable: d.is_removable() }).collect();
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
        for d in shown(all, &home) {
            println!("{} {} {} GB free of {} GB", d.name, d.mount, d.available >> 30, d.total >> 30);
        }
    }

    #[test]
    fn the_home_disk_then_the_external_ones() {
        let d = |mount: &str, removable: bool| Disk { name: mount.into(), mount: mount.into(), total: 100, available: 40, removable };
        let all = vec![d("/", false), d("/System/Volumes/Data", false), d("/System/Volumes/VM", false), d("/Volumes/Backup", false), d("/Library/Developer/CoreSimulator/Volumes/iOS", false), d("/Volumes/Backup", false), d("/Volumes/KEY", true)];
        let mounts = |v: Vec<Disk>| v.into_iter().map(|d| d.mount).collect::<Vec<_>>();
        assert_eq!(mounts(shown(all, Path::new("/Users/me"))), ["/", "/Volumes/Backup", "/Volumes/KEY"]);
        // Linux, a /home of its own.
        let all = vec![d("/", false), d("/home", false), d("/boot", false), d("/media/me/usb", true)];
        assert_eq!(mounts(shown(all, Path::new("/home/me"))), ["/home", "/media/me/usb"]);
    }

    #[test]
    fn docker_by_its_vm() {
        assert!(is_docker("com.docker.backend"));
        assert!(is_docker("com.apple.Virtualization.VirtualMachine"));
        assert!(is_docker("OrbStack Helper"));
        assert!(!is_docker("node"));
    }
}
