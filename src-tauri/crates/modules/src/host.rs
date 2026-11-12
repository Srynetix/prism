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

/// `branch` and `tag` list only with these options (any other creates, moves or deletes a ref).
const LISTS: &[&str] = &["-l", "--list", "-a", "--all", "-r", "--remotes", "--show-current", "-i", "--ignore-case", "--column", "--no-column", "--color", "--no-color", "--omit-empty", "--abbrev", "--no-abbrev", "-n", "--format", "--sort", "--contains", "--no-contains", "--merged", "--no-merged", "--points-at"];
/// Of those, the ones whose value may come as the next argument.
const VALUED: &[&str] = &["--format", "--sort", "--contains", "--no-contains", "--merged", "--no-merged", "--points-at"];

/// `args` a git command that reads: a subcommand that reads, first (an option there, `-C` or
/// `-c alias.x=!…`, would leave the clone or run something else), no `--output` (a file written),
/// no `--no-index` or `--contents` (a file outside the clone read); `branch` and `tag` listing.
fn reads(args: &[String]) -> bool {
    let Some(sub) = args.first().filter(|c| READS.contains(&c.as_str())) else { return false };
    if args.iter().any(|a| a.starts_with("--output") || a == "--no-index" || a.starts_with("--contents")) {
        return false;
    }
    !matches!(sub.as_str(), "branch" | "tag") || lists(&args[1..])
}

/// `branch`/`tag` options listing refs: known listing options (their values with them); a name only
/// as a pattern, after `--list` (without it, a name is a ref created).
fn lists(args: &[String]) -> bool {
    let listing = args.iter().any(|a| a == "-l" || a == "--list");
    let mut rest = args.iter();
    while let Some(a) = rest.next() {
        let name = a.split_once('=').map_or(a.as_str(), |(n, _)| n);
        if VALUED.contains(&name) && !a.contains('=') {
            rest.next();
        } else if a.starts_with('-') {
            // `-n<lines>` (a tag's message lines).
            if !(LISTS.contains(&name) || a.strip_prefix("-n").is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))) {
                return false;
            }
        } else if !listing {
            return false;
        }
    }
    true
}

/// At most this many calls of one module run side by side; the others wait their turn.
const SIDE_BY_SIDE: usize = 8;

/// A call's answer: bytes (text as UTF-8), or its error.
type Answer = Result<Vec<u8>, String>;

/// A module's calls started apart: each one's answer once finished, until taken.
#[derive(Default)]
pub struct Calls {
    inner: Mutex<Running>,
    changed: std::sync::Condvar,
    permits: std::sync::OnceLock<Arc<tokio::sync::Semaphore>>,
}

#[derive(Default)]
struct Running {
    next: u64,
    running: usize,
    answers: std::collections::HashMap<u64, Answer>,
    /// Finished and not yet said (`finished`).
    unsaid: Vec<u64>,
}

impl Calls {
    fn start(&self) -> u64 {
        let mut r = self.inner.lock().unwrap();
        r.next += 1;
        r.running += 1;
        r.next
    }

    fn finish(&self, id: u64, answer: Answer) {
        let mut r = self.inner.lock().unwrap();
        r.running -= 1;
        r.answers.insert(id, answer);
        r.unsaid.push(id);
        self.changed.notify_all();
    }

    fn permits(&self) -> Arc<tokio::sync::Semaphore> {
        self.permits.get_or_init(|| Arc::new(tokio::sync::Semaphore::new(SIDE_BY_SIDE))).clone()
    }

    /// Until one finishes unsaid, or none runs.
    fn wait(&self) {
        let r = self.inner.lock().unwrap();
        drop(self.changed.wait_while(r, |r| r.unsaid.is_empty() && r.running > 0));
    }
}

/// A call's work, its capability checked: what runs once started.
type Work = std::pin::Pin<Box<dyn std::future::Future<Output = Answer> + Send>>;

pub struct HostState {
    pub module: String,
    pub caps: Capabilities,
    /// The clones git may run in: the user's clones Prism gives its blocks (their repositories
    /// and every other `[[repos]]` with a clone), and those Prism asked it about (`hints`, `images`).
    pub repos: Arc<Mutex<Vec<PathBuf>>>,
    pub rt: tokio::runtime::Handle,
    pub stderr: Stderr,
    /// The time the current call spent waiting for Prism (git, GitLab, a program): not its own.
    pub waited: std::time::Duration,
    /// When the current call started.
    pub started: std::time::Instant,
    /// Its mark while it computes (`Module::mark`, and what it holds): lifted while Prism works for
    /// it, so that Prism quitting then is not taken for the module stopping it.
    pub mark: Option<(std::path::PathBuf, String)>,
    /// Its calls started apart (`begin-*`), running or answered.
    pub calls: Arc<Calls>,
    /// The part being ticked: the one ticked again when a call it started finishes.
    pub part: String,
    /// Told the part to tick again once a call finishes (`""`: none, a window's call waits).
    pub woke: Option<Arc<dyn Fn(String) + Send + Sync>>,
    wasi: WasiCtx,
    table: ResourceTable,
}

impl HostState {
    pub fn new(module: String, caps: Capabilities, repos: Arc<Mutex<Vec<PathBuf>>>, rt: tokio::runtime::Handle) -> Self {
        let stderr = Stderr { module: module.clone(), lines: Default::default() };
        // No preopened folder, no environment, no socket: clocks and randomness.
        let wasi = WasiCtx::builder().stderr(stderr.clone()).build();
        Self { module, caps, repos, rt, stderr, waited: Default::default(), started: std::time::Instant::now(), mark: None, calls: Default::default(), part: String::new(), woke: None, wasi, table: ResourceTable::new() }
    }

    /// What Prism does for it, its time counted apart from the call's own (`waited`).
    fn wait_for<T>(&mut self, f: impl std::future::Future<Output = T>) -> T {
        let rt = self.rt.clone();
        self.aside(|| rt.block_on(f))
    }

    /// `f`, Prism working for it: its mark lifted meanwhile, its time added to `waited`.
    fn aside<T>(&mut self, f: impl FnOnce() -> T) -> T {
        let t0 = std::time::Instant::now();
        self.lift(true);
        let out = f();
        self.lift(false);
        self.waited += t0.elapsed();
        out
    }

    /// `work` started apart: its id at once; its answer kept once finished, its part ticked then.
    fn begin(&mut self, work: Result<Work, String>) -> u64 {
        let id = self.calls.start();
        match work {
            Err(e) => self.calls.finish(id, Err(e)),
            Ok(work) => {
                let (calls, part, woke) = (self.calls.clone(), self.part.clone(), self.woke.clone());
                self.rt.spawn(async move {
                    let permits = calls.permits();
                    let _turn = permits.acquire().await;
                    calls.finish(id, work.await);
                    if let Some(woke) = woke {
                        woke(part);
                    }
                });
            }
        }
        id
    }

    /// Its mark lifted while Prism works for it, set again after.
    fn lift(&self, lifted: bool) {
        if let Some((path, text)) = &self.mark {
            let _ = if lifted { std::fs::remove_file(path) } else { std::fs::write(path, text) };
        }
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

/// Text out of an answer.
fn text(answer: Answer) -> Result<String, String> {
    answer.map(|b| String::from_utf8(b).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

/// Each capability's work, checked: refused, said at once.
impl HostState {
    fn gitlab_api_work(&self, method: String, path: String, body: Option<String>) -> Result<Work, String> {
        if !self.caps.gitlab {
            return Err(self.refused("gitlab"));
        }
        let method = method.to_ascii_uppercase();
        if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "DELETE") {
            return Err(format!("gitlab: method {method}"));
        }
        let body: Option<Value> = body.as_deref().map(serde_json::from_str).transpose().map_err(|e| format!("gitlab: body: {e}"))?;
        Ok(Box::pin(async move {
            let answer: anyhow::Result<Value> = match body {
                None if method == "GET" => prism_collectors::gitlab::api::<Value>(&path).await,
                body => prism_collectors::gitlab::glab_input::<Value>(&method, &path, &body.unwrap_or_else(|| Value::Object(Default::default()))).await,
            };
            answer.map(|v| v.to_string().into_bytes()).map_err(err)
        }))
    }

    fn gitlab_raw_work(&self, path: String) -> Result<Work, String> {
        if !self.caps.gitlab {
            return Err(self.refused("gitlab"));
        }
        Ok(Box::pin(async move { prism_core::forge::cli::Cli::Glab.run(&["api", path.as_str()]).await.map_err(err) }))
    }

    fn git_work(&self, repo: String, args: Vec<String>) -> Result<Work, String> {
        if !self.caps.git {
            return Err(self.refused("git"));
        }
        let Some(clone) = self.its_clone(Path::new(&repo)) else {
            return Err(self.refused(&format!("git in {repo} (not one of your clones)")));
        };
        if !reads(&args) {
            return Err(self.refused(&format!("git {} (a subcommand that reads: {})", args.join(" "), READS.join(", "))));
        }
        Ok(Box::pin(async move {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            prism_base::git::run(&clone, &args).await.map(String::into_bytes).map_err(err)
        }))
    }

    fn http_get_work(&self, url: String, headers: Vec<(String, String)>) -> Result<Work, String> {
        let host = reqwest::Url::parse(&url).ok().filter(|u| u.scheme() == "https" || u.scheme() == "http").and_then(|u| u.host_str().map(str::to_string));
        let Some(host) = host.filter(|h| self.caps.http.iter().any(|allowed| allowed.eq_ignore_ascii_case(h))) else {
            return Err(self.refused(&format!("http {url}")));
        };
        let mut request = prism_base::http::client().get(&url);
        for (k, v) in headers {
            request = request.header(k, v);
        }
        Ok(Box::pin(async move {
            let got: reqwest::Result<Vec<u8>> = async {
                let r = request.send().await?.error_for_status()?;
                Ok(r.bytes().await?.to_vec())
            }
            .await;
            got.map_err(|e| format!("{host}: {e}"))
        }))
    }

    fn exec_work(&self, program: String, args: Vec<String>) -> Result<Work, String> {
        if !self.caps.exec.contains(&program) {
            return Err(self.refused(&format!("exec {program}")));
        }
        Ok(Box::pin(async move {
            let out = tokio::process::Command::new(&program).args(&args).stdin(std::process::Stdio::null()).kill_on_drop(true).output().await.map_err(|e| format!("{program}: {e}"))?;
            if !out.status.success() {
                return Err(format!("{program}: {}", String::from_utf8_lossy(&out.stderr).trim()));
            }
            Ok(out.stdout)
        }))
    }
}

impl Host for HostState {
    fn gitlab_api(&mut self, method: String, path: String, body: Option<String>) -> Result<String, String> {
        let work = self.gitlab_api_work(method, path, body)?;
        text(self.wait_for(work))
    }

    fn gitlab_raw(&mut self, path: String) -> Result<Vec<u8>, String> {
        let work = self.gitlab_raw_work(path)?;
        self.wait_for(work)
    }

    fn git(&mut self, repo: String, args: Vec<String>) -> Result<String, String> {
        let work = self.git_work(repo, args)?;
        text(self.wait_for(work))
    }

    fn http_get(&mut self, url: String, headers: Vec<(String, String)>) -> Result<String, String> {
        let work = self.http_get_work(url, headers)?;
        text(self.wait_for(work))
    }

    fn exec(&mut self, program: String, args: Vec<String>) -> Result<String, String> {
        let work = self.exec_work(program, args)?;
        text(self.wait_for(work))
    }

    fn begin_gitlab_api(&mut self, method: String, path: String, body: Option<String>) -> u64 {
        let work = self.gitlab_api_work(method, path, body);
        self.begin(work)
    }

    fn begin_gitlab_raw(&mut self, path: String) -> u64 {
        let work = self.gitlab_raw_work(path);
        self.begin(work)
    }

    fn begin_git(&mut self, repo: String, args: Vec<String>) -> u64 {
        let work = self.git_work(repo, args);
        self.begin(work)
    }

    fn begin_http_get(&mut self, url: String, headers: Vec<(String, String)>) -> u64 {
        let work = self.http_get_work(url, headers);
        self.begin(work)
    }

    fn begin_exec(&mut self, program: String, args: Vec<String>) -> u64 {
        let work = self.exec_work(program, args);
        self.begin(work)
    }

    fn finished(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.calls.inner.lock().unwrap().unsaid)
    }

    fn take(&mut self, id: u64) -> Option<Result<Vec<u8>, String>> {
        self.calls.inner.lock().unwrap().answers.remove(&id)
    }

    fn wait(&mut self) {
        let calls = self.calls.clone();
        self.aside(|| calls.wait());
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
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
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
        // `branch` and `tag` list; creating, moving or deleting a ref, reading a file outside: refused.
        let v = |a: &str| a.split(' ').map(String::from).collect::<Vec<_>>();
        for ok in ["branch", "branch --show-current", "branch -a --contains abc", "branch --list fix/*", "tag --list v1.* --sort=-v:refname", "tag --points-at HEAD", "tag -n3 -l v1", "branch --merged main --format %(refname)"] {
            assert!(reads(&v(ok)), "{ok}");
        }
        for no in ["branch new", "branch -D main", "branch -f main HEAD~1", "branch -m a b", "branch -a new", "branch --set-upstream-to=x", "tag v9", "tag -d v1", "tag -f v1 HEAD", "tag -a v2 -m x", "diff --no-index /etc/hosts /dev/null", "blame --contents /etc/hosts f", "log --output=x"] {
            assert!(!reads(&v(no)), "{no}");
        }
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
