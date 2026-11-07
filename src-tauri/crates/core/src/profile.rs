//! Profiles: separate setups of Prism on the same machine (a company's, a personal one, a demo).
//! A profile is a folder: its `config.toml` and `packs/`, and, in the app's data folder, its
//! agents, tmux server, notes and layout; its secrets carry its id in the keychain. The default
//! profile is the one Prism always had: `~/.config/prism/config.toml`, the data folder itself,
//! `prism-clickup`. The others live in `~/.config/prism/profiles/<id>/`.
//!
//! One profile is active per process, chosen at launch: `prism --profile <id>`, `PRISM_PROFILE`,
//! otherwise the last one switched to (`~/.config/prism/active-profile`). The first Prism launched
//! with a window holds every window (`host`): another profile opened runs without a window (`prism
//! engine`), shown in a window of that app.

use prism_base::{i18n::tr, tr};
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT: &str = "default";
/// The last profile switched to, read at launch.
const ACTIVE_FILE: &str = "active-profile";

static ACTIVE: OnceLock<String> = OnceLock::new();
/// `prism --profile <id>`, taken from the arguments before anything else.
static FROM_ARGS: OnceLock<String> = OnceLock::new();

/// Takes `--profile <id>` (or `--profile=<id>`) out of the arguments, without remembering it:
/// what a relaunch passes on, its own profile named instead.
pub fn take_arg(args: &mut Vec<std::ffi::OsString>) {
    if let Some(i) = args.iter().position(|a| a == "--profile") {
        args.drain(i..(i + 2).min(args.len()));
    } else if let Some(i) = args.iter().position(|a| a.to_str().is_some_and(|a| a.starts_with("--profile="))) {
        args.remove(i);
    }
}

/// The profile of `prism --profile <id>`, read from the arguments before anything else.
pub fn set_from_args(id: &str) {
    let _ = FROM_ARGS.set(id.to_string());
}

/// A profile of that id exists (`default`, or a folder of `profiles/` with its `config.toml`).
pub fn exists(id: &str) -> bool {
    id == DEFAULT || (!id.is_empty() && !id.contains(['/', '\\']) && id != "." && id != ".." && config_of(id).is_file())
}

/// `~/.config/prism`: the default profile's folder, and the parent of the others.
pub fn root() -> PathBuf {
    crate::config::default_path().parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Folder of a profile: its `config.toml` and `packs/`.
pub fn dir(id: &str) -> PathBuf {
    if id == DEFAULT { root() } else { root().join("profiles").join(id) }
}

pub fn config_of(id: &str) -> PathBuf {
    dir(id).join("config.toml")
}

/// The active profile, decided once at launch (`init`).
pub fn active() -> &'static str {
    ACTIVE.get_or_init(resolve)
}

fn resolve() -> String {
    let wanted = FROM_ARGS
        .get()
        .cloned()
        .or_else(|| std::env::var("PRISM_PROFILE").ok())
        .or_else(|| std::fs::read_to_string(root().join(ACTIVE_FILE)).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT.into());
    if wanted != DEFAULT && !config_of(&wanted).is_file() {
        tracing::warn!("profile {wanted} not found: the default profile");
        return DEFAULT.into();
    }
    wanted
}

/// Hands the active profile down to what Prism starts (the agents' tmux server, their shells, a
/// relaunch): a `PRISM_PROFILE` inherited from another profile's Prism (this one opened from it,
/// `--profile` winning) would otherwise send `prism restart`, run from an agent, to that profile.
pub fn export() {
    // Called before any other thread starts (see `main`).
    prism_sys::env::set_var("PRISM_PROFILE", active());
}

/// The app's data folder for the active profile: the folder itself for the default one.
pub fn data_dir(base: &Path) -> PathBuf {
    data_dir_of(base, active())
}

fn data_dir_of(base: &Path, id: &str) -> PathBuf {
    if id == DEFAULT { base.to_path_buf() } else { base.join("profiles").join(id) }
}

/// Prism's cache folder for the active profile (scan results, Trivy's work folders): the
/// default profile's is `<cache>/prism` itself, as it always was, the others' are inside it. Each
/// profile scans its own repositories: a result must not show in another one.
pub fn cache_dir() -> PathBuf {
    cache_dir_of(active())
}

fn cache_dir_of(id: &str) -> PathBuf {
    data_dir_of(&dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("prism"), id)
}

/// A keychain service for the active profile: `prism-clickup`, `prism-clickup-perso`.
pub fn service(base: &str) -> String {
    service_of(base, active())
}

fn service_of(base: &str, id: &str) -> String {
    if id == DEFAULT { base.to_string() } else { format!("{base}-{id}") }
}

/// The app's identifier (`tauri.conf.json`): its data folder, known before Tauri starts.
pub const IDENTIFIER: &str = "app.prism.desktop";

/// The demo profile's config: sample data, nothing read (`demo`).
pub const DEMO_CONFIG: &str = include_str!("../demo/config.toml");

/// Prism's folders — config, data, caches, logs — the owner's only (0700), created so if missing.
/// What they hold (agents' prompts, tickets, MRs, meetings, notes, a scan's CVEs, logs) is
/// readable by every account of the machine otherwise, where homes are 0755 (Linux often).
/// The files already there are covered by their folder. Not a umask: the agents, tmux and git
/// Prism starts would inherit it, and the repositories' files with them.
pub fn private_dirs() {
    let logs = if cfg!(target_os = "macos") { dirs::home_dir().map(|h| h.join("Library/Logs")) } else { dirs::state_dir().or_else(dirs::data_local_dir) };
    let roots = [Some(root()), Some(data_base()), dirs::cache_dir().map(|c| c.join("prism")), logs.map(|l| l.join(IDENTIFIER))];
    for dir in roots.into_iter().flatten().filter(|d| !d.as_os_str().is_empty()) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            if let Err(e) = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).and_then(|_| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))) {
                eprintln!("prism: {} not made private: {e}", dir.display());
            }
        }
        #[cfg(not(unix))]
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("prism: {} not made: {e}", dir.display());
        }
    }
}

/// The app's data folder (`app_data_dir`), before Tauri starts.
pub fn data_base() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join(IDENTIFIER)
}

/// One Prism per profile: its pid, in the profile's data folder. Two profiles can be open at
/// once, each in its own Prism; the same one twice would share agents, tmux and notes.
fn lock_path(id: &str) -> PathBuf {
    data_dir_of(&data_base(), id).join("instance.pid")
}

/// What a Prism tells the others open on this computer of its agents (`presence.json`, in its data
/// folder): another profile's status bar shows who waits here, without the remote.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    /// Agents running.
    pub agents: u32,
    /// Those waiting for you (a question, a permission, a plan).
    pub waiting: u32,
    /// The title of the one waiting longest.
    #[serde(default)]
    pub first: Option<String>,
    /// Those waiting, longest first: what the app holding the windows notifies.
    #[serde(default)]
    pub waiting_agents: Vec<crate::state::WaitingAgent>,
}

fn presence_path(id: &str) -> PathBuf {
    data_dir_of(&data_base(), id).join("presence.json")
}

/// This Prism's presence, for the others (atomic write).
pub fn write_presence(p: &Presence) -> Result<()> {
    let path = presence_path(active());
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(p)?)?;
    std::fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))
}

/// Profile `id`'s presence, while its Prism runs (a file left by one that quit says nothing).
pub fn presence_of(id: &str) -> Option<Presence> {
    holder(id)?;
    serde_json::from_slice(&std::fs::read(presence_path(id)).ok()?).ok()
}

/// The Prism holding profile `id`, other than this one, if it is still running.
pub fn holder(id: &str) -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(lock_path(id)).ok()?.trim().parse().ok()?;
    (pid != std::process::id() && is_prism(pid)).then_some(pid)
}

/// A running process, and a Prism one (a pid left by a crash may have been reused).
#[cfg(unix)]
fn is_prism(pid: u32) -> bool {
    prism_sys::process::exists(pid)
        && std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("prism"))
}

/// Windows: no lock; the same profile twice is left to the user.
#[cfg(not(unix))]
fn is_prism(_pid: u32) -> bool {
    false
}

/// Takes the active profile's lock; `Some(pid)`: another Prism already has this profile open.
pub fn claim() -> Result<Option<u32>> {
    if let Some(pid) = holder(active()) {
        return Ok(Some(pid));
    }
    let path = lock_path(active());
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(&path, std::process::id().to_string())?;
    Ok(None)
}

/// At exit: the lock goes, if it is still this Prism's.
pub fn release() {
    let path = lock_path(active());
    if std::fs::read_to_string(&path).is_ok_and(|p| p.trim() == std::process::id().to_string()) {
        let _ = std::fs::remove_file(path);
    }
}

/// Brings the Prism holding a profile to the front (it shows its window on `SIGUSR1`).
#[cfg(unix)]
pub fn wake(pid: u32) {
    // A process checked to be Prism, which handles it.
    prism_sys::process::signal(pid, prism_sys::process::Signal::Usr1);
}

#[cfg(not(unix))]
pub fn wake(_pid: u32) {}

/// `prism restart`: the Prism holding the active profile restarts (it does on `SIGUSR2`, as the
/// console's `restart`), and the new one is awaited; none open, Prism starts, detached from the
/// terminal. One still starting has no handler yet and simply quits on the signal: it is started
/// again then.
#[cfg(unix)]
pub fn restart() -> Result<String> {
    let id = active();
    let Some(pid) = holder(id) else {
        start(id)?;
        return Ok(format!("Prism was not running (profile {id}): started"));
    };
    // A Prism without a window restarts in place (the same pid, `prism-server` under systemd):
    // its lock written again says it.
    let written = || std::fs::metadata(lock_path(id)).and_then(|m| m.modified()).ok();
    let before = written();
    // A process checked to be Prism, which handles it.
    prism_sys::process::signal(pid, prism_sys::process::Signal::Usr2);
    let mut gone = 0;
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        match holder(id) {
            Some(new) if new != pid => return Ok(format!("Prism restarted (profile {id}, pid {pid} → {new})")),
            Some(_) if written() != before => return Ok(format!("Prism restarted (profile {id}, pid {pid})")),
            None if !is_prism(pid) => {
                gone += 1;
                // Quit for 3 s with no successor: it had no handler yet.
                if gone == 12 {
                    start(id)?;
                    return Ok(format!("Prism (pid {pid}) quit instead of restarting: started again (profile {id})"));
                }
            }
            _ => {}
        }
    }
    bail!(tr!("Prism (pid {pid}) did not restart within 15 s", pid = pid))
}

/// What the binary runs Prism with, after its path: nothing for the app, `serve` for
/// `prism-server` (`set_start_args`).
static START_ARGS: std::sync::OnceLock<&'static [&'static str]> = std::sync::OnceLock::new();

/// The arguments `start` gives this binary to run Prism (`prism-server`: `serve`), set at launch.
pub fn set_start_args(args: &'static [&'static str]) {
    let _ = START_ARGS.set(args);
}

#[cfg_attr(not(unix), allow(dead_code))]
fn start_args() -> &'static [&'static str] {
    START_ARGS.get().copied().unwrap_or_default()
}

/// The Prism to start again: the running executable, or the AppImage it came from — an
/// AppImage runs from a mount (`/tmp/.mount_…`) gone as soon as it quits.
pub fn executable() -> Result<PathBuf> {
    if let Some(image) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_file()) {
        return Ok(image);
    }
    std::env::current_exe().context("locating the Prism executable")
}

/// Starts Prism on profile `id`, detached from the terminal (its output discarded). On Linux,
/// from a systemd unit (the watch's service, `INVOCATION_ID`), in a scope of its own: it would
/// otherwise live (its tmux and agents with it) in that service's control group.
#[cfg(unix)]
fn start(id: &str) -> Result<()> {
    let exe = executable()?;
    #[cfg(target_os = "linux")]
    let mut cmd = match crate::detect::which("systemd-run").filter(|_| std::env::var_os("INVOCATION_ID").is_some()) {
        Some(run) => {
            let mut cmd = std::process::Command::new(run);
            cmd.args(["--user", "--scope", "--quiet", "--collect", "--"]).arg(exe);
            cmd
        }
        None => std::process::Command::new(exe),
    };
    #[cfg(not(target_os = "linux"))]
    let mut cmd = std::process::Command::new(exe);
    cmd.args(start_args()).env("PRISM_PROFILE", id).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    prism_sys::process::new_session(&mut cmd).spawn().context("starting Prism")?;
    Ok(())
}

#[cfg(not(unix))]
pub fn restart() -> Result<String> {
    bail!("prism restart is not available on this system: use restart in Prism's console")
}

/// Opens profile `id` in another Prism, next to this one.
pub fn launch(id: &str) -> Result<()> {
    ensure!(id == DEFAULT || config_of(id).is_file(), tr!("unknown profile {id}", id = id));
    let exe = executable()?;
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    take_arg(&mut args);
    std::process::Command::new(exe).arg("--profile").arg(id).args(args).env("PRISM_PROFILE", id).spawn().context("opening Prism")?;
    Ok(())
}

/// The app holding the windows (`host.pid`, in the data folder): the first Prism launched with a
/// window, whatever its profile. The other profiles run without a window (`prism engine`), each
/// shown in a window of that app, reached through its local socket (`engine_socket`).
fn host_path() -> PathBuf {
    data_base().join("host.pid")
}

/// The app holding the windows, other than this process, if it is still running.
pub fn host() -> Option<u32> {
    let pid: u32 = std::fs::read_to_string(host_path()).ok()?.trim().parse().ok()?;
    (pid != std::process::id() && is_prism(pid)).then_some(pid)
}

/// This process is the app holding the windows.
pub fn is_host() -> bool {
    std::fs::read_to_string(host_path()).is_ok_and(|p| p.trim() == std::process::id().to_string())
}

/// This process becomes the app holding the windows, unless one runs: `Some(its pid)`. Claimed
/// at once (the file created only if absent): two launches at the same moment never both hold it.
pub fn claim_host() -> Result<Option<u32>> {
    use std::io::Write;
    let path = host_path();
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    for _ in 0..3 {
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(std::process::id().to_string().as_bytes())?;
                return Ok(None);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // Being written: its pid in a moment.
                let pid = std::fs::read_to_string(&path).ok().and_then(|p| p.trim().parse::<u32>().ok());
                match pid {
                    Some(pid) if pid != std::process::id() && is_prism(pid) => return Ok(Some(pid)),
                    Some(_) => {
                        let _ = std::fs::remove_file(&path);
                    }
                    None => std::thread::sleep(std::time::Duration::from_millis(50)),
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    // Left by a Prism gone and taken meanwhile by nobody alive: this one's.
    std::fs::write(&path, std::process::id().to_string())?;
    Ok(None)
}

/// At exit: the app's place goes, if it is still this one's.
pub fn release_host() {
    let path = host_path();
    if std::fs::read_to_string(&path).is_ok_and(|p| p.trim() == std::process::id().to_string()) {
        let _ = std::fs::remove_file(path);
    }
}

/// The profiles another launch asked the app to show (`prism --profile perso`), one per line.
fn requests_path() -> PathBuf {
    data_base().join("host-open")
}

/// Asks the app holding the windows (`pid`) to show profile `id`: its window opened, or brought
/// to the front.
pub fn ask_host(pid: u32, id: &str) -> Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(requests_path())?;
    writeln!(f, "{id}")?;
    drop(f);
    wake(pid);
    Ok(())
}

/// The profiles asked since (`ask_host`), each once, those that exist.
pub fn take_requests() -> Vec<String> {
    // Moved aside first: a request written meanwhile goes to a new file, never lost.
    let path = requests_path();
    let taken = path.with_extension("taken");
    if std::fs::rename(&path, &taken).is_err() {
        return Vec::new();
    }
    let text = std::fs::read_to_string(&taken).unwrap_or_default();
    let _ = std::fs::remove_file(&taken);
    let mut out: Vec<String> = Vec::new();
    for id in text.lines().map(str::trim).filter(|id| exists(id)) {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// The profiles open in the app's windows when it quit (beside `active-profile`, the app's own).
const OPEN_FILE: &str = "open-profiles";

/// Remembers the profiles open in the app's windows, for the next launch.
pub fn remember_open(ids: &[String]) -> Result<()> {
    std::fs::create_dir_all(root())?;
    std::fs::write(root().join(OPEN_FILE), ids.join("\n"))?;
    Ok(())
}

/// The profiles open in the app's windows last time, still there, the app's own aside.
pub fn reopen() -> Vec<String> {
    let text = std::fs::read_to_string(root().join(OPEN_FILE)).unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for id in text.lines().map(str::trim).filter(|id| *id != active() && exists(id)) {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// The local socket of profile `id`'s Prism (`prism_remote::Remote::serve_local`), in the data
/// folder (the owner's only). A Unix socket's path is short (104 bytes on macOS): a long one is
/// named by a hash of the id instead.
pub fn engine_socket(id: &str) -> PathBuf {
    let dir = data_base().join("engines");
    let plain = dir.join(format!("{id}.sock"));
    if plain.as_os_str().len() < 100 {
        return plain;
    }
    let hash = id.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    dir.join(format!("{:08x}.sock", hash as u32))
}

/// Starts profile `id`'s Prism without a window (`prism engine`), for a window of this app:
/// detached, its output in its own log. Its pid.
#[cfg(unix)]
pub fn start_engine(id: &str) -> Result<u32> {
    ensure!(exists(id), tr!("unknown profile {id}", id = id));
    let exe = executable()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("engine").env("PRISM_PROFILE", id).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    let child = prism_sys::process::new_session(&mut cmd).spawn().context("starting the profile's Prism")?;
    Ok(child.id())
}

#[cfg(not(unix))]
pub fn start_engine(_id: &str) -> Result<u32> {
    bail!("profiles in windows of one app: not on this system yet")
}

/// Stops profile `id`'s Prism (`SIGTERM`), if one runs: its agents go on in their tmux.
pub fn stop(id: &str) -> bool {
    #[cfg(unix)]
    if let Some(pid) = holder(id) {
        return prism_sys::process::signal(pid, prism_sys::process::Signal::Term);
    }
    let _ = id;
    false
}

/// Waits (up to `max`) for profile `id` to be free: its Prism gone.
pub fn wait_free(id: &str, max: std::time::Duration) -> bool {
    let until = std::time::Instant::now() + max;
    while holder(id).is_some() {
        if std::time::Instant::now() > until {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    true
}

/// The tmux server of the active profile's agents: `prism`, `prism-perso`.
pub fn tmux_socket() -> String {
    service_of("prism", active())
}

/// Every profile's id and name, read without loading the configs (the menu bar): the default
/// one first, then the others by name.
pub fn names() -> Vec<(String, String)> {
    let name_of = |id: &str| {
        std::fs::read_to_string(config_of(id))
            .ok()
            .and_then(|raw| raw.parse::<toml::Table>().ok())
            .and_then(|t| t.get("profile")?.get("name")?.as_str().map(|n| n.trim().to_string()))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| if id == DEFAULT { "Default".into() } else { id.to_string() })
    };
    let mut others: Vec<(String, String)> = std::fs::read_dir(root().join("profiles"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(String::from))
        .filter(|id| valid_id(id) && config_of(id).is_file())
        .map(|id| { let n = name_of(&id); (id, n) })
        .collect();
    others.sort_by_key(|a| a.1.to_lowercase());
    std::iter::once((DEFAULT.to_string(), name_of(DEFAULT))).chain(others).collect()
}

/// The folders a config works in: its repositories' clones, their `<repo>.worktrees`, and the
/// agents' worktrees folder.
pub fn folders(cfg: &crate::config::Config) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for r in cfg.repositories() {
        if let Some(p) = r.path {
            out.push(crate::agents::worktrees_dir(&p));
            out.push(p);
        }
    }
    out.extend(cfg.agents.worktrees_dir.clone());
    out
}

/// Claude Desktop sessions are the machine's: a profile shows those working in its folders, and
/// those working in no other profile's (a side project, Prism itself).
#[derive(Debug, Clone, Default)]
pub struct SessionScope {
    own: Vec<PathBuf>,
    others: Vec<PathBuf>,
}

impl SessionScope {
    /// The active profile's folders (`cfg`), and the other profiles' (their configs, read now).
    pub fn of(cfg: &crate::config::Config) -> Self {
        let others = std::iter::once(DEFAULT.to_string())
            .chain(std::fs::read_dir(root().join("profiles")).into_iter().flatten().flatten().filter_map(|e| e.file_name().to_str().map(String::from)))
            .filter(|id| id != active() && (id == DEFAULT || valid_id(id)))
            .filter_map(|id| crate::config::load(&config_of(&id)).ok())
            .flat_map(|c| folders(&c))
            .collect();
        Self { own: folders(cfg), others }
    }

    pub fn keeps(&self, folder: &Path) -> bool {
        let under = |roots: &[PathBuf]| roots.iter().any(|r| folder.starts_with(r));
        under(&self.own) || !under(&self.others)
    }
}

/// A profile, as the menu and Settings list it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "ProfileInfo"))]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// A color token of the app (`accent`, `gitlab`…) or a CSS color; empty: the default.
    pub color: String,
    pub config: String,
    pub active: bool,
    /// Packs it builds on, with their version.
    pub packs: Vec<String>,
    /// Agents still running in its tmux server.
    pub agents: u32,
    /// Its Prism runs (this one, or another, with a window or without).
    pub open: bool,
    /// Where it shows on this computer: `window` (in a window of the app), `background` (its Prism
    /// runs, no window shown) or `closed`.
    pub state: String,
    /// Its config could not be read.
    pub error: Option<String>,
}

/// Every profile: the default one first, then the others by name.
pub async fn list() -> Vec<Profile> {
    let mut ids = vec![DEFAULT.to_string()];
    if let Ok(entries) = std::fs::read_dir(root().join("profiles")) {
        let mut others: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join("config.toml").is_file())
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|id| valid_id(id))
            .collect();
        others.sort();
        ids.extend(others);
    }
    let mut out = Vec::new();
    for id in ids {
        out.push(describe(&id).await);
    }
    out.sort_by(|a, b| (a.id != DEFAULT).cmp(&(b.id != DEFAULT)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

async fn describe(id: &str) -> Profile {
    let path = config_of(id);
    let mut p = Profile {
        id: id.to_string(),
        name: if id == DEFAULT { "Default".into() } else { id.to_string() },
        color: String::new(),
        config: path.display().to_string(),
        active: id == active(),
        packs: Vec::new(),
        agents: running_agents(id).await,
        open: holder(id).is_some(),
        state: if id == active() { "window" } else if holder(id).is_some() { "background" } else { "closed" }.to_string(),
        error: None,
    };
    if !path.is_file() {
        return p;
    }
    match crate::config::load(&path) {
        Ok(cfg) => {
            if !cfg.profile.name.trim().is_empty() {
                p.name = cfg.profile.name.trim().to_string();
            }
            p.color = cfg.profile.color.trim().to_string();
        }
        Err(e) => p.error = Some(format!("{e:#}")),
    }
    if let Ok(raw) = std::fs::read_to_string(&path)
        && let Ok((_, _, packs)) = crate::packs::layered(&path, &raw)
    {
        p.packs = packs.into_iter().map(|k| match k.version {
            Some(v) => format!("{} {v}", k.name),
            None => k.name,
        }).collect();
    }
    p
}

/// Sessions of a profile's tmux server: its agents still running, Prism or not.
async fn running_agents(id: &str) -> u32 {
    let out = crate::platform::command("tmux").args(["-L", &service_of("prism", id), "list-sessions", "-F", "#{session_name}"]).output().await;
    out.ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.starts_with(crate::agents::PREFIX)).count() as u32)
        .unwrap_or(0)
}

/// A profile id: a folder name, lowercase letters, digits and `-`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id != DEFAULT && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !id.starts_with('-')
}

/// `Perso`, `Démo client` → `perso`, `demo-client`.
fn id_of(name: &str) -> String {
    crate::agents::slug(name)
}

/// The `[profile]` table of a new or renamed profile.
fn profile_table(name: &str, color: &str) -> String {
    let quote = |s: &str| toml::Value::String(s.to_string()).to_string();
    let mut t = format!("[profile]\nname = {}\n", quote(name.trim()));
    if !color.trim().is_empty() {
        t.push_str(&format!("color = {}\n", quote(color.trim())));
    }
    t
}

/// A new profile: its folder and a `config.toml` naming it; with `pack`, the pack imported into it
/// (a file, `glab:group/project:path@ref` or `gh:owner/repo:path@ref`); with `demo`, the demo profile's config (sample data,
/// nothing read). Otherwise every source is off, with Prism's starter pack: the pages wizard offers
/// its Developer template at the first launch.
pub async fn create(name: &str, color: &str, pack: Option<&str>, demo: bool) -> Result<Profile> {
    ensure!(!name.trim().is_empty(), tr(tr(tr("a profile needs a name"))));
    let id = id_of(name);
    ensure!(valid_id(&id), tr!("“{name}” cannot name a profile", name = name));
    let folder = dir(&id);
    ensure!(!folder.exists(), tr!("a profile {id} already exists ({folder})", id = id, folder = folder.display()));
    std::fs::create_dir_all(&folder).with_context(|| format!("creating {}", folder.display()))?;
    if let Err(e) = write_config(&config_of(&id), name, color, pack, demo).await {
        let _ = std::fs::remove_dir_all(&folder);
        return Err(e);
    }
    Ok(describe(&id).await)
}

/// The active profile's first `config.toml`, at the first launch (it has none): as `create` writes a
/// new profile's. The default profile's folder may hold other files already: only the config is new.
pub async fn setup(name: &str, color: &str, pack: Option<&str>, demo: bool) -> Result<()> {
    ensure!(!name.trim().is_empty(), tr(tr(tr("a profile needs a name"))));
    let path = config_of(active());
    ensure!(!path.exists(), tr!("{path} exists already", path = path.display()));
    let folder = path.parent().context("no config folder")?;
    std::fs::create_dir_all(folder).with_context(|| format!("creating {}", folder.display()))?;
    if let Err(e) = write_config(&path, name, color, pack, demo).await {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(())
}

/// A new `config.toml`: the demo's, or one naming the profile over a pack (Prism's starter pack,
/// every source off, without one).
async fn write_config(path: &Path, name: &str, color: &str, pack: Option<&str>, demo: bool) -> Result<()> {
    if demo {
        // The demo config, under the chosen name and color.
        let mut doc: toml_edit::DocumentMut = DEMO_CONFIG.parse()?;
        let fresh: toml_edit::DocumentMut = profile_table(name, color).parse()?;
        doc.insert("profile", fresh["profile"].clone());
        prism_base::files::write_private(path, doc.to_string())?;
        return Ok(());
    }
    let sources = if pack.is_some() {
        String::new()
    } else {
        "\n# No source yet: turn them on in Settings, or pick the template in the pages wizard.\n[forge]\nenabled = false\n\n[runners]\nenabled = false\n\n[clickup]\nenabled = false\n".into()
    };
    prism_base::files::write_private(path, format!("{}{sources}", profile_table(name, color)))?;
    crate::packs::import(path, pack.unwrap_or(crate::packs::STARTER)).await.context(tr("importing the pack"))?;
    Ok(())
}

/// Renames a profile, or changes its color, in its `config.toml`: its id (its folder) stays.
pub fn rename(id: &str, name: &str, color: &str) -> Result<()> {
    ensure!(!name.trim().is_empty(), tr(tr(tr("a profile needs a name"))));
    let path = config_of(id);
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = raw.parse().with_context(|| format!("{} is not valid TOML", path.display()))?;
    let fresh: toml_edit::DocumentMut = profile_table(name, color).parse()?;
    doc.insert("profile", fresh["profile"].clone());
    if color.trim().is_empty()
        && let Some(t) = doc.get_mut("profile").and_then(|t| t.as_table_mut())
    {
        t.remove("color");
    }
    std::fs::create_dir_all(dir(id))?;
    prism_base::files::write_private(&path, doc.to_string())?;
    Ok(())
}

/// Removes a profile: its folder (config and packs) and its data. Neither the default profile,
/// nor the active one, nor one whose agents still run.
pub async fn remove(id: &str, data_base: &Path) -> Result<()> {
    ensure!(id != DEFAULT, tr(tr(tr("the default profile is not removed"))));
    ensure!(valid_id(id), tr!("unknown profile {id}", id = id));
    ensure!(id != active(), tr(tr(tr("switch to another profile first"))));
    let running = running_agents(id).await;
    ensure!(running == 0, if running > 1 { tr!("{running} agents of {id} still running: stop them first", running = running, id = id) } else { tr!("an agent of {id} still running: stop it first", id = id) });
    let folder = dir(id);
    if !folder.join("config.toml").is_file() {
        bail!(tr!("unknown profile {id}", id = id));
    }
    std::fs::remove_dir_all(&folder).with_context(|| format!("removing {}", folder.display()))?;
    // Its data and its cache: what it scanned is of no use to another.
    for folder in [data_dir_of(data_base, id), cache_dir_of(id)] {
        if folder.exists() {
            let _ = std::fs::remove_dir_all(&folder);
        }
    }
    Ok(())
}

/// Makes `id` the profile of the next launches; the caller relaunches Prism on it (`relaunch`).
pub fn choose(id: &str) -> Result<()> {
    ensure!(id == DEFAULT || config_of(id).is_file(), tr!("unknown profile {id}", id = id));
    std::fs::create_dir_all(root())?;
    std::fs::write(root().join(ACTIVE_FILE), id)?;
    Ok(())
}

/// Starts Prism again on profile `id`: the same executable and arguments, without a former
/// `--profile`, with `PRISM_PROFILE` set for the new process only. The caller then exits.
pub fn relaunch(id: &str) -> Result<()> {
    let exe = executable()?;
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    take_arg(&mut args);
    std::process::Command::new(exe).args(args).env("PRISM_PROFILE", id).spawn().context("relaunching Prism")?;
    Ok(())
}

/// `[remote] keep_running`: the user's LaunchAgent that keeps profile `id`'s Prism running,
/// `app.prism.desktop.watch` (`….watch-perso` for another profile).
#[cfg_attr(not(unix), allow(dead_code))]
fn watch_label(id: &str) -> String {
    format!("{IDENTIFIER}.{}", service_of("watch", id))
}

#[cfg(target_os = "macos")]
fn watch_plist(id: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_default().join("Library/LaunchAgents").join(format!("{}.plist", watch_label(id)))
}

/// The LaunchAgent: `prism watch` on profile `id`, at login and every minute. Its own process
/// group left alone (`AbandonProcessGroup`): the Prism it starts outlives it.
#[cfg(target_os = "macos")]
fn watch_job(id: &str, exe: &Path) -> String {
    let xml = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key><array><string>{exe}</string><string>watch</string></array>
  <key>EnvironmentVariables</key><dict><key>PRISM_PROFILE</key><string>{id}</string></dict>
  <key>RunAtLoad</key><true/>
  <key>StartInterval</key><integer>60</integer>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>ProcessType</key><string>Background</string>
  <key>AbandonProcessGroup</key><true/>
</dict>
</plist>
"#,
        label = xml(&watch_label(id)),
        exe = xml(&exe.to_string_lossy()),
        id = xml(id),
    )
}

/// `[remote] keep_running` for the active profile, as `on` says: its LaunchAgent written and
/// loaded, or unloaded and removed. On, only for an installed Prism (inside its `.app`): a build
/// of the repository would be started in its place. Blocking (`launchctl`).
#[cfg(target_os = "macos")]
pub fn keep_running(on: bool) -> Result<()> {
    let id = active();
    let plist = watch_plist(id);
    let target = format!("gui/{}", prism_sys::process::uid());
    let launchctl = |args: &[&str]| std::process::Command::new("/bin/launchctl").args(args).output().context("launchctl");
    if !on {
        if plist.exists() {
            let _ = launchctl(&["bootout", &format!("{target}/{}", watch_label(id))]);
            std::fs::remove_file(&plist).with_context(|| format!("removing {}", plist.display()))?;
        }
        return Ok(());
    }
    let exe = executable()?;
    ensure!(exe.to_string_lossy().contains(".app/Contents/MacOS/"), tr("Keep Prism running: only for Prism installed as an app, not a build of its repository"));
    let job = watch_job(id, &exe);
    let loaded = launchctl(&["print", &format!("{target}/{}", watch_label(id))])?.status.success();
    if loaded && std::fs::read_to_string(&plist).is_ok_and(|p| p == job) {
        return Ok(());
    }
    std::fs::create_dir_all(plist.parent().unwrap_or(Path::new(".")))?;
    std::fs::write(&plist, &job).with_context(|| format!("writing {}", plist.display()))?;
    if loaded {
        let _ = launchctl(&["bootout", &format!("{target}/{}", watch_label(id))]);
    }
    let out = launchctl(&["bootstrap", &target, &plist.to_string_lossy()])?;
    ensure!(out.status.success(), "launchctl bootstrap {}: {}", plist.display(), String::from_utf8_lossy(&out.stderr).trim());
    Ok(())
}

/// Linux's counterpart of the LaunchAgent: a user systemd timer running `prism watch` every
/// minute, from the user's manager start on; the service a oneshot. The Prism it starts goes to a
/// scope of its own (`start`); `KillMode=process` keeps it alive all the same without systemd-run.
#[cfg(target_os = "linux")]
fn watch_service(id: &str, exe: &Path) -> String {
    // systemd's quoting: a path with spaces in double quotes, `\` and `"` escaped, `%` doubled.
    let exe = exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"").replace('%', "%%");
    format!(
        r#"[Unit]
Description=Prism watch (profile {id})

[Service]
Type=oneshot
ExecStart="{exe}" watch
Environment=PRISM_PROFILE={id}
KillMode=process
"#,
    )
}

/// 15 s after the user's manager starts (the login, or the boot with lingering), then every
/// minute. `watch` waits for the graphical session itself (`has_display`).
#[cfg(target_os = "linux")]
fn watch_timer() -> &'static str {
    r#"[Unit]
Description=Prism watch every minute

[Timer]
OnActiveSec=15
OnUnitActiveSec=60

[Install]
WantedBy=timers.target
"#
}

/// `[remote] keep_running` for the active profile, as `on` says: its units written and the timer
/// enabled, or disabled and removed. On, only for an installed Prism (not a build of the
/// repository, whose place would be written into the units). Blocking (`systemctl`).
#[cfg(target_os = "linux")]
pub fn keep_running(on: bool) -> Result<()> {
    use anyhow::{ensure, Context};
    let id = active();
    let user = dirs::home_dir().unwrap_or_default().join(".config/systemd/user");
    let label = watch_label(id);
    let service = user.join(format!("{label}.service"));
    let timer = user.join(format!("{label}.timer"));
    let unit = format!("{label}.timer");
    let systemctl = |args: &[&str]| std::process::Command::new("systemctl").arg("--user").args(args).output().context("systemctl");
    if !on {
        let had = timer.exists() || service.exists();
        if timer.exists() {
            let _ = systemctl(&["disable", "--now", &unit]);
            let _ = std::fs::remove_file(&timer);
        }
        let _ = std::fs::remove_file(&service);
        if had {
            let _ = systemctl(&["daemon-reload"]);
        }
        return Ok(());
    }
    let exe = executable()?;
    ensure!(!exe.to_string_lossy().contains("/target/"), tr("Keep Prism running: only for Prism installed as an app, not a build of its repository"));
    let body = watch_service(id, &exe);
    let enabled = systemctl(&["is-enabled", &unit]).map(|o| o.status.success()).unwrap_or(false);
    if enabled && std::fs::read_to_string(&service).is_ok_and(|s| s == body) {
        return Ok(());
    }
    std::fs::create_dir_all(&user)?;
    std::fs::write(&service, &body).with_context(|| format!("writing {}", service.display()))?;
    std::fs::write(&timer, watch_timer()).with_context(|| format!("writing {}", timer.display()))?;
    let _ = systemctl(&["daemon-reload"]);
    let out = systemctl(&["enable", "--now", &unit])?;
    ensure!(out.status.success(), "systemctl enable {unit}: {}", String::from_utf8_lossy(&out.stderr).trim());
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn keep_running(_on: bool) -> Result<()> {
    Ok(())
}

/// Whether `prism watch` starts Prism: none running, and either the first watch since the computer
/// started (the login), or a lock left behind (a crash: a Prism that quits removes its lock).
#[cfg_attr(not(unix), allow(dead_code))]
fn watch_starts(running: bool, new_boot: bool, lock_left: bool) -> bool {
    !running && (new_boot || lock_left)
}

/// When the computer started (`kern.boottime`, seconds).
#[cfg(target_os = "macos")]
fn boot_time() -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "kern.boottime"]).output().ok()?;
    // `{ sec = 1761900000, usec = 123456 } Sat Nov  1 …`
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.split("sec = ").nth(1)?.split(',').next()?.trim().parse().ok()
}

/// A display to open Prism's window on: the session's `DISPLAY` or `WAYLAND_DISPLAY`, which the
/// desktop gives the user's systemd manager once it is up.
#[cfg(target_os = "linux")]
fn has_display() -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY"].iter().any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
}

/// When the computer started (`/proc/stat`'s `btime`, seconds).
#[cfg(target_os = "linux")]
fn boot_time() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    stat.lines().find(|l| l.starts_with("btime "))?.split_whitespace().nth(1)?.parse().ok()
}

/// `prism watch`, run by the LaunchAgent of `keep_running`: starts the active profile's Prism
/// when `watch_starts` says so; with `[remote] keep_running` off (the config changed while Prism
/// was closed), removes the LaunchAgent instead. What it did, if anything.
#[cfg(unix)]
pub fn watch() -> Result<Option<String>> {
    let id = active();
    let wanted = std::fs::read_to_string(config_of(id)).ok().and_then(|raw| raw.parse::<toml::Table>().ok()).and_then(|t| {
        let remote = t.get("remote")?;
        let on = |k: &str| remote.get(k).and_then(toml::Value::as_bool).unwrap_or(false);
        Some(on("enabled") && on("keep_running"))
    });
    if wanted != Some(true) {
        keep_running(false)?;
        return Ok(Some(format!("keep_running is off for profile {id}: the watch is removed")));
    }
    // Linux: the graphical session not there yet (the user's manager starts before it, or at boot
    // with lingering): nothing done, nothing noted, the next minute looks again.
    #[cfg(target_os = "linux")]
    if !has_display() {
        return Ok(None);
    }
    let seen = data_dir_of(&data_base(), id).join("watch-boot");
    let boot = boot_time().map(|b| b.to_string()).unwrap_or_default();
    let new_boot = std::fs::read_to_string(&seen).ok().is_none_or(|b| b.trim() != boot);
    if new_boot {
        std::fs::create_dir_all(seen.parent().unwrap_or(Path::new(".")))?;
        std::fs::write(&seen, &boot)?;
    }
    let running = holder(id).is_some();
    let lock_left = !running && lock_path(id).exists();
    if !watch_starts(running, new_boot, lock_left) {
        return Ok(None);
    }
    // A lock left by a crash: the new Prism must find the profile free.
    let _ = std::fs::remove_file(lock_path(id));
    start(id)?;
    Ok(Some(format!("Prism started (profile {id}, {})", if new_boot { "login" } else { "after a crash" })))
}

#[cfg(not(unix))]
pub fn watch() -> Result<Option<String>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile's local socket: in the data folder, named by its id, a hash when too long for a
    /// Unix socket's path; two profiles never share one.
    #[test]
    fn each_profile_its_socket() {
        let short = engine_socket("perso");
        assert!(short.starts_with(data_base().join("engines")) && short.ends_with("perso.sock"));
        let long = "a-profile-whose-name-is-as-long-as-ids-go";
        let path = engine_socket(long);
        assert!(path.as_os_str().len() < 104 || data_base().as_os_str().len() > 80, "{}", path.display());
        assert_ne!(engine_socket("work"), engine_socket("perso"));
        assert_eq!(engine_socket(long), path, "the same one each time");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_watch_units_run_watch_every_minute() {
        let service = watch_service("perso", Path::new("/usr/bin/prism"));
        assert!(service.contains("ExecStart=\"/usr/bin/prism\" watch"));
        assert!(service.contains("Environment=PRISM_PROFILE=perso"));
        assert!(service.contains("KillMode=process"));
        // A path with a space, a quote and a percent, as systemd reads it.
        let odd = watch_service("default", Path::new("/opt/My Apps/pr\"ism%.AppImage"));
        assert!(odd.contains(r#"ExecStart="/opt/My Apps/pr\"ism%%.AppImage" watch"#), "{odd}");
        let timer = watch_timer();
        assert!(timer.contains("OnUnitActiveSec=60") && !timer.contains("Persistent"));
    }

    #[test]
    fn a_binary_says_how_it_runs_prism() {
        set_start_args(&["serve"]);
        assert_eq!(start_args(), ["serve"]);
    }

    #[test]
    fn names_and_ids() {
        assert_eq!(id_of("Démo client"), "demo-client");
        assert!(valid_id("perso") && valid_id("demo-2"));
        assert!(!valid_id("default") && !valid_id("Perso") && !valid_id("../x") && !valid_id(""));
        assert_eq!(service_of("prism-clickup", DEFAULT), "prism-clickup");
        assert_eq!(service_of("prism-clickup", "perso"), "prism-clickup-perso");
        assert_eq!(data_dir_of(Path::new("/d"), "perso"), Path::new("/d/profiles/perso"));
        assert_eq!(data_dir_of(Path::new("/d"), DEFAULT), Path::new("/d"));
        // The cache the same way: the default profile keeps its folder, the others have their own.
        assert!(cache_dir_of("perso").ends_with("prism/profiles/perso") && cache_dir_of(DEFAULT).ends_with("prism"));
    }

    #[test]
    fn sessions_of_a_profile() {
        let scope = SessionScope { own: vec!["/dev/side".into()], others: vec!["/dev/app".into(), "/dev/app.worktrees".into()] };
        assert!(scope.keeps(Path::new("/dev/side/src")), "its own folders");
        assert!(scope.keeps(Path::new("/dev/prism")), "no profile's folder");
        assert!(!scope.keeps(Path::new("/dev/app.worktrees/abc")), "another profile's");
        let shared = SessionScope { own: vec!["/dev/app".into()], others: vec!["/dev/app".into()] };
        assert!(shared.keeps(Path::new("/dev/app")), "in both");
    }

    #[test]
    fn the_watch_starts_prism_at_login_and_after_a_crash() {
        assert!(watch_starts(false, true, false), "the login");
        assert!(watch_starts(false, false, true), "a crash: its lock left behind");
        assert!(!watch_starts(false, false, false), "quit with ⌘Q: closed until the next login");
        assert!(!watch_starts(true, true, true), "already running");
        assert_eq!(watch_label(DEFAULT), "app.prism.desktop.watch");
        assert_eq!(watch_label("perso"), "app.prism.desktop.watch-perso");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_watch_job_parses() {
        let job = watch_job("perso", Path::new("/Applications/Prism & co.app/Contents/MacOS/prism"));
        let out = std::process::Command::new("/usr/bin/plutil").args(["-lint", "-"]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().and_then(|mut c| {
            std::io::Write::write_all(c.stdin.as_mut().unwrap(), job.as_bytes())?;
            c.wait_with_output()
        });
        assert!(out.is_ok_and(|o| o.status.success()), "{job}");
        assert!(job.contains("Prism &amp; co.app") && job.contains("<string>perso</string>"));
    }

    #[test]
    fn profile_table_quotes() {
        let t: toml::Table = profile_table("Perso \"x\"", "perso").parse().unwrap();
        assert_eq!(t["profile"]["name"].as_str(), Some("Perso \"x\""));
        assert_eq!(t["profile"]["color"].as_str(), Some("perso"));
    }
}
