//! The component's side of the world: what Prism calls (`configure`, `start`, `tick`, `call`,
//! `hints`, `images`), answered by the blocks.
//!
//! A block's collector is a future (its loop, spawned in `start`). Prism ticks a part when due;
//! the futures of that part are polled then, each up to its next `pause`, which tells which part
//! it belongs to (its context's source) and when it is due again. A future not yet polled belongs
//! to the block that spawned it, and runs on the first tick of one of its parts.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use serde_json::{json, Value};

use crate::bindings::prism::module::types::{Configured, Published, Tick};
use crate::block::{Block, BlockCtx, BlockEnv, Stages};
use crate::types::Conventions;

/// A future pending without a pause (it waited on something Prism does not give): tried again
/// this much later.
const STALLED: Duration = Duration::from_secs(60);

type Task = Pin<Box<dyn Future<Output = ()> + Send>>;

struct Running {
    future: Task,
    block: &'static str,
    /// The part its last pause was under; none until then.
    part: Option<&'static str>,
    /// When it is due again (ms since the epoch).
    at: i64,
    /// Polled once at least: its first read made.
    ran: bool,
    /// Pending on calls Prism runs: polled again once one of them wakes it (`wake`).
    waiting: bool,
    wake: Arc<Wake>,
    flags: Arc<Flags>,
}

/// A task's waker: set when a call it waits on finished.
#[derive(Default)]
struct Wake(AtomicBool);

impl std::task::Wake for Wake {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// What a `JoinHandle` reads of its future: stopped (`abort`), ended (done, stopped or dropped).
#[derive(Default)]
pub struct Flags {
    aborted: AtomicBool,
    finished: AtomicBool,
}

impl Drop for Running {
    /// Done, stopped or dropped (its block started again, its start failed): finished.
    fn drop(&mut self) {
        self.flags.finished.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct State {
    config: toml::Table,
    tasks: Vec<Running>,
    /// Spawned while something ran: joined to `tasks` once it is done.
    spawned: Vec<Running>,
    /// The block whose code is running (`start`, a tick, a call): what it spawns is its.
    starting: Option<&'static str>,
    /// The module's first block: what is spawned outside any block's code.
    first: Option<&'static str>,
    published: BTreeMap<&'static str, Value>,
    /// Each source's error as it stands (none: it answered).
    errors: HashMap<&'static str, Option<String>>,
    /// Sources whose error changed since the last tick.
    changed: Vec<&'static str>,
    paused: Option<(&'static str, Duration)>,
    /// The calls Prism runs for it, each one's waker (the future awaiting it).
    calls: HashMap<u64, Waker>,
    /// What was polled last waits on a call (`Call` pending).
    waits: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn publish(block: &'static str, state: Value) {
    with(|s| s.published.insert(block, state));
}

pub(crate) fn set_error(source: &'static str, err: Option<String>) {
    with(|s| {
        if s.errors.get(source) != Some(&err) {
            s.errors.insert(source, err);
            if !s.changed.contains(&source) {
                s.changed.push(source);
            }
        }
    });
}

pub(crate) fn paused(source: &'static str, every: Duration) {
    with(|s| s.paused = Some((source, every)));
}

/// Call `id` awaited by the future polled with `waker`: woken once Prism says it finished.
pub(crate) fn waiting(id: u64, waker: &Waker) {
    with(|s| {
        s.calls.insert(id, waker.clone());
        s.waits = true;
    });
}

/// The futures whose calls finished, woken.
fn wake_finished() {
    let finished = crate::host::finished();
    let wakers: Vec<Waker> = with(|s| finished.iter().filter_map(|id| s.calls.remove(id)).collect());
    for w in wakers {
        w.wake();
    }
}

/// A future spawned by a block (in `start`, a tick, a call): polled when Prism ticks one of its
/// parts. Spawned outside any block's code, it is the first block's.
pub(crate) fn spawn(future: Task) -> Arc<Flags> {
    let flags = Arc::new(Flags::default());
    with(|s| {
        let block = s.starting.or(s.first).unwrap_or("");
        s.spawned.push(Running { future, block, part: None, at: now_ms(), ran: false, waiting: false, wake: Default::default(), flags: flags.clone() });
    });
    flags
}

/// `f` run as `block`'s code: what it spawns is that block's, and joins the tasks after it.
fn as_block<T>(block: &'static str, f: impl FnOnce() -> T) -> T {
    let before = with(|s| s.starting.replace(block));
    let out = f();
    with(|s| {
        s.starting = before;
        let spawned = std::mem::take(&mut s.spawned);
        s.tasks.extend(spawned);
    });
    out
}

fn find(blocks: &'static [&'static dyn Block], id: &str) -> Result<&'static dyn Block, String> {
    blocks.iter().copied().find(|b| b.id() == id).ok_or_else(|| format!("no block {id} in this module"))
}

/// The block a part belongs to: the one with that id or declaring it, or the first.
fn owner(blocks: &'static [&'static dyn Block], part: &str) -> &'static str {
    blocks.iter().find(|b| b.id() == part || b.parts().contains(&part)).or(blocks.first()).map_or("", |b| b.id())
}

/// `future` run to its end now: polled again each time a call it waits on finishes.
fn block_on<T>(future: impl Future<Output = T>) -> Result<T, String> {
    let mut future = std::pin::pin!(future);
    let wake = Arc::new(Wake::default());
    let waker = Waker::from(wake.clone());
    let mut cx = Context::from_waker(&waker);
    let mut stalls = 0;
    loop {
        wake_finished();
        with(|s| s.waits = false);
        if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
            return Ok(v);
        }
        if with(|s| s.waits) {
            // Until one of the calls finishes (its own, or another's: polled again then).
            crate::host::wait();
        } else {
            stalls += 1;
            if stalls > 1000 {
                return Err("it waited on something Prism does not give a module (a timer, a thread)".into());
            }
        }
    }
}

pub fn configure(blocks: &'static [&'static dyn Block], config: String, lang: String, demo: bool) -> Result<Configured, String> {
    crate::log::install();
    let table: toml::Table = toml::from_str(&config).map_err(|e| format!("config: {e}"))?;
    let lang = crate::Lang::of(&lang);
    crate::i18n::set(blocks.iter().flat_map(|b| b.messages(lang).iter().copied()));
    with(|s| *s = State { config: table.clone(), first: blocks.first().map(|b| b.id()), ..Default::default() });
    for b in blocks {
        b.configure(&table).map_err(|e| format!("[{}]: {e:#}", b.id()))?;
    }
    // A block's own id among its parts once (a part may be named as its block).
    let mut parts: Vec<String> = Vec::new();
    for p in blocks.iter().flat_map(|b| std::iter::once(b.id()).chain(b.parts().iter().copied())) {
        if !parts.iter().any(|q| q == p) {
            parts.push(p.to_string());
        }
    }
    let stages = blocks.iter().find_map(|b| b.stages()).map(|p| serde_json::to_string(&p.stages().0).unwrap_or_default());
    let demo = if demo {
        let now = chrono::Utc::now();
        blocks.iter().filter_map(|b| Some((b.id().to_string(), b.demo(now)?.to_string()))).collect()
    } else {
        Vec::new()
    };
    Ok(Configured { parts, stages, demo })
}

pub fn start(blocks: &'static [&'static dyn Block], env: String) -> Result<(), String> {
    let value: Value = serde_json::from_str(&env).map_err(|e| format!("env: {e}"))?;
    let block = find(blocks, value["block"].as_str().unwrap_or_default())?;
    if let Ok(c) = serde_json::from_value::<Conventions>(value["conventions"].clone()) {
        crate::host::set_conventions(c);
    }
    let env: BlockEnv = serde_json::from_value(value).map_err(|e| format!("env: {e}"))?;
    let config = with(|s| {
        s.starting = Some(block.id());
        // Started again (a new config): its former loops stop.
        s.tasks.retain(|t| t.block != block.id());
        s.config.clone()
    });
    let started = block.start(&config, BlockCtx::new(block.id(), Arc::new(env)));
    with(|s| {
        s.starting = None;
        let spawned = std::mem::take(&mut s.spawned);
        // A start that failed runs nothing it spawned before failing: its error stays the block's.
        if started.is_ok() {
            s.tasks.extend(spawned);
        }
    });
    started.map_err(|e| format!("{e:#}"))
}

pub fn tick(blocks: &'static [&'static dyn Block], part: String) -> Tick {
    let now = now_ms();
    let block = owner(blocks, &part);
    // The futures due: this part's, its block's stalled ones, and one of its block's never run
    // (each its first read in a tick of its own, the block's state filling source by source). No part (`""`): none, what
    // was published since (by `start`) only.
    wake_finished();
    let due: Vec<Running> = with(|s| {
        let mut fresh = false;
        let (due, rest): (Vec<Running>, Vec<Running>) = std::mem::take(&mut s.tasks).into_iter().partition(|t| {
            if part.is_empty() {
                return false;
            }
            let mine = t.part == Some(part.as_str()) || (t.part.is_none() && t.block == block);
            // Waiting on calls: when one of them finished.
            if t.waiting {
                return mine && t.wake.0.load(Ordering::Relaxed);
            }
            if t.at > now {
                return false;
            }
            if t.part == Some(part.as_str()) {
                return true;
            }
            if !mine {
                return false;
            }
            // Run before (stalled, no pause): due as any. Never run: one a tick.
            let first = !t.ran && !fresh;
            fresh |= first;
            t.ran || first
        });
        s.tasks = rest;
        due
    });
    for mut t in due {
        if t.flags.aborted.load(Ordering::Relaxed) {
            continue;
        }
        with(|s| {
            s.paused = None;
            s.waits = false;
            s.starting = Some(t.block);
        });
        t.wake.0.store(false, Ordering::Relaxed);
        let waker = Waker::from(t.wake.clone());
        let done = t.future.as_mut().poll(&mut Context::from_waker(&waker)).is_ready();
        t.ran = true;
        let (paused, waits) = with(|s| {
            s.starting = None;
            (s.paused.take(), std::mem::take(&mut s.waits))
        });
        if done {
            continue;
        }
        t.waiting = false;
        match paused {
            Some((source, every)) => {
                t.part = Some(source);
                t.at = now_ms() + every.as_millis() as i64;
            }
            // Its calls running: polled again when one finishes (Prism ticks this part then).
            None if waits => t.waiting = true,
            None => t.at = now_ms() + STALLED.as_millis() as i64,
        }
        with(|s| s.tasks.push(t));
    }
    with(|s| {
        let spawned = std::mem::take(&mut s.spawned);
        s.tasks.extend(spawned);
    });
    let now = chrono::Utc::now();
    let published: Vec<Published> = with(|s| std::mem::take(&mut s.published))
        .into_iter()
        .map(|(id, state)| {
            let b = blocks.iter().find(|b| b.id() == id);
            let alerts = b.map(|b| b.alerts(&state)).filter(|a| !a.is_empty()).map(|a| serde_json::to_string(&a).unwrap_or_default());
            let changes = b.map(|b| b.changes(&state, now)).filter(|c| !c.is_empty()).map(|c| serde_json::to_string(&c).unwrap_or_default());
            Published { block: id.to_string(), state: state.to_string(), alerts, changes }
        })
        .collect();
    let (error, errors, next) = with(|s| {
        let error = s.errors.get(part.as_str()).cloned().flatten();
        let changed = std::mem::take(&mut s.changed);
        let errors = changed.into_iter().filter(|c| *c != part).map(|c| (c.to_string(), s.errors.get(c).cloned().flatten())).collect();
        // When this part is due again: its soonest future; none left, never (until a new start).
        // Those waiting on calls are not: Prism ticks the part when one finishes.
        let mine = s.tasks.iter().filter(|t| !part.is_empty() && !t.waiting && (t.part == Some(part.as_str()) || (t.part.is_none() && t.block == block)));
        let next = mine.map(|t| t.at).min().map_or(0, |at| (((at - now_ms()).max(0) + 999) / 1000).max(1) as u32);
        (error, errors, next)
    });
    Tick { published, error, errors, next }
}

pub fn call(blocks: &'static [&'static dyn Block], block: String, command: String, args: String, demo: bool) -> Result<String, String> {
    let b = find(blocks, &block)?;
    let args: Value = if args.is_empty() { Value::Null } else { serde_json::from_str(&args).map_err(|e| format!("args: {e}"))? };
    let answer = as_block(b.id(), || {
        if demo {
            Ok(b.demo_call(&command, &args).unwrap_or_else(|| Err(anyhow::anyhow!("The demo profile changes nothing: its data is a sample"))))
        } else {
            block_on(b.call(command, args))
        }
    })?;
    answer.map(|v| v.to_string()).map_err(|e| format!("{e:#}"))
}

pub fn hints(blocks: &'static [&'static dyn Block], repo: String) -> Result<String, String> {
    let (block, provider) = blocks.iter().find_map(|b| Some((b.id(), b.stages()?))).ok_or("no block of this module declares stages")?;
    let hints = as_block(block, || block_on(provider.hints(std::path::Path::new(&repo))))?.map_err(|e| format!("{e:#}"))?;
    let expected: Vec<Value> = hints.expected.into_iter().map(|((app, env), version)| json!({ "app": app, "env": env, "version": version })).collect();
    Ok(json!({ "apps": hints.apps, "expected": expected }).to_string())
}

pub fn images(blocks: &'static [&'static dyn Block], repo: String, project: String, local: String) -> Result<String, String> {
    let (block, provider) = blocks.iter().find_map(|b| Some((b.id(), b.images()?))).ok_or("no block of this module names images")?;
    let local: Vec<crate::block::LocalRepo> = serde_json::from_str(&local).map_err(|e| format!("local: {e}"))?;
    let targets = as_block(block, || block_on(provider.images(std::path::Path::new(&repo), &project, &local)))?.map_err(|e| format!("{e:#}"))?;
    serde_json::to_string(&targets).map_err(|e| e.to_string())
}

/// The stages the module declares, for a test.
#[allow(dead_code)]
pub(crate) fn stages_of(blocks: &'static [&'static dyn Block]) -> Option<Stages> {
    blocks.iter().find_map(|b| b.stages()).map(|p| p.stages())
}

/// What `spawn` returns: a handle to stop the future, or know it ended (it cannot be awaited:
/// nothing waits in a module).
pub struct JoinHandle<T> {
    pub(crate) flags: Arc<Flags>,
    pub(crate) _output: std::marker::PhantomData<fn() -> T>,
}

impl<T> JoinHandle<T> {
    /// Stops it: it is not polled again.
    pub fn abort(&self) {
        self.flags.aborted.store(true, Ordering::Relaxed);
    }

    /// It ended: done, stopped, or dropped with its block's former start.
    pub fn is_finished(&self) -> bool {
        self.flags.finished.load(Ordering::Relaxed)
    }
}

/// A value kept across starts and set again on each (rather than a `OnceLock`).
pub struct Swap<T: 'static>(Mutex<Option<&'static T>>);

impl<T: Send + Sync + 'static> Swap<T> {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub fn set(&self, value: T) -> &'static T {
        let value = Box::leak(Box::new(value));
        *self.0.lock().unwrap() = Some(value);
        value
    }

    pub fn get(&self) -> Option<&'static T> {
        *self.0.lock().unwrap()
    }
}

impl<T: Send + Sync + 'static> Default for Swap<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use anyhow::{anyhow, Result};
    use futures::future::BoxFuture;

    use super::*;
    use crate::sdk;

    /// A block of the tests: its parts, and what its `start` and `call` do.
    struct TestBlock {
        id: &'static str,
        parts: &'static [&'static str],
        start: fn() -> Result<()>,
        call: fn(),
    }

    impl Block for TestBlock {
        fn id(&self) -> &'static str {
            self.id
        }
        fn parts(&self) -> &'static [&'static str] {
            self.parts
        }
        fn start(&self, _: &toml::Table, _: BlockCtx) -> Result<()> {
            (self.start)()
        }
        fn call(&self, _: String, _: Value) -> BoxFuture<'static, Result<Value>> {
            let call = self.call;
            Box::pin(async move {
                call();
                Ok(Value::Null)
            })
        }
    }

    fn started(blocks: &'static [&'static dyn Block]) {
        configure(blocks, String::new(), "en".into(), false).unwrap();
        for b in blocks {
            let _ = start(blocks, json!({ "block": b.id() }).to_string());
        }
    }

    #[test]
    fn what_a_call_spawns_runs_on_its_blocks_next_tick() {
        static RAN: AtomicBool = AtomicBool::new(false);
        static A: TestBlock = TestBlock { id: "a", parts: &["a_reads"], start: || Ok(()), call: || drop(sdk::spawn(async { RAN.store(true, Ordering::Relaxed) })) };
        static BLOCKS: &[&dyn Block] = &[&A];
        started(BLOCKS);
        call(BLOCKS, "a".into(), "go".into(), String::new(), false).unwrap();
        assert!(!RAN.load(Ordering::Relaxed));
        tick(BLOCKS, "a_reads".into());
        assert!(RAN.load(Ordering::Relaxed), "the call's future ran on a tick of its block");
    }

    #[test]
    fn a_stalled_future_waits_its_minute_whichever_part_ticks() {
        static POLLS: AtomicUsize = AtomicUsize::new(0);
        static B: TestBlock = TestBlock {
            id: "b",
            parts: &["b1", "b2"],
            start: || {
                // Waits on something Prism does not give, before any pause.
                sdk::spawn(async {
                    POLLS.fetch_add(1, Ordering::Relaxed);
                    std::future::pending::<()>().await
                });
                Ok(())
            },
            call: || {},
        };
        static BLOCKS: &[&dyn Block] = &[&B];
        started(BLOCKS);
        let t = tick(BLOCKS, "b1".into());
        assert_eq!(POLLS.load(Ordering::Relaxed), 1);
        assert!(t.next >= 59, "due again a minute later: {}", t.next);
        tick(BLOCKS, "b2".into());
        tick(BLOCKS, "b1".into());
        assert_eq!(POLLS.load(Ordering::Relaxed), 1, "not polled again before its minute");
    }

    #[test]
    fn a_handle_says_its_future_ended() {
        static HANDLE: Mutex<Option<sdk::JoinHandle<()>>> = Mutex::new(None);
        static C: TestBlock = TestBlock {
            id: "c",
            parts: &[],
            start: || {
                *HANDLE.lock().unwrap() = Some(sdk::spawn(async {}));
                Ok(())
            },
            call: || {},
        };
        static BLOCKS: &[&dyn Block] = &[&C];
        started(BLOCKS);
        assert!(!HANDLE.lock().unwrap().as_ref().unwrap().is_finished());
        tick(BLOCKS, "c".into());
        assert!(HANDLE.lock().unwrap().as_ref().unwrap().is_finished(), "done: finished");
    }

    #[test]
    fn a_start_that_fails_runs_nothing_it_spawned() {
        static RAN: AtomicBool = AtomicBool::new(false);
        static D: TestBlock = TestBlock {
            id: "d",
            parts: &[],
            start: || {
                sdk::spawn(async { RAN.store(true, Ordering::Relaxed) });
                Err(anyhow!("[d] other: unreadable"))
            },
            call: || {},
        };
        static BLOCKS: &[&dyn Block] = &[&D];
        configure(BLOCKS, String::new(), "en".into(), false).unwrap();
        assert!(start(BLOCKS, json!({ "block": "d" }).to_string()).unwrap_err().contains("unreadable"));
        let t = tick(BLOCKS, "d".into());
        assert!(!RAN.load(Ordering::Relaxed));
        assert_eq!(t.next, 0, "nothing left to tick");
    }
}
