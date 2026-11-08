//! What differs between macOS, Linux and Windows: starting programs, opening folders in
//! applications, the terminal, secrets, the folders of Claude's apps. The rest of the app goes
//! through here rather than calling `open`, `security` or `pbcopy` itself.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use anyhow::Result;
#[cfg(target_os = "macos")]
use crate::{i18n::tr, tr};

/// Default `[open_with] terminal`: the one every system has.
#[cfg(target_os = "macos")]
pub const DEFAULT_TERMINAL: &str = "Terminal";
#[cfg(target_os = "windows")]
pub const DEFAULT_TERMINAL: &str = "wt";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const DEFAULT_TERMINAL: &str = "x-terminal-emulator";

/// Windows: a program started from the app (a GUI process) would open a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// A program run in the background (`git`, `glab`, `kubectl`…): on Windows, without a console
/// window flashing for each call, and found with `PATHEXT` (`claude` installed by npm is
/// `claude.cmd`, which Windows' own search, `.exe` only, misses).
pub fn command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    #[cfg(windows)]
    let program = {
        let p = std::path::Path::new(program.as_ref());
        let bare = p.components().count() == 1 && p.extension().is_none();
        match p.to_str().filter(|_| bare).and_then(which) {
            Some(found) => found.into_os_string(),
            None => program.as_ref().to_os_string(),
        }
    };
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// A program in the PATH. On Windows, with the extensions of `PATHEXT` (`git` is `git.exe`).
pub fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names = names(program, cfg!(windows).then(|| std::env::var("PATHEXT").ok()).flatten().as_deref(), cfg!(windows));
    std::env::split_paths(&path).find_map(|d| names.iter().map(|n| d.join(n)).find(|p| p.is_file()))
}

/// The file names `program` goes by in a folder of the PATH: itself; on Windows, itself with each
/// extension of `PATHEXT` first (by default `.COM;.EXE;.BAT;.CMD`), and itself alone only when it
/// has one already (`git.exe`).
fn names(program: &str, pathext: Option<&str>, windows: bool) -> Vec<String> {
    if !windows {
        return vec![program.to_string()];
    }
    let mut names: Vec<String> = pathext.unwrap_or(".COM;.EXE;.BAT;.CMD").split(';').map(str::trim).filter(|e| e.starts_with('.')).map(|e| format!("{program}{}", e.to_ascii_lowercase())).collect();
    if std::path::Path::new(program).extension().is_some() {
        names.insert(0, program.to_string());
    }
    names
}

/// macOS: runs `program` (found in the PATH) as a process responsible for itself rather than
/// for Prism, and waits for it; returns its exit code and its error output. What it leaves
/// running (the agents' tmux server) is then not Prism's for the system: no "running in the
/// background" once Prism quits, nor Prism's privacy permissions passed down. The attribute is a
/// private one of libSystem (the one terminals use); without it, a plain spawn. Blocking.
#[cfg(target_os = "macos")]
pub fn run_independent(program: &str, args: &[String]) -> Result<(i32, String)> {
    // Its error output, to report a failure: a file rather than a pipe, read once it has exited.
    let err_path = std::env::temp_dir().join(format!("prism-spawn-{}-{}.err", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    let code = prism_sys::process::spawn_disclaimed(program, args, &err_path);
    let err = std::fs::read_to_string(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&err_path);
    match code {
        Ok(code) => Ok((code, err)),
        Err(e) => anyhow::bail!(tr!("{program} not started: {err}", program = program, err = e)),
    }
}

/// Same as `command`, blocking.
pub fn std_command(program: impl AsRef<OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// How to install a missing tool, for error messages.
pub fn install_hint(package: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("brew install {package}")
    } else if cfg!(target_os = "windows") {
        match winget(package) {
            Some(id) => format!("winget install {id}"),
            None => crate::tr!("install {package} and put it in the PATH", package = package),
        }
    } else {
        crate::tr!("install the {package} package of your distribution", package = package)
    }
}

/// Windows: a package's winget id (`winget install Git.Git`), for those winget has.
fn winget(package: &str) -> Option<&'static str> {
    Some(match package {
        "git" => "Git.Git",
        "glab" => "GLab.GLab",
        "gh" => "GitHub.cli",
        "kubectl" => "Kubernetes.kubectl",
        "google-cloud-sdk" => "Google.CloudSDK",
        "trivy" => "AquaSecurity.Trivy",
        "docker" => "Docker.DockerDesktop",
        _ => return None,
    })
}

/// Opens `path` (folder or file) in application `app`, or with the system's default one.
/// macOS: an application name (`Visual Studio Code`); elsewhere, a command (`code`).
pub async fn open_with(path: &Path, app: Option<&str>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut cmd = command("open");
        if let Some(app) = app {
            cmd.arg("-a").arg(app);
        }
        let out = cmd.arg(path).output().await?;
        if !out.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        match app {
            Some(app) => spawn_detached(app, [path.as_os_str()], None),
            None => (opener()?.path)(path),
        }
    }
}

/// Shows `path` selected in the file manager (Finder, Explorer, Files…).
pub fn reveal(path: &Path) -> Result<()> {
    (opener()?.reveal)(path)
}

/// Opens a URL with its application (`ical://`, `claude://`…).
pub fn open_url(url: &str) -> Result<()> {
    (opener()?.url)(url)
}

/// How the system opens a file, a folder or a URL, and shows a file in the file manager: the
/// app's (Tauri's opener), given once at launch (`set_opener`).
pub struct Opener {
    pub path: fn(&Path) -> Result<()>,
    pub reveal: fn(&Path) -> Result<()>,
    pub url: fn(&str) -> Result<()>,
}

static OPENER: std::sync::OnceLock<Opener> = std::sync::OnceLock::new();

pub fn set_opener(opener: Opener) {
    let _ = OPENER.set(opener);
}

fn opener() -> Result<&'static Opener> {
    OPENER.get().ok_or_else(|| anyhow::anyhow!("no opener: the app sets it at launch"))
}

/// Starts a program that lives on without Prism (editor, terminal), without waiting for it.
#[cfg(not(target_os = "macos"))]
fn spawn_detached<I, S>(program: &str, args: I, cwd: Option<&Path>) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    // Windows: `code` and many editors are `.cmd` shims, that only `start` resolves. Its
    // command line is written by hand: cmd does not read the quoting of `arg`.
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        let mut c = std::process::Command::new("cmd");
        c.creation_flags(CREATE_NO_WINDOW);
        c.raw_arg(format!("/C start \"\" \"{program}\""));
        for a in args {
            c.raw_arg(format!("\"{}\"", a.as_ref().to_string_lossy()));
        }
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new(program);
        c.args(args);
        c
    };
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| anyhow::anyhow!("{program}: {e}"))?;
    Ok(())
}

/// Runs `argv` in a new window of terminal `app`. `work`: a folder where a launcher script can be
/// written (macOS opens a `.command` file in Terminal or iTerm).
pub async fn run_in_terminal(app: &str, argv: &[String], work: &Path, name: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let line = argv.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ");
        let script = work.join(format!("{name}.command"));
        std::fs::write(&script, format!("#!/bin/sh\nexec {line}\n"))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
        let out = command("open").arg("-a").arg(app).arg(&script).output().await?;
        if !out.status.success() {
            anyhow::bail!("open -a {app}: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (work, name);
        let mut args: Vec<String> = terminal_prefix(app).iter().map(|s| s.to_string()).collect();
        args.extend(argv.iter().cloned());
        spawn_detached(app, &args, None)
    }
}

/// A new window of terminal `app`, in folder `dir`. macOS: the terminal app opens the folder.
pub async fn open_terminal_in(dir: &Path, app: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    return open_with(dir, Some(app)).await;
    #[cfg(not(target_os = "macos"))]
    {
        // Windows Terminal ignores the folder it is started in: it is given explicitly.
        let is_wt = Path::new(app).file_stem().is_some_and(|s| s.eq_ignore_ascii_case("wt"));
        let args: Vec<&OsStr> = if is_wt { vec![OsStr::new("-d"), dir.as_os_str()] } else { Vec::new() };
        spawn_detached(app, args, Some(dir))
    }
}

/// What comes between a terminal and the command it must run: each terminal has its own.
#[cfg(not(target_os = "macos"))]
fn terminal_prefix(app: &str) -> &'static [&'static str] {
    let name = Path::new(app).file_stem().and_then(OsStr::to_str).unwrap_or(app).to_ascii_lowercase();
    match name.as_str() {
        "gnome-terminal" | "kgx" | "ptyxis" | "tilix" | "mate-terminal" | "xfce4-terminal" => &["--"],
        "wezterm" => &["start", "--"],
        "kitty" | "foot" | "ghostty" => &[],
        "wt" => &["new-tab", "--"],
        "konsole" | "alacritty" | "xterm" | "x-terminal-emulator" | "urxvt" | "terminator" => &["-e"],
        _ if cfg!(windows) => &[],
        _ => &["-e"],
    }
}

/// Settings, `[open_with]`: the installed applications to pick from. macOS: the `.app` of
/// `/Applications`; Linux: the commands of the desktop entries; Windows: none (typed by hand).
pub fn installed_apps() -> Vec<String> {
    #[allow(unused_mut)]
    let mut apps: Vec<String> = Vec::new();
    #[cfg(target_os = "macos")]
    apps.extend(
        ["/Applications", "/System/Applications", "/System/Applications/Utilities"]
            .iter()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flatten()
            .filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".app").map(String::from)),
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut dirs = vec![PathBuf::from("/usr/share/applications"), PathBuf::from("/var/lib/flatpak/exports/share/applications")];
        dirs.extend(dirs::data_dir().map(|d| d.join("applications")));
        apps.extend(
            dirs.iter()
                .filter_map(|d| std::fs::read_dir(d).ok())
                .flatten()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "desktop"))
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .filter_map(|raw| desktop_exec(&raw)),
        );
    }
    apps.sort_by_key(|a| a.to_lowercase());
    apps.dedup();
    apps
}

/// The program of a desktop entry (`Exec=/usr/bin/code --new-window %F` → `code`), unless hidden.
#[cfg(all(unix, not(target_os = "macos")))]
fn desktop_exec(raw: &str) -> Option<String> {
    if raw.lines().any(|l| l.trim() == "NoDisplay=true" || l.trim() == "Hidden=true") {
        return None;
    }
    let exec = raw.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let program = exec.split_whitespace().next()?.trim_matches('"');
    // Flatpak and env wrappers say nothing useful.
    if program == "env" || program.ends_with("flatpak") {
        return None;
    }
    Some(Path::new(program).file_name()?.to_str()?.to_string())
}

/// macOS' keychain tool, by its path: whatever comes first in the login shell's PATH is not
/// handed the tokens.
pub const SECURITY: &str = "/usr/bin/security";

/// A secret of the system keychain (`service`): macOS Keychain, Windows Credential Manager,
/// Secret Service on Linux. `None` when absent.
pub async fn secret(service: &str) -> Result<Option<String>> {
    // macOS: through `security`, which entries created with `security add-generic-password`
    // trust: no access prompt, whatever the account they were created with.
    #[cfg(target_os = "macos")]
    {
        let out = command(SECURITY).args(["find-generic-password", "-s", service, "-w"]).output().await?;
        Ok(out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let service = service.to_string();
        tokio::task::spawn_blocking(move || match keyring::Entry::new(&service, KEYRING_USER)?.get_password() {
            Ok(s) => Ok(Some(s.trim().to_string())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        })
        .await?
    }
}

/// Account under which Prism keeps its secrets in the keyring (Linux, Windows).
#[cfg(not(target_os = "macos"))]
const KEYRING_USER: &str = "prism";

/// Stores a secret in the system keychain (see `secret`).
pub fn store_secret(service: &str, value: &str) -> Result<()> {
    // macOS: through `security`, as `secret` reads it, its command given on standard input (`-i`):
    // never in the process list, and not cut at 128 characters as the `-w` prompt does. Read back
    // to be sure it is whole.
    #[cfg(target_os = "macos")]
    {
        use std::io::Write;
        anyhow::ensure!(!value.is_empty() && value.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\'), "a token is printable ASCII, without quotes nor backslashes");
        let user = std::env::var("USER").unwrap_or_else(|_| "prism".into());
        let mut child = std::process::Command::new(SECURITY).arg("-i").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped()).spawn()?;
        child.stdin.take().expect("piped stdin").write_all(format!("add-generic-password -U -s \"{service}\" -a \"{user}\" -w \"{value}\"\n").as_bytes())?;
        let out = child.wait_with_output()?;
        anyhow::ensure!(out.status.success(), "security: {}", String::from_utf8_lossy(&out.stderr).trim());
        let back = std::process::Command::new(SECURITY).args(["find-generic-password", "-s", service, "-w"]).output()?;
        anyhow::ensure!(String::from_utf8_lossy(&back.stdout).trim() == value, "the keychain did not keep the token whole");
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        keyring::Entry::new(service, KEYRING_USER)?.set_password(value)?;
        Ok(())
    }
}

/// Removes a secret from the system keychain (see `secret`); absent, nothing to do.
pub fn delete_secret(service: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new(SECURITY).args(["delete-generic-password", "-s", service]).output()?;
        // 44: no such item.
        anyhow::ensure!(out.status.success() || out.status.code() == Some(44), "security: {}", String::from_utf8_lossy(&out.stderr).trim());
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        match keyring::Entry::new(service, KEYRING_USER)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

/// Folder of the Claude desktop app's data (sessions, sidebar): `~/Library/Application Support/Claude`,
/// `%APPDATA%\Claude`, `~/.config/Claude`.
pub fn claude_desktop_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_default().join("Claude")
}

/// An agent CLI's temporary folder, where its sessions' scratchpads live: `/tmp/<name>-<uid>`
/// (`/private/tmp` on macOS), `%TEMP%\<name>` on Windows; `claude` for Claude Code, `commandcode`
/// for Command Code.
pub fn cli_tmp_dir(name: &str) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let uid = std::fs::metadata(dirs::home_dir()?).ok()?.uid();
        let tmp = if cfg!(target_os = "macos") { "/private/tmp" } else { "/tmp" };
        Some(PathBuf::from(format!("{tmp}/{name}-{uid}")))
    }
    #[cfg(not(unix))]
    Some(std::env::temp_dir().join(name))
}

/// Size on disk of a folder, in bytes: `du` where it exists, otherwise a walk of the tree.
pub async fn disk_usage(dir: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        let out = command("du").arg("-sk").arg(dir).output().await.ok()?;
        let kb: u64 = String::from_utf8_lossy(&out.stdout).split_whitespace().next()?.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(not(unix))]
    {
        fn walk(dir: &Path) -> u64 {
            let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
            entries
                .flatten()
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&e.path()),
                    Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0,
                })
                .sum()
        }
        let dir = dir.to_path_buf();
        tokio::task::spawn_blocking(move || walk(&dir)).await.ok()
    }
}

/// Erases folders in a process of their own, that outlives the caller (a hook ends right away).
pub fn remove_dirs_detached(paths: &[PathBuf]) {
    #[cfg(unix)]
    let child = std_command("rm").arg("-rf").args(paths).spawn();
    #[cfg(not(unix))]
    let child = {
        let mut cmd = std_command("cmd");
        cmd.arg("/C");
        for (i, p) in paths.iter().enumerate() {
            if i > 0 {
                cmd.arg("&");
            }
            cmd.args(["rmdir", "/S", "/Q"]).arg(p);
        }
        cmd.spawn()
    };
    if let Ok(mut child) = child {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    /// A program in the PATH: as is, or on Windows with `PATHEXT`'s extensions.
    #[test]
    fn a_program_has_its_windows_names() {
        assert_eq!(super::names("git", Some(".COM;.EXE"), false), ["git"]);
        assert_eq!(super::names("git", Some(".COM;.EXE;;.CMD"), true), ["git.com", "git.exe", "git.cmd"]);
        assert_eq!(super::names("claude", None, true), ["claude.com", "claude.exe", "claude.bat", "claude.cmd"]);
        assert_eq!(super::names("git.exe", Some(".EXE"), true), ["git.exe", "git.exe.exe"]);
    }

    use super::*;

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn desktop_entries() {
        assert_eq!(desktop_exec("[Desktop Entry]\nName=Code\nExec=/usr/share/code/code --unity-launch %F\n").as_deref(), Some("code"));
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=foo\nNoDisplay=true\n"), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn terminal_arguments() {
        assert_eq!(terminal_prefix("gnome-terminal"), &["--"]);
        assert_eq!(terminal_prefix("/usr/bin/konsole"), &["-e"]);
        assert_eq!(terminal_prefix("wezterm"), &["start", "--"]);
    }

    #[test]
    fn hints_name_the_package() {
        assert!(install_hint("tmux").contains("tmux"));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod keychain_tests {
    /// Writes the real keychain: `cargo test -- --ignored keychain`.
    #[test]
    #[ignore]
    fn a_long_token_is_kept_whole() {
        let token = format!("ATATT3x{}", "Ab9_-=".repeat(31));
        super::store_secret("prism-test-dummy", &token).unwrap();
        let back = std::process::Command::new(crate::platform::SECURITY).args(["find-generic-password", "-s", "prism-test-dummy", "-w"]).output().unwrap();
        let _ = std::process::Command::new(crate::platform::SECURITY).args(["delete-generic-password", "-s", "prism-test-dummy"]).output();
        assert_eq!(String::from_utf8_lossy(&back.stdout).trim(), token);
    }
}

/// Quotes a value for the shell: in single quotes for a POSIX shell; on Windows in double quotes,
/// which cmd, PowerShell (as an argument) and Git Bash all read.
pub fn quote(s: &str) -> String {
    quote_for(s, cfg!(windows))
}

/// `quote`, for Windows' shells or not.
pub fn quote_for(s: &str, windows: bool) -> String {
    if windows {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Restarts (`restart`) or shuts down (`shutdown`) the computer as its menu would: the
/// applications are asked to quit, and one with unsaved work may still refuse. macOS: an Apple
/// event to loginwindow; Linux: systemd (an active session's user may, by polkit's default);
/// Windows: `shutdown` at once, unforced (`/t 0` without `/f`).
pub async fn power(action: &str) -> anyhow::Result<()> {
    use anyhow::{bail, Context};
    #[cfg(target_os = "macos")]
    {
        let event = match action {
            "restart" => "aevtrest",
            "shutdown" => "aevtshut",
            other => bail!("no power action {other}: restart or shutdown"),
        };
        let script = format!("tell application \"loginwindow\" to «event {event}»");
        let out = command("osascript").args(["-e", &script]).output().await.context("osascript")?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            if err.contains("-1743") {
                bail!(tr("Prism may not send events to loginwindow: allow it in System Settings › Privacy & Security › Automation"));
            }
            bail!("the Mac did not {action}: {}", err.trim());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let flag = match action {
            "restart" => "/r",
            "shutdown" => "/s",
            other => bail!("no power action {other}: restart or shutdown"),
        };
        let out = command("shutdown").args([flag, "/t", "0"]).output().await.context("shutdown")?;
        anyhow::ensure!(out.status.success(), "shutdown {flag}: {}", String::from_utf8_lossy(&out.stderr).trim());
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let verb = match action {
            "restart" => "reboot",
            "shutdown" => "poweroff",
            other => bail!("no power action {other}: restart or shutdown"),
        };
        let out = command("systemctl").arg(verb).output().await.context("systemctl")?;
        anyhow::ensure!(out.status.success(), "systemctl {verb}: {}", String::from_utf8_lossy(&out.stderr).trim());
        Ok(())
    }
}

/// Whether the computer comes back by itself after a restart. macOS with FileVault waits for a
/// password at startup, before any session, Prism or tunnel: out of reach from afar, unless the
/// restart is an authenticated one (`arm_unlock`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Unlock {
    /// Nothing to unlock (no FileVault, or not macOS).
    Free,
    /// FileVault: the next restart skips the unlock once armed with the user's password.
    Password,
    /// FileVault on a Mac without authenticated restarts: it waits at startup.
    Stuck,
}

pub async fn restart_unlock() -> Unlock {
    #[cfg(target_os = "macos")]
    {
        let ask = |verb: &'static str| async move { command("/usr/bin/fdesetup").arg(verb).output().await.ok() };
        // `isactive`: status 0 and "true" with FileVault on.
        if !ask("isactive").await.is_some_and(|o| o.status.success()) {
            return Unlock::Free;
        }
        let supported = ask("supportsauthrestart").await.is_some_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "true");
        if supported { Unlock::Password } else { Unlock::Stuck }
    }
    #[cfg(not(target_os = "macos"))]
    Unlock::Free
}

/// macOS with FileVault: the next restart skips the unlock at startup (`fdesetup authrestart
/// -delayminutes -1`: armed, not restarting by itself; the restart stays `power`'s, the apps asked
/// to quit). `now`: the authenticated restart itself, at once (the apps not asked). `password` is the user's, an administrator's who may unlock the disk: given to
/// `fdesetup` as root through `osascript`'s standard input and a pipe of a private folder, never a
/// command line, a file nor the log. Until that restart, the unlock key stays in memory (and the
/// SMC): FileVault's protections are reduced meanwhile.
#[cfg(target_os = "macos")]
pub async fn arm_unlock(password: &str, now: bool) -> Result<()> {
    use anyhow::{bail, Context};
    use std::os::unix::fs::DirBuilderExt;
    use tokio::io::AsyncWriteExt;
    let user = std::env::var("USER").ok().filter(|u| !u.is_empty()).context("no user name ($USER)")?;
    let dir = std::env::temp_dir().join(format!("prism-unlock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let fifo = dir.join("in");
    let done = async {
        let made = command("/usr/bin/mkfifo").args(["-m", "600"]).arg(&fifo).output().await.context("mkfifo")?;
        anyhow::ensure!(made.status.success(), "mkfifo: {}", String::from_utf8_lossy(&made.stderr).trim());
        let xml = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Username</key><string>{}</string><key>Password</key><string>{}</string></dict></plist>\n",
            xml(&user),
            xml(password)
        );
        // Blocks until fdesetup opens the pipe to read it.
        let writer = {
            let fifo = fifo.clone();
            tokio::task::spawn_blocking(move || std::io::Write::write_all(&mut std::fs::OpenOptions::new().write(true).open(&fifo)?, plist.as_bytes()))
        };
        let applescript = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let delay = if now { "" } else { " -delayminutes -1" };
        let shell = format!("/usr/bin/fdesetup authrestart{delay} -inputplist < {}", quote(&fifo.to_string_lossy()));
        let script = format!("do shell script \"{}\" user name \"{}\" password \"{}\" with administrator privileges", applescript(&shell), applescript(&user), applescript(password));
        let mut child = command("/usr/bin/osascript")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("osascript")?;
        {
            let mut stdin = child.stdin.take().context("osascript's input")?;
            stdin.write_all(script.as_bytes()).await?;
        }
        let out = tokio::time::timeout(std::time::Duration::from_secs(60), child.wait_with_output()).await;
        // fdesetup gone without reading (a wrong password): a reader of Prism's own lets the writer go.
        let _reader = std::fs::OpenOptions::new().read(true).write(true).open(&fifo);
        let _ = writer.await;
        let out = out.map_err(|_| anyhow::anyhow!("fdesetup did not answer within 60 s"))?.context("osascript")?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).replace(password, "•••");
            if err.contains("-60005") || err.contains("-60007") || err.contains("incorrect") {
                bail!(tr("wrong password: it must be this Mac user's, an administrator who may unlock FileVault"));
            }
            bail!("fdesetup authrestart: {}", err.trim());
        }
        Ok(())
    }
    .await;
    let _ = std::fs::remove_dir_all(&dir);
    done
}

#[cfg(not(target_os = "macos"))]
pub async fn arm_unlock(_password: &str, _now: bool) -> Result<()> {
    Ok(())
}
