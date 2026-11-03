//! Settings › Remote › Away: what decides whether the computer stays within reach of the phone for
//! days without anyone at it — its sleep, its restarts (macOS updates, FileVault, a power cut),
//! Prism itself. Read when the page opens; nothing is changed.

use prism_base::{i18n::tr, platform::command};
use prism_core::{config::RemoteConfig, state::AwayCheck};

fn check(ok: bool, text: &str) -> AwayCheck {
    AwayCheck { ok, text: text.to_string() }
}

/// A program's output, if it ran. (These read the Mac; elsewhere only the tests use them.)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
async fn output(program: &str, args: &[&str]) -> Option<String> {
    let out = command(program).args(args).output().await.ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `pmset -g`'s value of `key` (` autorestart  1`), where the Mac has it.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pmset_value(settings: &str, key: &str) -> Option<String> {
    settings.lines().find_map(|l| {
        let mut words = l.split_whitespace();
        (words.next() == Some(key)).then(|| words.next().unwrap_or_default().to_string())
    })
}

/// `ioreg`'s `"AppleClamshellCausesSleep" = Yes`: closing the lid sleeps the Mac (no display plugged in).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn lid_sleeps(ioreg: &str) -> Option<bool> {
    let line = ioreg.lines().find(|l| l.contains("\"AppleClamshellCausesSleep\""))?;
    Some(line.trim_end().ends_with("Yes"))
}

pub async fn checks(cfg: &RemoteConfig) -> Vec<AwayCheck> {
    let mut out = Vec::new();
    if !cfg.enabled {
        out.push(check(false, tr("The remote is off: the phone reaches nothing (Remote › Access).")));
    }
    #[cfg(target_os = "macos")]
    {
        let (batt, settings, clamshell, updates, unlock) = tokio::join!(
            output("/usr/bin/pmset", &["-g", "batt"]),
            output("/usr/bin/pmset", &["-g"]),
            output("/usr/sbin/ioreg", &["-r", "-k", "AppleClamshellState", "-d", "1"]),
            output("/usr/bin/defaults", &["read", "/Library/Preferences/com.apple.SoftwareUpdate", "AutomaticallyInstallMacOSUpdates"]),
            prism_base::platform::restart_unlock(),
        );
        // Sleep: `caffeinate -s` holds on power only.
        let on_battery = batt.as_deref().is_some_and(|b| b.contains("'Battery Power'"));
        out.push(match (cfg.keep_awake, on_battery) {
            (false, _) => check(false, tr("Keep the computer awake is off: it may sleep, out of reach until woken.")),
            (true, true) => check(false, tr("On battery: macOS sleeps despite Keep the computer awake. Plug it in before leaving.")),
            (true, false) => check(true, tr("On power, kept awake: the screen may lock, Prism carries on.")),
        });
        if lid_sleeps(clamshell.as_deref().unwrap_or_default()) == Some(true) {
            out.push(check(false, tr("Closing the lid puts this Mac to sleep (no display plugged in): leave it open.")));
        }
        // Restarts: macOS's own, then whether the Mac comes back by itself.
        out.push(if updates.as_deref().map(str::trim) == Some("1") {
            check(false, tr("macOS installs its updates by itself and may restart for them: turn it off in System Settings › General › Software Update › Automatic Updates."))
        } else {
            check(true, tr("macOS does not restart by itself for its updates."))
        });
        use prism_base::platform::Unlock;
        out.push(match unlock {
            Unlock::Free => check(true, tr("No FileVault: after a restart the Mac comes back by itself, up to the login window or the session (automatic login).")),
            Unlock::Password if cfg.power => check(true, tr("FileVault: a restart from the phone takes the Mac's password and comes back by itself. A power cut or a crash still waits for the password at startup.")),
            Unlock::Password => check(false, tr("FileVault: after any restart the Mac waits for its password at startup. With Restart or shut down the computer (Exposure), a restart from the phone skips it.")),
            Unlock::Stuck => check(false, tr("FileVault on a Mac without authenticated restarts: after any restart it waits for its password at startup.")),
        });
        // `autorestart`: desktop Macs only.
        if let Some(v) = settings.as_deref().and_then(|s| pmset_value(s, "autorestart")) {
            out.push(if v == "1" {
                check(true, tr("After a power cut, the Mac starts again by itself."))
            } else {
                check(false, tr("After a power cut, the Mac stays off: sudo pmset autorestart 1 (System Settings › Energy › Start up automatically after a power failure)."))
            });
        }
        out.push(if cfg.keep_running {
            check(true, tr("Prism opens at login and again after a crash."))
        } else {
            check(false, tr("Prism is not opened again after a crash, nor at login unless in Login Items: turn on Keep Prism running."))
        });
    }
    // Sleep: `systemd-inhibit` holds whatever the power.
    #[cfg(target_os = "linux")]
    {
        out.push(if cfg.keep_awake {
            check(true, tr("Kept awake (systemd-inhibit): the screen may lock, Prism carries on."))
        } else {
            check(false, tr("Keep the computer awake is off: it may sleep, out of reach until woken."))
        });
        out.push(if cfg.keep_running {
            check(true, tr("Prism opens at login and again after a crash."))
        } else {
            check(false, tr("Prism is not opened again after a crash, nor at login: turn on Keep Prism running."))
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pmset_and_ioreg() {
        let pmset = "System-wide power settings:\nCurrently in use:\n standby              1\n autorestart          0\n womp                 1\n";
        assert_eq!(pmset_value(pmset, "autorestart").as_deref(), Some("0"));
        assert_eq!(pmset_value(pmset, "powernap"), None);
        assert_eq!(lid_sleeps("  | {\n      \"AppleClamshellCausesSleep\" = Yes\n      \"AppleClamshellState\" = No\n"), Some(true));
        assert_eq!(lid_sleeps("      \"AppleClamshellCausesSleep\" = No\n"), Some(false));
        assert_eq!(lid_sleeps(""), None);
    }
}
