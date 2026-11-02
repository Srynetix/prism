use std::{process::Command, time::Duration};

/// Variables taken from the login shell: `PATH` to find `glab`, `git`, `kubectl` and the
/// GKE auth plugin; `SSH_AUTH_SOCK` so that `git fetch` over SSH finds the agent; the locale, for
/// the agents' terminals (see `ensure_utf8_locale`); `CLAUDE_CONFIG_DIR`, so that `claude`, run by
/// Prism, uses the same configuration and credentials as in the terminal. An app launched from the Finder or a desktop
/// launcher only inherits a minimal environment. Windows: programs already get the user's full
/// environment, nothing to do.
const VARS: [&str; 6] = ["PATH", "SSH_AUTH_SOCK", "LANG", "LC_ALL", "LC_CTYPE", "CLAUDE_CONFIG_DIR"];
/// The variables that decide the locale's encoding, in the order the C library reads them.
const LOCALE: [&str; 3] = ["LC_ALL", "LC_CTYPE", "LANG"];
const MARKER: &str = "__PRISM_ENV__";

/// To be called at the very start of `main`, before starting any thread.
pub fn adopt_login_env() {
    if cfg!(windows) {
        return;
    }
    adopt_shell_vars();
    ensure_utf8_locale();
}

/// A UTF-8 locale for what Prism runs, above all its tmux server, whose panes (agents, shells)
/// inherit it: without one (an app opened from the Finder has no `LANG`, and macOS's Terminal
/// sets it itself, not the shell profile), zsh takes "é" for two unknown bytes and `pbcopy` /
/// `pbpaste` garble accents. The locale of macOS's settings in UTF-8, failing that `en_US.UTF-8`
/// (Linux: `C.UTF-8`). Kept as is when the environment already has a UTF-8 one.
fn ensure_utf8_locale() {
    let current = LOCALE.iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()));
    if current.as_deref().is_some_and(is_utf8) {
        return;
    }
    let lang = if cfg!(target_os = "macos") {
        let apple = Command::new("defaults").args(["read", "-g", "AppleLocale"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        utf8_lang(apple.as_deref(), |l| std::path::Path::new("/usr/share/locale").join(l).is_dir())
    } else {
        "C.UTF-8".to_string()
    };
    // Called before any other thread starts (see `main`). A non-UTF-8 LC_ALL / LC_CTYPE would
    // override LANG: they go.
    prism_sys::env::remove_var("LC_ALL");
    prism_sys::env::remove_var("LC_CTYPE");
    prism_sys::env::set_var("LANG", lang);
}

fn is_utf8(locale: &str) -> bool {
    let l = locale.to_ascii_lowercase();
    l.contains("utf-8") || l.contains("utf8")
}

/// `fr_FR` (macOS's `AppleLocale`, `en_US@rg=frzzzz` with a region of its own) as `fr_FR.UTF-8`,
/// when the system has that locale; failing that, `en_US.UTF-8`.
fn utf8_lang(apple_locale: Option<&str>, exists: impl Fn(&str) -> bool) -> String {
    apple_locale
        .map(|l| l.split('@').next().unwrap_or(l).trim())
        .filter(|l| !l.is_empty())
        .map(|l| format!("{l}.UTF-8"))
        .filter(|l| exists(l))
        .unwrap_or_else(|| "en_US.UTF-8".into())
}

/// The login shell's `VARS`, when the app was not started from it.
fn adopt_shell_vars() {
    let default = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
    let shell = std::env::var("SHELL").unwrap_or_else(|_| default.into());
    let script: String = VARS.iter().map(|v| format!("printf '{MARKER}{v}=%s\\n' \"${v}\";")).collect();
    let child = Command::new(&shell)
        .args(["-ilc", &script])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn();
    let Ok(mut child) = child else { return };

    // A blocking shell profile must not prevent the app from starting.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                return;
            }
        }
    }
    let Ok(out) = child.wait_with_output() else { return };

    // The marker isolates the values from any welcome messages printed by the profile.
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((name, value)) = line.split_once(MARKER).and_then(|(_, rest)| rest.split_once('=')) else { continue };
        let value = value.trim();
        if value.is_empty() || !VARS.contains(&name) {
            continue;
        }
        // The shell's PATH replaces the Finder's; the others are only taken if missing.
        if name == "PATH" || std::env::var_os(name).is_none() {
            // Called before any other thread starts (see `main`).
            prism_sys::env::set_var(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_a_utf8_locale() {
        let has = |l: &str| ["fr_FR.UTF-8", "en_US.UTF-8"].contains(&l);
        assert_eq!(utf8_lang(Some("fr_FR"), has), "fr_FR.UTF-8");
        assert_eq!(utf8_lang(Some("en_US@rg=frzzzz"), has), "en_US.UTF-8");
        assert_eq!(utf8_lang(Some("xx_YY"), has), "en_US.UTF-8", "a locale the system lacks");
        assert_eq!(utf8_lang(None, has), "en_US.UTF-8");
        assert!(is_utf8("fr_FR.UTF-8") && is_utf8("C.utf8") && !is_utf8("C") && !is_utf8("fr_FR.ISO8859-1"));
    }
}
