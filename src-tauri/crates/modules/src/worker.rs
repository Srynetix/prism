//! A module's component on a thread of its own: configured, started for each of its blocks, then
//! ticked part by part when due; the window's calls, the stages' hints and the images wait their
//! turn there. A component that traps (a panic, a call past its deadline) is said as its blocks'
//! error and started again after a while, three times at most in five minutes.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use prism_core::alerts::Alert;
use prism_core::blocks::BlockCtx;
use prism_core::changes::ChangeEvent;
use prism_core::images::{ImageTarget, LocalRepo};
use prism_core::stages::{Hints, Stage, Stages};
use serde_json::{json, Value};
use wasmtime::component::{HasSelf, Linker};
use wasmtime::Store;

use crate::bindings;
use crate::cache::{self, w};
use crate::host::HostState;
use crate::manifest::Spec;

/// The longest a call may compute, what Prism does for it (git, GitLab, a program) apart: then it
/// traps (a loop that never ends).
const DEADLINE: Duration = if cfg!(test) { Duration::from_secs(3) } else { Duration::from_secs(120) };
/// The longest a call may last, what Prism does for it included (a loop of host calls).
const LONGEST: Duration = Duration::from_secs(600);
/// The wait before a component that trapped starts again, and how many times in `CRASH_WINDOW`.
const RESTART_AFTER: Duration = if cfg!(test) { Duration::from_secs(1) } else { Duration::from_secs(30) };
const CRASHES: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(300);
/// How long `configure` is waited for (the first compilation of a component included).
const CONFIGURE_WAIT: Duration = Duration::from_secs(90);

/// What the blocks read of their module between two calls.
#[derive(Default)]
pub struct Shared {
    pub stages: Option<Stages>,
    /// In the demo profile: each block's sample state.
    pub demo: Option<HashMap<String, Value>>,
    pub alerts: HashMap<String, Vec<Alert>>,
    pub changes: HashMap<String, Vec<ChangeEvent>>,
    /// The tables it was last configured with.
    config: Option<String>,
    /// A block of it started: a change of its config waits for a restart unless it `reloads`.
    started: bool,
    /// Not trusted yet: the config it is to be configured with, and its blocks to start, once it is.
    waiting_config: Option<String>,
    waiting: Vec<(String, BlockCtx, Value)>,
}

/// A module's version as trusted (`load_trusted`).
#[derive(Clone)]
pub struct Trusted {
    pub capabilities: crate::manifest::Capabilities,
    pub tables: Vec<String>,
    pub bytes: Arc<Vec<u8>>,
}

/// The module in `folder`, if it is the version `fingerprint` says: its manifest and its
/// component's bytes, the folder's fingerprint the same before and after reading them.
pub fn load_trusted(folder: &std::path::Path, fingerprint: &str) -> Result<(Spec, Vec<u8>)> {
    let changed = || anyhow!("its folder is not the version trusted: trust it again");
    anyhow::ensure!(prism_core::modules::fingerprint_of(folder) == fingerprint, changed());
    let spec = crate::manifest::read(folder)?;
    let bytes = std::fs::read(&spec.component)?;
    anyhow::ensure!(prism_core::modules::fingerprint_of(folder) == fingerprint, changed());
    Ok((spec, bytes))
}

pub struct Module {
    pub id: String,
    pub spec: Spec,
    /// Every id of its blocks and parts, for their errors (`BlockCtx::part`).
    pub names: Vec<&'static str>,
    pub shared: Mutex<Shared>,
    /// I trust this version of it (`prism_core::modules::trust`): its component runs only then.
    trusted: std::sync::atomic::AtomicBool,
    /// What the version trusted declares and is, read with its fingerprint checked: what its
    /// component reaches, the tables it is given, its bytes. Never read again from the disk, where
    /// another version may be by then.
    trusted_as: Mutex<Option<Trusted>>,
    /// Left while its component runs (`prism_core::modules::stopped_mark`): Prism dying in a call
    /// leaves it, and the same component is not run at the next launch.
    mark: PathBuf,
    tx: Mutex<Option<mpsc::Sender<Msg>>>,
    me: OnceLock<std::sync::Weak<Module>>,
}

enum Msg {
    Configure { config: String, lang: String, demo: bool, done: mpsc::Sender<Result<()>> },
    Start { block: String, ctx: BlockCtx, env: Value },
    Call { block: String, command: String, args: Value, demo: bool, reply: tokio::sync::oneshot::Sender<Result<Value>> },
    Hints { repo: PathBuf, reply: tokio::sync::oneshot::Sender<Result<Hints>> },
    Images { repo: PathBuf, project: String, local: Vec<LocalRepo>, reply: tokio::sync::oneshot::Sender<Result<Vec<ImageTarget>>> },
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

impl Module {
    /// `trusted`: the version trusted, its bytes read (`load_trusted`); none, it waits.
    pub fn new(id: String, spec: Spec, mark: PathBuf, trusted: Option<Vec<u8>>) -> Self {
        let names = spec.blocks.iter().flat_map(|b| std::iter::once(&b.id).chain(&b.parts)).map(|s| leak(s)).collect();
        let trusted_as = trusted.map(|bytes| Trusted { capabilities: spec.capabilities.clone(), tables: spec.tables.clone(), bytes: Arc::new(bytes) });
        Self { id, names, shared: Default::default(), trusted: trusted_as.is_some().into(), trusted_as: Mutex::new(trusted_as), spec, mark, tx: Mutex::new(None), me: OnceLock::new() }
    }

    /// The version trusted, as it is to run.
    pub fn trusted_as(&self) -> Option<Trusted> {
        self.trusted_as.lock().unwrap().clone()
    }

    /// Trusted now, as `spec` (read with its fingerprint checked) and its bytes: refused when its
    /// blocks are not those Prism registered at launch (they show once Prism restarts).
    pub fn trust_as(&self, spec: Spec, bytes: Vec<u8>) -> bool {
        let shape = |s: &Spec| s.blocks.iter().map(|b| (b.id.clone(), b.parts.clone(), b.provides.clone(), b.repos, b.reloads, b.remote.clone())).collect::<Vec<_>>();
        if shape(&spec) != shape(&self.spec) {
            return false;
        }
        *self.trusted_as.lock().unwrap() = Some(Trusted { capabilities: spec.capabilities, tables: spec.tables, bytes: Arc::new(bytes) });
        self.trust_now();
        true
    }

    pub fn is_trusted(&self) -> bool {
        self.trusted.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn untrusted(&self) -> anyhow::Error {
        anyhow!(prism_base::tr!("the module {id} waits for your trust: Settings › Packs › Modules", id = self.id))
    }

    /// Trusted now: configured and its blocks started, as they were waiting.
    pub fn trust_now(&self) {
        if self.trusted.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let (config, waiting) = {
            let mut shared = self.shared.lock().unwrap();
            (shared.waiting_config.take(), std::mem::take(&mut shared.waiting))
        };
        if let Some(config) = config
            && let Err(e) = self.configure_text(config)
        {
            tracing::error!("module {}: {e:#}", self.id);
        }
        for (block, ctx, env) in waiting {
            if let Err(e) = self.start(&block, ctx, env) {
                tracing::error!("module {}: {e:#}", self.id);
            }
        }
    }

    /// Its thread, compiling its component at once (the first `configure` waits less).
    pub fn warm(self: &Arc<Self>) {
        let _ = self.me.set(Arc::downgrade(self));
        if self.is_trusted() {
            let _ = self.sender();
        }
    }

    /// A change of the config configures it anew: when each of its blocks `reloads`.
    pub fn reloads(&self) -> bool {
        self.spec.blocks.iter().all(|b| b.reloads)
    }

    fn sender(&self) -> Result<mpsc::Sender<Msg>> {
        let mut tx = self.tx.lock().unwrap();
        if let Some(tx) = tx.as_ref() {
            return Ok(tx.clone());
        }
        let me = self.me.get().and_then(std::sync::Weak::upgrade).ok_or_else(|| anyhow!("module {}: not running", self.id))?;
        let (send, recv) = mpsc::channel();
        let rt = prism_base::rt::handle();
        std::thread::Builder::new().name(format!("module {}", self.id)).spawn(move || Worker::new(me, rt).run(recv))?;
        *tx = Some(send.clone());
        Ok(send)
    }

    fn send(&self, msg: Msg) -> Result<()> {
        self.sender()?.send(msg).map_err(|_| anyhow!("module {}: its thread stopped", self.id))
    }

    /// The config tables it is given: the trusted version's (the manifest's until it is trusted).
    pub fn tables(&self) -> Vec<String> {
        self.trusted_as().map_or_else(|| self.spec.tables.clone(), |t| t.tables)
    }

    /// Its tables of the merged config, as TOML: not the whole config.
    pub fn config_text(&self, config: &toml::Table) -> String {
        let tables = self.tables();
        let mine: toml::Table = config.iter().filter(|(k, _)| tables.iter().any(|t| t == *k)).map(|(k, v)| (k.clone(), v.clone())).collect();
        toml::to_string(&mine).unwrap_or_default()
    }

    /// Configured with `config` (its tables): once for its blocks, again when they changed and it
    /// reloads.
    pub fn configure(&self, config: &toml::Table) -> Result<()> {
        let text = self.config_text(config);
        if !self.is_trusted() {
            self.shared.lock().unwrap().waiting_config = Some(text);
            return Ok(());
        }
        self.configure_text(text)
    }

    fn configure_text(&self, text: String) -> Result<()> {
        {
            let shared = self.shared.lock().unwrap();
            if shared.config.as_deref() == Some(text.as_str()) || (shared.started && shared.config.is_some() && !self.reloads()) {
                return Ok(());
            }
        }
        let lang = match prism_base::i18n::lang() {
            prism_base::i18n::Lang::Fr => "fr",
            prism_base::i18n::Lang::En => "en",
        };
        let (done, wait) = mpsc::channel();
        self.send(Msg::Configure { config: text.clone(), lang: lang.into(), demo: false, done })?;
        let answer = wait.recv_timeout(CONFIGURE_WAIT).map_err(|_| anyhow!("module {}: configure did not answer", self.id))?;
        // Kept even when it failed: the same config is not tried again until it changes.
        self.shared.lock().unwrap().config = Some(text);
        answer
    }

    pub fn start(&self, block: &str, ctx: BlockCtx, env: Value) -> Result<()> {
        if !self.is_trusted() {
            // Said on its block; started once trusted.
            ctx.set_error(Some(format!("{:#}", self.untrusted())));
            let mut shared = self.shared.lock().unwrap();
            shared.waiting.retain(|(b, _, _)| b != block);
            shared.waiting.push((block.to_string(), ctx, env));
            return Ok(());
        }
        self.shared.lock().unwrap().started = true;
        self.send(Msg::Start { block: block.into(), ctx, env })
    }

    /// Its blocks' sample states, the component configured for the demo once.
    pub fn demo(&self, block: &str) -> Option<Value> {
        if !self.is_trusted() {
            return None;
        }
        if self.shared.lock().unwrap().demo.is_none() {
            let config = self.shared.lock().unwrap().config.clone().unwrap_or_default();
            let (done, wait) = mpsc::channel();
            self.send(Msg::Configure { config, lang: "en".into(), demo: true, done }).ok()?;
            if let Ok(Err(e)) | Err(e) = wait.recv_timeout(CONFIGURE_WAIT).map_err(|_| anyhow!("no answer")) {
                tracing::error!("module {}: demo: {e:#}", self.id);
            }
        }
        self.shared.lock().unwrap().demo.as_ref()?.get(block).cloned()
    }

    pub async fn call(&self, block: &str, command: String, args: Value, demo: bool) -> Result<Value> {
        if !self.is_trusted() {
            return Err(self.untrusted());
        }
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.send(Msg::Call { block: block.into(), command, args, demo, reply })?;
        answer.await.map_err(|_| anyhow!("module {}: no answer", self.id))?
    }

    pub async fn hints(&self, repo: PathBuf) -> Result<Hints> {
        if !self.is_trusted() {
            return Err(self.untrusted());
        }
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.send(Msg::Hints { repo, reply })?;
        answer.await.map_err(|_| anyhow!("module {}: no answer", self.id))?
    }

    pub async fn images(&self, repo: PathBuf, project: String, local: Vec<LocalRepo>) -> Result<Vec<ImageTarget>> {
        if !self.is_trusted() {
            return Err(self.untrusted());
        }
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.send(Msg::Images { repo, project, local, reply })?;
        answer.await.map_err(|_| anyhow!("module {}: no answer", self.id))?
    }

    /// A source of its manifest (a block's id, a part), as the core keeps it.
    fn source(&self, name: &str) -> Option<&'static str> {
        self.names.iter().copied().find(|n| *n == name)
    }

    /// The block a part's error goes under: the one declaring it, or the first.
    fn owner(&self, part: &str) -> &str {
        let spec = &self.spec.blocks;
        spec.iter().find(|b| b.id == part || b.parts.iter().any(|p| p == part)).unwrap_or(&spec[0]).id.as_str()
    }
}

struct Instance {
    store: Store<HostState>,
    block: bindings::Block,
}

/// How a module was configured last: to configure it again after a trap.
#[derive(Clone)]
struct Configured {
    config: String,
    lang: String,
    demo: bool,
}

struct Worker {
    module: Arc<Module>,
    rt: tokio::runtime::Handle,
    repos: Arc<Mutex<Vec<PathBuf>>>,
    instance: Option<Instance>,
    /// Compiled once (or read from the cache), instantiated anew after a trap or a new config.
    component: Option<wasmtime::component::Component>,
    configured: Option<Configured>,
    parts: Vec<String>,
    /// Its started blocks: their context, and the environment they were started with.
    started: BTreeMap<String, (BlockCtx, Value)>,
    due: HashMap<String, Instant>,
    crashes: Vec<Instant>,
    /// Trapped: started again then (none: given up, or running).
    restart_at: Option<Instant>,
    given_up: Option<String>,
    /// The undeclared sources already said in the log (once each).
    unknown: Vec<String>,
}

impl Worker {
    fn new(module: Arc<Module>, rt: tokio::runtime::Handle) -> Self {
        Self { module, rt, repos: Default::default(), instance: None, component: None, configured: None, parts: Vec::new(), started: BTreeMap::new(), due: HashMap::new(), crashes: Vec::new(), restart_at: None, given_up: None, unknown: Vec::new() }
    }

    fn id(&self) -> &str {
        &self.module.id
    }

    fn run(mut self, recv: mpsc::Receiver<Msg>) {
        // Compiled (or read from the cache) before anything asks.
        match self.module.trusted_as().ok_or_else(|| anyhow!("not trusted")).and_then(|t| cache::component_bytes(&self.module.spec.component, &t.bytes)) {
            Ok(c) => self.component = Some(c),
            Err(e) => tracing::error!("module {}: {e:#}", self.id()),
        }
        loop {
            let next = self.next();
            let msg = match next {
                None => recv.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
                Some(at) => recv.recv_timeout(at.saturating_duration_since(Instant::now())),
            };
            match msg {
                Ok(msg) => self.handle(msg),
                Err(mpsc::RecvTimeoutError::Timeout) => self.due_now(),
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// When something is due: a restart, or the next part of a started block.
    fn next(&self) -> Option<Instant> {
        if self.given_up.is_some() {
            return None;
        }
        if let Some(at) = self.restart_at {
            return Some(at);
        }
        self.due.iter().filter(|(part, _)| self.started.contains_key(self.module.owner(part))).map(|(_, at)| *at).min()
    }

    fn due_now(&mut self) {
        if self.restart_at.is_some_and(|at| at <= Instant::now()) {
            self.restart_at = None;
            self.restart();
            return;
        }
        let now = Instant::now();
        let due: Option<String> = self.due.iter().filter(|(part, at)| **at <= now && self.started.contains_key(self.module.owner(part))).min_by_key(|(_, at)| **at).map(|(p, _)| p.clone());
        if let Some(part) = due {
            self.tick(&part);
        }
    }

    fn instantiate(&mut self) -> Result<Instance> {
        let component = match &self.component {
            Some(c) => c.clone(),
            None => {
                let t = self.module.trusted_as().ok_or_else(|| self.module.untrusted())?;
                self.component.insert(cache::component_bytes(&self.module.spec.component, &t.bytes)?).clone()
            }
        };
        let mut linker: Linker<HostState> = Linker::new(cache::engine());
        w(wasmtime_wasi::p2::add_to_linker_sync(&mut linker))?;
        w(bindings::Block::add_to_linker::<HostState, HasSelf<HostState>>(&mut linker, |s| s))?;
        let capabilities = self.module.trusted_as().ok_or_else(|| self.module.untrusted())?.capabilities;
        let host = HostState::new(self.module.id.clone(), capabilities, self.repos.clone(), self.rt.clone());
        let mut store = Store::new(cache::engine(), host);
        store.set_epoch_deadline(deadline());
        // At its deadline, the time it waited for Prism since is given back: it traps once it
        // computed `DEADLINE` of its own, or lasted `LONGEST`.
        store.epoch_deadline_callback(|mut ctx| {
            let host = ctx.data_mut();
            let waited = std::mem::take(&mut host.waited);
            if host.started.elapsed() >= LONGEST {
                return Err(wasmtime::Error::msg(format!("a call longer than {} minutes, what Prism did for it included", LONGEST.as_secs() / 60)));
            }
            let back = (waited.as_millis() / cache::EPOCH.as_millis()) as u64;
            if back == 0 {
                return Err(wasmtime::Error::msg(format!("a call computing longer than {} minutes", DEADLINE.as_secs() / 60)));
            }
            Ok(wasmtime::UpdateDeadline::Continue(back))
        });
        let block = w(bindings::Block::instantiate(&mut store, &component, &linker))?;
        Ok(Instance { store, block })
    }

    /// A call to its component; a trap drops the instance, says so on its blocks and plans its
    /// restart.
    fn call<T>(&mut self, f: impl FnOnce(&mut Store<HostState>, &bindings::Block) -> wasmtime::Result<T>) -> Result<T> {
        if let Some(why) = &self.given_up {
            return Err(anyhow!("{why}"));
        }
        if self.instance.is_none() && self.restart_at.is_some() {
            return Err(anyhow!("module {}: starting again", self.id()));
        }
        let mark = self.marked(true);
        let out = self.call_marked(f, mark);
        self.marked(false);
        if let Some(i) = self.instance.as_mut() {
            i.store.data_mut().mark = None;
        }
        out
    }

    fn call_marked<T>(&mut self, f: impl FnOnce(&mut Store<HostState>, &bindings::Block) -> wasmtime::Result<T>, mark: Option<String>) -> Result<T> {
        if self.instance.is_none() {
            self.instance = Some(self.instantiate()?);
        }
        let i = self.instance.as_mut().unwrap();
        i.store.set_epoch_deadline(deadline());
        let host = i.store.data_mut();
        host.started = Instant::now();
        host.waited = Duration::ZERO;
        host.mark = mark.map(|text| (self.module.mark.clone(), text));
        match f(&mut i.store, &i.block) {
            Ok(v) => Ok(v),
            Err(trap) => Err(self.crashed(trap)),
        }
    }

    /// Its mark, while its component runs (lifted while Prism works for it, `HostState::mark`):
    /// what it holds, set.
    fn marked(&self, on: bool) -> Option<String> {
        let mark = &self.module.mark;
        if !on {
            let _ = std::fs::remove_file(mark);
            return None;
        }
        let fingerprint = prism_core::modules::fingerprint(&self.module.spec.component)?;
        if let Some(dir) = mark.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(mark, &fingerprint);
        Some(fingerprint)
    }

    fn crashed(&mut self, trap: wasmtime::Error) -> anyhow::Error {
        let said = self.instance.take().map(|i| i.store.data().stderr.take()).unwrap_or_default();
        // A Rust panic says why on its standard error, before the trap.
        let why = said.iter().rev().find(|l| l.contains("panicked")).cloned().or_else(|| said.last().cloned());
        // Why it trapped (a deadline's, its own words), under the wasm backtrace.
        let cause = trap.root_cause().to_string();
        let message = match why {
            Some(why) => format!("the module {} stopped: {why}", self.id()),
            None => format!("the module {} stopped: {cause}", self.id()),
        };
        tracing::error!("{message} ({trap:?})");
        let now = Instant::now();
        self.crashes.retain(|at| now.duration_since(*at) < CRASH_WINDOW);
        self.crashes.push(now);
        let message = if self.crashes.len() >= CRASHES {
            let given_up = format!("{message} ({CRASHES} times in {} minutes: no more until Prism restarts)", CRASH_WINDOW.as_secs() / 60);
            self.given_up = Some(given_up.clone());
            given_up
        } else {
            self.restart_at = Some(now + RESTART_AFTER);
            message
        };
        for (ctx, _) in self.started.values() {
            ctx.set_error(Some(message.clone()));
        }
        anyhow!(message)
    }

    /// After a trap: configured and started again as it was.
    fn restart(&mut self) {
        let Some(c) = self.configured.clone() else { return };
        if self.configure(c).is_err() {
            return;
        }
        let started: Vec<(String, Value)> = self.started.iter().map(|(b, (_, env))| (b.clone(), env.clone())).collect();
        for (block, env) in started {
            self.start_block(&block, env);
        }
    }

    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Configure { config, lang, demo, done } => {
                // A fresh instance: nothing of the former config stays.
                self.instance = None;
                self.restart_at = None;
                self.started.clear();
                self.due.clear();
                let _ = done.send(self.configure(Configured { config, lang, demo }));
            }
            Msg::Start { block, ctx, env } => {
                let same = self.started.get(&block).is_some_and(|(_, e)| *e == env);
                self.started.insert(block.clone(), (ctx, env.clone()));
                if !same {
                    self.start_block(&block, env);
                }
            }
            Msg::Call { block, command, args, demo, reply } => {
                if demo && !self.configured.as_ref().is_some_and(|c| c.demo) {
                    let config = self.configured.as_ref().map(|c| c.config.clone()).unwrap_or_default();
                    if let Err(e) = self.configure(Configured { config, lang: "en".into(), demo: true }) {
                        let _ = reply.send(Err(e));
                        return;
                    }
                }
                let args = args.to_string();
                let answer = self.call(|s, b| b.call_call(s, &block, &command, &args, demo)).and_then(|r| r.map_err(|e| anyhow!(e))).and_then(|text| parse(&text, "call"));
                let _ = reply.send(answer);
            }
            Msg::Hints { repo, reply } => {
                self.allow(&repo);
                let text = repo.display().to_string();
                let answer = self.call(|s, b| b.call_hints(s, &text)).and_then(|r| r.map_err(|e| anyhow!(e))).and_then(|text| hints(&text));
                let _ = reply.send(answer);
            }
            Msg::Images { repo, project, local, reply } => {
                self.allow(&repo);
                for l in &local {
                    self.allow(&l.clone);
                }
                let (text, local) = (repo.display().to_string(), json!(local.iter().map(|l| json!({ "name": l.name, "clone": l.clone })).collect::<Vec<_>>()).to_string());
                let answer = self.call(|s, b| b.call_images(s, &text, &project, &local)).and_then(|r| r.map_err(|e| anyhow!(e))).and_then(|text| serde_json::from_str::<Vec<ImageTarget>>(&text).map_err(|e| anyhow!("images: {e}")));
                let _ = reply.send(answer);
            }
        }
    }

    fn allow(&self, repo: &std::path::Path) {
        let mut repos = self.repos.lock().unwrap();
        if !repos.iter().any(|r| r == repo) {
            repos.push(repo.to_path_buf());
        }
    }

    fn configure(&mut self, c: Configured) -> Result<()> {
        self.configured = Some(c.clone());
        let configured = self.call(|s, b| b.call_configure(s, &c.config, &c.lang, c.demo))?.map_err(|e| anyhow!("module {}: {e}", self.id()))?;
        self.parts = configured.parts.into_iter().filter(|p| !p.is_empty()).collect();
        let stages = match configured.stages.as_deref().map(serde_json::from_str::<Vec<Stage>>) {
            Some(Ok(s)) => Some(Stages(s)),
            Some(Err(e)) => {
                tracing::error!("module {}: stages: {e}", self.id());
                None
            }
            None => None,
        };
        let demo = c.demo.then(|| configured.demo.into_iter().filter_map(|(block, state)| Some((block, serde_json::from_str(&state).ok()?))).collect());
        let mut shared = self.module.shared.lock().unwrap();
        shared.stages = stages;
        if c.demo {
            shared.demo = demo;
        }
        Ok(())
    }

    fn start_block(&mut self, block: &str, env: Value) {
        if let Some(clone) = env["clone"].as_str() {
            self.allow(std::path::Path::new(clone));
        }
        // Its repositories, and the other clones Prism gives a block (`BlockEnv::clones`: a
        // team's base images, say), as inside Prism.
        for clone in ["repos", "clones"].iter().flat_map(|k| env[*k].as_array().into_iter().flatten()).filter_map(|r| r["clone"].as_str()) {
            self.allow(std::path::Path::new(clone));
        }
        let text = env.to_string();
        let started = self.call(|s, b| b.call_start(s, &text)).and_then(|r| r.map_err(|e| anyhow!(e)));
        let ctx = self.started.get(block).map(|(c, _)| c.clone());
        match started {
            Ok(()) => {
                // Started (again, after a trap): its former error goes.
                if let Some(ctx) = &ctx {
                    ctx.set_error(None);
                }
                let now = Instant::now();
                let mine: Vec<String> = self.parts.iter().filter(|p| self.module.owner(p) == block).cloned().collect();
                for part in mine {
                    self.due.insert(part, now);
                }
                // What it published starting (its state, on before its first read answers).
                self.tick("");
            }
            Err(e) => {
                tracing::error!("module {}: start {block}: {e:#}", self.id());
                if let Some(ctx) = ctx {
                    ctx.set_error(Some(format!("{e:#}")));
                }
            }
        }
    }

    fn tick(&mut self, part: &str) {
        let t0 = Instant::now();
        let tick = match self.call(|s, b| b.call_tick(s, part)) {
            Ok(t) => t,
            // Said on its blocks (`crashed`); this part is read again once it restarts.
            Err(_) => return,
        };
        tracing::debug!("module {}: {part:?} in {:?}, next {} s", self.id(), t0.elapsed(), tick.next);
        for p in tick.published {
            let Some((ctx, _)) = self.started.get(&p.block) else {
                tracing::warn!("module {}: a state for {}, not one of its started blocks", self.id(), p.block);
                continue;
            };
            let state: Value = match serde_json::from_str(&p.state) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("module {}: {}'s state: {e}", self.id(), p.block);
                    continue;
                }
            };
            let alerts = p.alerts.as_deref().map(serde_json::from_str::<Vec<Alert>>).transpose();
            let changes = p.changes.as_deref().map(serde_json::from_str::<Vec<ChangeEvent>>).transpose();
            {
                let mut shared = self.module.shared.lock().unwrap();
                match alerts {
                    Ok(a) => drop(shared.alerts.insert(p.block.clone(), a.unwrap_or_default())),
                    Err(e) => tracing::error!("module {}: {}'s alerts: {e}", self.id(), p.block),
                }
                match changes {
                    Ok(c) => drop(shared.changes.insert(p.block.clone(), c.unwrap_or_default())),
                    Err(e) => tracing::error!("module {}: {}'s changes: {e}", self.id(), p.block),
                }
            }
            ctx.publish(&state);
        }
        // No part (`""`, after a start): what it published only, its errors besides.
        let own = Some((part.to_string(), tick.error)).filter(|_| !part.is_empty());
        for (source, error) in own.into_iter().chain(tick.errors) {
            // A source its manifest does not declare goes nowhere: under another block's id, it
            // would overwrite that one's error.
            let Some(source) = self.module.source(&source) else {
                if !self.unknown.contains(&source) {
                    tracing::warn!("module {}: an error for {source}, not one of its manifest's blocks or parts ([[wasm.blocks]] parts)", self.id());
                    self.unknown.push(source);
                }
                continue;
            };
            if let Some((ctx, _)) = self.started.get(self.module.owner(source)) {
                ctx.part(source).set_error(error);
            }
        }
        if part.is_empty() {
            return;
        }
        if tick.next == 0 {
            self.due.remove(part);
        } else {
            self.due.insert(part.to_string(), Instant::now() + Duration::from_secs(tick.next.into()));
        }
    }
}

fn deadline() -> u64 {
    (DEADLINE.as_millis() / cache::EPOCH.as_millis()) as u64
}

fn parse(text: &str, what: &str) -> Result<Value> {
    if text.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(text).map_err(|e| anyhow!("{what}: not JSON: {e}"))
}

/// `{ apps: [...], expected: [{ app, env, version }] }`, the module's hints.
fn hints(text: &str) -> Result<Hints> {
    #[derive(serde::Deserialize)]
    struct Expected {
        app: String,
        env: String,
        version: String,
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(default)]
    struct Raw {
        apps: Vec<String>,
        expected: Vec<Expected>,
    }
    let raw: Raw = serde_json::from_str(text).map_err(|e| anyhow!("hints: {e}"))?;
    Ok(Hints { apps: raw.apps, expected: raw.expected.into_iter().map(|e| ((e.app, e.env), e.version)).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_error_goes_under_a_source_its_manifest_declares_only() {
        let folder = std::env::temp_dir().join(format!("prism-worker-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("b.wasm"), b"").unwrap();
        let spec = crate::manifest::parse(&folder, "[wasm]\ncomponent = \"b.wasm\"\n[[wasm.blocks]]\nid = \"a\"\nparts = [\"a_reads\"]\n[[wasm.blocks]]\nid = \"b\"\n").unwrap();
        let m = Module::new("m".into(), spec, folder.join(".stopped/m"), Some(Vec::new()));
        assert_eq!(m.source("a_reads").map(|s| (s, m.owner(s))), Some(("a_reads", "a")));
        assert_eq!(m.source("b").map(|s| m.owner(s)), Some("b"));
        // Undeclared: not the first block's own error.
        assert_eq!(m.source("undeclared"), None);
        std::fs::remove_dir_all(folder).unwrap();
    }
}
