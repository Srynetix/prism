//! What a module's component reaches of the world: the `host` functions, each one a capability
//! its manifest declares, and nothing else. A function it did not declare fails, said in its
//! error (and in Prism's log); WASI gives it clocks and randomness, its standard error going to
//! Prism's log.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use serde_json::Value;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::cli::{IsTerminal, StdoutStream};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::bindings::prism::module::host::Host;
use crate::manifest::Capabilities;

/// The git subcommands a module may run: they read.
const READS: &[&str] = &["log", "show", "rev-parse", "rev-list", "cherry", "diff", "ls-files", "ls-tree", "cat-file", "status", "branch", "tag", "describe", "for-each-ref", "merge-base", "blame", "shortlog"];

/// `args` a git command that reads: a subcommand that reads, first (an option there, `-C` or
/// `-c alias.x=!…`, would leave the clone or run something else), and no `--output` (a file written).
fn reads(args: &[String]) -> bool {
    args.first().is_some_and(|c| READS.contains(&c.as_str())) && !args.iter().any(|a| a.starts_with("--output"))
}

pub struct HostState {
    pub module: String,
    pub caps: Capabilities,
    /// The clones git may run in: the user's clones Prism gives its blocks (their repositories
    /// and every other `[[repos]]` with a clone), and those Prism asked it about (`hints`, `images`).
    pub repos: Arc<Mutex<Vec<PathBuf>>>,
    pub rt: tokio::runtime::Handle,
    pub stderr: Stderr,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl HostState {
    pub fn new(module: String, caps: Capabilities, repos: Arc<Mutex<Vec<PathBuf>>>, rt: tokio::runtime::Handle) -> Self {
        let stderr = Stderr { module: module.clone(), lines: Default::default() };
        // No preopened folder, no environment, no socket: clocks and randomness.
        let wasi = WasiCtx::builder().stderr(stderr.clone()).build();
        Self { module, caps, repos, rt, stderr, wasi, table: ResourceTable::new() }
    }

    fn refused(&self, what: &str) -> String {
        let e = format!("{what}: not among this module's capabilities ([wasm] capabilities in its module.toml)");
        tracing::warn!("module {}: {e}", self.module);
        e
    }

    /// `repo` one of the clones it may read.
    fn its_clone(&self, repo: &Path) -> Option<PathBuf> {
        let real = repo.canonicalize().ok()?;
        self.repos.lock().unwrap().iter().any(|r| r.canonicalize().is_ok_and(|r| r == real)).then_some(real)
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

impl crate::bindings::prism::module::types::Host for HostState {}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

impl Host for HostState {
    fn gitlab_api(&mut self, method: String, path: String, body: Option<String>) -> Result<String, String> {
        if !self.caps.gitlab {
            return Err(self.refused("gitlab"));
        }
        let method = method.to_ascii_uppercase();
        if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE") {
            return Err(format!("gitlab: method {method}"));
        }
        let answer: anyhow::Result<Value> = if method == "GET" && body.is_none() {
            self.rt.block_on(prism_collectors::gitlab::api::<Value>(&path))
        } else {
            let body: Value = body.as_deref().map(serde_json::from_str).transpose().map_err(|e| format!("gitlab: body: {e}"))?.unwrap_or_else(|| Value::Object(Default::default()));
            self.rt.block_on(prism_collectors::gitlab::glab_input::<Value>(&method, &path, &body))
        };
        answer.map(|v| v.to_string()).map_err(err)
    }

    fn gitlab_raw(&mut self, path: String) -> Result<Vec<u8>, String> {
        if !self.caps.gitlab {
            return Err(self.refused("gitlab"));
        }
        self.rt.block_on(prism_core::forge::cli::Cli::Glab.run(&["api", path.as_str()])).map_err(err)
    }

    fn git(&mut self, repo: String, args: Vec<String>) -> Result<String, String> {
        if !self.caps.git {
            return Err(self.refused("git"));
        }
        let Some(clone) = self.its_clone(Path::new(&repo)) else {
            return Err(self.refused(&format!("git in {repo} (not one of your clones)")));
        };
        if !reads(&args) {
            return Err(self.refused(&format!("git {} (a subcommand that reads: {})", args.join(" "), READS.join(", "))));
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.rt.block_on(prism_base::git::run(&clone, &args)).map_err(err)
    }

    fn http_get(&mut self, url: String, headers: Vec<(String, String)>) -> Result<String, String> {
        let host = reqwest::Url::parse(&url).ok().filter(|u| u.scheme() == "https" || u.scheme() == "http").and_then(|u| u.host_str().map(str::to_string));
        let Some(host) = host.filter(|h| self.caps.http.iter().any(|allowed| allowed.eq_ignore_ascii_case(h))) else {
            return Err(self.refused(&format!("http {url}")));
        };
        let mut request = prism_base::http::client().get(&url);
        for (k, v) in headers {
            request = request.header(k, v);
        }
        self.rt
            .block_on(async move {
                let r = request.send().await?.error_for_status()?;
                r.text().await
            })
            .map_err(|e| format!("{host}: {e}"))
    }

    fn exec(&mut self, program: String, args: Vec<String>) -> Result<String, String> {
        if !self.caps.exec.contains(&program) {
            return Err(self.refused(&format!("exec {program}")));
        }
        let out = std::process::Command::new(&program).args(&args).stdin(std::process::Stdio::null()).output().map_err(|e| format!("{program}: {e}"))?;
        if !out.status.success() {
            return Err(format!("{program}: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn allow_log_project(&mut self, project: String) {
        prism_collectors::gitlab::allow_log_project(&project);
    }

    fn log(&mut self, level: u8, message: String) {
        let m = &self.module;
        match level {
            0 => tracing::error!("module {m}: {message}"),
            1 => tracing::warn!("module {m}: {message}"),
            2 => tracing::info!("module {m}: {message}"),
            _ => tracing::debug!("module {m}: {message}"),
        }
    }
}

/// The component's standard error: each line to Prism's log, the last ones kept (a panic's
/// message, said with the block's error).
#[derive(Clone)]
pub struct Stderr {
    module: String,
    lines: Arc<Mutex<(Vec<u8>, VecDeque<String>)>>,
}

impl Stderr {
    /// The lines written since the last call (a panic says why there).
    pub fn take(&self) -> Vec<String> {
        self.lines.lock().unwrap().1.drain(..).collect()
    }

    fn write(&self, bytes: &[u8]) {
        let mut guard = self.lines.lock().unwrap();
        let (partial, lines) = &mut *guard;
        partial.extend_from_slice(bytes);
        while let Some(end) = partial.iter().position(|b| *b == b'\n') {
            let line = String::from_utf8_lossy(&partial[..end]).trim_end().to_string();
            partial.drain(..=end);
            tracing::info!("module {}: {line}", self.module);
            lines.push_back(line);
            if lines.len() > 20 {
                lines.pop_front();
            }
        }
    }
}

impl IsTerminal for Stderr {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for Stderr {
    fn async_stream(&self) -> Box<dyn tokio::io::AsyncWrite + Send + Sync> {
        Box::new(self.clone())
    }
}

impl tokio::io::AsyncWrite for Stderr {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        self.write(buf);
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(caps: Capabilities, repos: Vec<PathBuf>) -> HostState {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let handle = rt.handle().clone();
        std::mem::forget(rt);
        HostState::new("test".into(), caps, Arc::new(Mutex::new(repos)), handle)
    }

    #[test]
    fn refuses_what_it_did_not_declare() {
        let mut none = state(Capabilities::default(), vec![]);
        for e in [
            none.gitlab_api("GET".into(), "projects".into(), None).unwrap_err(),
            none.gitlab_raw("projects".into()).unwrap_err(),
            none.git(".".into(), vec!["status".into()]).unwrap_err(),
            none.http_get("https://example.org/".into(), vec![]).unwrap_err(),
            none.exec("ls".into(), vec![]).unwrap_err(),
        ] {
            assert!(e.contains("not among this module's capabilities"), "{e}");
        }
        // Declared, but elsewhere: git out of its clones, another host, another program.
        let mine = std::env::temp_dir().join(format!("prism-host-{}", std::process::id()));
        std::fs::create_dir_all(&mine).unwrap();
        let mut some = state(Capabilities { git: true, gitlab: false, http: vec!["example.org".into()], exec: vec!["true".into()] }, vec![mine.clone()]);
        assert!(some.git(std::env::temp_dir().display().to_string(), vec!["status".into()]).unwrap_err().contains("not one of your clones"));
        // `cherry` reads (is an MR's commit in a tag?).
        assert!(reads(&["cherry".into(), "v1".into(), "abc^2".into(), "abc^1".into()]));
        assert!(some.git(format!("{}/..", mine.display()), vec!["status".into()]).is_err());
        for escape in [vec!["-C", "/"], vec!["-c", "alias.x=!true", "x"], vec!["push"], vec!["log", "--output=/tmp/x"], vec![]] {
            let e = some.git(mine.display().to_string(), escape.iter().map(|a| a.to_string()).collect()).unwrap_err();
            assert!(e.contains("a subcommand that reads"), "{escape:?}: {e}");
        }
        assert!(some.http_get("https://evil.example.com/".into(), vec![]).unwrap_err().contains("capabilities"));
        assert!(some.http_get("file:///etc/hosts".into(), vec![]).is_err());
        assert!(some.exec("sh".into(), vec!["-c".into(), "true".into()]).unwrap_err().contains("capabilities"));
        #[cfg(unix)]
        assert_eq!(some.exec("true".into(), vec![]), Ok(String::new()));
        std::fs::remove_dir_all(mine).unwrap();
    }

    #[test]
    fn its_standard_error_goes_by_lines() {
        let s = state(Capabilities::default(), vec![]);
        s.stderr.write(b"one\ntw");
        s.stderr.write(b"o\n");
        assert_eq!(s.stderr.take(), ["one", "two"]);
        assert!(s.stderr.take().is_empty());
    }
}
