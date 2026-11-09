//! One Prism per profile: its lock and presence, the process holding it (checked to be Prism),
//! the signals another launch sends it (wake, restart), starting one detached, the host that holds
//! the windows, the profiles to reopen, and a windowless engine's socket.

use super::*;

/// One Prism per profile: its pid, in the profile's data folder. Two profiles can be open at
/// once, each in its own Prism; the same one twice would share agents, tmux and notes.
pub(super) fn lock_path(id: &str) -> PathBuf {
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

pub(super) fn presence_path(id: &str) -> PathBuf {
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
pub(super) fn is_prism(pid: u32) -> bool {
    prism_sys::process::exists(pid)
        && std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "comm="])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("prism"))
}

/// Windows: a running process whose executable is Prism's (`prism.exe`, its sessions' copy).
#[cfg(windows)]
pub(super) fn is_prism(pid: u32) -> bool {
    prism_sys::windows::executable(pid).and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())).is_some_and(|n| n.contains("prism"))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn is_prism(_pid: u32) -> bool {
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

/// Brings the Prism holding a profile to the front (it shows its window on `SIGUSR1`; on
/// Windows, on `WAKE` through its pipe).
#[cfg(unix)]
pub fn wake(pid: u32) {
    // A process checked to be Prism, which handles it.
    prism_sys::process::signal(pid, prism_sys::process::Signal::Usr1);
}

#[cfg(windows)]
pub fn wake(pid: u32) {
    prism_sys::windows::signal(pid, WAKE);
}

#[cfg(not(any(unix, windows)))]
pub fn wake(_pid: u32) {}

/// Windows: what another launch sends a Prism through its pipe (`prism_sys::windows::signal_pipe`),
/// Unix's SIGUSR1: its window to the front.
pub const WAKE: u8 = b'w';
/// Windows: Unix's SIGUSR2, a restart (`prism restart`).
pub const RESTART: u8 = b'r';
/// Windows: Unix's SIGTERM, a clean quit (`stop`: the app taking a profile over, or quitting with
/// the Prisms it started).
pub const STOP: u8 = b's';

/// Asks the Prism `pid` to restart: false if it could not be reached.
pub(super) fn ask_restart(pid: u32) -> bool {
    #[cfg(unix)]
    return prism_sys::process::signal(pid, prism_sys::process::Signal::Usr2);
    #[cfg(windows)]
    return prism_sys::windows::signal(pid, RESTART);
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// `prism restart`: the Prism holding the active profile restarts (it does on `SIGUSR2`, as the
/// console's `restart`), and the new one is awaited; none open, Prism starts, detached from the
/// terminal. One still starting has no handler yet and simply quits on the signal: it is started
/// again then (Windows: it has no pipe yet, and is left to start).
#[cfg(any(unix, windows))]
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
    ask_restart(pid);
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
pub(super) static START_ARGS: std::sync::OnceLock<&'static [&'static str]> = std::sync::OnceLock::new();

/// The arguments `start` gives this binary to run Prism (`prism-server`: `serve`), set at launch.
pub fn set_start_args(args: &'static [&'static str]) {
    let _ = START_ARGS.set(args);
}

#[cfg_attr(not(unix), allow(dead_code))]
pub(super) fn start_args() -> &'static [&'static str] {
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
#[cfg(any(unix, windows))]
pub(super) fn start(id: &str) -> Result<()> {
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
    #[cfg(unix)]
    prism_sys::process::new_session(&mut cmd);
    // Out of the console's process group, without one of its own: the terminal closed, it goes on.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0000_0008 | 0x0000_0200);
    cmd.spawn().context("starting Prism")?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
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
pub(super) fn host_path() -> PathBuf {
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
pub(super) fn requests_path() -> PathBuf {
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
pub(super) const OPEN_FILE: &str = "open-profiles";

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
/// named by a hash of the id instead. On Windows, a named pipe named after that path
/// (`\\.\pipe\prism-engine-…`): each profile, and each data folder, its own.
pub fn engine_socket(id: &str) -> PathBuf {
    let dir = data_base().join("engines");
    let plain = dir.join(format!("{id}.sock"));
    if cfg!(windows) {
        return PathBuf::from(format!(r"\\.\pipe\prism-engine-{:016x}", crate::mcp::fnv(&plain)));
    }
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

/// Windows: out of the app's console and process group (`DETACHED_PROCESS`,
/// `CREATE_NEW_PROCESS_GROUP`), as `start` does, without a console of its own.
#[cfg(windows)]
pub fn start_engine(id: &str) -> Result<u32> {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    ensure!(exists(id), tr!("unknown profile {id}", id = id));
    let exe = executable()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("engine").env("PRISM_PROFILE", id).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    let child = cmd.spawn().context("starting the profile's Prism")?;
    Ok(child.id())
}

#[cfg(not(any(unix, windows)))]
pub fn start_engine(_id: &str) -> Result<u32> {
    bail!("profiles in windows of one app: not on this system yet")
}

/// Stops profile `id`'s Prism (`SIGTERM`; Windows: `STOP` through its pipe), if one runs: its
/// agents go on in their sessions. Whether it was asked; `wait_free` waits for it to be gone.
pub fn stop(id: &str) -> bool {
    let Some(pid) = holder(id) else { return false };
    #[cfg(unix)]
    return prism_sys::process::signal(pid, prism_sys::process::Signal::Term);
    #[cfg(windows)]
    return prism_sys::windows::signal(pid, STOP);
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
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
