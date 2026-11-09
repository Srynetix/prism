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

mod lock;
mod watch;
pub use lock::*;
pub use watch::*;

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

/// The active profile's `config.toml` (`~/.config/prism/config.toml` for the default one), or
/// `PRISM_CONFIG`.
pub fn config_path() -> PathBuf {
    std::env::var("PRISM_CONFIG").map(PathBuf::from).unwrap_or_else(|_| config_of(active()))
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
pub const DEMO_CONFIG: &str = include_str!("../../demo/config.toml");

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
        // Windows: an ACL of this account and the system only, its parents' left out (Administrators).
        #[cfg(windows)]
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| prism_sys::windows::private_dir(&dir)) {
            eprintln!("prism: {} not made private: {e}", dir.display());
        }
        #[cfg(not(any(unix, windows)))]
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("prism: {} not made: {e}", dir.display());
        }
    }
}

/// The app's data folder (`app_data_dir`), before Tauri starts.
pub fn data_base() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join(IDENTIFIER)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile's local socket: in the data folder, named by its id, a hash when too long for a
    /// Unix socket's path; two profiles never share one.
    #[test]
    fn each_profile_its_socket() {
        let short = engine_socket("perso");
        if cfg!(windows) {
            assert!(short.to_string_lossy().starts_with(r"\\.\pipe\prism-engine-"), "{}", short.display());
        } else {
            assert!(short.starts_with(data_base().join("engines")) && short.ends_with("perso.sock"));
        }
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
