//! Running a custom block's program, or one of its actions: never through a shell (the program
//! and its arguments as given; a `script` run by its shebang, which only the kernel reads), in a
//! process group of its own that its timeout kills whole, its output capped.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use prism_base::{i18n::tr, tr};
use sha2::Digest;
use tokio::io::{AsyncRead, AsyncReadExt};

/// What is read of the standard output: beyond, cut and said.
pub const MAX_STDOUT: usize = 1024 * 1024;
/// What is kept of the error output, its end being what tells.
pub const MAX_STDERR: usize = 64 * 1024;
/// What of stderr is shown with a failure.
const STDERR_TAIL: usize = 2000;

/// What to run.
#[derive(Debug, Clone)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
}

/// How it went.
#[derive(Debug, Clone, Default)]
pub struct Ran {
    /// None: killed (its timeout, or a signal).
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub stdout_cut: bool,
    pub stderr_cut: bool,
    pub took: Duration,
}

impl Ran {
    #[cfg(test)]
    pub fn ok(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }

    /// Why it failed, in a few words (its error output apart: `stderr_tail`, which may hold a
    /// secret); `None` when it did not.
    pub fn failure(&self) -> Option<String> {
        Some(if self.timed_out {
            tr!("no answer after {s} s: stopped, its children with it", s = self.took.as_secs())
        } else if self.stdout_cut {
            tr!("more than {mb} MB printed: cut", mb = MAX_STDOUT / (1024 * 1024))
        } else {
            match self.code {
                Some(0) => return None,
                Some(c) => tr!("exit code {code}", code = c),
                None => tr("killed by a signal").to_string(),
            }
        })
    }

    pub fn stderr_tail(&self) -> String {
        let t = tail(self.stderr.trim_end(), STDERR_TAIL);
        // Its start already dropped while it ran: said, as `tail` says its own cut.
        if self.stderr_cut && !t.starts_with('…') { format!("…{t}") } else { t }
    }
}

/// The end of `s`, `max` bytes at most, at a character's edge.
pub fn tail(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &s[start..])
}

/// `~` and `~/…` as the home folder.
pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~") {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => dirs::home_dir().unwrap_or_default().join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

/// How long its output may still come once the program has exited (a child finishing its line),
/// before what it left running is stopped.
const AFTER_EXIT: Duration = Duration::from_secs(2);

/// Runs `spec`: its timeout kills its whole process group (what it started too). Its exit is what
/// is waited for, not the end of its output: a child left running with the pipes (a daemon, `cmd &`)
/// neither holds the read up nor turns it into a timeout; it is stopped with the group.
pub async fn run(spec: &Spec) -> Result<Ran> {
    let started = Instant::now();
    let mut cmd = prism_base::platform::command(&spec.program);
    cmd.args(&spec.args).current_dir(&spec.cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn().with_context(|| tr!("{program} not started", program = spec.program.display().to_string()))?;
    let pid = child.id();
    let (out, err) = (child.stdout.take().context("stdout")?, child.stderr.take().context("stderr")?);
    let (out_buf, err_buf) = (Captured::default(), Captured::default());
    let readers = [tokio::spawn(capped(out, MAX_STDOUT, false, out_buf.clone())), tokio::spawn(capped(err, MAX_STDERR, true, err_buf.clone()))];
    let status = tokio::time::timeout(spec.timeout, child.wait()).await;
    let timed_out = status.is_err();
    if timed_out {
        kill_group(pid);
        let _ = child.kill().await;
    } else {
        // What it printed last, then what it left behind goes with its group.
        let _ = tokio::time::timeout(AFTER_EXIT, async {
            for r in &readers {
                while !r.is_finished() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        })
        .await;
        kill_group(pid);
    }
    // A pipe still held (a child out of the group): what was read so far.
    tokio::time::sleep(Duration::from_millis(if readers.iter().all(|r| r.is_finished()) { 0 } else { 100 })).await;
    for r in &readers {
        r.abort();
    }
    let ((stdout, stdout_cut), (stderr, stderr_cut)) = (out_buf.take(), err_buf.take());
    let took = started.elapsed();
    match status {
        Ok(status) => Ok(Ran { code: status?.code(), stdout, stderr, timed_out: false, stdout_cut, stderr_cut, took }),
        Err(_) => Ok(Ran { code: None, timed_out: true, stderr, stderr_cut, took, ..Default::default() }),
    }
}

#[cfg(unix)]
fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        prism_sys::process::kill_group(pid);
    }
}

#[cfg(not(unix))]
fn kill_group(_: Option<u32>) {}

/// What a pipe gave so far, and whether some was dropped: read whether its reader ended or not.
#[derive(Clone, Default)]
struct Captured(std::sync::Arc<std::sync::Mutex<(Vec<u8>, bool)>>);

impl Captured {
    fn take(&self) -> (String, bool) {
        let g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (String::from_utf8_lossy(&g.0).into_owned(), g.1)
    }
}

/// At most `max` bytes of `r` (the start, or the end with `keep_end`) into `into`, the rest read
/// and dropped so that the program never blocks on a full pipe.
async fn capped(mut r: impl AsyncRead + Unpin, max: usize, keep_end: bool, into: Captured) {
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut g = into.0.lock().unwrap_or_else(|e| e.into_inner());
        if !keep_end && g.0.len() >= max {
            g.1 = true;
            continue;
        }
        g.0.extend_from_slice(&buf[..n]);
        if g.0.len() > max {
            g.1 = true;
            if keep_end {
                let extra = g.0.len() - max;
                g.0.drain(..extra);
            } else {
                g.0.truncate(max);
            }
        }
    }
}

/// The folder of the scripts written from the config: private (0700).
pub fn scripts_dir(data: &Path) -> Result<PathBuf> {
    let dir = data.join("custom-blocks");
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(dir)
}

fn sha(text: &str) -> String {
    super::config::hex(&sha2::Sha256::digest(text.as_bytes()))
}

/// `script` as a file of `dir` named `<name>-<hash>`, executable by me alone, written atomically;
/// the former files of `name` removed. Its content checked against its hash just before it runs.
pub fn write_script(dir: &Path, name: &str, script: &str) -> Result<PathBuf> {
    let hash = sha(script);
    let path = dir.join(format!("{name}-{}", &hash[..16]));
    if std::fs::read_to_string(&path).ok().is_none_or(|t| t != script) {
        let tmp = dir.join(format!(".{name}-{}.tmp", std::process::id()));
        prism_base::files::write_private(&tmp, script)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700))?;
        }
        std::fs::rename(&tmp, &path)?;
    }
    // The former versions of this script.
    let prefix = format!("{name}-");
    for entry in std::fs::read_dir(dir)?.flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.starts_with(&prefix) && entry.path() != path && file[prefix.len()..].len() == 16 && file[prefix.len()..].chars().all(|c| c.is_ascii_hexdigit()) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    Ok(path)
}

/// The scripts of `dir` no block names any more (`keep`: the names still declared).
pub fn prune_scripts(dir: &Path, keep: &[String]) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        let Some((name, hash)) = file.rsplit_once('-') else { continue };
        if hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()) && !keep.iter().any(|k| k == name) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The program and arguments that run `path`, a script written by `write_script`: itself (its
/// shebang), or `/bin/sh` without one. Its content is checked first: changed since it was written,
/// it does not run.
pub fn script_program(path: &Path, script: &str) -> Result<(PathBuf, Vec<String>)> {
    let on_disk = std::fs::read_to_string(path).with_context(|| path.display().to_string())?;
    if sha(&on_disk) != sha(script) {
        bail!("{}", tr!("{path} changed since Prism wrote it: not run", path = path.display().to_string()));
    }
    Ok(if script.starts_with("#!") { (path.to_path_buf(), Vec::new()) } else { (PathBuf::from("/bin/sh"), vec![path.display().to_string()]) })
}

/// The environment's additions: `keychain:<service>` read from the system keychain.
pub async fn env(env: &std::collections::BTreeMap<String, String>) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for (k, v) in env {
        let value = match v.strip_prefix("keychain:") {
            Some(service) => prism_base::platform::secret(service.trim()).await?.with_context(|| tr!("env {name}: no {service} in the keychain", name = k, service = service.trim()))?,
            None => v.clone(),
        };
        out.push((k.clone(), value));
    }
    Ok(out)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn spec(program: &str, args: &[&str], timeout: u64) -> Spec {
        Spec { program: program.into(), args: args.iter().map(|s| s.to_string()).collect(), cwd: std::env::temp_dir(), env: vec![("PRISM_X".into(), "1".into())], timeout: Duration::from_secs(timeout) }
    }

    #[tokio::test]
    async fn runs_without_a_shell() {
        let ran = run(&spec("/bin/echo", &["$HOME", "; rm -rf /"], 5)).await.unwrap();
        assert!(ran.ok());
        assert_eq!(ran.stdout, "$HOME ; rm -rf /\n");
        let ran = run(&spec("/bin/sh", &["-c", "echo $PRISM_X; echo oops >&2; exit 3"], 5)).await.unwrap();
        assert_eq!((ran.code, ran.stdout.as_str()), (Some(3), "1\n"));
        assert_eq!(ran.failure().as_deref(), Some("exit code 3"));
        assert_eq!(ran.stderr_tail(), "oops");
    }

    /// The timeout kills the children too: the grandchild's `sleep` would write its file otherwise.
    #[tokio::test]
    async fn the_timeout_kills_the_children() {
        let mark = std::env::temp_dir().join(format!("prism-custom-kill-{}", std::process::id()));
        let _ = std::fs::remove_file(&mark);
        let script = format!("(sleep 2; touch {}) & sleep 30", mark.display());
        let ran = run(&spec("/bin/sh", &["-c", &script], 1)).await.unwrap();
        assert!(ran.timed_out && !ran.ok());
        tokio::time::sleep(Duration::from_millis(2500)).await;
        assert!(!mark.exists(), "a child outlived the timeout");
    }

    /// A child left running with the pipes: the program's exit and output are what count.
    #[tokio::test]
    async fn a_child_left_behind_does_not_hold_the_read() {
        let ran = run(&spec("/bin/sh", &["-c", "sleep 30 & echo '{\"kind\":\"gauge\",\"value\":1}'"], 10)).await.unwrap();
        assert!(ran.ok(), "{ran:?}");
        assert!(ran.took < Duration::from_secs(5), "{:?}", ran.took);
        assert_eq!(ran.stdout.trim(), "{\"kind\":\"gauge\",\"value\":1}");
    }

    #[tokio::test]
    async fn the_output_is_capped() {
        let ran = run(&spec("/bin/sh", &["-c", "head -c 2000000 /dev/zero | tr '\\0' a"], 10)).await.unwrap();
        assert!(ran.stdout_cut);
        assert_eq!(ran.stdout.len(), MAX_STDOUT);
        assert!(ran.failure().is_some());
    }

    #[tokio::test]
    async fn scripts_run_by_their_shebang_or_sh() {
        let dir = std::env::temp_dir().join(format!("prism-custom-scripts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let dir = scripts_dir(&dir).unwrap();
        for (script, want) in [("#!/bin/sh\necho shebang\n", "shebang\n"), ("echo plain\n", "plain\n"), ("#!/usr/bin/env bash\necho ${BASH_VERSION:+bash}\n", "bash\n")] {
            let path = write_script(&dir, "t", script).unwrap();
            let (program, args) = script_program(&path, script).unwrap();
            let ran = run(&Spec { program, args: args.clone(), ..spec("", &[], 5) }).await.unwrap();
            assert_eq!(ran.stdout, want, "{script}");
        }
        // One file per name: the former ones gone.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        // Changed on the disk after Prism wrote it: refused.
        let path = write_script(&dir, "t", "echo a\n").unwrap();
        std::fs::write(&path, "echo b\n").unwrap();
        assert!(script_program(&path, "echo a\n").is_err());
        prune_scripts(&dir, &[]);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
