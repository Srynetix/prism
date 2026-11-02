//! The Dev servers block's collector. Every `poll_seconds`: the panes of Prism's tmux server (the
//! agents and their shells), the process tree (`ps`), the TCP ports listened on (`lsof`, or `ss`
//! on Linux) and the Compose containers (`docker ps`, when Docker answers). A shell whose
//! processes listen on a port runs a server; one that had a server and is back at its prompt
//! shows its last lines, for why it stopped. A server an agent started itself (its Bash tool) is
//! one of its processes under a shell. The rest, system and application programs aside, is
//! "outside Prism".

use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use prism_base::{i18n::tr, tr};
use chrono::{DateTime, Utc};

use super::{DevServer, DevServersConfig, DevServersState, Listener};
use crate::BlockCtx;

/// A command that does not answer within this is given up for this pass.
const TIMEOUT: Duration = Duration::from_secs(5);
/// Lines of a stopped server's shell kept.
const TAIL_LINES: usize = 12;

struct Reader {
    refresh: tokio::sync::Notify,
    memory: Mutex<Memory>,
}

#[derive(Default)]
struct Memory {
    /// When each server was first seen, by key.
    since: HashMap<String, DateTime<Utc>>,
    /// The last pass's servers, by key: one gone back to its prompt becomes `stopped`.
    last: HashMap<String, DevServer>,
    /// Stopped servers kept until their shell runs something again, closes, or is dismissed.
    stopped: HashMap<String, DevServer>,
    /// What the window shows: the only pids and containers `stop` and `kill` reach.
    shown: DevServersState,
    /// Servers stopped from Prism: they leave without a "stopped" line.
    stopping: HashSet<String>,
}

static READER: prism_base::rt::Swap<Reader> = prism_base::rt::Swap::new();

fn reader() -> Result<&'static Reader> {
    READER.get().context(tr("the Dev servers block is off ([dev_servers] enabled)"))
}

pub async fn run(cfg: DevServersConfig, ctx: BlockCtx) {
    let every = Duration::from_secs(cfg.poll_seconds.max(2));
    let r = READER.set(Reader { refresh: tokio::sync::Notify::new(), memory: Default::default() });
    let mut first = true;
    loop {
        if !std::mem::take(&mut first) {
            tokio::select! {
                _ = ctx.pause(every) => {}
                _ = r.refresh.notified() => {}
            }
        }
        match pass(&ctx, cfg.scan_outside).await {
            Ok(read) => {
                let state = r.settle(read, cfg.scan_outside).await;
                ctx.set_error(None);
                ctx.publish(&state);
            }
            Err(e) => ctx.set_error(Some(format!("{e:#}"))),
        }
    }
}

pub fn refresh() {
    if let Some(r) = READER.get() {
        r.refresh.notify_one();
    }
}

/// Ctrl-C in a server's shell; `docker stop` for a container; SIGTERM for a server an agent started.
pub async fn stop(key: &str) -> Result<()> {
    let r = reader()?;
    let shown = r.memory.lock().unwrap().shown.servers.iter().any(|s| s.key == key);
    if !shown {
        bail!(tr!("no such server: {key}", key = key));
    }
    r.memory.lock().unwrap().stopping.insert(key.to_string());
    if key.starts_with(crate::agents::tmux::SHELL) {
        tmux(&["send-keys", "-t", &crate::agents::tmux::Tmux::session(key), "C-c"]).await?;
    } else if let Some(name) = key.strip_prefix("docker:") {
        output("docker", &["stop", name]).await?;
    } else if let Some(pid) = key.strip_prefix("pid:") {
        output("kill", &[pid]).await?;
    }
    refresh_soon();
    Ok(())
}

/// SIGTERM to a process holding a port outside Prism, or `docker stop` for an outside container.
pub async fn kill(pid: u32) -> Result<()> {
    let r = reader()?;
    let found = r.memory.lock().unwrap().shown.outside.iter().find(|l| l.pid == pid && pid != 0).cloned();
    match found {
        Some(l) if l.docker => output("docker", &["stop", &l.program]).await.map(drop)?,
        Some(_) => output("kill", &[&pid.to_string()]).await.map(drop)?,
        None => bail!(tr!("pid {pid} is not one the block shows", pid = pid)),
    }
    refresh_soon();
    Ok(())
}

pub fn dismiss(key: &str) {
    if let Some(r) = READER.get() {
        r.memory.lock().unwrap().stopped.remove(key);
        r.refresh.notify_one();
    }
}

/// A stop takes effect a moment later: read again then.
fn refresh_soon() {
    prism_base::rt::spawn(async {
        tokio::time::sleep(Duration::from_millis(800)).await;
        refresh();
    });
}

/// One read: what runs now, before the memory of previous passes.
struct Read {
    running: Vec<DevServer>,
    /// The shells at their prompt, by key; and every pane still open.
    idle: HashSet<String>,
    open: HashSet<String>,
    outside: Vec<Listener>,
}

impl Reader {
    /// Folds a read into the memory: first-seen times, stopped servers and their last lines.
    async fn settle(&self, read: Read, scan_outside: bool) -> DevServersState {
        let now = Utc::now();
        let (gone, stopping) = {
            let mut m = self.memory.lock().unwrap();
            let running: HashSet<&str> = read.running.iter().map(|s| s.key.as_str()).collect();
            let gone: Vec<DevServer> = m.last.values().filter(|s| !running.contains(s.key.as_str()) && read.idle.contains(&s.key)).cloned().collect();
            let stopping = std::mem::take(&mut m.stopping);
            (gone, stopping)
        };
        // The shells whose server just stopped: their last lines, read outside the lock.
        let mut stopped_now = Vec::new();
        for s in gone.into_iter().filter(|s| !stopping.contains(&s.key)) {
            let tail = tail(&s.key).await.unwrap_or_default();
            let failed = reads_as_failure(&tail);
            stopped_now.push(DevServer { state: "stopped".into(), ports: Vec::new(), tail: Some(tail).filter(|t| !t.is_empty()), failed, ..s });
        }
        let mut m = self.memory.lock().unwrap();
        for s in stopped_now {
            m.stopped.insert(s.key.clone(), s);
        }
        // A stopped line leaves when its shell closes or runs something again.
        let busy: HashSet<String> = read.running.iter().map(|s| s.key.clone()).collect();
        m.stopped.retain(|key, _| read.open.contains(key) && !busy.contains(key));
        let mut servers = read.running;
        for s in &mut servers {
            s.since = Some(*m.since.entry(s.key.clone()).or_insert(now));
        }
        let keep: HashSet<String> = servers.iter().map(|s| s.key.clone()).collect();
        m.since.retain(|k, _| keep.contains(k));
        m.last = servers.iter().filter(|s| s.state == "running").map(|s| (s.key.clone(), s.clone())).collect();
        servers.extend(m.stopped.values().cloned());
        servers.sort_by(|a, b| (&a.agent, a.docker, a.shell, &a.key).cmp(&(&b.agent, b.docker, b.shell, &b.key)));
        let state = DevServersState { servers, outside: read.outside, scan_outside, read_at: Some(now) };
        m.shown = state.clone();
        state
    }
}

/// An agent's folder: its worktree, or the folder it was started in; and when it was launched.
struct AgentDir {
    id: String,
    dir: String,
    created_at: DateTime<Utc>,
}

/// The agent a Compose container belongs to: one whose folder it was started from, launched
/// before the container started (the last such, when several share the folder). A container
/// already running when the agent was launched is not its (nor one whose start is unknown).
fn owner<'a>(c: &Container, agents: &'a [AgentDir]) -> Option<&'a AgentDir> {
    let wd = c.working_dir.as_deref()?;
    let started = c.started_at?;
    agents
        .iter()
        .filter(|a| !a.dir.is_empty() && (wd == a.dir || wd.starts_with(&format!("{}/", a.dir))) && a.created_at <= started)
        .max_by_key(|a| a.created_at)
}

async fn pass(ctx: &BlockCtx, scan_outside: bool) -> Result<Read> {
    let agents: Vec<AgentDir> = ctx.snapshot(|s| s.agents.iter().map(|a| AgentDir { id: a.id.clone(), dir: a.worktree.clone().unwrap_or_else(|| a.cwd.clone()), created_at: a.created_at }).collect());
    let (panes, procs, ports, containers) = tokio::join!(panes(), processes(), listeners(), containers());
    let panes = panes?;
    let procs = procs?;
    let ports = ports?;
    let containers = containers.unwrap_or_default();
    let known: HashSet<&str> = agents.iter().map(|a| a.id.as_str()).collect();

    let mut running = Vec::new();
    let mut idle = HashSet::new();
    let mut open = HashSet::new();
    // Every process under Prism's tmux panes (and Prism itself): not "outside".
    let mut ours: HashSet<u32> = procs.descendants(std::process::id());
    ours.insert(std::process::id());
    let recorded = crate::agents::tmux::shell_commands();
    for p in panes.iter().filter(|p| known.contains(p.agent.as_str())) {
        open.insert(p.key.clone());
        let tree = procs.descendants(p.pid);
        ours.extend(&tree);
        ours.insert(p.pid);
        if let Some(server_pid) = procs.parent(p.pid) {
            // The tmux server: whatever else it runs (a pane of an agent not shown) stays ours.
            ours.insert(server_pid);
        }
        match p.shell {
            Some(n) => {
                let mut found: Vec<u16> = tree.iter().filter_map(|pid| ports.get(pid)).flatten().copied().collect();
                found.sort();
                found.dedup();
                let busy = !is_shell(&p.command);
                let line = recorded.get(&p.key).map(|(c, _)| c.clone()).or_else(|| procs.children(p.pid).first().map(|c| procs.args(*c).to_string()));
                if !busy {
                    idle.insert(p.key.clone());
                    // The line Prism typed is the shell's no longer once it is back at its prompt.
                    if recorded.get(&p.key).is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(10)) {
                        crate::agents::tmux::forget_shell_command(&p.key);
                    }
                    continue;
                }
                let line = line.unwrap_or_else(|| p.command.clone());
                let state = if !found.is_empty() {
                    "running"
                } else if looks_like_server(&line) {
                    "starting"
                } else {
                    continue;
                };
                running.push(DevServer { key: p.key.clone(), agent: p.agent.clone(), shell: Some(n), typed: recorded.contains_key(&p.key), command: line, program: p.command.clone(), ports: found, state: state.into(), tail: None, failed: false, docker: false, since: None });
            }
            None => {
                // A server the agent started itself: a listening process under a shell (its
                // Bash tool) or a server's command it ran, not the CLI nor its MCP servers.
                for pid in tree.iter().copied().filter(|pid| ports.contains_key(pid) && procs.started_by_command(*pid, p.pid)) {
                    let mut found = ports[&pid].clone();
                    found.sort();
                    found.dedup();
                    running.push(DevServer { key: format!("pid:{pid}"), agent: p.agent.clone(), shell: None, typed: false, command: procs.args(pid).to_string(), program: procs.program(pid), ports: found, state: "running".into(), tail: None, failed: false, docker: false, since: None });
                }
            }
        }
    }

    // Compose containers started from an agent's folder since it was launched.
    let mut outside = Vec::new();
    for c in &containers {
        match owner(c, &agents) {
            Some(a) => running.push(DevServer { key: format!("docker:{}", c.name), agent: a.id.clone(), shell: None, typed: false, command: c.image.clone(), program: c.name.clone(), ports: c.ports.clone(), state: "running".into(), tail: None, failed: false, docker: true, since: None }),
            None if scan_outside => outside.extend(c.ports.iter().map(|&port| Listener { port, pid: docker_pid(&c.name), program: c.name.clone(), command: c.image.clone(), docker: true })),
            None => {}
        }
    }
    // A container has no pid of its own here: a stand-in one, which `kill` turns into `docker stop`.

    if scan_outside {
        let docker_ports: HashSet<u16> = containers.iter().flat_map(|c| c.ports.iter().copied()).collect();
        let mut pids: Vec<&u32> = ports.keys().collect();
        pids.sort();
        for &pid in pids {
            if ours.contains(&pid) || procs.is_system(pid) {
                continue;
            }
            for &port in &ports[&pid] {
                if !docker_ports.contains(&port) && !outside.iter().any(|l| l.port == port) {
                    outside.push(Listener { port, pid, program: procs.program(pid), command: procs.args(pid).to_string(), docker: false });
                }
            }
        }
        outside.sort_by_key(|l| l.port);
    }
    Ok(Read { running, idle, open, outside })
}

/// A container's stand-in pid: stable for its name, never a real process's (above `pid_max`).
fn docker_pid(name: &str) -> u32 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut h);
    0x4000_0000 | (h.finish() as u32 & 0x3fff_ffff)
}

/* ---------- tmux ---------- */

struct PaneInfo {
    /// The terminal key: the agent's id, or `shell:<agent>:<n>`.
    key: String,
    agent: String,
    shell: Option<u32>,
    pid: u32,
    command: String,
}

async fn tmux(args: &[&str]) -> Result<String> {
    let socket = crate::profile::tmux_socket();
    let mut all = vec!["-L", socket.as_str()];
    all.extend_from_slice(args);
    output("tmux", &all).await
}

async fn panes() -> Result<Vec<PaneInfo>> {
    let out = match tmux(&["list-panes", "-a", "-F", "#{session_name}\t#{pane_pid}\t#{pane_current_command}\t#{pane_dead}"]).await {
        Ok(out) => out,
        // No server: no agent.
        Err(e) if e.to_string().contains("no server") || e.to_string().contains("error connecting") => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    Ok(parse_panes(&out))
}

fn parse_panes(out: &str) -> Vec<PaneInfo> {
    out.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let (session, pid, command, dead) = (f.next()?, f.next()?.parse().ok()?, f.next()?.to_string(), f.next()? == "1");
            if dead {
                return None;
            }
            if let Some(rest) = session.strip_prefix("prismsh-") {
                let (agent, n) = rest.rsplit_once('-')?;
                let n: u32 = n.parse().ok()?;
                Some(PaneInfo { key: format!("{}{agent}:{n}", crate::agents::tmux::SHELL), agent: agent.into(), shell: Some(n), pid, command })
            } else {
                let agent = session.strip_prefix(crate::agents::tmux::PREFIX)?;
                Some(PaneInfo { key: agent.into(), agent: agent.into(), shell: None, pid, command })
            }
        })
        .collect()
}

/// A shell's last non-empty lines.
async fn tail(key: &str) -> Result<String> {
    let out = tmux(&["capture-pane", "-p", "-J", "-t", &crate::agents::tmux::Tmux::session(key), "-S", "-60"]).await?;
    let lines: Vec<&str> = out.lines().map(str::trim_end).collect();
    let end = lines.iter().rposition(|l| !l.is_empty()).map_or(0, |i| i + 1);
    // The prompt the shell is back at is not part of why it stopped.
    let end = if end > 0 && looks_like_prompt(lines[end - 1]) { end - 1 } else { end };
    let lines: Vec<&str> = lines[..end].iter().copied().filter(|l| !l.is_empty()).collect();
    Ok(lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n"))
}

fn looks_like_prompt(line: &str) -> bool {
    let l = line.trim_end();
    ["$", "%", "#", "❯", "➜", ">"].iter().any(|p| l.ends_with(p)) || l.contains("❯ ") || l.starts_with("➜ ")
}

/// The last lines of a stopped server read as an error, not as a Ctrl-C.
fn reads_as_failure(tail: &str) -> bool {
    let t = tail.to_lowercase();
    if t.lines().last().is_some_and(|l| l.contains("^c")) || t.contains("keyboardinterrupt") {
        return false;
    }
    ["error", "traceback", "exception", "panicked", "eaddrinuse", "address already in use", "exited with", "exit status", "failed", "err!", "segmentation fault", "killed"].iter().any(|w| t.contains(w))
}

/// A shell at its prompt rather than a program it runs (`pane_current_command`).
fn is_shell(command: &str) -> bool {
    matches!(command.trim_start_matches('-'), "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "mksh" | "tcsh" | "csh" | "nu" | "xonsh" | "elvish" | "pwsh")
}

/// A command line that starts a server, by its words: `dev`, `serve`, `start`, `watch`, `up`, `run`server…
pub fn looks_like_server(line: &str) -> bool {
    line.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| matches!(w.to_ascii_lowercase().as_str(), "dev" | "serve" | "server" | "start" | "watch" | "up" | "runserver" | "uvicorn" | "gunicorn" | "vite" | "next" | "storybook" | "preview"))
}

/* ---------- processes ---------- */

#[derive(Default)]
struct Procs {
    parent: HashMap<u32, u32>,
    children: HashMap<u32, Vec<u32>>,
    args: HashMap<u32, String>,
}

impl Procs {
    fn parent(&self, pid: u32) -> Option<u32> {
        self.parent.get(&pid).copied()
    }

    fn children(&self, pid: u32) -> Vec<u32> {
        self.children.get(&pid).cloned().unwrap_or_default()
    }

    fn descendants(&self, pid: u32) -> HashSet<u32> {
        let mut out = HashSet::new();
        let mut todo = vec![pid];
        while let Some(p) = todo.pop() {
            for &c in self.children.get(&p).into_iter().flatten() {
                if out.insert(c) {
                    todo.push(c);
                }
            }
        }
        out
    }

    fn args(&self, pid: u32) -> &str {
        self.args.get(&pid).map(String::as_str).unwrap_or_default()
    }

    /// Its program's name: `node`, `postgres`.
    fn program(&self, pid: u32) -> String {
        let first = self.args(pid).split_whitespace().next().unwrap_or_default();
        first.rsplit('/').next().unwrap_or(first).to_string()
    }

    /// Started by a command line of the CLI `top`, not by the CLI itself (its MCP servers): a shell
    /// between them (Claude's Bash tool), or a server's command run straight from the CLI (Codex
    /// runs `npm run dev` without a shell; an MCP server's `server` is not one). `pid` itself counts as
    /// that command.
    fn started_by_command(&self, pid: u32, top: u32) -> bool {
        let mut x = pid;
        while x != top && x > 1 {
            let Some(parent) = self.parent(x) else { return false };
            if (x != pid && is_shell(&self.program(x))) || (parent == top && looks_like_server(self.args(x)) && !self.args(x).to_ascii_lowercase().contains("mcp")) {
                return true;
            }
            x = parent;
        }
        false
    }

    /// A system or application program (macOS's services, an app's helper): never a dev server.
    fn is_system(&self, pid: u32) -> bool {
        let exe = self.args(pid).split_whitespace().next().unwrap_or_default();
        ["/System/", "/usr/libexec/", "/usr/sbin/", "/sbin/", "/Applications/", "/Library/", "/snap/", "/usr/lib/"].iter().any(|p| exe.starts_with(p))
            || exe.contains("/Library/")
            || exe.contains(".app/")
    }
}

async fn processes() -> Result<Procs> {
    let out = output("ps", &["-eo", "pid=,ppid=,args="]).await?;
    Ok(parse_ps(&out))
}

fn parse_ps(out: &str) -> Procs {
    let mut p = Procs::default();
    for line in out.lines() {
        let line = line.trim_start();
        let Some((pid, rest)) = line.split_once(char::is_whitespace) else { continue };
        let rest = rest.trim_start();
        let (ppid, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        let (Ok(pid), Ok(ppid)) = (pid.parse::<u32>(), ppid.parse::<u32>()) else { continue };
        p.parent.insert(pid, ppid);
        p.children.entry(ppid).or_default().push(pid);
        p.args.insert(pid, args.trim().to_string());
    }
    p
}

/* ---------- ports ---------- */

/// The TCP ports listened on, by pid: `lsof`, otherwise `ss` (Linux without lsof).
async fn listeners() -> Result<HashMap<u32, Vec<u16>>> {
    match output("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"]).await {
        Ok(out) => Ok(parse_lsof(&out)),
        // lsof exits 1 when nothing listens.
        Err(e) if e.to_string().contains("exit status: 1") && !e.to_string().contains("not found") => Ok(HashMap::new()),
        Err(e) => match output("ss", &["-ltnpH"]).await {
            Ok(out) => Ok(parse_ss(&out)),
            Err(_) => Err(e.context(tr("the listening ports are read with lsof (or ss)"))),
        },
    }
}

fn parse_lsof(out: &str) -> HashMap<u32, Vec<u16>> {
    let mut map: HashMap<u32, Vec<u16>> = HashMap::new();
    let mut pid = None;
    for line in out.lines() {
        if let Some(p) = line.strip_prefix('p') {
            pid = p.parse().ok();
        } else if let (Some(addr), Some(pid)) = (line.strip_prefix('n'), pid)
            && let Some(port) = addr.rsplit(':').next().and_then(|p| p.parse().ok())
        {
            let ports = map.entry(pid).or_default();
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    map
}

fn parse_ss(out: &str) -> HashMap<u32, Vec<u16>> {
    let mut map: HashMap<u32, Vec<u16>> = HashMap::new();
    for line in out.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        let Some(port) = cols.get(3).and_then(|a| a.rsplit(':').next()).and_then(|p| p.parse::<u16>().ok()) else { continue };
        for part in line.split("pid=").skip(1) {
            if let Ok(pid) = part.split(|c: char| !c.is_ascii_digit()).next().unwrap_or_default().parse::<u32>() {
                let ports = map.entry(pid).or_default();
                if !ports.contains(&port) {
                    ports.push(port);
                }
            }
        }
    }
    map
}

/* ---------- docker ---------- */

struct Container {
    name: String,
    image: String,
    ports: Vec<u16>,
    working_dir: Option<String>,
    /// When it started (`docker inspect`): read for the Compose ones only.
    started_at: Option<DateTime<Utc>>,
}

/// The running containers with published ports; none when Docker is absent or does not answer.
async fn containers() -> Result<Vec<Container>> {
    let out = output("docker", &["ps", "--format", "{{.Names}}\t{{.Image}}\t{{.Ports}}\t{{.Label \"com.docker.compose.project.working_dir\"}}"]).await?;
    let mut list = parse_docker(&out);
    let compose: Vec<&str> = list.iter().filter(|c| c.working_dir.is_some()).map(|c| c.name.as_str()).collect();
    if !compose.is_empty() {
        let mut args = vec!["container", "inspect", "--format", "{{.Name}}\t{{.State.StartedAt}}"];
        args.extend(compose);
        // Unknown starts belong to no agent: the containers still show, outside Prism.
        let started = output("docker", &args).await.map(|o| parse_started(&o)).unwrap_or_default();
        for c in &mut list {
            c.started_at = started.get(&c.name).copied();
        }
    }
    Ok(list)
}

/// `docker inspect`'s `/name\t2026-10-19T16:39:51.123456789Z` lines, by name.
fn parse_started(out: &str) -> HashMap<String, DateTime<Utc>> {
    out.lines()
        .filter_map(|l| {
            let (name, at) = l.split_once('\t')?;
            Some((name.trim().trim_start_matches('/').to_string(), DateTime::parse_from_rfc3339(at.trim()).ok()?.with_timezone(&Utc)))
        })
        .collect()
}

fn parse_docker(out: &str) -> Vec<Container> {
    out.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let (name, image, ports, wd) = (f.next()?, f.next()?, f.next().unwrap_or_default(), f.next().unwrap_or_default());
            // `0.0.0.0:5432->5432/tcp, :::5432->5432/tcp`: the host side.
            let mut published: Vec<u16> = ports.split(", ").filter_map(|p| p.split_once("->")?.0.rsplit(':').next()?.parse().ok()).collect();
            published.sort();
            published.dedup();
            (!published.is_empty()).then(|| Container { name: name.into(), image: image.into(), ports: published, working_dir: Some(wd.trim().to_string()).filter(|w| !w.is_empty()), started_at: None })
        })
        .collect()
}

async fn output(program: &str, args: &[&str]) -> Result<String> {
    let run = crate::platform::command(program).args(args).kill_on_drop(true).output();
    let out = tokio::time::timeout(TIMEOUT, run).await.with_context(|| tr!("{program} did not answer", program = program))?.with_context(|| tr!("{program} not found", program = program))?;
    if !out.status.success() {
        bail!("{program} {}: {} ({})", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).trim(), out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_panes_of_agents_and_shells() {
        let panes = parse_panes("prism-a1-b2\t100\tclaude\t0\nprismsh-a1-b2-3\t200\tnode\t0\nprismsh-a1-b2-4\t300\tzsh\t1\nother\t1\tzsh\t0\n");
        let keys: Vec<(&str, &str, Option<u32>)> = panes.iter().map(|p| (p.key.as_str(), p.agent.as_str(), p.shell)).collect();
        assert_eq!(keys, [("a1-b2", "a1-b2", None), ("shell:a1-b2:3", "a1-b2", Some(3))]);
    }

    #[test]
    fn reads_ports_from_lsof_and_ss() {
        let lsof = parse_lsof("p812\nf7\nn*:5432\nf8\nn[::1]:5432\np900\nf3\nn127.0.0.1:5173\n");
        assert_eq!(lsof[&812], [5432]);
        assert_eq!(lsof[&900], [5173]);
        let ss = parse_ss("LISTEN 0 4096 127.0.0.1:5432 0.0.0.0:* users:((\"postgres\",pid=812,fd=6))\nLISTEN 0 511 [::]:3000 [::]:* users:((\"node\",pid=40,fd=20),(\"node\",pid=41,fd=20))\n");
        assert_eq!(ss[&812], [5432]);
        assert_eq!((ss[&40].clone(), ss[&41].clone()), (vec![3000], vec![3000]));
    }

    #[test]
    fn reads_the_process_tree() {
        let p = parse_ps("    1     0 /sbin/launchd\n  100     1 tmux -L prism\n  200   100 -zsh\n  210   200 just api-dev\n  220   210 /bin/sh -c uvicorn app:main\n  230   220 python3 -m uvicorn app:main --port 8000\n");
        assert_eq!(p.descendants(200), HashSet::from([210, 220, 230]));
        assert_eq!(p.program(230), "python3");
        assert!(p.started_by_command(230, 100));
        assert!(!p.started_by_command(200, 100));
        assert!(p.is_system(1));
        assert!(!p.is_system(230));
        // Codex runs a command without a shell; its MCP server is not one.
        let p = parse_ps("  100     1 codex\n  110   100 npm run dev\n  120   110 node vite --host\n  130   100 node mcp-server.js\n");
        assert!(p.started_by_command(120, 100));
        assert!(p.started_by_command(110, 100));
        assert!(!p.started_by_command(130, 100));
    }

    #[test]
    fn reads_published_ports_and_compose_folders() {
        let c = parse_docker("shop-db-1\tpostgres:16\t0.0.0.0:5433->5432/tcp, :::5433->5432/tcp\t/work/shop\nworker\tshop/worker\t\t\nredis\tredis:7\t127.0.0.1:6379->6379/tcp\t\n");
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].ports.clone(), c[0].working_dir.as_deref()), (vec![5433], Some("/work/shop")));
        assert_eq!((c[1].name.as_str(), c[1].working_dir.as_deref()), ("redis", None));
    }

    #[test]
    fn a_container_is_an_agents_only_when_started_after_it() {
        let at = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        let started = parse_started("/shop-db-1\t2026-10-19T10:00:00.123456789Z\n/old\t0001-01-01T00:00:00Z\n");
        assert_eq!(started["shop-db-1"], at("2026-10-19T10:00:00.123456789Z"));
        let agent = |id: &str, dir: &str, created: &str| AgentDir { id: id.into(), dir: dir.into(), created_at: at(created) };
        let agents = [agent("early", "/work/shop", "2026-10-19T09:00:00Z"), agent("late", "/work/shop", "2026-10-19T09:30:00Z"), agent("after", "/work/shop", "2026-10-19T11:00:00Z")];
        let container = |started_at: Option<&str>| Container { name: "shop-db-1".into(), image: "postgres".into(), ports: vec![5433], working_dir: Some("/work/shop/infra".into()), started_at: started_at.map(at) };
        assert_eq!(owner(&container(Some("2026-10-19T10:00:00Z")), &agents).map(|a| a.id.as_str()), Some("late"), "the last agent launched before it started");
        assert!(owner(&container(Some("2026-10-19T08:00:00Z")), &agents).is_none(), "already running when the agents were launched");
        assert!(owner(&container(None), &agents).is_none(), "its start unknown");
    }

    #[test]
    fn tells_a_failure_from_a_ctrl_c() {
        assert!(reads_as_failure("ERROR: [Errno 48] address already in use"));
        assert!(!reads_as_failure("INFO: Shutting down\n^C"));
        assert!(!reads_as_failure("Traceback (most recent call last):\nKeyboardInterrupt"));
        assert!(!reads_as_failure("  VITE v6.0.0  ready in 300 ms"));
    }

    /// What this machine holds outside Prism: `cargo test outside_here -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn outside_here() {
        let (procs, ports) = (processes().await.unwrap(), listeners().await.unwrap());
        for (pid, ports) in ports.iter().filter(|(pid, _)| !procs.is_system(**pid)) {
            println!("{pid} {ports:?} {}", procs.args(*pid));
        }
    }

    /// A server under a shell is found by its shell's pid: `cargo test finds_a_real_server -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn finds_a_real_server() {
        let mut sh = tokio::process::Command::new("sh").args(["-c", "python3 -m http.server 8765 --bind 127.0.0.1 >/dev/null 2>&1"]).kill_on_drop(true).spawn().unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let (procs, ports) = (processes().await.unwrap(), listeners().await.unwrap());
        let tree = procs.descendants(sh.id().unwrap());
        let found: Vec<u16> = tree.iter().filter_map(|p| ports.get(p)).flatten().copied().collect();
        for pid in &tree {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
        }
        sh.kill().await.unwrap();
        assert_eq!(found, [8765]);
    }

    #[test]
    fn tells_a_server_by_its_words() {
        assert!(looks_like_server("pnpm --dir web dev"));
        assert!(looks_like_server("just api-dev"));
        assert!(looks_like_server("docker compose up db"));
        assert!(!looks_like_server("pytest -x"));
        assert!(!looks_like_server("vim README.md"));
    }
}
