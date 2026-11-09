//! A custom block run once to test it, before it is trusted (an agent's `prism_run_custom_block`):
//! never as the block runs once added, but in a sandbox of the system's — macOS's `sandbox-exec`,
//! Linux's `bwrap` — without the home folder (no `~/.ssh`, `~/.config`, keychain, repositories),
//! without a token (an emptied environment, no `keychain:` value), writing only in a temporary
//! folder thrown away after, none of the session's services (clipboard, preferences, Apple events,
//! Docker's socket), outbound network allowed (a block often asks an API) — on macOS not to
//! `localhost`, though this computer's other addresses (its LAN's, Tailscale's) stay reachable, and
//! on Linux to all of it (cutting it off would cut the network) —, 20 s and 256 MB
//! at most, nothing left running. Without a sandbox on the computer, the test is refused: a block
//! nobody trusted never runs outside one.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use prism_base::i18n::tr;
use serde::Serialize;

use super::{config, output, run, Def};

/// The longest a test runs.
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// The memory it may take, its children with it.
pub const MEMORY: u64 = 256 * 1024 * 1024;

/// What a test gave.
#[derive(Debug, Clone, Serialize)]
pub struct Tested {
    /// `sandbox-exec` or `bwrap`.
    pub sandbox: &'static str,
    /// What the block would show.
    pub output: output::Output,
    /// Why it failed (exit code, timeout, memory, JSON off the contract), or none.
    pub error: Option<String>,
    /// The end of its error output.
    pub stderr: String,
    pub code: Option<i32>,
    pub took_ms: u64,
    /// Its standard output, as printed (cut beyond 64 KB), to see what did not follow the contract.
    pub stdout: String,
}

/// The sandbox this computer has, if any.
pub fn available() -> Option<&'static str> {
    if cfg!(target_os = "macos") && Path::new("/usr/bin/sandbox-exec").is_file() {
        return Some("sandbox-exec");
    }
    if cfg!(target_os = "linux") && prism_core::detect::which("bwrap").is_some() {
        return Some("bwrap");
    }
    None
}

/// Runs `def` once in the sandbox.
pub async fn test(def: &Def) -> Result<Tested> {
    def.check()?;
    let Some(sandbox) = available() else {
        bail!(tr("no sandbox on this computer (sandbox-exec on macOS, bwrap on Linux, none on Windows): a block nobody trusted is not run outside one. It runs once added and trusted in Prism."));
    };
    let dir = temp_dir()?;
    let done = test_in(def, sandbox, &dir).await;
    let _ = std::fs::remove_dir_all(&dir);
    done
}

async fn test_in(def: &Def, sandbox: &'static str, dir: &Path) -> Result<Tested> {
    let (program, mut args) = match &def.script {
        Some(script) => {
            let path = run::write_script(dir, &def.id, script)?;
            let (program, first) = run::script_program(&path, script)?;
            (program, first)
        }
        None => {
            let (head, rest) = def.command.split_first().context("command")?;
            (run::expand(head), rest.to_vec())
        }
    };
    args.extend(def.args.iter().cloned());
    let env = environment(def, dir);
    let (wrapper, wrapped) = match sandbox {
        "sandbox-exec" => (PathBuf::from("/usr/bin/sandbox-exec"), [vec!["-p".to_string(), macos_profile(dir), program.display().to_string()], args].concat()),
        _ => (prism_core::detect::which("bwrap").context("bwrap")?, [bwrap_args(dir, &env), vec!["--".into(), program.display().to_string()], args].concat()),
    };
    // bwrap empties the environment itself (`--clearenv`, then `--setenv`): it is given none of Prism's.
    let spec = run::Spec { program: wrapper, args: wrapped, cwd: dir.to_path_buf(), env: if sandbox == "bwrap" { Vec::new() } else { env }, timeout: def.timeout().min(TIMEOUT), memory: Some(MEMORY) };
    let ran = run::run_clean(&spec).await?;
    let (out, why) = output::read(&ran.stdout);
    let contract = why.map(|why| prism_base::tr!("the output does not follow the contract: {why}", why = why));
    Ok(Tested { sandbox, output: out, error: ran.failure().or(contract), stderr: ran.stderr_tail(), code: ran.code, took_ms: ran.took.as_millis() as u64, stdout: output::cut(&ran.stdout, 64 * 1024) })
}

/// A private folder of its own, outside the home folder and the user's temporary one: the only
/// place it writes.
fn temp_dir() -> Result<PathBuf> {
    let base = if cfg!(target_os = "macos") { PathBuf::from("/private/tmp") } else { std::env::temp_dir() };
    let dir = base.join(format!("prism-sandbox-{}", uuid::Uuid::new_v4().simple()));
    // Made here, never one already there (another account's in a shared folder).
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(), 0o700).create(&dir)?;
    #[cfg(not(unix))]
    std::fs::create_dir(&dir)?;
    Ok(std::fs::canonicalize(&dir)?)
}

/// Its whole environment: the system's `PATH` (none of the home folder's), the language, `HOME`
/// and `TMPDIR` the temporary folder, and the block's own values but a keychain's or a secret's.
fn environment(def: &Def, dir: &Path) -> Vec<(String, String)> {
    let path = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"].join(":");
    let dir = dir.display().to_string();
    let mut env = vec![
        ("PATH".to_string(), path),
        ("HOME".to_string(), dir.clone()),
        ("TMPDIR".to_string(), dir),
        ("LANG".to_string(), std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into())),
        ("PRISM_BLOCK".to_string(), def.id.clone()),
        ("PRISM_SANDBOX".to_string(), "1".to_string()),
    ];
    for (k, v) in &def.env {
        if !v.starts_with("keychain:") && !config::secret_name(k) && !env.iter().any(|(e, _)| e == k) {
            env.push((k.clone(), v.clone()));
        }
    }
    env
}

/// A string in a sandbox profile.
fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// macOS's profile: everything denied but running programs, reading the system (not the homes, not
/// the shared temporary folders), the network by IP but `localhost` (no local socket, no server on
/// loopback: an SSH agent's, a database's; a profile names no other address of this computer, so
/// one listening on all of them is reached through its LAN or Tailscale address), the system services a program needs to run, resolve
/// a name and check a certificate — not the others (the clipboard, other apps' preferences, Apple
/// events, the keychain) — and writing in `dir`. The last rule that matches wins.
fn macos_profile(dir: &Path) -> String {
    let dir = quoted(&dir.display().to_string());
    format!(
        r#"(version 1)
(deny default)
(allow process-fork)
(allow process-exec)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))
(allow sysctl-read)
(allow mach-lookup (global-name "com.apple.system.opendirectoryd.libinfo") (global-name "com.apple.system.opendirectoryd.membership") (global-name "com.apple.system.notification_center") (global-name "com.apple.system.logger") (global-name "com.apple.logd") (global-name "com.apple.diagnosticd") (global-name "com.apple.trustd") (global-name "com.apple.trustd.agent") (global-name "com.apple.ocspd") (global-name "com.apple.SystemConfiguration.configd") (global-name "com.apple.SystemConfiguration.DNSConfiguration") (global-name "com.apple.dnssd.service") (global-name "com.apple.analyticsd"))
(allow ipc-posix-shm-read* (ipc-posix-name "apple.shm.notification_center"))
(allow system-socket)
(allow network-outbound (remote ip))
(deny network-outbound (remote ip "localhost:*"))
(allow network-outbound (literal "/private/var/run/mDNSResponder"))
(allow file-read*)
(deny file-read* (subpath "/Users") (subpath "/private/tmp") (subpath "/private/var/folders") (subpath "/Volumes") (subpath "/Library/Keychains"))
(allow file-read-metadata (literal "/Users") (literal "/private/tmp") (literal "/private/var/folders"))
(allow file-read* file-write* (subpath {dir}))
(allow file-write* file-ioctl (literal "/dev/null") (literal "/dev/zero") (literal "/dev/tty") (regex #"^/dev/fd/"))
"#
    )
}

/// The sockets of the system's daemons hidden from a test on Linux: containers' (root's equal for
/// their group) and the system bus.
const SYSTEM_SOCKETS: [&str; 7] = ["/run/docker.sock", "/var/run/docker.sock", "/run/podman/podman.sock", "/run/containerd/containerd.sock", "/run/crio/crio.sock", "/run/dbus/system_bus_socket", "/var/run/dbus/system_bus_socket"];

/// bwrap's options: the system read-only, the homes, the shared temporary folders and the session's
/// sockets (`/run/user`) hidden behind empty ones, the system daemons' sockets too
/// (`SYSTEM_SOCKETS`), `dir` writable, new namespaces but the network's (a block asks an API: this
/// computer's own servers stay reachable, unlike macOS; no `DISPLAY` nor home, an X server's
/// abstract socket refuses it), an emptied environment, nothing left once Prism goes.
fn bwrap_args(dir: &Path, env: &[(String, String)]) -> Vec<String> {
    let dir = dir.display().to_string();
    let mut args: Vec<String> = ["--ro-bind", "/", "/", "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/home", "--tmpfs", "/root", "--tmpfs", "/tmp", "--tmpfs", "/var/tmp", "--tmpfs", "/run/user"].map(String::from).to_vec();
    // The system's sockets that would give it more than this account has (Docker's: root's equal
    // for the `docker` group), each covered by an empty file where there is one; the rest of /run
    // stays (resolv.conf lives there: resolvconf, NetworkManager, systemd-resolved; NixOS's programs).
    for socket in SYSTEM_SOCKETS {
        args.extend(["--ro-bind-try".into(), "/dev/null".into(), socket.to_string()]);
    }
    if let Some(home) = dirs::home_dir().filter(|h| !h.starts_with("/home") && !h.starts_with("/root") && h != Path::new("/")) {
        args.extend(["--tmpfs".into(), home.display().to_string()]);
    }
    args.extend(["--bind".into(), dir.clone(), dir.clone(), "--chdir".into(), dir]);
    args.extend(["--unshare-user", "--unshare-ipc", "--unshare-pid", "--unshare-uts", "--unshare-cgroup-try", "--die-with-parent", "--new-session", "--clearenv"].map(String::from));
    for (k, v) in env {
        args.extend(["--setenv".into(), k.clone(), v.clone()]);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(script: &str) -> Def {
        let toml = format!("id = \"t\"\nscript = '''\n{script}'''\nenv = {{ PLAIN = \"x\", API_TOKEN = \"s3cr3t\", KEY = \"keychain:svc\" }}\n");
        toml::from_str(&toml).unwrap()
    }

    #[test]
    fn the_environment_holds_no_secret() {
        let env = environment(&def("echo\n"), Path::new("/tmp/x"));
        let get = |k: &str| env.iter().find(|(e, _)| e == k).map(|(_, v)| v.as_str());
        assert_eq!(get("HOME"), Some("/tmp/x"));
        assert_eq!(get("PLAIN"), Some("x"));
        assert_eq!(get("API_TOKEN"), None);
        assert_eq!(get("KEY"), None);
        assert!(!get("PATH").unwrap().contains(&dirs::home_dir().unwrap().display().to_string()));
    }

    #[test]
    fn bwrap_hides_the_home_and_keeps_the_folder() {
        let args = bwrap_args(Path::new("/tmp/prism-sandbox-1"), &[("A".into(), "1".into())]).join(" ");
        assert!(args.contains("--tmpfs /home") && args.contains("--bind /tmp/prism-sandbox-1 /tmp/prism-sandbox-1") && args.contains("--clearenv --setenv A 1"), "{args}");
        assert!(!args.contains("--unshare-net"));
        assert!(args.contains("--ro-bind-try /dev/null /run/docker.sock") && args.contains("--tmpfs /run/user") && !args.contains("--tmpfs /run "), "Docker's socket hidden, the rest of /run kept: {args}");
    }

    /// The real sandbox, where there is one: the home unreadable, the folder writable, the JSON read.
    #[tokio::test]
    async fn a_test_cannot_read_the_home() {
        if available().is_none() {
            return;
        }
        let home = dirs::home_dir().unwrap();
        let script = format!("#!/bin/sh\nls {} >/dev/null 2>&1 && echo '{{\"kind\":\"markdown\",\"text\":\"home read\"}}' && exit 0\necho hi > \"$HOME/f\" && cat \"$HOME/f\" >/dev/null && echo '{{\"kind\":\"gauge\",\"label\":\"ok\",\"value\":1}}'\n", home.display());
        let def = Def { env: Default::default(), ..def(&script) };
        let t = test(&def).await.unwrap();
        assert!(t.error.is_none(), "{t:?}");
        assert!(matches!(t.output.body, output::Body::Stats { .. }), "{t:?}");
    }

    /// macOS: none of the services that would let a test read past its folder — the clipboard,
    /// another app's preferences, Apple events — nor a server of this computer.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_test_reaches_no_service_of_the_session() {
        if available().is_none() {
            return;
        }
        let script = "#!/bin/sh\nleak() { echo \"{\\\"kind\\\":\\\"markdown\\\",\\\"text\\\":\\\"$1\\\"}\"; exit 0; }\npbpaste >/dev/null 2>&1 && leak clipboard\ndefaults read com.apple.finder 2>/dev/null | grep -q = && leak preferences\nosascript -e 'tell application \"System Events\" to get name of every process' >/dev/null 2>&1 && leak events\n/usr/bin/nc -z -w 1 127.0.0.1 22 >/dev/null 2>&1 && leak localhost\nid -un >/dev/null 2>&1 || leak 'no user'\necho '{\"kind\":\"gauge\",\"label\":\"ok\",\"value\":1}'\n";
        let def = Def { env: Default::default(), ..def(script) };
        let t = test(&def).await.unwrap();
        assert!(matches!(t.output.body, output::Body::Stats { .. }), "{t:?}");
    }
}
