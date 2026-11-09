//! `[remote] keep_running`: the user's LaunchAgent (macOS) or systemd user timer (Linux) that keeps
//! a profile's Prism running, and what it decides at each run (`prism watch`).

use super::*;

/// `[remote] keep_running`: the user's LaunchAgent that keeps profile `id`'s Prism running,
/// `app.prism.desktop.watch` (`….watch-perso` for another profile).
#[cfg_attr(not(unix), allow(dead_code))]
pub(super) fn watch_label(id: &str) -> String {
    format!("{IDENTIFIER}.{}", service_of("watch", id))
}

#[cfg(target_os = "macos")]
pub(super) fn watch_plist(id: &str) -> PathBuf {
    dirs::home_dir().unwrap_or_default().join("Library/LaunchAgents").join(format!("{}.plist", watch_label(id)))
}

/// The LaunchAgent: `prism watch` on profile `id`, at login and every minute. Its own process
/// group left alone (`AbandonProcessGroup`): the Prism it starts outlives it.
#[cfg(target_os = "macos")]
pub(super) fn watch_job(id: &str, exe: &Path) -> String {
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
pub(super) fn watch_service(id: &str, exe: &Path) -> String {
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
pub(super) fn watch_timer() -> &'static str {
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
pub(super) fn watch_starts(running: bool, new_boot: bool, lock_left: bool) -> bool {
    !running && (new_boot || lock_left)
}

/// When the computer started (`kern.boottime`, seconds).
#[cfg(target_os = "macos")]
pub(super) fn boot_time() -> Option<u64> {
    let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "kern.boottime"]).output().ok()?;
    // `{ sec = 1761900000, usec = 123456 } Sat Nov  1 …`
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.split("sec = ").nth(1)?.split(',').next()?.trim().parse().ok()
}

/// A display to open Prism's window on: the session's `DISPLAY` or `WAYLAND_DISPLAY`, which the
/// desktop gives the user's systemd manager once it is up.
#[cfg(target_os = "linux")]
pub(super) fn has_display() -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY"].iter().any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
}

/// When the computer started (`/proc/stat`'s `btime`, seconds).
#[cfg(target_os = "linux")]
pub(super) fn boot_time() -> Option<u64> {
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
